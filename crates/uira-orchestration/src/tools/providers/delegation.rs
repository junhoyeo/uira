//! Delegation tool provider - enables subagent orchestration

use crate::features::background_agent::{
    get_background_manager, BackgroundManager, BackgroundTaskConfig, BackgroundTaskStatus,
    LaunchInput,
};
use crate::tools::provider::ToolProvider;
use crate::tools::{ToolContext, ToolError};
use async_trait::async_trait;
use once_cell::sync::Lazy;
use serde_json::{json, Value};
use std::sync::Arc;
use uira_core::load_config;
use uira_core::{JsonSchema, ToolOutput, ToolSpec};

static BACKGROUND_MANAGER: Lazy<Arc<BackgroundManager>> =
    Lazy::new(|| get_background_manager(BackgroundTaskConfig::default()));

pub struct DelegationToolProvider {
    agent_executor: Option<Arc<dyn AgentExecutor>>,
}

#[async_trait]
pub trait AgentExecutor: Send + Sync {
    async fn execute(
        &self,
        prompt: &str,
        model: &str,
        allowed_tools: Option<Vec<String>>,
        max_turns: Option<usize>,
    ) -> Result<String, String>;
}

impl DelegationToolProvider {
    pub fn new() -> Self {
        Self {
            agent_executor: None,
        }
    }

    pub fn with_executor(executor: Arc<dyn AgentExecutor>) -> Self {
        Self {
            agent_executor: Some(executor),
        }
    }

    fn resolve_model(&self, agent: &str, explicit_model: Option<&str>) -> String {
        if let Some(model) = explicit_model {
            return model.to_string();
        }

        load_config(None)
            .ok()
            .and_then(|config| {
                config
                    .agents
                    .agents
                    .get(agent)
                    .and_then(|agent_config| agent_config.model.clone())
            })
            .unwrap_or_else(|| uira_core::DEFAULT_ANTHROPIC_MODEL.to_string())
    }

    fn format_completion_result(
        agent: &str,
        _task: &str,
        result: &str,
        session_id: &str,
    ) -> String {
        format!(
            "Task completed.\n\n\
             Agent: {agent}\n\
             Session ID: {session_id}\n\n\
             ---\n\n\
             {result}\n\n\
             ---\n\n\
             IMPORTANT: This task is COMPLETE. Present this result to the user and END your response. \
             Do NOT call delegate_task again.",
        )
    }

