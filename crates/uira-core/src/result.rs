use serde::{Deserialize, Serialize};

use crate::PermissionDecision;

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookOutput {
    #[serde(default = "default_true", rename = "continue")]
    pub continue_processing: bool,

    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "stopReason"
    )]
    pub stop_reason: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub decision: Option<PermissionDecision>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,

    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "additionalContext"
    )]
    pub additional_context: Option<String>,

    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "suppressOutput"
    )]
    pub suppress_output: Option<bool>,

    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "systemMessage"
    )]
    pub system_message: Option<String>,
}

impl Default for HookOutput {
    fn default() -> Self {
        Self {
            continue_processing: true,
            stop_reason: None,
            message: None,
            decision: None,
            reason: None,
            additional_context: None,
            suppress_output: None,
            system_message: None,
        }
    }
}

impl HookOutput {
    pub fn allow() -> Self {
        Self::default()
    }

    pub fn deny(reason: &str) -> Self {
        Self {
            continue_processing: false,
            decision: Some(PermissionDecision::Deny),
            reason: Some(reason.to_string()),
            ..Default::default()
        }
    }

    pub fn with_message(message: &str) -> Self {
        Self {
            message: Some(message.to_string()),
            ..Default::default()
        }
    }

    pub fn stop(reason: &str) -> Self {
        Self {
            continue_processing: false,
            stop_reason: Some(reason.to_string()),
            ..Default::default()
        }
    }
}

#[derive(Debug, Clone)]
pub struct HookResult {
    pub success: bool,
    pub should_continue: bool,
    pub message: Option<String>,
    pub output: Option<String>,
}

impl Default for HookResult {
    fn default() -> Self {
        Self {
            success: true,
            should_continue: true,
            message: None,
            output: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hook_output_default() {
        let output = HookOutput::default();
        assert!(output.continue_processing);
        assert!(output.message.is_none());
    }

    #[test]
    fn test_hook_output_with_message() {
        let output = HookOutput::with_message("test message");
        assert!(output.continue_processing);
        assert_eq!(output.message, Some("test message".to_string()));
    }

    #[test]
    fn test_hook_output_deny() {
        let output = HookOutput::deny("not allowed");
        assert!(!output.continue_processing);
        assert!(matches!(output.decision, Some(PermissionDecision::Deny)));
    }

    #[test]
    fn test_serialize_output() {
        let output = HookOutput::with_message("hello");
        let json = serde_json::to_string(&output).unwrap();
        assert!(json.contains("\"continue\":true"));
        assert!(json.contains("\"message\":\"hello\""));
    }
}
