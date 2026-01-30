use std::path::PathBuf;

use anyhow::Result;
use uira_agent::{Agent, AgentConfig, AgentLoopError};
use uira_protocol::Provider;
use uira_providers::{ModelClientBuilder, ProviderConfig};

use super::{
    prompts::build_system_prompt, CompletionDetector, GitTracker, VerificationResult,
    WorkflowConfig, WorkflowState, WorkflowTask, WorkflowVerifier,
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
    agent: Agent,
    state: WorkflowState,
    completion_detector: CompletionDetector,
    git_tracker: GitTracker,
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
    pub async fn new(task: WorkflowTask, config: WorkflowConfig) -> Result<Self> {
        let existing_state = WorkflowState::read(task);

        let provider = parse_provider(&config.provider);
        let provider_config = ProviderConfig {
            provider,
            api_key: None,
            base_url: None,
            model: config.model.clone(),
            max_tokens: None,
            temperature: None,
            timeout_seconds: Some(120),
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
            if let Some(ref rollout_path) = state.rollout_path {
                Agent::resume_from_rollout(agent_config, client, PathBuf::from(rollout_path))?
            } else {
                Agent::new(agent_config, client)
            }
        } else {
            Agent::new(agent_config, client)
        };

        let agent = agent.with_rollout()?;

        let git_tracker = GitTracker::new(&config.working_directory);

        let state = existing_state.unwrap_or_else(|| {
            WorkflowState::new(task, agent.session().id.to_string(), config.max_iterations)
        });

        Ok(Self {
            task,
            config,
            agent,
            state,
            completion_detector: CompletionDetector::new(),
            git_tracker,
        })
    }

    pub async fn run(&mut self) -> Result<WorkflowResult> {
        let initial_prompt = self.build_initial_prompt();

        if let Some(path) = self.agent.rollout_path() {
            self.state.rollout_path = Some(path.to_string_lossy().to_string());
        }
        self.state.write()?;

        loop {
            if self.state.iteration >= self.state.max_iterations {
                let files_modified = self.git_tracker.get_modifications();
                let result = WorkflowResult::MaxIterationsReached {
                    iterations: self.state.iteration,
                    files_modified,
                };
                WorkflowState::clear(self.task)?;
                return Ok(result);
            }

            let prompt = if self.state.iteration == 0 {
                initial_prompt.clone()
            } else {
                self.build_continuation_prompt()
            };

            match self.agent.run(&prompt).await {
                Ok(exec_result) => {
                    let response_text = &exec_result.output;

                    if self.completion_detector.is_done(response_text) {
                        let verification =
                            WorkflowVerifier::verify(self.task, &self.config.working_directory)?;

                        match verification {
                            VerificationResult::Pass => {
                                let summary =
                                    self.completion_detector.extract_summary(response_text);
                                let files_modified = self.git_tracker.get_modifications();

                                if self.config.auto_stage && !files_modified.is_empty() {
                                    self.git_tracker.stage_files(&files_modified)?;
                                }

                                let result = WorkflowResult::Complete {
                                    iterations: self.state.iteration + 1,
                                    files_modified,
                                    summary,
                                };

                                WorkflowState::clear(self.task)?;
                                return Ok(result);
                            }
                            VerificationResult::Fail {
                                remaining_issues,
                                details,
                            } => {
                                self.state.increment();
                                self.state.write()?;

                                if self.state.iteration >= self.state.max_iterations {
                                    let result = WorkflowResult::VerificationFailed {
                                        remaining_issues,
                                        details,
                                    };
                                    WorkflowState::clear(self.task)?;
                                    return Ok(result);
                                }
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
                    WorkflowState::clear(self.task)?;
                    return Ok(WorkflowResult::Failed {
                        error: e.to_string(),
                    });
                }
            }
        }
    }

    fn build_initial_prompt(&self) -> String {
        let files_context = if self.config.files.is_empty() {
            if self.config.staged_only {
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
