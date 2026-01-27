use async_trait::async_trait;

use super::orchestrator_constants::{
    is_allowed_path, is_write_edit_tool, orchestrator_delegation_required, DIRECT_WORK_REMINDER,
    VERIFICATION_REMINDER,
};
use crate::hook::{Hook, HookContext, HookResult};
use crate::types::{HookEvent, HookInput, HookOutput};

pub const HOOK_NAME: &str = "uira-orchestrator";

pub struct UiraOrchestratorHook;

impl UiraOrchestratorHook {
    fn extract_file_path(tool_input: &serde_json::Value) -> Option<String> {
        tool_input
            .get("filePath")
            .or_else(|| tool_input.get("path"))
            .or_else(|| tool_input.get("file"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
    }

    fn process_pre_tool(&self, input: &HookInput, _context: &HookContext) -> HookOutput {
        let tool_name = match &input.tool_name {
            Some(name) => name,
            None => return HookOutput::pass(),
        };

        if !is_write_edit_tool(tool_name) {
            return HookOutput::pass();
        }

        let tool_input = match &input.tool_input {
            Some(input) => input,
            None => return HookOutput::pass(),
        };

        let file_path = match Self::extract_file_path(tool_input) {
            Some(path) => path,
            None => return HookOutput::pass(),
        };

        if is_allowed_path(&file_path) {
            return HookOutput::pass();
        }

        let warning = orchestrator_delegation_required(&file_path);
        HookOutput::continue_with_message(warning)
    }

    fn process_post_tool(&self, input: &HookInput, _context: &HookContext) -> HookOutput {
        let tool_name = match &input.tool_name {
            Some(name) => name,
            None => return HookOutput::pass(),
        };

        if is_write_edit_tool(tool_name) {
            let tool_input = match &input.tool_input {
                Some(input) => input,
                None => return HookOutput::pass(),
            };

            if let Some(file_path) = Self::extract_file_path(tool_input) {
                if !is_allowed_path(&file_path) {
                    return HookOutput::continue_with_message(DIRECT_WORK_REMINDER);
                }
            }
        }

        if tool_name == "delegate_task" || tool_name.ends_with("__delegate_task") {
            let reminder = format!(
                "<system-reminder>\n{}\n</system-reminder>",
                VERIFICATION_REMINDER
            );

            return HookOutput::continue_with_message(reminder);
        }

        HookOutput::pass()
    }
}

#[async_trait]
impl Hook for UiraOrchestratorHook {
    fn name(&self) -> &str {
        HOOK_NAME
    }

    fn events(&self) -> &[HookEvent] {
        &[HookEvent::PreToolUse, HookEvent::PostToolUse]
    }

    async fn execute(
        &self,
        event: HookEvent,
        input: &HookInput,
        context: &HookContext,
    ) -> HookResult {
        match event {
            HookEvent::PreToolUse => Ok(self.process_pre_tool(input, context)),
            HookEvent::PostToolUse => Ok(self.process_post_tool(input, context)),
            _ => Ok(HookOutput::pass()),
        }
    }

    fn priority(&self) -> i32 {
        100
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::collections::HashMap;

    fn create_test_context() -> HookContext {
        HookContext::new(None, "/tmp".to_string())
    }

    fn create_test_input(tool_name: &str, tool_input: serde_json::Value) -> HookInput {
        HookInput {
            session_id: None,
            prompt: None,
            message: None,
            parts: None,
            tool_name: Some(tool_name.to_string()),
            tool_input: Some(tool_input),
            tool_output: None,
            directory: None,
            stop_reason: None,
            user_requested: None,
            transcript_path: None,
            extra: HashMap::new(),
        }
    }

    #[tokio::test]
    async fn test_pre_tool_allows_uira_paths() {
        let hook = UiraOrchestratorHook;
        let context = create_test_context();
        let input = create_test_input("Write", json!({"filePath": ".uira/plans/test.md"}));

        let result = hook
            .execute(HookEvent::PreToolUse, &input, &context)
            .await
            .unwrap();

        assert!(result.should_continue);
        assert!(result.message.is_none());
    }

    #[tokio::test]
    async fn test_pre_tool_warns_source_files() {
        let hook = UiraOrchestratorHook;
        let context = create_test_context();
        let input = create_test_input("Write", json!({"filePath": "src/main.rs"}));

        let result = hook
            .execute(HookEvent::PreToolUse, &input, &context)
            .await
            .unwrap();

        assert!(result.should_continue);
        assert!(result.message.is_some());
        assert!(result.message.unwrap().contains("DELEGATION REQUIRED"));
    }

    #[tokio::test]
    async fn test_pre_tool_ignores_non_write_tools() {
        let hook = UiraOrchestratorHook;
        let context = create_test_context();
        let input = create_test_input("Read", json!({"filePath": "src/main.rs"}));

        let result = hook
            .execute(HookEvent::PreToolUse, &input, &context)
            .await
            .unwrap();

        assert!(result.should_continue);
        assert!(result.message.is_none());
    }

    #[tokio::test]
    async fn test_post_tool_adds_reminder_after_source_edit() {
        let hook = UiraOrchestratorHook;
        let context = create_test_context();
        let mut input = create_test_input("Edit", json!({"filePath": "src/lib.rs"}));
        input.tool_output = Some(json!("Edit successful"));

        let result = hook
            .execute(HookEvent::PostToolUse, &input, &context)
            .await
            .unwrap();

        assert!(result.should_continue);
        assert!(result.message.is_some());
        assert!(result.message.unwrap().contains("DELEGATION REQUIRED"));
    }

    #[tokio::test]
    async fn test_post_tool_adds_verification_after_delegate_task() {
        let hook = UiraOrchestratorHook;
        let context = create_test_context();
        let mut input = create_test_input(
            "delegate_task",
            json!({"agent": "executor", "prompt": "do something"}),
        );
        input.tool_output = Some(json!({"success": true, "result": "Task completed"}));

        let result = hook
            .execute(HookEvent::PostToolUse, &input, &context)
            .await
            .unwrap();

        assert!(result.should_continue);
        assert!(result.message.is_some());
        assert!(result.message.unwrap().contains("MANDATORY VERIFICATION"));
    }

    #[tokio::test]
    async fn test_post_tool_adds_verification_for_mcp_delegate_task() {
        let hook = UiraOrchestratorHook;
        let context = create_test_context();
        let mut input = create_test_input(
            "mcp__uira__delegate_task",
            json!({"agent": "executor", "prompt": "do something"}),
        );
        input.tool_output = Some(json!({"success": true}));

        let result = hook
            .execute(HookEvent::PostToolUse, &input, &context)
            .await
            .unwrap();

        assert!(result.should_continue);
        assert!(result.message.is_some());
        assert!(result.message.unwrap().contains("MANDATORY VERIFICATION"));
    }

    #[test]
    fn test_hook_name() {
        let hook = UiraOrchestratorHook;
        assert_eq!(hook.name(), HOOK_NAME);
    }

    #[test]
    fn test_hook_events() {
        let hook = UiraOrchestratorHook;
        assert_eq!(
            hook.events(),
            &[HookEvent::PreToolUse, HookEvent::PostToolUse]
        );
    }

    #[test]
    fn test_hook_priority() {
        let hook = UiraOrchestratorHook;
        assert_eq!(hook.priority(), 100);
    }
}
