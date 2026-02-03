use crate::events::{ApprovalDecision, Event, FileChangeType, SessionEndReason};
use uira_protocol::{Item, ThreadEvent, TokenUsage};

impl From<ThreadEvent> for Event {
    fn from(thread_event: ThreadEvent) -> Self {
        match thread_event {
            ThreadEvent::ThreadStarted { thread_id } => Event::SessionStarted {
                session_id: thread_id,
                parent_id: None,
            },
            ThreadEvent::TurnStarted { turn_number } => Event::TurnStarted {
                session_id: String::new(),
                turn_number,
            },
            ThreadEvent::TurnCompleted { turn_number, usage } => Event::TurnCompleted {
                session_id: String::new(),
                turn_number,
                usage,
            },
            ThreadEvent::ItemStarted { item } => match item {
                Item::ToolCall { id, name, input } => Event::ToolExecutionStarted {
                    session_id: String::new(),
                    tool_call_id: id,
                    tool_name: name,
                    input,
                },
                Item::ApprovalRequest {
                    id,
                    tool_name,
                    input,
                    reason,
                } => Event::ApprovalRequested {
                    session_id: String::new(),
                    request_id: id,
                    tool_name,
                    input,
                    reason,
                },
                _ => Event::MessagesTransform {
                    session_id: String::new(),
                },
            },
            ThreadEvent::ItemCompleted { item } => match item {
                Item::ToolResult {
                    tool_call_id,
                    output,
                    is_error,
                } => Event::ToolExecutionCompleted {
                    session_id: String::new(),
                    tool_call_id: tool_call_id.clone(),
                    tool_name: String::new(),
                    output: serde_json::json!({ "output": output, "is_error": is_error }),
                    error: if is_error { Some(output) } else { None },
                    duration_ms: 0,
                },
                Item::ApprovalDecision {
                    request_id,
                    approved,
                } => Event::ApprovalDecided {
                    session_id: String::new(),
                    request_id,
                    decision: if approved {
                        ApprovalDecision::Approved
                    } else {
                        ApprovalDecision::Denied { reason: None }
                    },
                },
                Item::FileChange {
                    path,
                    change_type,
                    patch,
                } => Event::FileChanged {
                    session_id: String::new(),
                    path,
                    change_type: match change_type {
                        uira_protocol::FileChangeType::Create => FileChangeType::Create,
                        uira_protocol::FileChangeType::Modify => FileChangeType::Modify,
                        uira_protocol::FileChangeType::Delete => FileChangeType::Delete,
                        uira_protocol::FileChangeType::Rename => FileChangeType::Rename,
                    },
                    patch,
                },
                _ => Event::MessagesTransform {
                    session_id: String::new(),
                },
            },
            ThreadEvent::ContentDelta { delta } => Event::ContentDelta {
                session_id: String::new(),
                delta,
            },
            ThreadEvent::ThinkingDelta { thinking } => Event::ThinkingDelta {
                session_id: String::new(),
                delta: thinking,
            },
            ThreadEvent::WaitingForInput { prompt } => Event::UserInputRequested {
                session_id: String::new(),
                prompt,
            },
            ThreadEvent::Error {
                message,
                recoverable,
            } => Event::Error {
                session_id: String::new(),
                message,
                recoverable,
            },
            ThreadEvent::ThreadCompleted { usage: _ } => Event::SessionEnded {
                session_id: String::new(),
                reason: SessionEndReason::Completed,
            },
            ThreadEvent::ThreadCancelled => Event::SessionEnded {
                session_id: String::new(),
                reason: SessionEndReason::Cancelled,
            },
            ThreadEvent::GoalVerificationStarted { goals, method } => {
                Event::GoalVerificationStarted {
                    session_id: String::new(),
                    goals,
                    method,
                }
            }
            ThreadEvent::GoalVerificationResult {
                goal,
                score,
                target,
                passed,
                duration_ms,
            } => Event::GoalVerificationResult {
                session_id: String::new(),
                goal,
                score,
                target,
                passed,
                duration_ms,
            },
            ThreadEvent::GoalVerificationCompleted {
                all_passed,
                passed_count,
                total_count,
            } => Event::GoalVerificationCompleted {
                session_id: String::new(),
                all_passed,
                passed_count,
                total_count,
            },
            ThreadEvent::BackgroundTaskSpawned {
                task_id,
                description,
                agent,
            } => Event::BackgroundTaskSpawned {
                task_id,
                description,
                agent,
            },
            ThreadEvent::BackgroundTaskProgress {
                task_id,
                status,
                message,
            } => Event::BackgroundTaskProgress {
                task_id,
                status,
                message,
            },
            ThreadEvent::BackgroundTaskCompleted {
                task_id,
                success,
                result_preview,
                duration_secs,
            } => Event::BackgroundTaskCompleted {
                task_id,
                success,
                result_preview,
                duration_secs,
            },
            ThreadEvent::ModelSwitched { model, provider } => Event::ModelSwitched {
                session_id: String::new(),
                model,
                provider,
            },
            _ => Event::MessagesTransform {
                session_id: String::new(),
            },
        }
    }
}

