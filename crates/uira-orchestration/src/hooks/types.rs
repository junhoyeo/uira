use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum HookEvent {
    UserPromptSubmit,
    Stop,
    SessionStart,
    PreToolUse,
    PostToolUse,
    SessionIdle,
    MessagesTransform,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum HookType {
    KeywordDetector,
    StopContinuation,
    Ralph,
    PersistentMode,
    SessionStart,
    PreToolUse,
    PostToolUse,
    Autopilot,
    ThinkMode,
    RulesInjector,
    CommentChecker,
    DelegationEnforcer,
    Recovery,
    PreemptiveCompaction,
    BackgroundNotification,
    DirectoryReadmeInjector,
    EmptyMessageSanitizer,
    ThinkingBlockValidator,
    NonInteractiveEnv,
    AgentUsageReminder,
    Ultrawork,
    UltraQA,
    Notepad,
    Learner,
    Ultrapilot,
    UiraOrchestrator,
    PluginPatterns,
    TodoContinuation,
}

impl HookType {
    pub fn name(&self) -> &'static str {
        match self {
            Self::KeywordDetector => "keyword-detector",
            Self::StopContinuation => "stop-continuation",
            Self::Ralph => "ralph",
            Self::PersistentMode => "persistent-mode",
            Self::SessionStart => "session-start",
            Self::PreToolUse => "pre-tool-use",
            Self::PostToolUse => "post-tool-use",
            Self::Autopilot => "autopilot",
            Self::ThinkMode => "think-mode",
            Self::RulesInjector => "rules-injector",
            Self::CommentChecker => "comment-checker",
            Self::DelegationEnforcer => "delegation-enforcer",
            Self::Recovery => "recovery",
            Self::PreemptiveCompaction => "preemptive-compaction",
            Self::BackgroundNotification => "background-notification",
            Self::DirectoryReadmeInjector => "directory-readme-injector",
            Self::EmptyMessageSanitizer => "empty-message-sanitizer",
            Self::ThinkingBlockValidator => "thinking-block-validator",
            Self::NonInteractiveEnv => "non-interactive-env",
            Self::AgentUsageReminder => "agent-usage-reminder",
            Self::Ultrawork => "ultrawork",
            Self::UltraQA => "ultraqa",
            Self::Notepad => "notepad",
            Self::Learner => "learner",
            Self::Ultrapilot => "ultrapilot",
            Self::UiraOrchestrator => "uira-orchestrator",
            Self::PluginPatterns => "plugin-patterns",
            Self::TodoContinuation => "todo-continuation",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessagePart {
    #[serde(rename = "type")]
    pub part_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookInput {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<Message>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub parts: Option<Vec<MessagePart>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_name: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_input: Option<serde_json::Value>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_output: Option<serde_json::Value>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub directory: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_reason: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_requested: Option<bool>,

    /// Path to transcript JSONL file for accessing conversation history
    /// This is passed from StopInput which already has this field in uira-core
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transcript_path: Option<String>,

    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

impl HookInput {
    pub fn get_prompt_text(&self) -> String {
        if let Some(prompt) = &self.prompt {
            return prompt.clone();
        }
        if let Some(message) = &self.message {
            if let Some(content) = &message.content {
                return content.clone();
            }
        }
        if let Some(parts) = &self.parts {
            return parts
                .iter()
                .filter(|p| p.part_type == "text")
                .filter_map(|p| p.text.as_ref())
                .cloned()
                .collect::<Vec<_>>()
                .join(" ");
        }
        String::new()
    }

    pub fn get_directory(&self) -> String {
        self.directory.clone().unwrap_or_else(|| {
            std::env::current_dir()
                .unwrap()
                .to_string_lossy()
                .to_string()
        })
    }

    /// Get the last assistant text response from transcript JSONL
    ///
    /// Parses the transcript file (if available) and extracts the last
    /// assistant message's text content.
    pub fn get_last_assistant_response(&self) -> Option<String> {
        let transcript_path = self.transcript_path.as_ref()?;
        let path = std::path::Path::new(transcript_path);
        if !path.exists() {
            return None;
        }

        let content = std::fs::read_to_string(path).ok()?;

        // Parse JSONL from end to find last assistant message
        for line in content.lines().rev() {
            if line.trim().is_empty() {
                continue;
            }
            if let Ok(entry) = serde_json::from_str::<serde_json::Value>(line) {
                if entry.get("type").and_then(|v| v.as_str()) == Some("progress") {
                    if let Some(data) = entry.get("data") {
                        if let Some(msg) = data.get("message") {
                            if msg.get("type").and_then(|v| v.as_str()) == Some("assistant") {
                                if let Some(content) = msg
                                    .get("message")
                                    .and_then(|m| m.get("content"))
                                    .and_then(|c| c.as_array())
                                {
                                    let texts: Vec<&str> = content
                                        .iter()
                                        .filter_map(|item| {
                                            if item.get("type").and_then(|t| t.as_str())
                                                == Some("text")
                                            {
                                                item.get("text").and_then(|t| t.as_str())
                                            } else {
                                                None
                                            }
                                        })
                                        .collect();

                                    if !texts.is_empty() {
                                        return Some(texts.join("\n"));
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        None
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookOutput {
    #[serde(rename = "continue")]
    pub should_continue: bool,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub modified_input: Option<serde_json::Value>,
}

impl HookOutput {
    pub fn continue_with_message(message: impl Into<String>) -> Self {
        Self {
            should_continue: true,
            message: Some(message.into()),
            reason: None,
            modified_input: None,
        }
    }

    pub fn block_with_reason(reason: impl Into<String>) -> Self {
        Self {
            should_continue: false,
            message: None,
            reason: Some(reason.into()),
            modified_input: None,
        }
    }

    pub fn pass() -> Self {
        Self {
            should_continue: true,
            message: None,
            reason: None,
            modified_input: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hook_input_get_prompt_text_from_prompt() {
        let input = HookInput {
            session_id: None,
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

        assert_eq!(input.get_prompt_text(), "test prompt");
    }

    #[test]
    fn test_hook_input_get_prompt_text_from_message() {
        let input = HookInput {
            session_id: None,
            prompt: None,
            message: Some(Message {
                content: Some("message content".to_string()),
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
        };

        assert_eq!(input.get_prompt_text(), "message content");
    }

    #[test]
    fn test_hook_input_get_prompt_text_from_parts() {
        let input = HookInput {
            session_id: None,
            prompt: None,
            message: None,
            parts: Some(vec![
                MessagePart {
                    part_type: "text".to_string(),
                    text: Some("part 1".to_string()),
                },
                MessagePart {
                    part_type: "text".to_string(),
                    text: Some("part 2".to_string()),
                },
            ]),
            tool_name: None,
            tool_input: None,
            tool_output: None,
            directory: None,
            stop_reason: None,
            user_requested: None,
            transcript_path: None,
            extra: HashMap::new(),
        };

        assert_eq!(input.get_prompt_text(), "part 1 part 2");
    }

    #[test]
    fn test_hook_output_continue_with_message() {
        let output = HookOutput::continue_with_message("test message");
        assert!(output.should_continue);
        assert_eq!(output.message, Some("test message".to_string()));
        assert!(output.reason.is_none());
    }

    #[test]
    fn test_hook_output_block_with_reason() {
        let output = HookOutput::block_with_reason("blocked");
        assert!(!output.should_continue);
        assert_eq!(output.reason, Some("blocked".to_string()));
        assert!(output.message.is_none());
    }

    #[test]
    fn test_hook_output_pass() {
        let output = HookOutput::pass();
        assert!(output.should_continue);
        assert!(output.message.is_none());
        assert!(output.reason.is_none());
    }

    #[test]
    fn test_get_last_assistant_response_no_transcript() {
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
            transcript_path: None,
            extra: HashMap::new(),
        };

        assert!(input.get_last_assistant_response().is_none());
    }

    #[test]
    fn test_get_last_assistant_response_missing_file() {
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
            transcript_path: Some("/nonexistent/path/transcript.jsonl".to_string()),
            extra: HashMap::new(),
        };

        assert!(input.get_last_assistant_response().is_none());
    }
}