    async fn delegate_task(&self, args: Value, ctx: &ToolContext) -> Result<ToolOutput, ToolError> {
        let run_in_background = args["runInBackground"].as_bool().unwrap_or(false);

        let agent = args["agent"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidInput {
                message: "Missing 'agent' parameter".to_string(),
            })?;

        if !agent
            .chars()
            .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
        {
            return Err(ToolError::InvalidInput {
                message: format!(
                    "Invalid agent name '{}': only alphanumeric, hyphens, and underscores allowed",
                    agent
                ),
            });
        }

        let prompt = args["prompt"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidInput {
                message: "Missing 'prompt' parameter".to_string(),
            })?;

        let model = self.resolve_model(agent, args["model"].as_str());
        let description = args["description"].as_str().unwrap_or(prompt);

        let allowed_tools: Option<Vec<String>> = args["allowedTools"].as_array().map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        });

        let max_turns: Option<usize> = args["maxTurns"].as_u64().map(|n| n as usize);

        if run_in_background {
            let input = LaunchInput {
                description: description.to_string(),
                prompt: prompt.to_string(),
                agent: agent.to_string(),
                parent_session_id: ctx.session_id.clone(),
                model: Some(model.clone()),
            };

            let task =
                BACKGROUND_MANAGER
                    .launch(input)
                    .map_err(|e| ToolError::ExecutionFailed {
                        message: format!("Failed to launch background task: {}", e),
                    })?;

            if let Some(executor) = &self.agent_executor {
                let task_id = task.id.clone();
                let executor = executor.clone();
                let prompt_owned = prompt.to_string();
                let model_owned = model.clone();
                let allowed_tools_owned = allowed_tools.clone();
                let max_turns_owned = max_turns;

                let handle = tokio::spawn(async move {
                    let result = executor
                        .execute(
                            &prompt_owned,
                            &model_owned,
                            allowed_tools_owned,
                            max_turns_owned,
                        )
                        .await;

                    match result {
                        Ok(output) => {
                            BACKGROUND_MANAGER.complete_task(&task_id, output);
                        }
                        Err(e) => {
                            BACKGROUND_MANAGER.fail_task(&task_id, e);
                        }
                    }
                });

                let task_id_watcher = task.id.clone();
                tokio::spawn(async move {
                    if let Err(e) = handle.await {
                        let error_msg = if e.is_panic() {
                            "Task panicked during execution".to_string()
                        } else if e.is_cancelled() {
                            "Task was cancelled by runtime".to_string()
                        } else {
                            format!("Task failed: {}", e)
                        };
                        BACKGROUND_MANAGER.fail_task(&task_id_watcher, error_msg);
                    }
                });
            }

            Ok(ToolOutput::text(
                serde_json::to_string_pretty(&json!({
                    "taskId": task.id,
                    "agent": task.agent,
                    "description": task.description,
                    "status": "running",
                    "message": "Task started in background. Use background_output to get results."
                }))
                .unwrap(),
            ))
        } else {
            match &self.agent_executor {
                Some(executor) => {
                    let subagent_session_id = format!("sub_{}", uuid::Uuid::new_v4());
                    let result = executor
                        .execute(prompt, &model, allowed_tools, max_turns)
                        .await;
                    match result {
                        Ok(output) => {
                            let formatted = Self::format_completion_result(
                                agent,
                                description,
                                &output,
                                &subagent_session_id,
                            );
                            Ok(ToolOutput::text(formatted))
                        }
                        Err(e) => Err(ToolError::ExecutionFailed { message: e }),
                    }
                }
                None => Err(ToolError::ExecutionFailed {
                    message: "No agent executor configured. Delegation is not available."
                        .to_string(),
                }),
            }
        }
    }

    async fn background_output(&self, args: Value) -> Result<ToolOutput, ToolError> {
        let task_id = args["taskId"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidInput {
                message: "Missing 'taskId' parameter".to_string(),
            })?;

        let block = args["block"].as_bool().unwrap_or(false);
        let timeout_secs = args["timeout"].as_u64().unwrap_or(120);

        if block {
            let start = std::time::Instant::now();
            let timeout = std::time::Duration::from_secs(timeout_secs);

            loop {
                if let Some(task) = BACKGROUND_MANAGER.get_task(task_id) {
                    match task.status {
                        BackgroundTaskStatus::Completed => {
                            return Ok(ToolOutput::text(
                                task.result
                                    .unwrap_or_else(|| "Task completed with no output".to_string()),
                            ));
                        }
                        BackgroundTaskStatus::Error => {
                            return Err(ToolError::ExecutionFailed {
                                message: task.error.unwrap_or_else(|| {
                                    "Task failed with unknown error".to_string()
                                }),
                            });
                        }
                        BackgroundTaskStatus::Cancelled => {
                            return Ok(ToolOutput::text(
                                serde_json::to_string_pretty(&json!({
                                    "taskId": task_id,
                                    "status": "cancelled",
                                    "message": "Task was cancelled"
                                }))
                                .unwrap(),
                            ));
                        }
                        _ => {
                            if start.elapsed() > timeout {
                                return Ok(ToolOutput::text(
                                    serde_json::to_string_pretty(&json!({
                                        "taskId": task_id,
                                        "status": format!("{:?}", task.status).to_lowercase(),
                                        "message": "Timeout waiting for task completion"
                                    }))
                                    .unwrap(),
                                ));
                            }
                            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                        }
                    }
                } else {
                    return Err(ToolError::NotFound {
                        name: format!("Task not found: {}", task_id),
                    });
                }
            }
        } else if let Some(task) = BACKGROUND_MANAGER.get_task(task_id) {
            let status_str = match task.status {
                BackgroundTaskStatus::Queued => "queued",
                BackgroundTaskStatus::Pending => "pending",
                BackgroundTaskStatus::Running => "running",
                BackgroundTaskStatus::Completed => "completed",
                BackgroundTaskStatus::Error => "error",
                BackgroundTaskStatus::Cancelled => "cancelled",
            };

            let mut response = json!({
                "taskId": task.id,
                "status": status_str,
                "agent": task.agent,
                "startedAt": task.started_at.to_rfc3339(),
            });

            if let Some(completed_at) = task.completed_at {
                response["completedAt"] = json!(completed_at.to_rfc3339());
            }

            if let Some(result) = task.result {
                response["result"] = json!(result);
            }

            if let Some(error) = task.error {
                response["error"] = json!(error);
            }

            if let Some(progress) = task.progress {
                response["progress"] = json!({
                    "toolCalls": progress.tool_calls,
                    "lastTool": progress.last_tool,
                    "lastUpdate": progress.last_update.to_rfc3339(),
                });
            }

            Ok(ToolOutput::text(
                serde_json::to_string_pretty(&response).unwrap(),
            ))
        } else {
            Err(ToolError::NotFound {
                name: format!("Task not found: {}", task_id),
            })
        }
    }

    async fn background_cancel(&self, args: Value) -> Result<ToolOutput, ToolError> {
        let cancel_all = args["all"].as_bool().unwrap_or(false);

        if cancel_all {
            let tasks = BACKGROUND_MANAGER.get_all_tasks();
            let mut cancelled = 0;
            for task in tasks {
                if !matches!(
                    task.status,
                    BackgroundTaskStatus::Completed
                        | BackgroundTaskStatus::Error
                        | BackgroundTaskStatus::Cancelled
                ) {
                    BACKGROUND_MANAGER.cancel_task(&task.id);
                    cancelled += 1;
                }
            }
            Ok(ToolOutput::text(
                serde_json::to_string_pretty(&json!({
                    "cancelled": cancelled,
                    "message": format!("Cancelled {} background task(s)", cancelled)
                }))
                .unwrap(),
            ))
        } else if let Some(task_id) = args["taskId"].as_str() {
            if let Some(task) = BACKGROUND_MANAGER.cancel_task(task_id) {
                let status_str = match task.status {
                    BackgroundTaskStatus::Queued => "queued",
                    BackgroundTaskStatus::Pending => "pending",
                    BackgroundTaskStatus::Running => "running",
                    BackgroundTaskStatus::Completed => "completed",
                    BackgroundTaskStatus::Error => "error",
                    BackgroundTaskStatus::Cancelled => "cancelled",
                };
                let message = if task.status == BackgroundTaskStatus::Cancelled {
                    "Task cancelled successfully"
                } else {
                    "Task was already in terminal state"
                };
                Ok(ToolOutput::text(
                    serde_json::to_string_pretty(&json!({
                        "taskId": task.id,
                        "status": status_str,
                        "message": message
                    }))
                    .unwrap(),
                ))
            } else {
                Err(ToolError::NotFound {
                    name: format!("Task not found: {}", task_id),
                })
            }
        } else {
            Err(ToolError::InvalidInput {
                message: "Must provide either 'taskId' or 'all: true'".to_string(),
            })
        }
    }
}

