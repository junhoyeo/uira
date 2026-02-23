//! Delegation Enforcer Hook
//!
//! Auto-injects the model parameter on Task tool calls when missing.
//! This ensures all agent delegations have explicit model routing.

use async_trait::async_trait;
use serde_json::Value;

use super::super::hook::{Hook, HookContext, HookResult};
use super::super::types::{HookEvent, HookInput, HookOutput};

pub const HOOK_NAME: &str = "delegation-enforcer";

pub struct DelegationEnforcerHook;

impl DelegationEnforcerHook {
    pub fn new() -> Self {
        Self
    }

    /// Extract agent name from subagent_type, stripping "uira:" prefix if present
    fn extract_agent_name(subagent_type: &str) -> String {
        subagent_type
            .strip_prefix("uira:")
            .unwrap_or(subagent_type)
            .to_string()
    }

    /// Map agent tier to model name
    fn tier_to_model(agent_name: &str) -> Option<&'static str> {
        // Extract tier from agent name
        if agent_name.ends_with("-low") {
            Some("haiku")
        } else if agent_name.ends_with("-medium") {
            Some("sonnet")
        } else if agent_name.ends_with("-high") {
            Some("opus")
        } else {
            // For base agents without explicit tier suffix, use defaults
            match agent_name {
                "architect" | "planner" | "critic" | "qa-tester" | "security-reviewer"
                | "code-reviewer" => Some("opus"),
                "executor" | "librarian" | "designer" | "analyst" | "vision" | "scientist"
                | "build-fixer" | "tdd-guide" => Some("sonnet"),
                "explore" | "writer" => Some("haiku"),
                _ => None,
            }
        }
    }
}

impl Default for DelegationEnforcerHook {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Hook for DelegationEnforcerHook {
    fn name(&self) -> &str {
        HOOK_NAME
    }

    fn events(&self) -> &[HookEvent] {
        &[HookEvent::PreToolUse]
    }

