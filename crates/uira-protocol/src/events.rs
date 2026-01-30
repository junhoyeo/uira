//! Event types for streaming and JSONL output

use crate::TokenUsage;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Top-level event for JSONL streaming
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
#[non_exhaustive]
pub enum ThreadEvent {
    /// Thread has started
    ThreadStarted { thread_id: String },

    /// A new turn has started
    TurnStarted { turn_number: usize },

    /// A turn has completed
    TurnCompleted {
        turn_number: usize,
        usage: TokenUsage,
    },

    /// An item has started processing
    ItemStarted { item: Item },

    /// An item has completed
    ItemCompleted { item: Item },

    /// Content is being streamed
    ContentDelta { delta: String },

    /// Thinking/reasoning content is being streamed
    ThinkingDelta { thinking: String },

    /// Waiting for user input
    WaitingForInput { prompt: String },

    /// Error occurred
    Error { message: String, recoverable: bool },

    /// Thread has completed
    ThreadCompleted { usage: TokenUsage },

    /// Thread was cancelled
    ThreadCancelled,

    // Goal Verification Events
    /// Goal verification has started
    GoalVerificationStarted { goals: Vec<String>, method: String },
    /// Result of a single goal verification
    GoalVerificationResult {
        goal: String,
        score: f64,
        target: f64,
        passed: bool,
        duration_ms: u64,
    },
    /// All goal verifications completed
    GoalVerificationCompleted {
        all_passed: bool,
        passed_count: usize,
        total_count: usize,
    },

    // Ralph Mode Events
    /// Ralph iteration has started
    RalphIterationStarted {
        iteration: u32,
        max_iterations: u32,
        prompt: String,
    },
    /// Ralph is continuing (verification failed)
    RalphContinuation {
        reason: String,
        confidence: u32,
        details: String,
    },
    /// Ralph circuit breaker tripped
    RalphCircuitBreak { reason: String, iteration: u32 },

    // Background Task Events
    /// Background task has been spawned
    BackgroundTaskSpawned {
        task_id: String,
        description: String,
        agent: String,
    },
    /// Background task progress update
    BackgroundTaskProgress {
        task_id: String,
        status: String,
        message: Option<String>,
    },
    /// Background task has completed
    BackgroundTaskCompleted {
        task_id: String,
        success: bool,
        result_preview: Option<String>,
        duration_secs: f64,
    },

    /// Model was switched at runtime
    ModelSwitched { model: String, provider: String },
}

/// Item types that can be processed
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Item {
    /// Agent is thinking/reasoning
    Thinking {
        #[serde(skip_serializing_if = "Option::is_none")]
        content: Option<String>,
    },

    /// Agent message (text response)
    AgentMessage { content: String },

    /// Tool is being called
    ToolCall {
        id: String,
        name: String,
        input: serde_json::Value,
    },

    /// Tool has returned a result
    ToolResult {
        tool_call_id: String,
        output: String,
        is_error: bool,
    },

    /// Command execution (bash, etc.)
    CommandExecution {
        command: String,
        exit_code: i32,
        stdout: String,
        stderr: String,
    },

    /// File change
    FileChange {
        path: PathBuf,
        change_type: FileChangeType,
        #[serde(skip_serializing_if = "Option::is_none")]
        patch: Option<String>,
    },

    /// Approval request
    ApprovalRequest {
        id: String,
        tool_name: String,
        input: serde_json::Value,
        reason: String,
    },

    /// Approval decision
    ApprovalDecision { request_id: String, approved: bool },

    /// MCP tool call
    McpToolCall {
        server: String,
        tool: String,
        result: serde_json::Value,
    },
}

/// Type of file change
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FileChangeType {
    Create,
    Modify,
    Delete,
    Rename,
}

/// Agent state for status reporting
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentState {
    Idle,
    WaitingForUser,
    Thinking,
    ExecutingTool,
    WaitingForApproval,
    Complete,
    Failed,
    Cancelled,
}

impl std::fmt::Display for AgentState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Idle => write!(f, "idle"),
            Self::WaitingForUser => write!(f, "waiting for user"),
            Self::Thinking => write!(f, "thinking"),
            Self::ExecutingTool => write!(f, "executing tool"),
            Self::WaitingForApproval => write!(f, "waiting for approval"),
            Self::Complete => write!(f, "complete"),
            Self::Failed => write!(f, "failed"),
            Self::Cancelled => write!(f, "cancelled"),
        }
    }
}

/// Error types that can occur during agent execution
#[derive(Debug, Clone, thiserror::Error, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AgentError {
    #[error("model error: {message}")]
    ModelError { message: String },

    #[error("tool error: {tool} - {message}")]
    ToolError { tool: String, message: String },

    #[error("sandbox error: {message}")]
    SandboxError { message: String },

    #[error("context exceeded: {used} tokens used, {limit} limit")]
    ContextExceeded { used: u64, limit: u64 },

    #[error("approval denied: {reason}")]
    ApprovalDenied { reason: String },

    #[error("cancelled by user")]
    Cancelled,

    #[error("timeout after {seconds}s")]
    Timeout { seconds: u64 },

    #[error("max turns exceeded: {turns}")]
    MaxTurnsExceeded { turns: usize },

    #[error("configuration error: {message}")]
    ConfigError { message: String },
}

