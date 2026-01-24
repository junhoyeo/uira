use astrape_hooks::hook::{Hook, HookContext, HookResult};
use astrape_hooks::hooks::keyword_detector::KeywordDetectorHook;
use astrape_hooks::registry::HookRegistry;
use astrape_hooks::types::{HookEvent, HookInput, HookOutput};
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;

/// Mock hook for testing priority ordering and message combining
struct MockHook {
    name: String,
    priority: i32,
    message: String,
}

impl MockHook {
    fn new(name: impl Into<String>, priority: i32, message: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            priority,
            message: message.into(),
        }
    }
}

#[async_trait]
impl Hook for MockHook {
    fn name(&self) -> &str {
        &self.name
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
        Ok(HookOutput::continue_with_message(&self.message))
    }

    fn priority(&self) -> i32 {
        self.priority
    }
}

/// Mock blocking hook for testing execution stop behavior
struct BlockingHook {
    priority: i32,
}

impl BlockingHook {
    fn new(priority: i32) -> Self {
        Self { priority }
    }
}

#[async_trait]
impl Hook for BlockingHook {
    fn name(&self) -> &str {
        "blocking-hook"
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
        Ok(HookOutput::block_with_reason("Execution blocked by test"))
    }

    fn priority(&self) -> i32 {
        self.priority
    }
}

/// Helper to create test HookInput
fn create_test_input(prompt: impl Into<String>) -> HookInput {
    HookInput {
        session_id: None,
        prompt: Some(prompt.into()),
        message: None,
        parts: None,
        tool_name: None,
        tool_input: None,
        tool_output: None,
        directory: None,
        stop_reason: None,
        user_requested: None,
        extra: HashMap::new(),
    }
}

#[tokio::test]
async fn test_multi_hook_execution_with_priority_ordering() {
    let mut registry = HookRegistry::new();

    // Register hooks with different priorities
    registry.register(Arc::new(MockHook::new("high", 200, "HIGH PRIORITY")));
    registry.register(Arc::new(MockHook::new("medium", 100, "MEDIUM PRIORITY")));
    registry.register(Arc::new(MockHook::new("low", 50, "LOW PRIORITY")));

    let input = create_test_input("test prompt");
    let context = HookContext::new(None, "/tmp".to_string());

    let result = registry
        .execute_hooks(HookEvent::UserPromptSubmit, &input, &context)
        .await
        .unwrap();

    // Should continue
    assert!(result.should_continue);

    // Messages should be combined in priority order (highest first)
    let message = result.message.unwrap();
    assert!(message.contains("HIGH PRIORITY"));
    assert!(message.contains("MEDIUM PRIORITY"));
    assert!(message.contains("LOW PRIORITY"));

    // Verify order: HIGH should appear before MEDIUM, MEDIUM before LOW
    let high_pos = message.find("HIGH PRIORITY").unwrap();
    let medium_pos = message.find("MEDIUM PRIORITY").unwrap();
    let low_pos = message.find("LOW PRIORITY").unwrap();

    assert!(high_pos < medium_pos);
    assert!(medium_pos < low_pos);
}

#[tokio::test]
async fn test_blocking_hook_stops_execution() {
    let mut registry = HookRegistry::new();

    // Register blocking hook with high priority
    registry.register(Arc::new(BlockingHook::new(200)));

    // Register normal hook with lower priority (should NOT execute)
    registry.register(Arc::new(MockHook::new(
        "should-not-run",
        100,
        "THIS SHOULD NOT APPEAR",
    )));

    let input = create_test_input("test prompt");
    let context = HookContext::new(None, "/tmp".to_string());

    let result = registry
        .execute_hooks(HookEvent::UserPromptSubmit, &input, &context)
        .await
        .unwrap();

    // Should NOT continue
    assert!(!result.should_continue);

    // Should have blocking reason
    assert_eq!(result.reason, Some("Execution blocked by test".to_string()));

    // Should NOT have message from lower priority hook
    assert!(result.message.is_none());
}

