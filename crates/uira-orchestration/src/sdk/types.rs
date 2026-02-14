//! Core types for Uira SDK
//!
//! Agent-related types are defined in the agents module and re-exported here
//! for backwards compatibility.

use serde::{Deserialize, Serialize};

// Re-export agent types from agents module
pub use crate::agents::{
    AgentCategory, AgentCost, AgentPromptMetadata, DelegationTrigger, ModelType, RoutingTier,
};

/// Agent state during execution
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AgentStatus {
    Idle,
    Running,
    Completed,
    Error,
}

/// State of an active agent
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentState {
    /// Agent name
    pub name: String,
    /// Current status
    pub status: AgentStatus,
    /// Last message from agent
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_message: Option<String>,
    /// Start timestamp (millis since epoch)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_time: Option<u64>,
}

/// Background task state
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TaskStatus {
    Pending,
    Running,
    Completed,
    Error,
}

/// Background task definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackgroundTask {
    /// Task identifier
    pub id: String,
    /// Agent executing the task
    pub agent_name: String,
    /// Task prompt
    pub prompt: String,
    /// Current status
    pub status: TaskStatus,
    /// Result if completed
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<String>,
    /// Error if failed
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Hook event types (matching Claude Code's hook events)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HookEvent {
    PreToolUse,
    PostToolUse,
    Stop,
    SessionStart,
    SessionEnd,
    UserPromptSubmit,
}

impl HookEvent {
    pub fn as_str(&self) -> &'static str {
        match self {
            HookEvent::PreToolUse => "PreToolUse",
            HookEvent::PostToolUse => "PostToolUse",
            HookEvent::Stop => "Stop",
            HookEvent::SessionStart => "SessionStart",
            HookEvent::SessionEnd => "SessionEnd",
            HookEvent::UserPromptSubmit => "UserPromptSubmit",
        }
    }
}

/// Hook context passed to hook handlers
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct HookContext {
    /// Tool name (for tool-related hooks)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_name: Option<String>,
    /// Tool input
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_input: Option<serde_json::Value>,
    /// Tool output
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_output: Option<serde_json::Value>,
    /// Session identifier
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
}

/// Hook result returned by hook handlers
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookResult {
    /// Whether to continue execution
    #[serde(rename = "continue")]
    pub should_continue: bool,
    /// Optional message
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    /// Modified input (for input transformation hooks)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub modified_input: Option<serde_json::Value>,
}

impl Default for HookResult {
    fn default() -> Self {
        Self {
            should_continue: true,
            message: None,
            modified_input: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_model_type_serialize() {
        let model = ModelType::Sonnet;
        let json = serde_json::to_string(&model).unwrap();
        assert_eq!(json, r#""sonnet""#);
    }

    #[test]
    fn test_agent_category_default_model() {
        assert_eq!(AgentCategory::Exploration.default_model(), ModelType::Haiku);
        assert_eq!(AgentCategory::Advisor.default_model(), ModelType::Opus);
        assert_eq!(AgentCategory::Specialist.default_model(), ModelType::Sonnet);
    }

    #[test]
    fn test_hook_result_default() {
        let result = HookResult::default();
        assert!(result.should_continue);
        assert!(result.message.is_none());
    }
}