impl Default for DelegationToolProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ToolProvider for DelegationToolProvider {
    fn specs(&self) -> Vec<ToolSpec> {
        vec![
            ToolSpec::new(
                "delegate_task",
                "Delegate a task to a specialized subagent. The subagent runs autonomously with its own tool access.",
                JsonSchema::object()
                    .property("agent", JsonSchema::string().description("Agent name (e.g., 'explore', 'architect', 'executor')"))
                    .property("prompt", JsonSchema::string().description("Task/prompt for the agent to execute"))
                    .property("model", JsonSchema::string().description("Override model (e.g., 'claude-sonnet-4-20250514'). Uses agent default if not specified"))
                    .property("allowedTools", JsonSchema::array(JsonSchema::string()).description("Tools to allow (e.g., ['Read', 'Glob']). Defaults to agent's configured tools"))
                    .property("maxTurns", JsonSchema::number().description("Maximum turns before stopping. Uses agent default (100) if not specified"))
                    .property("runInBackground", JsonSchema::boolean().description("If true, runs in background and returns task_id"))
                    .required(&["agent", "prompt"]),
            ),
            ToolSpec::new(
                "background_output",
                "Get output from a background task. Returns immediately if complete, otherwise shows current status.",
                JsonSchema::object()
                    .property("taskId", JsonSchema::string().description("Task ID from delegate_task with runInBackground=true"))
                    .property("block", JsonSchema::boolean().description("If true, blocks until complete (max 120s)"))
                    .property("timeout", JsonSchema::number().description("Timeout in seconds when blocking. Default: 120"))
                    .required(&["taskId"]),
            ),
            ToolSpec::new(
                "background_cancel",
                "Cancel a running background task or all tasks.",
                JsonSchema::object()
                    .property("taskId", JsonSchema::string().description("Task ID to cancel"))
                    .property("all", JsonSchema::boolean().description("If true, cancels ALL running tasks")),
            ),
        ]
    }

    fn handles(&self, name: &str) -> bool {
        matches!(
            name,
            "delegate_task" | "background_output" | "background_cancel"
        )
    }

    async fn execute(
        &self,
        name: &str,
        input: Value,
        ctx: &ToolContext,
    ) -> Result<ToolOutput, ToolError> {
        match name {
            "delegate_task" => self.delegate_task(input, ctx).await,
            "background_output" => self.background_output(input).await,
            "background_cancel" => self.background_cancel(input).await,
            _ => Err(ToolError::NotFound {
                name: name.to_string(),
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_delegation_provider_handles() {
        let provider = DelegationToolProvider::new();
        assert!(provider.handles("delegate_task"));
        assert!(provider.handles("background_output"));
        assert!(provider.handles("background_cancel"));
        assert!(!provider.handles("lsp_goto_definition"));
        assert!(!provider.handles("read_file"));
    }

    #[test]
    fn test_delegation_provider_specs() {
        let provider = DelegationToolProvider::new();
        let specs = provider.specs();
        assert_eq!(specs.len(), 3);
        assert!(specs.iter().any(|s| s.name == "delegate_task"));
        assert!(specs.iter().any(|s| s.name == "background_output"));
        assert!(specs.iter().any(|s| s.name == "background_cancel"));
    }
}