impl AgentError {
    pub fn is_recoverable(&self) -> bool {
        matches!(
            self,
            Self::ToolError { .. } | Self::ApprovalDenied { .. } | Self::Timeout { .. }
        )
    }
}

/// Progress update during agent execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Progress {
    pub state: AgentState,
    pub turn: usize,
    pub message: Option<String>,
    pub tool_name: Option<String>,
    pub usage: TokenUsage,
}

/// Execution result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionResult {
    pub success: bool,
    pub output: String,
    pub turns: usize,
    pub usage: TokenUsage,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<AgentError>,
}

impl ExecutionResult {
    pub fn success(output: impl Into<String>, turns: usize, usage: TokenUsage) -> Self {
        Self {
            success: true,
            output: output.into(),
            turns,
            usage,
            error: None,
        }
    }

    pub fn failure(error: AgentError, turns: usize, usage: TokenUsage) -> Self {
        Self {
            success: false,
            output: String::new(),
            turns,
            usage,
            error: Some(error),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_thread_event_serialization() {
        let event = ThreadEvent::ThreadStarted {
            thread_id: "thread_123".to_string(),
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("\"type\":\"thread_started\""));
    }

    #[test]
    fn test_item_serialization() {
        let item = Item::AgentMessage {
            content: "Hello!".to_string(),
        };
        let json = serde_json::to_string(&item).unwrap();
        assert!(json.contains("\"type\":\"agent_message\""));
    }

    #[test]
    fn test_agent_error_recoverable() {
        assert!(AgentError::ToolError {
            tool: "bash".to_string(),
            message: "command failed".to_string(),
        }
        .is_recoverable());

        assert!(!AgentError::Cancelled.is_recoverable());
    }

    #[test]
    fn test_execution_result() {
        let result = ExecutionResult::success("Done!", 5, TokenUsage::default());
        assert!(result.success);
        assert!(result.error.is_none());

        let failure = ExecutionResult::failure(AgentError::Cancelled, 3, TokenUsage::default());
        assert!(!failure.success);
        assert!(failure.error.is_some());
    }

    #[test]
    fn test_new_event_serialization() {
        // Goal verification events
        let event = ThreadEvent::GoalVerificationStarted {
            goals: vec!["test-coverage".to_string(), "lint".to_string()],
            method: "auto".to_string(),
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("\"type\":\"goal_verification_started\""));
        let parsed: ThreadEvent = serde_json::from_str(&json).unwrap();
        match parsed {
            ThreadEvent::GoalVerificationStarted { goals, method } => {
                assert_eq!(goals.len(), 2);
                assert_eq!(method, "auto");
            }
            _ => panic!("Wrong variant"),
        }

        let event = ThreadEvent::GoalVerificationResult {
            goal: "test-coverage".to_string(),
            score: 85.5,
            target: 80.0,
            passed: true,
            duration_ms: 1234,
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("\"type\":\"goal_verification_result\""));

        let event = ThreadEvent::GoalVerificationCompleted {
            all_passed: true,
            passed_count: 3,
            total_count: 3,
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("\"type\":\"goal_verification_completed\""));

        // Ralph mode events
        let event = ThreadEvent::RalphIterationStarted {
            iteration: 1,
            max_iterations: 10,
            prompt: "Fix all tests".to_string(),
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("\"type\":\"ralph_iteration_started\""));

        let event = ThreadEvent::RalphContinuation {
            reason: "verification_failed".to_string(),
            confidence: 45,
            details: "2 tests still failing".to_string(),
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("\"type\":\"ralph_continuation\""));

        let event = ThreadEvent::RalphCircuitBreak {
            reason: "stagnation".to_string(),
            iteration: 5,
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("\"type\":\"ralph_circuit_break\""));

        // Background task events
        let event = ThreadEvent::BackgroundTaskSpawned {
            task_id: "bg_123".to_string(),
            description: "Running tests".to_string(),
            agent: "executor".to_string(),
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("\"type\":\"background_task_spawned\""));

        let event = ThreadEvent::BackgroundTaskProgress {
            task_id: "bg_123".to_string(),
            status: "running".to_string(),
            message: Some("50% complete".to_string()),
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("\"type\":\"background_task_progress\""));

        let event = ThreadEvent::BackgroundTaskCompleted {
            task_id: "bg_123".to_string(),
            success: true,
            result_preview: Some("All tests passed".to_string()),
            duration_secs: 12.5,
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("\"type\":\"background_task_completed\""));
    }
}
