use std::path::PathBuf;

use anyhow::Result;
use uira_agent::{Agent, AgentConfig, AgentLoopError};
use uira_core::Provider;
use uira_providers::{ModelClientBuilder, ProviderConfig};

use super::{
    detectors::{Detector, RenderBudget, Scope},
    prompts::build_system_prompt,
    CompletionDetector, GitTracker, WorkflowConfig, WorkflowState, WorkflowTask,
};

#[derive(Debug)]
pub enum WorkflowResult {
    Complete {
        iterations: u32,
        files_modified: Vec<String>,
        summary: Option<String>,
    },
    MaxIterationsReached {
        iterations: u32,
        files_modified: Vec<String>,
    },
    VerificationFailed {
        remaining_issues: usize,
        details: String,
    },
    Cancelled,
    Failed {
        error: String,
    },
}

pub struct AgentWorkflow {
    task: WorkflowTask,
    config: WorkflowConfig,
    agent: Option<Agent>,
    state: WorkflowState,
    completion_detector: CompletionDetector,
    git_tracker: GitTracker,
    detector: Option<Box<dyn Detector>>,
    scope: Option<Scope>,
}

fn parse_provider(s: &str) -> Provider {
    match s.to_lowercase().as_str() {
        "anthropic" => Provider::Anthropic,
        "openai" => Provider::OpenAI,
        "google" => Provider::Google,
        "ollama" => Provider::Ollama,
        "opencode" => Provider::OpenCode,
        "openrouter" => Provider::OpenRouter,
        _ => Provider::Custom,
    }
}

impl AgentWorkflow {
    pub async fn new(
        task: WorkflowTask,
        config: WorkflowConfig,
        detector: Option<Box<dyn Detector>>,
        scope: Option<Scope>,
    ) -> Result<Self> {
        let existing_state = WorkflowState::read(task);

        let git_tracker = GitTracker::new(&config.working_directory);

        let has_detector = detector.is_some() && scope.is_some();
        let (agent, state) = if has_detector {
            let state = existing_state.unwrap_or_else(|| {
                WorkflowState::new(task, "pending".to_string(), config.max_iterations)
            });
            (None, state)
        } else {
            let (agent, state) =
                Self::create_agent_and_state(task, &config, existing_state).await?;
            (Some(agent), state)
        };

        Ok(Self {
            task,
            config,
            agent,
            state,
            completion_detector: CompletionDetector::new(),
            git_tracker,
            detector,
            scope,
        })
    }

    async fn create_agent_and_state(
        task: WorkflowTask,
        config: &WorkflowConfig,
        existing_state: Option<WorkflowState>,
    ) -> Result<(Agent, WorkflowState)> {
        let provider = parse_provider(&config.provider);
        let provider_config = ProviderConfig {
            provider,
            api_key: None,
            base_url: None,
            model: config.model.clone(),
            max_tokens: None,
            temperature: None,
            timeout_seconds: Some(120),
            max_retries: Some(3),
            enable_thinking: false,
            thinking_budget: None,
        };

        let client = ModelClientBuilder::new()
            .with_config(provider_config)
            .build()
            .map_err(|e| anyhow::anyhow!("Failed to create model client: {}", e))?;

        let agent_config = AgentConfig::new()
            .with_max_turns(config.max_iterations as usize * 10)
            .with_working_directory(&config.working_directory)
            .with_system_prompt(build_system_prompt(task, &config.task_options))
            .full_auto();

        let agent = if let Some(ref state) = existing_state {
            if let Some(ref session_path) = state.session_path {
                Agent::resume_from_session(agent_config, client, PathBuf::from(session_path))?
            } else {
                Agent::new(agent_config, client)
            }
        } else {
            Agent::new(agent_config, client)
        };

        let agent = agent.with_session_recording()?;

        let state = existing_state.unwrap_or_else(|| {
            WorkflowState::new(task, agent.session().id.to_string(), config.max_iterations)
        });

        Ok((agent, state))
    }

    pub async fn run(&mut self) -> Result<WorkflowResult> {
        let has_detector = self.detector.is_some() && self.scope.is_some();
        if has_detector {
            self.run_with_detector().await
        } else {
            self.run_legacy().await
        }
    }

