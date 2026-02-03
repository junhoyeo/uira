use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use uira_protocol::TokenUsage;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventCategory {
    Session,
    Turn,
    Tool,
    Approval,
    Content,
    Goal,
    Background,
    System,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Event {
    // ============================================================================
    // Session Events (replaces HookEvent::SessionStart, Stop)
    // ============================================================================
    SessionStarted {
        session_id: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        parent_id: Option<String>,
    },
    SessionResumed {
        session_id: String,
    },
    SessionForked {
        session_id: String,
        parent_id: String,
        fork_point_message_id: Option<String>,
    },
    SessionEnded {
        session_id: String,
        reason: SessionEndReason,
    },
    SessionIdle {
        session_id: String,
    },

    // ============================================================================
    // Turn Events (replaces ThreadEvent::TurnStarted, TurnCompleted)
    // ============================================================================
    TurnStarted {
        session_id: String,
        turn_number: usize,
    },
    TurnCompleted {
        session_id: String,
        turn_number: usize,
        usage: TokenUsage,
    },

    // ============================================================================
    // User Input Events (replaces HookEvent::UserPromptSubmit)
    // ============================================================================
    UserPromptSubmitted {
        session_id: String,
        prompt: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        directory: Option<String>,
    },
    UserInputRequested {
        session_id: String,
        prompt: String,
    },

    // ============================================================================
    // Tool Events (replaces HookEvent::PreToolUse, PostToolUse)
    // ============================================================================
    ToolExecutionStarted {
        session_id: String,
        tool_call_id: String,
        tool_name: String,
        input: serde_json::Value,
    },
    ToolExecutionCompleted {
        session_id: String,
        tool_call_id: String,
        tool_name: String,
        output: serde_json::Value,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
        duration_ms: u64,
    },
    ToolRetried {
        session_id: String,
        tool_call_id: String,
        tool_name: String,
        attempt: u32,
        reason: String,
    },

    // ============================================================================
    // Approval Events
    // ============================================================================
    ApprovalRequested {
        session_id: String,
        request_id: String,
        tool_name: String,
        input: serde_json::Value,
        reason: String,
    },
    ApprovalDecided {
        session_id: String,
        request_id: String,
        decision: ApprovalDecision,
    },
    ApprovalCached {
        session_id: String,
        tool_name: String,
        pattern: String,
    },

    // ============================================================================
    // Content Streaming Events
    // ============================================================================
    ContentDelta {
        session_id: String,
        delta: String,
    },
    ThinkingDelta {
        session_id: String,
        delta: String,
    },
    MessageCompleted {
        session_id: String,
        content: String,
    },

    // ============================================================================
    // Permission Events
    // ============================================================================
    PermissionEvaluated {
        session_id: String,
        permission: String,
        path: String,
        action: PermissionAction,
        rule_matched: Option<String>,
    },

    // ============================================================================
    // File Events
    // ============================================================================
    FileChanged {
        session_id: String,
        path: PathBuf,
        change_type: FileChangeType,
        #[serde(skip_serializing_if = "Option::is_none")]
        patch: Option<String>,
    },

    // ============================================================================
    // Goal Verification Events
    // ============================================================================
    GoalVerificationStarted {
        session_id: String,
        goals: Vec<String>,
        method: String,
    },
    GoalVerificationResult {
        session_id: String,
        goal: String,
        score: f64,
        target: f64,
        passed: bool,
        duration_ms: u64,
    },
    GoalVerificationCompleted {
        session_id: String,
        all_passed: bool,
        passed_count: usize,
        total_count: usize,
    },

    // ============================================================================
    // Background Task Events
    // ============================================================================
    BackgroundTaskSpawned {
        task_id: String,
        description: String,
        agent: String,
    },
    BackgroundTaskProgress {
        task_id: String,
        status: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        message: Option<String>,
    },
    BackgroundTaskCompleted {
        task_id: String,
        success: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        result_preview: Option<String>,
        duration_secs: f64,
    },

    // ============================================================================
    // Compaction Events
    // ============================================================================
    CompactionStarted {
        session_id: String,
        strategy: String,
        token_count_before: usize,
    },
    CompactionCompleted {
        session_id: String,
        token_count_before: usize,
        token_count_after: usize,
        messages_removed: usize,
    },

    // ============================================================================
    // System Events
    // ============================================================================
    ModelSwitched {
        session_id: String,
        model: String,
        provider: String,
    },
    Error {
        session_id: String,
        message: String,
        recoverable: bool,
    },

    // ============================================================================
    // Transform Events (for hook compatibility)
    // ============================================================================
    MessagesTransform {
        session_id: String,
    },
}

impl Event {
    pub fn category(&self) -> EventCategory {
        match self {
            Self::SessionStarted { .. }
            | Self::SessionResumed { .. }
            | Self::SessionForked { .. }
            | Self::SessionEnded { .. }
            | Self::SessionIdle { .. } => EventCategory::Session,

            Self::TurnStarted { .. } | Self::TurnCompleted { .. } => EventCategory::Turn,

            Self::UserPromptSubmitted { .. } | Self::UserInputRequested { .. } => {
                EventCategory::Session
            }

            Self::ToolExecutionStarted { .. }
            | Self::ToolExecutionCompleted { .. }
            | Self::ToolRetried { .. } => EventCategory::Tool,

            Self::ApprovalRequested { .. }
            | Self::ApprovalDecided { .. }
            | Self::ApprovalCached { .. } => EventCategory::Approval,

            Self::ContentDelta { .. }
            | Self::ThinkingDelta { .. }
            | Self::MessageCompleted { .. } => EventCategory::Content,

            Self::PermissionEvaluated { .. } | Self::FileChanged { .. } => EventCategory::System,

            Self::GoalVerificationStarted { .. }
            | Self::GoalVerificationResult { .. }
            | Self::GoalVerificationCompleted { .. } => EventCategory::Goal,

            Self::BackgroundTaskSpawned { .. }
            | Self::BackgroundTaskProgress { .. }
            | Self::BackgroundTaskCompleted { .. } => EventCategory::Background,

            Self::CompactionStarted { .. } | Self::CompactionCompleted { .. } => {
                EventCategory::System
            }

            Self::ModelSwitched { .. } | Self::Error { .. } | Self::MessagesTransform { .. } => {
                EventCategory::System
            }
        }
    }

    pub fn session_id(&self) -> Option<&str> {
        match self {
            Self::SessionStarted { session_id, .. }
            | Self::SessionResumed { session_id }
            | Self::SessionForked { session_id, .. }
            | Self::SessionEnded { session_id, .. }
            | Self::SessionIdle { session_id }
            | Self::TurnStarted { session_id, .. }
            | Self::TurnCompleted { session_id, .. }
            | Self::UserPromptSubmitted { session_id, .. }
            | Self::UserInputRequested { session_id, .. }
            | Self::ToolExecutionStarted { session_id, .. }
            | Self::ToolExecutionCompleted { session_id, .. }
            | Self::ToolRetried { session_id, .. }
            | Self::ApprovalRequested { session_id, .. }
            | Self::ApprovalDecided { session_id, .. }
            | Self::ApprovalCached { session_id, .. }
            | Self::ContentDelta { session_id, .. }
            | Self::ThinkingDelta { session_id, .. }
            | Self::MessageCompleted { session_id, .. }
            | Self::PermissionEvaluated { session_id, .. }
            | Self::FileChanged { session_id, .. }
            | Self::GoalVerificationStarted { session_id, .. }
            | Self::GoalVerificationResult { session_id, .. }
            | Self::GoalVerificationCompleted { session_id, .. }
            | Self::CompactionStarted { session_id, .. }
            | Self::CompactionCompleted { session_id, .. }
            | Self::ModelSwitched { session_id, .. }
            | Self::Error { session_id, .. }
            | Self::MessagesTransform { session_id } => Some(session_id),

            Self::BackgroundTaskSpawned { .. }
            | Self::BackgroundTaskProgress { .. }
            | Self::BackgroundTaskCompleted { .. } => None,
        }
    }

    pub fn event_name(&self) -> &'static str {
        match self {
            Self::SessionStarted { .. } => "session_started",
            Self::SessionResumed { .. } => "session_resumed",
            Self::SessionForked { .. } => "session_forked",
            Self::SessionEnded { .. } => "session_ended",
            Self::SessionIdle { .. } => "session_idle",
            Self::TurnStarted { .. } => "turn_started",
            Self::TurnCompleted { .. } => "turn_completed",
            Self::UserPromptSubmitted { .. } => "user_prompt_submitted",
            Self::UserInputRequested { .. } => "user_input_requested",
            Self::ToolExecutionStarted { .. } => "tool_execution_started",
            Self::ToolExecutionCompleted { .. } => "tool_execution_completed",
            Self::ToolRetried { .. } => "tool_retried",
            Self::ApprovalRequested { .. } => "approval_requested",
            Self::ApprovalDecided { .. } => "approval_decided",
            Self::ApprovalCached { .. } => "approval_cached",
            Self::ContentDelta { .. } => "content_delta",
            Self::ThinkingDelta { .. } => "thinking_delta",
            Self::MessageCompleted { .. } => "message_completed",
            Self::PermissionEvaluated { .. } => "permission_evaluated",
            Self::FileChanged { .. } => "file_changed",
            Self::GoalVerificationStarted { .. } => "goal_verification_started",
            Self::GoalVerificationResult { .. } => "goal_verification_result",
            Self::GoalVerificationCompleted { .. } => "goal_verification_completed",
            Self::BackgroundTaskSpawned { .. } => "background_task_spawned",
            Self::BackgroundTaskProgress { .. } => "background_task_progress",
            Self::BackgroundTaskCompleted { .. } => "background_task_completed",
            Self::CompactionStarted { .. } => "compaction_started",
            Self::CompactionCompleted { .. } => "compaction_completed",
            Self::ModelSwitched { .. } => "model_switched",
            Self::Error { .. } => "error",
            Self::MessagesTransform { .. } => "messages_transform",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionEndReason {
    Completed,
    Cancelled,
    Error,
    MaxTurns,
    Timeout,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalDecision {
    Approved,
    ApprovedForSession,
    Denied { reason: Option<String> },
    Edited { new_input: serde_json::Value },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PermissionAction {
    Allow,
    Deny,
    Ask,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FileChangeType {
    Create,
    Modify,
    Delete,
    Rename,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_event_category() {
        let event = Event::SessionStarted {
            session_id: "test".to_string(),
            parent_id: None,
        };
        assert_eq!(event.category(), EventCategory::Session);

        let event = Event::ToolExecutionStarted {
            session_id: "test".to_string(),
            tool_call_id: "tc_1".to_string(),
            tool_name: "bash".to_string(),
            input: serde_json::json!({}),
        };
        assert_eq!(event.category(), EventCategory::Tool);
    }

    #[test]
    fn test_event_session_id() {
        let event = Event::TurnStarted {
            session_id: "ses_123".to_string(),
            turn_number: 1,
        };
        assert_eq!(event.session_id(), Some("ses_123"));

        let event = Event::BackgroundTaskSpawned {
            task_id: "bg_1".to_string(),
            description: "test".to_string(),
            agent: "explore".to_string(),
        };
        assert_eq!(event.session_id(), None);
    }

    #[test]
    fn test_event_serialization() {
        let event = Event::ApprovalDecided {
            session_id: "ses_123".to_string(),
            request_id: "req_1".to_string(),
            decision: ApprovalDecision::ApprovedForSession,
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("\"type\":\"approval_decided\""));

        let parsed: Event = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.event_name(), "approval_decided");
    }
}
