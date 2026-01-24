use async_trait::async_trait;

use crate::hook::{Hook, HookContext, HookResult};
use crate::types::{HookEvent, HookInput, HookOutput};

pub const HOOK_NAME: &str = "astrape-orchestrator";

/// Astrape Orchestrator Hook
///
/// This hook provides orchestration capabilities for the Astrape system.
/// Currently a stub implementation pending full feature development.
pub struct AstrapeOrchestratorHook;

#[async_trait]
impl Hook for AstrapeOrchestratorHook {
    fn name(&self) -> &str {
        HOOK_NAME
    }

    fn events(&self) -> &[HookEvent] {
        &[]
    }

    async fn execute(
        &self,
        _event: HookEvent,
        _input: &HookInput,
        _context: &HookContext,
    ) -> HookResult {
        Ok(HookOutput::pass())
    }

    fn priority(&self) -> i32 {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[tokio::test]
    async fn test_hook_returns_pass() {
        let hook = AstrapeOrchestratorHook;
        let input = HookInput {
            session_id: None,
            prompt: Some("test".to_string()),
            message: None,
            parts: None,
            tool_name: None,
            tool_input: None,
            tool_output: None,
            directory: None,
            stop_reason: None,
            user_requested: None,
            transcript_path: None,
            extra: HashMap::new(),
        };
        let context = HookContext::new(None, "/tmp".to_string());

        let result = hook
            .execute(HookEvent::UserPromptSubmit, &input, &context)
            .await
            .unwrap();

        assert!(result.should_continue);
        assert!(result.message.is_none());
    }

    #[test]
    fn test_hook_name() {
        let hook = AstrapeOrchestratorHook;
        assert_eq!(hook.name(), HOOK_NAME);
    }

    #[test]
    fn test_hook_events() {
        let hook = AstrapeOrchestratorHook;
        assert_eq!(hook.events(), &[]);
    }

    #[test]
    fn test_hook_priority() {
        let hook = AstrapeOrchestratorHook;
        assert_eq!(hook.priority(), 0);
    }
}
