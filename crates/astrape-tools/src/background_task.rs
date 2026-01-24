use crate::types::{ToolDefinition, ToolError, ToolInput, ToolOutput};
use astrape_features::background_agent::{
    get_background_manager, BackgroundTask, BackgroundTaskConfig, BackgroundTaskStatus, LaunchInput,
};
use serde_json::{json, Value};
use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;

/// Extract a required string field from input, returning ToolError on failure.
fn get_required_string(input: &Value, field: &str) -> Result<String, ToolError> {
    input
        .get(field)
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| ToolError::InvalidInput {
            message: format!("missing required field: {field}"),
        })
}

/// Extract an optional string field from input.
fn get_optional_string(input: &Value, field: &str) -> Option<String> {
    input
        .get(field)
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}

/// Extract an optional u64 field from input.
fn get_optional_u64(input: &Value, field: &str) -> Option<u64> {
    input.get(field).and_then(|v| v.as_u64())
}

/// Extract an optional bool field from input.
fn get_optional_bool(input: &Value, field: &str) -> Option<bool> {
    input.get(field).and_then(|v| v.as_bool())
}

/// Serialize a BackgroundTask to a JSON response object.
fn task_to_json(task: &BackgroundTask) -> Value {
    json!({
        "task_id": task.id,
        "session_id": task.session_id,
        "parent_session_id": task.parent_session_id,
        "description": task.description,
        "agent": task.agent,
        "status": format!("{:?}", task.status).to_lowercase(),
        "queued_at": task.queued_at.map(|dt| dt.to_rfc3339()),
        "started_at": task.started_at.to_rfc3339(),
        "completed_at": task.completed_at.map(|dt| dt.to_rfc3339()),
        "result": task.result,
        "error": task.error,
        "progress": task.progress.as_ref().map(|p| json!({
            "tool_calls": p.tool_calls,
            "last_tool": p.last_tool,
            "last_update": p.last_update.to_rfc3339(),
            "last_message": p.last_message,
            "last_message_at": p.last_message_at.map(|dt| dt.to_rfc3339()),
        })),
        "concurrency_key": task.concurrency_key,
    })
}

/// Handle the "launch" action: start a new background task.
async fn handle_launch(
    input: &Value,
    manager: &Arc<astrape_features::background_agent::BackgroundManager>,
) -> Result<ToolOutput, ToolError> {
    let description = get_required_string(input, "description")?;
    let prompt = get_required_string(input, "prompt")?;
    let agent = get_optional_string(input, "agent").unwrap_or_else(|| "executor".to_string());
    let parent_session_id =
        get_optional_string(input, "parent_session_id").unwrap_or_else(|| "unknown".to_string());
    let model = get_optional_string(input, "model");

    let launch_input = LaunchInput {
        description,
        prompt,
        agent,
        parent_session_id,
        model,
    };

    match manager.launch(launch_input) {
        Ok(task) => {
            let response = json!({
                "success": true,
                "action": "launch",
                "task": task_to_json(&task),
            });
            Ok(ToolOutput::text(
                serde_json::to_string_pretty(&response).unwrap(),
            ))
        }
        Err(e) => {
            let response = json!({
                "success": false,
                "action": "launch",
                "error": e,
            });
            Ok(ToolOutput::text(
                serde_json::to_string_pretty(&response).unwrap(),
            ))
        }
    }
}

/// Handle the "output" action: retrieve output from a running/completed task.
async fn handle_output(
    input: &Value,
    manager: &Arc<astrape_features::background_agent::BackgroundManager>,
) -> Result<ToolOutput, ToolError> {
    let task_id = get_required_string(input, "taskId")?;
    let block = get_optional_bool(input, "block").unwrap_or(false);
    let timeout_ms = get_optional_u64(input, "timeout").unwrap_or(30_000);

    let start = std::time::Instant::now();
    let timeout = Duration::from_millis(timeout_ms);

    loop {
        let task = manager.get_task(&task_id);

        match task {
            Some(t) => {
                let is_terminal = matches!(
                    t.status,
                    BackgroundTaskStatus::Completed
                        | BackgroundTaskStatus::Error
                        | BackgroundTaskStatus::Cancelled
                );

                if is_terminal || !block {
                    let response = json!({
                        "success": true,
                        "action": "output",
                        "task": task_to_json(&t),
                        "is_complete": is_terminal,
                    });
                    return Ok(ToolOutput::text(
                        serde_json::to_string_pretty(&response).unwrap(),
                    ));
                }

                // Still running and blocking requested - wait and retry
                if start.elapsed() >= timeout {
                    let response = json!({
                        "success": true,
                        "action": "output",
                        "task": task_to_json(&t),
                        "is_complete": false,
                        "timeout": true,
                    });
                    return Ok(ToolOutput::text(
                        serde_json::to_string_pretty(&response).unwrap(),
                    ));
                }

                sleep(Duration::from_millis(100)).await;
            }
            None => {
                let response = json!({
                    "success": false,
                    "action": "output",
                    "error": format!("Task not found: {task_id}"),
                });
                return Ok(ToolOutput::text(
                    serde_json::to_string_pretty(&response).unwrap(),
                ));
            }
        }
    }
}