#[tokio::test]
async fn test_blocking_hook_with_lower_priority_allows_higher_priority() {
    let mut registry = HookRegistry::new();

    // Register normal hook with HIGH priority (should execute)
    registry.register(Arc::new(MockHook::new(
        "high-priority",
        200,
        "HIGH PRIORITY MESSAGE",
    )));

    // Register blocking hook with LOWER priority (should block after high runs)
    registry.register(Arc::new(BlockingHook::new(100)));

    // Register another normal hook with even lower priority (should NOT execute)
    registry.register(Arc::new(MockHook::new(
        "should-not-run",
        50,
        "THIS SHOULD NOT APPEAR",
    )));

    let input = create_test_input("test prompt");
    let context = HookContext::new(None, "/tmp".to_string());

    let result = registry
        .execute_hooks(HookEvent::UserPromptSubmit, &input, &context)
        .await
        .unwrap();

    // Should NOT continue (blocked)
    assert!(!result.should_continue);

    // Should have blocking reason
    assert_eq!(result.reason, Some("Execution blocked by test".to_string()));

    // Should NOT have messages (blocking hook returns immediately)
    // Note: Current implementation returns immediately on block, so high priority message is lost
    // This matches the TypeScript behavior where blocking stops all execution
    assert!(result.message.is_none());
}

#[tokio::test]
async fn test_keyword_detector_integration_ultrawork() {
    let mut registry = HookRegistry::new();
    registry.register(Arc::new(KeywordDetectorHook::new()));

    let input = create_test_input("ultrawork: implement this feature");
    let context = HookContext::new(None, "/tmp".to_string());

    let result = registry
        .execute_hooks(HookEvent::UserPromptSubmit, &input, &context)
        .await
        .unwrap();

    assert!(result.should_continue);
    assert!(result.message.is_some());

    let message = result.message.unwrap();
    assert!(message.contains("ULTRAWORK MODE ACTIVATED"));
    assert!(message.contains("maximum parallel agent execution"));
}

#[tokio::test]
async fn test_keyword_detector_integration_ralph() {
    let mut registry = HookRegistry::new();
    registry.register(Arc::new(KeywordDetectorHook::new()));

    let input = create_test_input("ralph: complete this task until done");
    let context = HookContext::new(None, "/tmp".to_string());

    let result = registry
        .execute_hooks(HookEvent::UserPromptSubmit, &input, &context)
        .await
        .unwrap();

    assert!(result.should_continue);
    assert!(result.message.is_some());

    let message = result.message.unwrap();
    assert!(message.contains("RALPH MODE ACTIVATED"));
    assert!(message.contains("self-referential loop"));
}

#[tokio::test]
async fn test_keyword_detector_integration_search() {
    let mut registry = HookRegistry::new();
    registry.register(Arc::new(KeywordDetectorHook::new()));

    let input = create_test_input("search for the implementation of this function");
    let context = HookContext::new(None, "/tmp".to_string());

    let result = registry
        .execute_hooks(HookEvent::UserPromptSubmit, &input, &context)
        .await
        .unwrap();

    assert!(result.should_continue);
    assert!(result.message.is_some());

    let message = result.message.unwrap();
    assert!(message.contains("SEARCH MODE ACTIVATED"));
}

#[tokio::test]
async fn test_keyword_detector_integration_analyze() {
    let mut registry = HookRegistry::new();
    registry.register(Arc::new(KeywordDetectorHook::new()));

    let input = create_test_input("analyze this codebase thoroughly");
    let context = HookContext::new(None, "/tmp".to_string());

    let result = registry
        .execute_hooks(HookEvent::UserPromptSubmit, &input, &context)
        .await
        .unwrap();

    assert!(result.should_continue);
    assert!(result.message.is_some());

    let message = result.message.unwrap();
    assert!(message.contains("ANALYZE MODE ACTIVATED"));
}

