use uira_hooks::hook::{Hook, HookContext, HookResult};
use uira_hooks::hooks::keyword_detector::KeywordDetectorHook;
use uira_hooks::registry::HookRegistry;
use uira_hooks::types::{HookEvent, HookInput, HookOutput};
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
        transcript_path: None,
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

mod ralph_goals_integration {
    use super::*;
    use uira_hooks::hooks::ralph::RalphHook;
    use std::fs;
    use tempfile::TempDir;

    fn create_test_directory() -> TempDir {
        tempfile::tempdir().expect("Failed to create temp directory")
    }

    fn write_ralph_state(dir: &std::path::Path, session_id: &str) {
        let uira_dir = dir.join(".uira");
        fs::create_dir_all(&uira_dir).unwrap();

        let state = serde_json::json!({
            "active": true,
            "iteration": 1,
            "max_iterations": 10,
            "completion_promise": "TASK COMPLETE",
            "session_id": session_id,
            "prompt": "Test ralph goal verification",
            "started_at": "2026-01-24T00:00:00Z",
            "last_checked_at": "2026-01-24T00:00:00Z",
            "min_confidence": 50,
            "require_dual_condition": false,
            "session_hours": 24,
            "circuit_breaker": {
                "state": "closed",
                "consecutive_no_progress": 0,
                "error_history": [],
                "output_sizes": [],
                "trip_reason": null
            },
            "circuit_config": {
                "no_progress_threshold": 3,
                "same_error_threshold": 5,
                "output_decline_threshold": 70
            }
        });

        fs::write(
            uira_dir.join("ralph-state.json"),
            serde_json::to_string_pretty(&state).unwrap(),
        )
        .unwrap();
    }

    fn write_goals_config(dir: &std::path::Path, target: f64) {
        let config = format!(
            r#"goals:
  auto_verify: true
  goals:
    - name: progress-check
      command: cat progress.txt
      target: {}
"#,
            target
        );
        fs::write(dir.join("uira.yml"), config).unwrap();
    }

    fn write_progress(dir: &std::path::Path, value: u32) {
        fs::write(dir.join("progress.txt"), value.to_string()).unwrap();
    }

    fn write_transcript_with_promise(dir: &std::path::Path) -> std::path::PathBuf {
        let transcript_path = dir.join("test-transcript.jsonl");
        let entry = serde_json::json!({
            "type": "progress",
            "data": {
                "message": {
                    "type": "assistant",
                    "message": {
                        "content": [
                            { "type": "text", "text": "<promise>TASK COMPLETE</promise>" }
                        ]
                    }
                }
            }
        });
        fs::write(
            &transcript_path,
            serde_json::to_string(&entry).unwrap() + "\n",
        )
        .unwrap();
        transcript_path
    }

    fn create_stop_input(
        session_id: &str,
        dir: &std::path::Path,
        transcript_path: Option<&std::path::Path>,
    ) -> HookInput {
        HookInput {
            session_id: Some(session_id.to_string()),
            prompt: Some(String::new()),
            message: None,
            parts: None,
            tool_name: None,
            tool_input: None,
            tool_output: None,
            directory: Some(dir.to_string_lossy().to_string()),
            stop_reason: Some("end_turn".to_string()),
            user_requested: Some(false),
            transcript_path: transcript_path.map(|p| p.to_string_lossy().to_string()),
            extra: HashMap::new(),
        }
    }

    #[tokio::test]
    async fn test_ralph_goals_pass_returns_complete() {
        let temp_dir = create_test_directory();
        let dir = temp_dir.path();

        write_ralph_state(dir, "test-session");
        write_goals_config(dir, 90.0);
        write_progress(dir, 95);
        let transcript_path = write_transcript_with_promise(dir);

        let mut registry = HookRegistry::new();
        registry.register(Arc::new(RalphHook::new()));

        let input = create_stop_input("test-session", dir, Some(&transcript_path));
        let context = HookContext::new(
            Some("test-session".to_string()),
            dir.to_string_lossy().to_string(),
        );

        let result = registry
            .execute_hooks(HookEvent::Stop, &input, &context)
            .await
            .unwrap();

        assert!(
            !result.should_continue,
            "Should stop hook chain on ralph complete"
        );
        assert!(
            result
                .message
                .as_ref()
                .map_or(false, |m| m.contains("RALPH COMPLETE")),
            "Should contain RALPH COMPLETE message, got: {:?}",
            result.message
        );
        assert!(
            !dir.join(".uira/ralph-state.json").exists(),
            "Ralph state should be cleared on success"
        );
    }