/// Handle the "cancel" action: cancel a running task.
async fn handle_cancel(
    input: &Value,
    manager: &Arc<astrape_features::background_agent::BackgroundManager>,
) -> Result<ToolOutput, ToolError> {
    let task_id = get_required_string(input, "taskId")?;

    match manager.cancel_task(&task_id) {
        Some(task) => {
            let response = json!({
                "success": true,
                "action": "cancel",
                "task": task_to_json(&task),
                "was_already_terminal": matches!(
                    task.status,
                    BackgroundTaskStatus::Completed
                        | BackgroundTaskStatus::Error
                ),
            });
            Ok(ToolOutput::text(
                serde_json::to_string_pretty(&response).unwrap(),
            ))
        }
        None => {
            let response = json!({
                "success": false,
                "action": "cancel",
                "error": format!("Task not found: {task_id}"),
            });
            Ok(ToolOutput::text(
                serde_json::to_string_pretty(&response).unwrap(),
            ))
        }
    }
}

/// Handle the "list" action: return all active background tasks.
async fn handle_list(
    manager: &Arc<astrape_features::background_agent::BackgroundManager>,
) -> Result<ToolOutput, ToolError> {
    let tasks = manager.get_all_tasks();

    let tasks_json: Vec<Value> = tasks.iter().map(task_to_json).collect();

    let running_count = tasks
        .iter()
        .filter(|t| t.status == BackgroundTaskStatus::Running)
        .count();
    let queued_count = tasks
        .iter()
        .filter(|t| t.status == BackgroundTaskStatus::Queued)
        .count();
    let completed_count = tasks
        .iter()
        .filter(|t| {
            matches!(
                t.status,
                BackgroundTaskStatus::Completed
                    | BackgroundTaskStatus::Error
                    | BackgroundTaskStatus::Cancelled
            )
        })
        .count();

    let response = json!({
        "success": true,
        "action": "list",
        "tasks": tasks_json,
        "summary": {
            "total": tasks.len(),
            "running": running_count,
            "queued": queued_count,
            "completed": completed_count,
        },
    });
    Ok(ToolOutput::text(
        serde_json::to_string_pretty(&response).unwrap(),
    ))
}

/// Create the background_task tool handler.
async fn handle_background_task(input: ToolInput) -> Result<ToolOutput, ToolError> {
    let action = get_required_string(&input, "action")?;

    // Get or create the singleton BackgroundManager
    let config = BackgroundTaskConfig::default();
    let manager = get_background_manager(config);

    match action.as_str() {
        "launch" => handle_launch(&input, &manager).await,
        "output" => handle_output(&input, &manager).await,
        "cancel" => handle_cancel(&input, &manager).await,
        "list" => handle_list(&manager).await,
        _ => Err(ToolError::InvalidInput {
            message: format!(
                "invalid action: {action}. Valid actions are: launch, output, cancel, list"
            ),
        }),
    }
}

