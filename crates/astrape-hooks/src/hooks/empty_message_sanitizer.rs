//! Empty Message Sanitizer Hook
//!
//! Port of oh-my-claudecode's `src/hooks/empty-message-sanitizer/*`.
//!
//! Ensures every message (except the optional final assistant message) has
//! non-empty content to avoid Anthropic API validation errors.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

use crate::hook::{Hook, HookContext, HookResult};
use crate::types::{HookEvent, HookInput, HookOutput};

pub const PLACEHOLDER_TEXT: &str = "[user interrupted]";

pub const HOOK_NAME: &str = "empty-message-sanitizer";
pub const DEBUG_PREFIX: &str = "[empty-message-sanitizer]";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorPatterns {
    pub empty_content: String,
    pub empty_text: String,
    pub no_valid_parts: String,
}

pub fn error_patterns() -> ErrorPatterns {
    ErrorPatterns {
        empty_content: "all messages must have non-empty content".to_string(),
        empty_text: "message contains empty text part".to_string(),
        no_valid_parts: "message has no valid content parts".to_string(),
    }
}

fn tool_part_types() -> HashSet<&'static str> {
    ["tool", "tool_use", "tool_result"].into_iter().collect()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessagePart {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(rename = "messageID", skip_serializing_if = "Option::is_none")]
    pub message_id: Option<String>,
    #[serde(rename = "sessionID", skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    #[serde(rename = "type")]
    pub part_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub synthetic: Option<bool>,
    #[serde(flatten)]
    pub extra: std::collections::HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageInfo {
    pub id: String,
    pub role: String,
    #[serde(rename = "sessionID", skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    #[serde(flatten)]
    pub extra: std::collections::HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageWithParts {
    pub info: MessageInfo,
    pub parts: Vec<MessagePart>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmptyMessageSanitizerInput {
    pub messages: Vec<MessageWithParts>,
    #[serde(rename = "sessionId", skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmptyMessageSanitizerOutput {
    pub messages: Vec<MessageWithParts>,
    #[serde(rename = "sanitizedCount")]
    pub sanitized_count: usize,
    pub modified: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct EmptyMessageSanitizerConfig {
    #[serde(rename = "placeholderText", skip_serializing_if = "Option::is_none")]
    pub placeholder_text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub debug: Option<bool>,
}

pub fn has_text_content(part: &MessagePart) -> bool {
    if part.part_type == "text" {
        return part
            .text
            .as_deref()
            .map(|t| !t.trim().is_empty())
            .unwrap_or(false);
    }
    false
}

pub fn is_tool_part(part: &MessagePart) -> bool {
    tool_part_types().contains(part.part_type.as_str())
}

pub fn has_valid_content(parts: &[MessagePart]) -> bool {
    parts.iter().any(|p| has_text_content(p) || is_tool_part(p))
}

pub fn sanitize_message(
    message: &mut MessageWithParts,
    is_last_message: bool,
    placeholder_text: &str,
) -> bool {
    let is_assistant = message.info.role == "assistant";
    if is_last_message && is_assistant {
        return false;
    }

    // If no valid content at all, inject/replace.
    if !has_valid_content(&message.parts) {
        // Try to replace an existing empty text part first.
        for part in message.parts.iter_mut() {
            if part.part_type == "text" {
                let empty = part.text.as_deref().map(|t| t.trim().is_empty()).unwrap_or(true);
                if empty {
                    part.text = Some(placeholder_text.to_string());
                    part.synthetic = Some(true);
                    return true;
                }
            }
        }

        // Otherwise inject a new text part before first tool part.
        let insert_index = message.parts.iter().position(is_tool_part);
        let new_part = MessagePart {
            id: Some(format!("synthetic_{}", crate::hooks::recovery::generate_part_id())),
            message_id: Some(message.info.id.clone()),
            session_id: message.info.session_id.clone().or_else(|| Some(String::new())),
            part_type: "text".to_string(),
            text: Some(placeholder_text.to_string()),
            synthetic: Some(true),
            extra: std::collections::HashMap::new(),
        };

        match insert_index {
            Some(idx) => message.parts.insert(idx, new_part),
            None => message.parts.push(new_part),
        }

        return true;
    }

    // Also sanitize any empty text parts that exist alongside valid content.
    let mut sanitized = false;
    for part in message.parts.iter_mut() {
        if part.part_type == "text" {
            if part.text.as_deref().map(|t| t.trim() == "").unwrap_or(false) {
                part.text = Some(placeholder_text.to_string());
                part.synthetic = Some(true);
                sanitized = true;
            }
        }
    }

    sanitized
}

pub fn sanitize_messages(
    mut input: EmptyMessageSanitizerInput,
    config: Option<&EmptyMessageSanitizerConfig>,
) -> EmptyMessageSanitizerOutput {
    let placeholder_text = config
        .and_then(|c| c.placeholder_text.as_deref())
        .unwrap_or(PLACEHOLDER_TEXT);

    let mut sanitized_count = 0usize;
    for i in 0..input.messages.len() {
        let is_last = i == input.messages.len().saturating_sub(1);
        if sanitize_message(&mut input.messages[i], is_last, placeholder_text) {
            sanitized_count += 1;
        }
    }

    EmptyMessageSanitizerOutput {
        messages: input.messages,
        sanitized_count,
        modified: sanitized_count > 0,
    }
}

fn parse_messages_from_input(input: &HookInput) -> Option<(serde_json::Value, Vec<MessageWithParts>)> {
    let v = input
        .tool_input
        .as_ref()
        .or_else(|| input.extra.get("messages"))?;

    if v.is_array() {
        let msgs = serde_json::from_value::<Vec<MessageWithParts>>(v.clone()).ok()?;
        return Some((v.clone(), msgs));
    }

    if v.is_object() {
        let msgs_val = v.get("messages")?.clone();
        let msgs = serde_json::from_value::<Vec<MessageWithParts>>(msgs_val).ok()?;
        return Some((v.clone(), msgs));
    }

    None
}

fn build_modified_input(original: &serde_json::Value, messages: &[MessageWithParts]) -> serde_json::Value {
    if original.is_array() {
        serde_json::to_value(messages).unwrap_or_else(|_| serde_json::Value::Null)
    } else if original.is_object() {
        let mut obj = original.as_object().cloned().unwrap_or_default();
        obj.insert(
            "messages".to_string(),
            serde_json::to_value(messages).unwrap_or_else(|_| serde_json::Value::Null),
        );
        serde_json::Value::Object(obj)
    } else {
        serde_json::json!({ "messages": messages })
    }
}

#[derive(Debug, Clone, Default)]
pub struct EmptyMessageSanitizerHook {
    config: EmptyMessageSanitizerConfig,
}

impl EmptyMessageSanitizerHook {
    pub fn new() -> Self {
        Self {
            config: EmptyMessageSanitizerConfig::default(),
        }
    }

    pub fn with_config(config: EmptyMessageSanitizerConfig) -> Self {
        Self { config }
    }
}

#[async_trait]
impl Hook for EmptyMessageSanitizerHook {
    fn name(&self) -> &str {
        HOOK_NAME
    }

    fn events(&self) -> &[HookEvent] {
        &[HookEvent::MessagesTransform]
    }

    async fn execute(
        &self,
        _event: HookEvent,
        input: &HookInput,
        _context: &HookContext,
    ) -> HookResult {
        let Some((original, messages)) = parse_messages_from_input(input) else {
            return Ok(HookOutput::pass());
        };

        let out = sanitize_messages(
            EmptyMessageSanitizerInput {
                messages,
                session_id: input.session_id.clone(),
            },
            Some(&self.config),
        );

        if !out.modified {
            return Ok(HookOutput::pass());
        }

        Ok(HookOutput {
            should_continue: true,
            message: None,
            reason: None,
            modified_input: Some(build_modified_input(&original, &out.messages)),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn msg(id: &str, role: &str, parts: Vec<MessagePart>) -> MessageWithParts {
        MessageWithParts {
            info: MessageInfo {
                id: id.to_string(),
                role: role.to_string(),
                session_id: Some("s".to_string()),
                extra: std::collections::HashMap::new(),
            },
            parts,
        }
    }

    #[test]
    fn test_sanitize_injects_for_empty_parts() {
        let mut m = msg("a1", "assistant", vec![]);
        let changed = sanitize_message(&mut m, false, PLACEHOLDER_TEXT);
        assert!(changed);
        assert!(has_valid_content(&m.parts));
        assert_eq!(m.parts[0].part_type, "text");
        assert_eq!(m.parts[0].text.as_deref(), Some(PLACEHOLDER_TEXT));
    }

    #[test]
    fn test_sanitize_skips_final_assistant() {
        let mut m = msg("a_last", "assistant", vec![]);
        let changed = sanitize_message(&mut m, true, PLACEHOLDER_TEXT);
        assert!(!changed);
        assert!(m.parts.is_empty());
    }

    #[test]
    fn test_sanitize_replaces_empty_text_part_even_with_tool() {
        let mut m = msg(
            "u1",
            "user",
            vec![
                MessagePart {
                    id: Some("p1".to_string()),
                    message_id: None,
                    session_id: None,
                    part_type: "text".to_string(),
                    text: Some("".to_string()),
                    synthetic: None,
                    extra: std::collections::HashMap::new(),
                },
                MessagePart {
                    id: Some("p2".to_string()),
                    message_id: None,
                    session_id: None,
                    part_type: "tool_use".to_string(),
                    text: None,
                    synthetic: None,
                    extra: std::collections::HashMap::new(),
                },
            ],
        );

        assert!(sanitize_message(&mut m, false, PLACEHOLDER_TEXT));
        assert_eq!(m.parts[0].text.as_deref(), Some(PLACEHOLDER_TEXT));
        assert!(m.parts[0].synthetic.unwrap_or(false));
    }
}
