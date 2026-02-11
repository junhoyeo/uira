//! Delegate Task Tool Handler
//!
//! The MOST CRITICAL tool - connects the agent system to actual delegation.
//! Handles agent lookup, model routing, and task delegation.

use crate::types::{ToolDefinition, ToolError, ToolInput, ToolOutput};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::Arc;
use uira_orchestration::get_agent_definitions;
use uira_orchestration::model_routing::{
    route_task, ModelTier, RoutingConfigOverrides, RoutingContext,
};
use uira_orchestration::ModelType;
use uuid::Uuid;

/// Parameters for delegate_task tool
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DelegateTaskParams {
    /// Agent type to delegate to (e.g., "uira:executor")
    agent: String,
    /// Task description/prompt
    prompt: String,
    /// Optional model override (haiku, sonnet, opus)
    #[serde(default)]
    model: Option<String>,
    /// Whether to run in background
    #[serde(default)]
    run_in_background: bool,
}

/// Response from delegate_task
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DelegateTaskResponse {
    /// Whether delegation was successful
    success: bool,
    /// Agent type that was used
    agent_type: String,
    /// Model tier used for this task
    model_used: String,
    /// Model type (haiku/sonnet/opus)
    model_type: String,
    /// Task ID for background tasks
    #[serde(skip_serializing_if = "Option::is_none")]
    task_id: Option<String>,
    /// Session ID for tracking
    #[serde(skip_serializing_if = "Option::is_none")]
    session_id: Option<String>,
    /// Status message
    status: String,
    /// Routing reasons
    #[serde(skip_serializing_if = "Vec::is_empty")]
    routing_reasons: Vec<String>,
    /// Agent description
    #[serde(skip_serializing_if = "Option::is_none")]
    agent_description: Option<String>,
}

/// Parse model string to ModelType
fn parse_model_type(model: &str) -> Option<ModelType> {
    match model.to_lowercase().as_str() {
        "haiku" | "claude-haiku" | "low" => Some(ModelType::Haiku),
        "sonnet" | "claude-sonnet" | "medium" => Some(ModelType::Sonnet),
        "opus" | "claude-opus" | "high" => Some(ModelType::Opus),
        _ => None,
    }
}

/// Convert ModelTier to ModelType
fn tier_to_model_type(tier: ModelTier) -> ModelType {
    match tier {
        ModelTier::Low => ModelType::Haiku,
        ModelTier::Medium => ModelType::Sonnet,
        ModelTier::High => ModelType::Opus,
    }
}

/// Extract base agent name from prefixed agent type
/// e.g., "uira:executor" -> "executor"
fn extract_agent_name(agent_type: &str) -> &str {
    agent_type.split(':').next_back().unwrap_or(agent_type)
}

/// Handle delegate_task tool invocation
async fn handle_delegate_task(input: ToolInput) -> Result<ToolOutput, ToolError> {
    // Parse input parameters
    let params: DelegateTaskParams =
        serde_json::from_value(input).map_err(|e| ToolError::InvalidInput {
            message: format!("Failed to parse delegate_task parameters: {}", e),
        })?;

    // Extract base agent name (remove prefix if present)
    let agent_name = extract_agent_name(&params.agent);

    // Look up agent definition
    let agent_definitions = get_agent_definitions(None);
    let agent_config = agent_definitions.get(agent_name);

    let (agent_description, agent_default_model) = match agent_config {
        Some(config) => (Some(config.description.clone()), config.default_model),
        None => (None, None),
    };

    // Determine model to use
    // Priority: explicit model param > routing decision > agent default > Sonnet
    let (final_model, final_tier, routing_reasons) = if let Some(model_str) = &params.model {
        // Explicit model override
        if let Some(model_type) = parse_model_type(model_str) {
            let tier = match model_type {
                ModelType::Haiku => ModelTier::Low,
                ModelType::Sonnet => ModelTier::Medium,
                ModelType::Opus => ModelTier::High,
                ModelType::Inherit => ModelTier::Medium,
            };
            (
                model_type,
                tier,
                vec![format!("Explicit model override: {}", model_str)],
            )
        } else {
            return Err(ToolError::InvalidInput {
                message: format!("Invalid model: {}. Use haiku, sonnet, or opus.", model_str),
            });
        }
    } else {
        // Use model routing to determine best model
        let routing_context = RoutingContext {
            task_prompt: params.prompt.clone(),
            agent_type: Some(agent_name.to_string()),
            explicit_model: agent_default_model,
            ..Default::default()
        };

        let decision = route_task(routing_context, RoutingConfigOverrides::default());

        (
            tier_to_model_type(decision.tier),
            decision.tier,
            decision.reasons,
        )
    };

    // Generate task/session IDs
    let session_id = Uuid::new_v4().to_string();
    let task_id = if params.run_in_background {
        Some(Uuid::new_v4().to_string())
    } else {
        None
    };

    // Build response
    let response = DelegateTaskResponse {
        success: true,
        agent_type: params.agent.clone(),
        model_used: final_tier.as_str().to_string(),
        model_type: final_model.as_str().to_string(),
        task_id: task_id.clone(),
        session_id: Some(session_id.clone()),
        status: if params.run_in_background {
            format!(
                "Task delegated to {} in background. Task ID: {}",
                agent_name,
                task_id.as_ref().unwrap()
            )
        } else {
            format!(
                "Task delegated to {} ({}). Session: {}",
                agent_name,
                final_model.as_str(),
                session_id
            )
        },
        routing_reasons,
        agent_description,
    };

    let json_response =
        serde_json::to_string_pretty(&response).map_err(|e| ToolError::ExecutionFailed {
            message: format!("Failed to serialize response: {}", e),
        })?;

    Ok(ToolOutput::text(json_response))
}

