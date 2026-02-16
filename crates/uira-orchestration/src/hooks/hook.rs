use async_trait::async_trait;
use std::collections::HashMap;

use super::types::{HookEvent, HookInput, HookOutput};

/// Context passed to hooks during execution
#[derive(Debug, Clone)]
pub struct HookContext {
    /// Session identifier
    pub session_id: Option<String>,
    /// Working directory
    pub directory: String,
    /// Additional context data
    pub data: HashMap<String, serde_json::Value>,
}

impl HookContext {
    pub fn new(session_id: Option<String>, directory: String) -> Self {
        Self {
            session_id,
            directory,
            data: HashMap::new(),
        }
    }

    pub fn with_data(mut self, key: impl Into<String>, value: serde_json::Value) -> Self {
        self.data.insert(key.into(), value);
        self
    }
}

/// Result type for hook execution
pub type HookResult = anyhow::Result<HookOutput>;

/// Core trait for all hooks in the Uira system
///
/// Hooks are async functions that process events from Claude Code and can:
/// - Inject messages into the conversation
/// - Block operations (return continue=false)
/// - Modify tool inputs
/// - Track state across sessions
#[async_trait]
pub trait Hook: Send + Sync {
    /// Unique identifier for this hook
    fn name(&self) -> &str;

    /// Events this hook should be triggered on
    fn events(&self) -> &[HookEvent];

    /// Execute the hook logic
    ///
    /// # Arguments
    /// * `event` - The event that triggered this hook
    /// * `input` - Input data from Claude Code
    /// * `context` - Execution context with session info
    ///
    /// # Returns
    /// * `Ok(HookOutput)` - Hook executed successfully
    /// * `Err(e)` - Hook failed (will be logged but won't block execution)
    async fn execute(
        &self,
        event: HookEvent,
        input: &HookInput,
        context: &HookContext,
    ) -> HookResult;

    /// Priority for hook execution (higher = earlier)
    /// Default: 0
    fn priority(&self) -> i32 {
        0
    }

    /// Whether this hook is enabled
    /// Default: true
    fn is_enabled(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestHook;

    #[async_trait]
    impl Hook for TestHook {
        fn name(&self) -> &str {
            "test-hook"
        }

        fn events(&self) -> &[HookEvent] {
            &[HookEvent::UserPromptSubmit]
        }

        async fn execute(
            &self,
            _event: HookEvent,
            _input: &HookInput,
            _context: &HookContext,
        ) -> HookResult {
            Ok(HookOutput::pass())
        }
    }

    #[tokio::test]
    async fn test_hook_execution() {
        let hook = TestHook;
        let input = HookInput {
            session_id: Some("test-session".to_string()),
            prompt: Some("test prompt".to_string()),
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
        let context = HookContext::new(Some("test-session".to_string()), "/tmp".to_string());

        let result = hook
            .execute(HookEvent::UserPromptSubmit, &input, &context)
            .await;

        assert!(result.is_ok());
        let output = result.unwrap();
        assert!(output.should_continue);
    }

    #[test]
    fn test_hook_context_with_data() {
        let context = HookContext::new(Some("session".to_string()), "/tmp".to_string())
            .with_data("key", serde_json::json!("value"));

        assert_eq!(context.data.get("key"), Some(&serde_json::json!("value")));
    }
}