    async fn run_with_detector(&mut self) -> Result<WorkflowResult> {
        let detector = self.detector.take().expect("detector must be set");
        let scope = self.scope.take().expect("scope must be set");

        let issues = detector.detect(&scope)?;

        if issues.is_empty() {
            let _ = WorkflowState::clear(self.task);
            return Ok(WorkflowResult::Complete {
                iterations: 0,
                files_modified: vec![],
                summary: Some("No issues found.".to_string()),
            });
        }

        let (agent, state) =
            Self::create_agent_and_state(self.task, &self.config, Some(self.state.clone())).await?;
        self.agent = Some(agent);
        self.state = state;

        if let Some(ref agent) = self.agent {
            if let Some(path) = agent.session_path() {
                self.state.session_path = Some(path.to_string_lossy().to_string());
            }
        }
        self.state.write()?;

        let mut current_issues = issues;

        loop {
            if self.state.iteration >= self.state.max_iterations {
                let files_modified = self.git_tracker.get_modifications();
                let result = WorkflowResult::MaxIterationsReached {
                    iterations: self.state.iteration,
                    files_modified,
                };
                let _ = WorkflowState::clear(self.task);
                return Ok(result);
            }

            let prompt = detector.render_prompt(
                &current_issues,
                &RenderBudget {
                    max_issues: 50,
                    include_context: true,
                },
            );

            let agent = self
                .agent
                .as_mut()
                .expect("agent must exist after lazy creation");

            match agent.run(&prompt).await {
                Ok(exec_result) => {
                    let response_text = &exec_result.output;

                    if self.completion_detector.is_done(response_text) {
                        let remaining = detector.detect(&scope)?;

                        if remaining.is_empty() {
                            let summary = self.completion_detector.extract_summary(response_text);
                            let files_modified = self.git_tracker.get_modifications();

                            if self.config.auto_stage && !files_modified.is_empty() {
                                self.git_tracker.stage_files(&files_modified)?;
                            }

                            // Handle auto-commit if enabled
                            if self.config.auto_commit && !files_modified.is_empty() {
                                let commit_msg = GitTracker::generate_commit_message(
                                    self.task.name(),
                                    &files_modified,
                                    summary.as_deref(),
                                );
                                self.git_tracker.commit(&commit_msg)?;
                                println!("   Committed: {}", commit_msg);
                            }

                            let result = WorkflowResult::Complete {
                                iterations: self.state.iteration + 1,
                                files_modified,
                                summary,
                            };

                            let _ = WorkflowState::clear(self.task);
                            return Ok(result);
                        } else {
                            current_issues = remaining.clone();
                            self.state.increment();
                            self.state.write()?;

                            if self.state.iteration >= self.state.max_iterations {
                                let result = WorkflowResult::VerificationFailed {
                                    remaining_issues: remaining.len(),
                                    details: format!(
                                        "{} issues remain after {} iterations",
                                        remaining.len(),
                                        self.state.iteration
                                    ),
                                };
                                let _ = WorkflowState::clear(self.task);
                                return Ok(result);
                            }
                        }
                    } else {
                        self.state.increment();
                        self.state.write()?;
                    }
                }
                Err(AgentLoopError::Cancelled) => {
                    return Ok(WorkflowResult::Cancelled);
                }
                Err(e) => {
                    let _ = WorkflowState::clear(self.task);
                    return Ok(WorkflowResult::Failed {
                        error: e.to_string(),
                    });
                }
            }
        }
    }

    async fn run_legacy(&mut self) -> Result<WorkflowResult> {
        let initial_prompt = self.build_initial_prompt();

        {
            let agent = self
                .agent
                .as_mut()
                .expect("legacy path requires eager agent creation");
            if let Some(path) = agent.session_path() {
                self.state.session_path = Some(path.to_string_lossy().to_string());
            }
        }
        self.state.write()?;

        loop {
            if self.state.iteration >= self.state.max_iterations {
                let files_modified = self.git_tracker.get_modifications();
                let result = WorkflowResult::MaxIterationsReached {
                    iterations: self.state.iteration,
                    files_modified,
                };
                let _ = WorkflowState::clear(self.task);
                return Ok(result);
            }

            let prompt = if self.state.iteration == 0 {
                initial_prompt.clone()
            } else {
                self.build_continuation_prompt()
            };

            let agent = self.agent.as_mut().expect("agent must exist");
            match agent.run(&prompt).await {
                Ok(exec_result) => {
                    let response_text = &exec_result.output;

                    if self.completion_detector.is_done(response_text) {
                        let summary = self.completion_detector.extract_summary(response_text);
                        let files_modified = self.git_tracker.get_modifications();

                        if self.config.auto_stage && !files_modified.is_empty() {
                            self.git_tracker.stage_files(&files_modified)?;
                        }

                        // Handle auto-commit if enabled
                        if self.config.auto_commit && !files_modified.is_empty() {
                            let commit_msg = GitTracker::generate_commit_message(
                                self.task.name(),
                                &files_modified,
                                summary.as_deref(),
                            );
                            self.git_tracker.commit(&commit_msg)?;
                            println!("   Committed: {}", commit_msg);
                        }

                        let result = WorkflowResult::Complete {
                            iterations: self.state.iteration + 1,
                            files_modified,
                            summary,
                        };

                        let _ = WorkflowState::clear(self.task);
                        return Ok(result);
                    } else {
                        self.state.increment();
                        self.state.write()?;
                    }
                }
                Err(AgentLoopError::Cancelled) => {
                    return Ok(WorkflowResult::Cancelled);
                }
                Err(e) => {
                    let _ = WorkflowState::clear(self.task);
                    return Ok(WorkflowResult::Failed {
                        error: e.to_string(),
                    });
                }
            }
        }
    }

    fn build_initial_prompt(&self) -> String {
        let files_context = if self.config.files.is_empty() {
            if self.config.cached_only {
                "Process only staged files (use `git diff --cached --name-only`)."
            } else {
                "Process all relevant files in the repository."
            }
        } else {
            "Process the specified files."
        };

        format!(
            "Begin the {task} workflow.\n\n\
            {files_context}\n\n\
            Files to process: {files}\n\n\
            Remember: Output <DONE/> when all issues are fixed.",
            task = self.task.name(),
            files_context = files_context,
            files = if self.config.files.is_empty() {
                "(auto-detect)".to_string()
            } else {
                self.config.files.join(", ")
            },
        )
    }

    fn build_continuation_prompt(&self) -> String {
        format!(
            "Continue the {task} workflow.\n\n\
            Iteration: {iter}/{max}\n\
            Files modified so far: {files}\n\n\
            Continue fixing issues. Output <DONE/> when complete.",
            task = self.task.name(),
            iter = self.state.iteration + 1,
            max = self.state.max_iterations,
            files = self.git_tracker.get_modifications().len(),
        )
    }
}