pub fn tool_definition() -> ToolDefinition {
    ToolDefinition::new(
        "background_task",
        "Manage background tasks for parallel execution. Actions: launch (start new task), output (get task output), cancel (stop task), list (show all tasks).",
        json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["launch", "output", "cancel", "list"],
                    "description": "The action to perform"
                },
                "description": {
                    "type": "string",
                    "description": "Task description (required for launch)"
                },
                "prompt": {
                    "type": "string",
                    "description": "Task prompt/instructions (required for launch)"
                },
                "agent": {
                    "type": "string",
                    "description": "Agent type to use (optional for launch, defaults to 'executor')"
                },
                "parent_session_id": {
                    "type": "string",
                    "description": "Parent session ID for tracking (optional for launch)"
                },
                "model": {
                    "type": "string",
                    "description": "Model to use for the task (optional for launch)"
                },
                "taskId": {
                    "type": "string",
                    "description": "Task ID (required for output and cancel)"
                },
                "block": {
                    "type": "boolean",
                    "description": "Whether to block until task completes (optional for output, default false)"
                },
                "timeout": {
                    "type": "integer",
                    "minimum": 0,
                    "description": "Timeout in milliseconds for blocking output (optional, default 30000)"
                }
            },
            "required": ["action"]
        }),
        Arc::new(|input: ToolInput| Box::pin(handle_background_task(input))),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use astrape_features::background_agent::reset_background_manager;

    #[tokio::test]
    async fn test_launch_action() {
        reset_background_manager();

        let input = json!({
            "action": "launch",
            "description": "Test task",
            "prompt": "Do something",
            "agent": "executor",
            "parent_session_id": "test-session"
        });

        let result = handle_background_task(input).await;
        assert!(result.is_ok());

        let output = result.unwrap();
        let text = match &output.content[0] {
            crate::types::ToolContent::Text { text } => text,
        };
        let response: Value = serde_json::from_str(text).unwrap();
        assert_eq!(response["success"], true);
        assert_eq!(response["action"], "launch");
        assert!(response["task"]["task_id"].is_string());

        reset_background_manager();
    }

    #[tokio::test]
    async fn test_list_action() {
        reset_background_manager();

        let input = json!({
            "action": "list"
        });

        let result = handle_background_task(input).await;
        assert!(result.is_ok());

        let output = result.unwrap();
        let text = match &output.content[0] {
            crate::types::ToolContent::Text { text } => text,
        };
        let response: Value = serde_json::from_str(text).unwrap();
        assert_eq!(response["success"], true);
        assert_eq!(response["action"], "list");
        assert!(response["tasks"].is_array());

        reset_background_manager();
    }

    #[tokio::test]
    async fn test_invalid_action() {
        let input = json!({
            "action": "invalid"
        });

        let result = handle_background_task(input).await;
        assert!(result.is_err());

        match result.unwrap_err() {
            ToolError::InvalidInput { message } => {
                assert!(message.contains("invalid action"));
            }
            _ => panic!("Expected InvalidInput error"),
        }
    }

    #[tokio::test]
    async fn test_missing_action() {
        let input = json!({});

        let result = handle_background_task(input).await;
        assert!(result.is_err());

        match result.unwrap_err() {
            ToolError::InvalidInput { message } => {
                assert!(message.contains("missing required field: action"));
            }
            _ => panic!("Expected InvalidInput error"),
        }
    }

    #[tokio::test]
    async fn test_output_task_not_found() {
        reset_background_manager();

        let input = json!({
            "action": "output",
            "taskId": "nonexistent-task"
        });

        let result = handle_background_task(input).await;
        assert!(result.is_ok());

        let output = result.unwrap();
        let text = match &output.content[0] {
            crate::types::ToolContent::Text { text } => text,
        };
        let response: Value = serde_json::from_str(text).unwrap();
        assert_eq!(response["success"], false);
        assert!(response["error"].as_str().unwrap().contains("not found"));

        reset_background_manager();
    }

    #[tokio::test]
    async fn test_cancel_task_not_found() {
        reset_background_manager();

        let input = json!({
            "action": "cancel",
            "taskId": "nonexistent-task"
        });

        let result = handle_background_task(input).await;
        assert!(result.is_ok());

        let output = result.unwrap();
        let text = match &output.content[0] {
            crate::types::ToolContent::Text { text } => text,
        };
        let response: Value = serde_json::from_str(text).unwrap();
        assert_eq!(response["success"], false);
        assert!(response["error"].as_str().unwrap().contains("not found"));

        reset_background_manager();
    }

    #[tokio::test]
    async fn test_launch_and_cancel() {
        reset_background_manager();

        // Launch a task
        let launch_input = json!({
            "action": "launch",
            "description": "Task to cancel",
            "prompt": "Do something"
        });

        let launch_result = handle_background_task(launch_input).await.unwrap();
        let launch_text = match &launch_result.content[0] {
            crate::types::ToolContent::Text { text } => text,
        };
        let launch_response: Value = serde_json::from_str(launch_text).unwrap();

        // Check if launch was successful before trying to cancel
        if launch_response["success"] == true {
            let task_id = launch_response["task"]["task_id"].as_str().unwrap();

            // Cancel the task
            let cancel_input = json!({
                "action": "cancel",
                "taskId": task_id
            });

            let cancel_result = handle_background_task(cancel_input).await.unwrap();
            let cancel_text = match &cancel_result.content[0] {
                crate::types::ToolContent::Text { text } => text,
            };
            let cancel_response: Value = serde_json::from_str(cancel_text).unwrap();
            assert_eq!(cancel_response["success"], true);
            assert_eq!(cancel_response["task"]["status"], "cancelled");
        } else {
            // Launch failed due to max tasks - this is expected in parallel test runs
            // Just verify we got a proper error response
            assert!(launch_response["error"].is_string());
        }

        reset_background_manager();
    }
}
