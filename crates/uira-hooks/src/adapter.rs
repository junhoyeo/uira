//! Hook Event Adapter - Bridges HookRegistry to EventBus
//!
//! This adapter allows the existing hooks to receive events from the new
//! unified event bus system while we migrate to the new subscriber model.

use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;

use crate::hook::HookContext;
use crate::registry::HookRegistry;
use crate::types::{HookEvent, HookInput, HookOutput, Message};
use uira_events::{
    Event, EventCategory, EventHandler, HandlerResult, SessionEndReason, SubscriptionFilter,
};

/// Adapter that wraps the HookRegistry to work with the new EventBus
pub struct HookEventAdapter {
    registry: Arc<HookRegistry>,
    working_directory: String,
}

impl HookEventAdapter {
    pub fn new(registry: Arc<HookRegistry>, working_directory: String) -> Self {
        Self {
            registry,
            working_directory,
        }
    }

    /// Convert a new Event to the legacy HookEvent
    fn event_to_hook_event(event: &Event) -> Option<HookEvent> {
        match event {
            Event::SessionStarted { .. } => Some(HookEvent::SessionStart),
            Event::SessionEnded { .. } => Some(HookEvent::Stop),
            Event::SessionIdle { .. } => Some(HookEvent::SessionIdle),
            Event::UserInputRequested { .. } => Some(HookEvent::UserPromptSubmit),
            Event::UserPromptSubmitted { .. } => Some(HookEvent::UserPromptSubmit),
            Event::ToolExecutionStarted { .. } => Some(HookEvent::PreToolUse),
            Event::ToolExecutionCompleted { .. } => Some(HookEvent::PostToolUse),
            Event::MessagesTransform { .. } => Some(HookEvent::MessagesTransform),
            _ => None,
        }
    }

    /// Convert a new Event to legacy HookInput
    fn event_to_hook_input(event: &Event) -> HookInput {
        match event {
            Event::SessionStarted { session_id, .. } => HookInput {
                session_id: Some(session_id.clone()),
                prompt: None,
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
            },

            Event::SessionEnded { session_id, reason } => {
                let stop_reason = match reason {
                    SessionEndReason::Completed => "completed",
                    SessionEndReason::Cancelled => "cancelled",
                    SessionEndReason::Error => "error",
                    SessionEndReason::Timeout => "timeout",
                    SessionEndReason::MaxTurns => "max_turns",
                };
                HookInput {
                    session_id: Some(session_id.clone()),
                    prompt: None,
                    message: None,
                    parts: None,
                    tool_name: None,
                    tool_input: None,
                    tool_output: None,
                    directory: None,
                    stop_reason: Some(stop_reason.to_string()),
                    user_requested: Some(matches!(reason, SessionEndReason::Cancelled)),
                    transcript_path: None,
                    extra: HashMap::new(),
                }
            }

            Event::UserInputRequested {
                session_id, prompt, ..
            } => HookInput {
                session_id: Some(session_id.clone()),
                prompt: Some(prompt.clone()),
                message: Some(Message {
                    content: Some(prompt.clone()),
                }),
                parts: None,
                tool_name: None,
                tool_input: None,
                tool_output: None,
                directory: None,
                stop_reason: None,
                user_requested: None,
                transcript_path: None,
                extra: HashMap::new(),
            },

            Event::UserPromptSubmitted {
                session_id,
                prompt,
                directory,
            } => HookInput {
                session_id: Some(session_id.clone()),
                prompt: Some(prompt.clone()),
                message: Some(Message {
                    content: Some(prompt.clone()),
                }),
                parts: None,
                tool_name: None,
                tool_input: None,
                tool_output: None,
                directory: directory.clone(),
                stop_reason: None,
                user_requested: None,
                transcript_path: None,
                extra: HashMap::new(),
            },

            Event::ToolExecutionStarted {
                session_id,
                tool_call_id,
                tool_name,
                input,
            } => {
                let mut extra = HashMap::new();
                extra.insert("tool_call_id".to_string(), serde_json::json!(tool_call_id));
                HookInput {
                    session_id: Some(session_id.clone()),
                    prompt: None,
                    message: None,
                    parts: None,
                    tool_name: Some(tool_name.clone()),
                    tool_input: Some(input.clone()),
                    tool_output: None,
                    directory: None,
                    stop_reason: None,
                    user_requested: None,
                    transcript_path: None,
                    extra,
                }
            }

            Event::ToolExecutionCompleted {
                session_id,
                tool_call_id,
                tool_name,
                output,
                error,
                duration_ms,
            } => {
                let mut extra = HashMap::new();
                extra.insert("tool_call_id".to_string(), serde_json::json!(tool_call_id));
                extra.insert("duration_ms".to_string(), serde_json::json!(duration_ms));
                if let Some(err) = error {
                    extra.insert("error".to_string(), serde_json::json!(err));
                }
                HookInput {
                    session_id: Some(session_id.clone()),
                    prompt: None,
                    message: None,
                    parts: None,
                    tool_name: Some(tool_name.clone()),
                    tool_input: None,
                    tool_output: Some(output.clone()),
                    directory: None,
                    stop_reason: None,
                    user_requested: None,
                    transcript_path: None,
                    extra,
                }
            }

            Event::MessagesTransform { session_id } => HookInput {
                session_id: Some(session_id.clone()),
                prompt: None,
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
            },

            _ => HookInput {
                session_id: event.session_id().map(|s| s.to_string()),
                prompt: None,
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
            },
        }
    }