#[tokio::test]
async fn test_keyword_detector_priority_ralph_over_search() {
    let mut registry = HookRegistry::new();
    registry.register(Arc::new(KeywordDetectorHook::new()));

    // Prompt contains both "ralph" and "search" - ralph should win
    let input = create_test_input("ralph: search for this and complete it");
    let context = HookContext::new(None, "/tmp".to_string());

    let result = registry
        .execute_hooks(HookEvent::UserPromptSubmit, &input, &context)
        .await
        .unwrap();

    assert!(result.should_continue);
    assert!(result.message.is_some());

    let message = result.message.unwrap();
    // Should activate RALPH (higher priority), not SEARCH
    assert!(message.contains("RALPH MODE ACTIVATED"));
    assert!(!message.contains("SEARCH MODE ACTIVATED"));
}

#[tokio::test]
async fn test_keyword_detector_with_other_hooks() {
    let mut registry = HookRegistry::new();

    // Register keyword detector with priority 100
    registry.register(Arc::new(KeywordDetectorHook::new()));

    // Register another hook with higher priority
    registry.register(Arc::new(MockHook::new(
        "pre-keyword",
        150,
        "PRE-KEYWORD MESSAGE",
    )));

    // Register another hook with lower priority
    registry.register(Arc::new(MockHook::new(
        "post-keyword",
        50,
        "POST-KEYWORD MESSAGE",
    )));

    let input = create_test_input("ultrawork: build this");
    let context = HookContext::new(None, "/tmp".to_string());

    let result = registry
        .execute_hooks(HookEvent::UserPromptSubmit, &input, &context)
        .await
        .unwrap();

    assert!(result.should_continue);
    assert!(result.message.is_some());

    let message = result.message.unwrap();

    // All three messages should be present
    assert!(message.contains("PRE-KEYWORD MESSAGE"));
    assert!(message.contains("ULTRAWORK MODE ACTIVATED"));
    assert!(message.contains("POST-KEYWORD MESSAGE"));

    // Verify order: pre-keyword (150) > keyword (100) > post-keyword (50)
    let pre_pos = message.find("PRE-KEYWORD MESSAGE").unwrap();
    let keyword_pos = message.find("ULTRAWORK MODE ACTIVATED").unwrap();
    let post_pos = message.find("POST-KEYWORD MESSAGE").unwrap();

    assert!(pre_pos < keyword_pos);
    assert!(keyword_pos < post_pos);
}

#[tokio::test]
async fn test_no_hooks_registered_returns_pass() {
    let registry = HookRegistry::new();

    let input = create_test_input("test prompt");
    let context = HookContext::new(None, "/tmp".to_string());

    let result = registry
        .execute_hooks(HookEvent::UserPromptSubmit, &input, &context)
        .await
        .unwrap();

    assert!(result.should_continue);
    assert!(result.message.is_none());
    assert!(result.reason.is_none());
}

#[tokio::test]
async fn test_hooks_for_different_events_dont_interfere() {
    let mut registry = HookRegistry::new();

    // Register hook for UserPromptSubmit
    registry.register(Arc::new(MockHook::new(
        "prompt-hook",
        100,
        "PROMPT MESSAGE",
    )));

    // Execute for different event (Stop) - should not trigger the hook
    let input = create_test_input("test prompt");
    let context = HookContext::new(None, "/tmp".to_string());

    let result = registry
        .execute_hooks(HookEvent::Stop, &input, &context)
        .await
        .unwrap();

    // Should pass (no hooks for Stop event)
    assert!(result.should_continue);
    assert!(result.message.is_none());
}

#[tokio::test]
async fn test_empty_prompt_returns_pass() {
    let mut registry = HookRegistry::new();
    registry.register(Arc::new(KeywordDetectorHook::new()));

    let input = create_test_input("");
    let context = HookContext::new(None, "/tmp".to_string());

    let result = registry
        .execute_hooks(HookEvent::UserPromptSubmit, &input, &context)
        .await
        .unwrap();

    assert!(result.should_continue);
    assert!(result.message.is_none());
}