pub fn tool_definition() -> ToolDefinition {
    ToolDefinition::new(
        "delegate_task",
        "Delegate a task to a specialized agent. Supports model routing and background execution.",
        json!({
            "type": "object",
            "properties": {
                "agent": {
                    "type": "string",
                    "description": "Agent type to delegate to (e.g., 'uira:executor', 'architect', 'explore')"
                },
                "prompt": {
                    "type": "string",
                    "description": "Task description for the agent"
                },
                "model": {
                    "type": "string",
                    "enum": ["haiku", "sonnet", "opus"],
                    "description": "Optional model override. If not specified, model routing determines the best model."
                },
                "runInBackground": {
                    "type": "boolean",
                    "default": false,
                    "description": "Whether to run the task in the background"
                }
            },
            "required": ["agent", "prompt"]
        }),
        Arc::new(|input| Box::pin(handle_delegate_task(input))),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_agent_name() {
        assert_eq!(extract_agent_name("uira:executor"), "executor");
        assert_eq!(extract_agent_name("executor"), "executor");
        assert_eq!(extract_agent_name("uira:architect"), "architect");
    }

    #[test]
    fn test_parse_model_type() {
        assert_eq!(parse_model_type("haiku"), Some(ModelType::Haiku));
        assert_eq!(parse_model_type("SONNET"), Some(ModelType::Sonnet));
        assert_eq!(parse_model_type("opus"), Some(ModelType::Opus));
        assert_eq!(parse_model_type("low"), Some(ModelType::Haiku));
        assert_eq!(parse_model_type("medium"), Some(ModelType::Sonnet));
        assert_eq!(parse_model_type("high"), Some(ModelType::Opus));
        assert_eq!(parse_model_type("invalid"), None);
    }

    #[tokio::test]
    async fn test_delegate_task_basic() {
        let input = json!({
            "agent": "uira:executor",
            "prompt": "Add error handling to auth module"
        });

        let result = handle_delegate_task(input).await;
        assert!(result.is_ok());

        let output = result.unwrap();
        let text = match &output.content[0] {
            crate::types::ToolContent::Text { text } => text,
        };

        let response: DelegateTaskResponse = serde_json::from_str(text).unwrap();
        assert!(response.success);
        assert_eq!(response.agent_type, "uira:executor");
        assert!(response.session_id.is_some());
    }

    #[tokio::test]
    async fn test_delegate_task_with_model_override() {
        let input = json!({
            "agent": "explore",
            "prompt": "Find auth files",
            "model": "haiku"
        });

        let result = handle_delegate_task(input).await;
        assert!(result.is_ok());

        let output = result.unwrap();
        let text = match &output.content[0] {
            crate::types::ToolContent::Text { text } => text,
        };

        let response: DelegateTaskResponse = serde_json::from_str(text).unwrap();
        assert_eq!(response.model_type, "haiku");
        assert!(response
            .routing_reasons
            .iter()
            .any(|r| r.contains("Explicit")));
    }

    #[tokio::test]
    async fn test_delegate_task_background() {
        let input = json!({
            "agent": "executor",
            "prompt": "Long running task",
            "runInBackground": true
        });

        let result = handle_delegate_task(input).await;
        assert!(result.is_ok());

        let output = result.unwrap();
        let text = match &output.content[0] {
            crate::types::ToolContent::Text { text } => text,
        };

        let response: DelegateTaskResponse = serde_json::from_str(text).unwrap();
        assert!(response.task_id.is_some());
        assert!(response.status.contains("background"));
    }

    #[tokio::test]
    async fn test_delegate_task_invalid_model() {
        let input = json!({
            "agent": "executor",
            "prompt": "Task",
            "model": "gpt-4"
        });

        let result = handle_delegate_task(input).await;
        assert!(matches!(result, Err(ToolError::InvalidInput { .. })));
    }

    #[tokio::test]
    async fn test_delegate_task_unknown_agent_still_works() {
        // Unknown agents should still work, just without agent-specific metadata
        let input = json!({
            "agent": "unknown-agent",
            "prompt": "Do something"
        });

        let result = handle_delegate_task(input).await;
        assert!(result.is_ok());

        let output = result.unwrap();
        let text = match &output.content[0] {
            crate::types::ToolContent::Text { text } => text,
        };

        let response: DelegateTaskResponse = serde_json::from_str(text).unwrap();
        assert!(response.success);
        assert!(response.agent_description.is_none());
    }
}