    /// Convert legacy HookOutput to new HandlerResult
    fn hook_output_to_handler_result(output: HookOutput) -> HandlerResult {
        if output.should_continue {
            if let Some(msg) = output.message {
                HandlerResult::with_message(msg)
            } else {
                HandlerResult::pass()
            }
        } else {
            HandlerResult::block(output.reason.unwrap_or_else(|| "blocked".to_string()))
        }
    }
}

#[async_trait]
impl EventHandler for HookEventAdapter {
    fn name(&self) -> &str {
        "hook-event-adapter"
    }

    fn filter(&self) -> SubscriptionFilter {
        SubscriptionFilter::new().categories([
            EventCategory::Session,
            EventCategory::Tool,
            EventCategory::Content,
        ])
    }

    async fn handle(&self, event: &Event) -> HandlerResult {
        let hook_event = match Self::event_to_hook_event(event) {
            Some(he) => he,
            None => return HandlerResult::pass(),
        };

        let hook_input = Self::event_to_hook_input(event);
        let context = HookContext::new(
            event.session_id().map(|s| s.to_string()),
            self.working_directory.clone(),
        );

        match self
            .registry
            .execute_hooks(hook_event, &hook_input, &context)
            .await
        {
            Ok(output) => Self::hook_output_to_handler_result(output),
            Err(e) => {
                eprintln!("[hook-event-adapter] Error executing hooks: {}", e);
                HandlerResult::pass()
            }
        }
    }

    fn priority(&self) -> i32 {
        -100
    }
}

/// Helper to create a HookEventAdapter with default hooks
pub fn create_hook_event_adapter(working_directory: String) -> HookEventAdapter {
    let registry = Arc::new(crate::registry::default_hooks());
    HookEventAdapter::new(registry, working_directory)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_event_to_hook_event_mapping() {
        let session_started = Event::SessionStarted {
            session_id: "test".to_string(),
            parent_id: None,
        };
        assert_eq!(
            HookEventAdapter::event_to_hook_event(&session_started),
            Some(HookEvent::SessionStart)
        );

        let session_ended = Event::SessionEnded {
            session_id: "test".to_string(),
            reason: SessionEndReason::Completed,
        };
        assert_eq!(
            HookEventAdapter::event_to_hook_event(&session_ended),
            Some(HookEvent::Stop)
        );

        let tool_started = Event::ToolExecutionStarted {
            session_id: "test".to_string(),
            tool_call_id: "tc_1".to_string(),
            tool_name: "bash".to_string(),
            input: serde_json::json!({}),
        };
        assert_eq!(
            HookEventAdapter::event_to_hook_event(&tool_started),
            Some(HookEvent::PreToolUse)
        );

        let tool_completed = Event::ToolExecutionCompleted {
            session_id: "test".to_string(),
            tool_call_id: "tc_1".to_string(),
            tool_name: "bash".to_string(),
            output: serde_json::json!({}),
            error: None,
            duration_ms: 100,
        };
        assert_eq!(
            HookEventAdapter::event_to_hook_event(&tool_completed),
            Some(HookEvent::PostToolUse)
        );

        let turn_started = Event::TurnStarted {
            session_id: "test".to_string(),
            turn_number: 1,
        };
        assert_eq!(HookEventAdapter::event_to_hook_event(&turn_started), None);
    }

    #[test]
    fn test_event_to_hook_input() {
        let event = Event::ToolExecutionStarted {
            session_id: "ses_123".to_string(),
            tool_call_id: "tc_456".to_string(),
            tool_name: "read".to_string(),
            input: serde_json::json!({"path": "/tmp/test.txt"}),
        };

        let input = HookEventAdapter::event_to_hook_input(&event);

        assert_eq!(input.session_id, Some("ses_123".to_string()));
        assert_eq!(input.tool_name, Some("read".to_string()));
        assert_eq!(
            input.tool_input,
            Some(serde_json::json!({"path": "/tmp/test.txt"}))
        );
        assert_eq!(
            input.extra.get("tool_call_id"),
            Some(&serde_json::json!("tc_456"))
        );
    }

    #[test]
    fn test_hook_output_to_handler_result() {
        let pass = HookOutput::pass();
        let result = HookEventAdapter::hook_output_to_handler_result(pass);
        assert!(result.should_continue);
        assert!(result.message.is_none());

        let with_msg = HookOutput::continue_with_message("test message");
        let result = HookEventAdapter::hook_output_to_handler_result(with_msg);
        assert!(result.should_continue);
        assert_eq!(result.message, Some("test message".to_string()));

        let block = HookOutput::block_with_reason("blocked reason");
        let result = HookEventAdapter::hook_output_to_handler_result(block);
        assert!(!result.should_continue);
        assert_eq!(result.message, Some("blocked reason".to_string()));
    }
}