    #[tokio::test]
    async fn test_ralph_goals_fail_returns_verification_failure() {
        let temp_dir = create_test_directory();
        let dir = temp_dir.path();

        write_ralph_state(dir, "test-session");
        write_goals_config(dir, 90.0);
        write_progress(dir, 80);
        let transcript_path = write_transcript_with_promise(dir);

        let mut registry = HookRegistry::new();
        registry.register(Arc::new(RalphHook::new()));

        let input = create_stop_input("test-session", dir, Some(&transcript_path));
        let context = HookContext::new(
            Some("test-session".to_string()),
            dir.to_string_lossy().to_string(),
        );

        let result = registry
            .execute_hooks(HookEvent::Stop, &input, &context)
            .await
            .unwrap();

        assert!(
            !result.should_continue,
            "Should block on verification failure"
        );
        assert!(
            result
                .reason
                .as_ref()
                .map_or(false, |r| r.contains("verification-failed")),
            "Should contain verification-failed in reason, got: {:?}",
            result.reason
        );
        assert!(
            dir.join(".uira/ralph-state.json").exists(),
            "Ralph state should remain active on failure"
        );
    }

    #[tokio::test]
    async fn test_ralph_without_transcript_continues_loop() {
        let temp_dir = create_test_directory();
        let dir = temp_dir.path();

        write_ralph_state(dir, "test-session");

        let mut registry = HookRegistry::new();
        registry.register(Arc::new(RalphHook::new()));

        let input = create_stop_input("test-session", dir, None);
        let context = HookContext::new(
            Some("test-session".to_string()),
            dir.to_string_lossy().to_string(),
        );

        let result = registry
            .execute_hooks(HookEvent::Stop, &input, &context)
            .await
            .unwrap();

        assert!(!result.should_continue, "Should block to continue loop");
        assert!(
            result
                .reason
                .as_ref()
                .map_or(false, |r| r.contains("ralph-continuation")),
            "Should contain ralph-continuation in reason, got: {:?}",
            result.reason
        );
    }

    #[tokio::test]
    async fn test_ralph_inactive_state_passes() {
        let temp_dir = create_test_directory();
        let dir = temp_dir.path();

        // Use a unique session ID that won't match any leftover global state
        let unique_session = format!("unique-inactive-{}", std::process::id());

        let mut registry = HookRegistry::new();
        registry.register(Arc::new(RalphHook::new()));

        let input = create_stop_input(&unique_session, dir, None);
        let context = HookContext::new(
            Some(unique_session.clone()),
            dir.to_string_lossy().to_string(),
        );

        let result = registry
            .execute_hooks(HookEvent::Stop, &input, &context)
            .await
            .unwrap();

        assert!(
            result.should_continue,
            "Should pass when no ralph state exists (mismatched session_id)"
        );
    }

    #[tokio::test]
    async fn test_transcript_parsing_extracts_promise() {
        let temp_dir = create_test_directory();
        let dir = temp_dir.path();
        let transcript_path = write_transcript_with_promise(dir);

        let input = HookInput {
            session_id: None,
            prompt: None,
            message: None,
            parts: None,
            tool_name: None,
            tool_input: None,
            tool_output: None,
            directory: None,
            stop_reason: None,
            user_requested: None,
            transcript_path: Some(transcript_path.to_string_lossy().to_string()),
            extra: HashMap::new(),
        };

        let response = input.get_last_assistant_response();
        assert!(
            response.is_some(),
            "Should extract response from transcript"
        );
        assert!(
            response.as_ref().unwrap().contains("TASK COMPLETE"),
            "Should contain promise text, got: {:?}",
            response
        );
    }
}