impl From<Event> for Option<ThreadEvent> {
    fn from(event: Event) -> Self {
        match event {
            Event::SessionStarted { session_id, .. } => Some(ThreadEvent::ThreadStarted {
                thread_id: session_id,
            }),
            Event::TurnStarted { turn_number, .. } => {
                Some(ThreadEvent::TurnStarted { turn_number })
            }
            Event::TurnCompleted {
                turn_number, usage, ..
            } => Some(ThreadEvent::TurnCompleted { turn_number, usage }),
            Event::ContentDelta { delta, .. } => Some(ThreadEvent::ContentDelta { delta }),
            Event::ThinkingDelta { delta, .. } => {
                Some(ThreadEvent::ThinkingDelta { thinking: delta })
            }
            Event::UserInputRequested { prompt, .. } => {
                Some(ThreadEvent::WaitingForInput { prompt })
            }
            Event::Error {
                message,
                recoverable,
                ..
            } => Some(ThreadEvent::Error {
                message,
                recoverable,
            }),
            Event::SessionEnded { reason, .. } => match reason {
                SessionEndReason::Completed => Some(ThreadEvent::ThreadCompleted {
                    usage: TokenUsage::default(),
                }),
                SessionEndReason::Cancelled => Some(ThreadEvent::ThreadCancelled),
                _ => Some(ThreadEvent::ThreadCompleted {
                    usage: TokenUsage::default(),
                }),
            },
            Event::GoalVerificationStarted { goals, method, .. } => {
                Some(ThreadEvent::GoalVerificationStarted { goals, method })
            }
            Event::GoalVerificationResult {
                goal,
                score,
                target,
                passed,
                duration_ms,
                ..
            } => Some(ThreadEvent::GoalVerificationResult {
                goal,
                score,
                target,
                passed,
                duration_ms,
            }),
            Event::GoalVerificationCompleted {
                all_passed,
                passed_count,
                total_count,
                ..
            } => Some(ThreadEvent::GoalVerificationCompleted {
                all_passed,
                passed_count,
                total_count,
            }),
            Event::BackgroundTaskSpawned {
                task_id,
                description,
                agent,
            } => Some(ThreadEvent::BackgroundTaskSpawned {
                task_id,
                description,
                agent,
            }),
            Event::BackgroundTaskProgress {
                task_id,
                status,
                message,
            } => Some(ThreadEvent::BackgroundTaskProgress {
                task_id,
                status,
                message,
            }),
            Event::BackgroundTaskCompleted {
                task_id,
                success,
                result_preview,
                duration_secs,
            } => Some(ThreadEvent::BackgroundTaskCompleted {
                task_id,
                success,
                result_preview,
                duration_secs,
            }),
            Event::ModelSwitched {
                model, provider, ..
            } => Some(ThreadEvent::ModelSwitched { model, provider }),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_thread_event_to_event() {
        let thread_event = ThreadEvent::ThreadStarted {
            thread_id: "test_123".to_string(),
        };
        let event: Event = thread_event.into();

        match event {
            Event::SessionStarted { session_id, .. } => {
                assert_eq!(session_id, "test_123");
            }
            _ => panic!("Wrong event type"),
        }
    }

    #[test]
    fn test_event_to_thread_event() {
        let event = Event::ContentDelta {
            session_id: "test".to_string(),
            delta: "Hello".to_string(),
        };
        let thread_event: Option<ThreadEvent> = event.into();

        match thread_event {
            Some(ThreadEvent::ContentDelta { delta }) => {
                assert_eq!(delta, "Hello");
            }
            _ => panic!("Wrong event type"),
        }
    }
}