    async fn execute(
        &self,
        _event: HookEvent,
        input: &HookInput,
        _context: &HookContext,
    ) -> HookResult {
        // Check if this is a Task or Agent tool call
        let Some(tool_name) = input.tool_name.as_deref() else {
            return Ok(HookOutput::pass());
        };

        if tool_name != "Task" && tool_name != "Agent" {
            return Ok(HookOutput::pass());
        }

        // Get tool input
        let Some(tool_input) = &input.tool_input else {
            return Ok(HookOutput::pass());
        };

        // Check if it has subagent_type
        let Some(subagent_type) = tool_input.get("subagent_type").and_then(|v| v.as_str()) else {
            return Ok(HookOutput::pass());
        };

        // If model is already provided, do nothing
        if tool_input.get("model").is_some() {
            return Ok(HookOutput::pass());
        }

        // Extract agent name and determine default model
        let agent_name = Self::extract_agent_name(subagent_type);
        let Some(default_model) = Self::tier_to_model(&agent_name) else {
            // Unknown agent, pass through
            return Ok(HookOutput::pass());
        };

        // Clone and inject model parameter
        let mut modified_input = tool_input.clone();
        if let Some(obj) = modified_input.as_object_mut() {
            obj.insert(
                "model".to_string(),
                Value::String(default_model.to_string()),
            );
        }

        Ok(HookOutput {
            should_continue: true,
            message: None,
            reason: None,
            modified_input: Some(modified_input),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::collections::HashMap;

    #[test]
    fn test_extract_agent_name() {
        assert_eq!(
            DelegationEnforcerHook::extract_agent_name("uira:architect"),
            "architect"
        );
        assert_eq!(
            DelegationEnforcerHook::extract_agent_name("executor"),
            "executor"
        );
    }

    #[test]
    fn test_tier_to_model() {
        // Tiered variants
        assert_eq!(
            DelegationEnforcerHook::tier_to_model("architect-low"),
            Some("haiku")
        );
        assert_eq!(
            DelegationEnforcerHook::tier_to_model("executor-medium"),
            Some("sonnet")
        );
        assert_eq!(
            DelegationEnforcerHook::tier_to_model("designer-high"),
            Some("opus")
        );

        // Base agents with default tiers
        assert_eq!(
            DelegationEnforcerHook::tier_to_model("architect"),
            Some("opus")
        );
        assert_eq!(
            DelegationEnforcerHook::tier_to_model("executor"),
            Some("sonnet")
        );
        assert_eq!(
            DelegationEnforcerHook::tier_to_model("explore"),
            Some("haiku")
        );
        assert_eq!(
            DelegationEnforcerHook::tier_to_model("planner"),
            Some("opus")
        );
        assert_eq!(
            DelegationEnforcerHook::tier_to_model("writer"),
            Some("haiku")
        );

        // Unknown agent
        assert_eq!(DelegationEnforcerHook::tier_to_model("unknown-agent"), None);
    }

    #[tokio::test]
    async fn test_hook_injects_model_when_missing() {
        let hook = DelegationEnforcerHook::new();
        let context = HookContext::new(Some("test-session".to_string()), "/tmp".to_string(), None);

        let input = HookInput {
            session_id: Some("test-session".to_string()),
            prompt: None,
            message: None,
            parts: None,
            tool_name: Some("Task".to_string()),
            tool_input: Some(json!({
                "subagent_type": "uira:architect",
                "prompt": "Analyze this code"
            })),
            tool_output: None,
            directory: None,
            stop_reason: None,
            user_requested: None,
            transcript_path: None,
            extra: HashMap::new(),
        };

        let result = hook
            .execute(HookEvent::PreToolUse, &input, &context)
            .await
            .unwrap();

        assert!(result.should_continue);
        assert!(result.modified_input.is_some());

        let modified = result.modified_input.unwrap();
        assert_eq!(modified.get("model").and_then(|v| v.as_str()), Some("opus"));
        assert_eq!(
            modified.get("subagent_type").and_then(|v| v.as_str()),
            Some("uira:architect")
        );
    }

    #[tokio::test]
    async fn test_hook_preserves_existing_model() {
        let hook = DelegationEnforcerHook::new();
        let context = HookContext::new(Some("test-session".to_string()), "/tmp".to_string(), None);

        let input = HookInput {
            session_id: Some("test-session".to_string()),
            prompt: None,
            message: None,
            parts: None,
            tool_name: Some("Task".to_string()),
            tool_input: Some(json!({
                "subagent_type": "uira:executor",
                "model": "haiku",
                "prompt": "Do something"
            })),
            tool_output: None,
            directory: None,
            stop_reason: None,
            user_requested: None,
            transcript_path: None,
            extra: HashMap::new(),
        };

        let result = hook
            .execute(HookEvent::PreToolUse, &input, &context)
            .await
            .unwrap();

        assert!(result.should_continue);
        // No modification when model already exists
        assert!(result.modified_input.is_none());
    }

    #[tokio::test]
    async fn test_hook_ignores_non_task_tools() {
        let hook = DelegationEnforcerHook::new();
        let context = HookContext::new(Some("test-session".to_string()), "/tmp".to_string(), None);

        let input = HookInput {
            session_id: Some("test-session".to_string()),
            prompt: None,
            message: None,
            parts: None,
            tool_name: Some("Grep".to_string()),
            tool_input: Some(json!({
                "pattern": "test"
            })),
            tool_output: None,
            directory: None,
            stop_reason: None,
            user_requested: None,
            transcript_path: None,
            extra: HashMap::new(),
        };

        let result = hook
            .execute(HookEvent::PreToolUse, &input, &context)
            .await
            .unwrap();

        assert!(result.should_continue);
        assert!(result.modified_input.is_none());
    }

    #[tokio::test]
    async fn test_hook_handles_tiered_variants() {
        let hook = DelegationEnforcerHook::new();
        let context = HookContext::new(Some("test-session".to_string()), "/tmp".to_string(), None);

        let test_cases = vec![
            ("executor-low", "haiku"),
            ("architect-medium", "sonnet"),
            ("designer-high", "opus"),
        ];

        for (agent, expected_model) in test_cases {
            let input = HookInput {
                session_id: Some("test-session".to_string()),
                prompt: None,
                message: None,
                parts: None,
                tool_name: Some("Task".to_string()),
                tool_input: Some(json!({
                    "subagent_type": agent,
                    "prompt": "Test"
                })),
                tool_output: None,
                directory: None,
                stop_reason: None,
                user_requested: None,
                transcript_path: None,
                extra: HashMap::new(),
            };

            let result = hook
                .execute(HookEvent::PreToolUse, &input, &context)
                .await
                .unwrap();

            assert!(result.should_continue);
            assert!(result.modified_input.is_some());

            let modified = result.modified_input.unwrap();
            assert_eq!(
                modified.get("model").and_then(|v| v.as_str()),
                Some(expected_model),
                "Failed for agent: {}",
                agent
            );
        }
    }
}
