//! Thinking Block Validator Hook
//!
//! Prevents Anthropic extended-thinking models from rejecting assistant messages
//! whose content starts with `tool_use`/`text` without a leading `thinking` block.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::hook::{Hook, HookContext, HookResult};
use crate::types::{HookEvent, HookInput, HookOutput};

pub const HOOK_NAME: &str = "thinking-block-validator";

pub const CONTENT_PART_TYPES: &[&str] = &["tool", "tool_use", "text"];
pub const THINKING_PART_TYPES: &[&str] = &["thinking", "reasoning"];

pub const THINKING_MODEL_PATTERNS: &[&str] = &[
    "thinking",
    "-high",
    "claude-sonnet-4",
    "claude-opus-4",
    "claude-3",
];

pub const DEFAULT_THINKING_CONTENT: &str = "[Continuing from previous reasoning]";
pub const SYNTHETIC_THINKING_ID_PREFIX: &str = "prt_0000000000_synthetic_thinking";
pub const PREVENTED_ERROR: &str = "Expected thinking/redacted_thinking but found tool_use";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessagePart {
    #[serde(rename = "type")]
    pub part_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(rename = "sessionID", skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    #[serde(rename = "messageID", skip_serializing_if = "Option::is_none")]
    pub message_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thinking: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub synthetic: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageInfo {
    pub id: String,
    pub role: String,
    #[serde(rename = "sessionID", skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    #[serde(rename = "modelID", skip_serializing_if = "Option::is_none")]
    pub model_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageWithParts {
    pub info: MessageInfo,
    pub parts: Vec<MessagePart>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationResult {
    pub valid: bool,
    pub fixed: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub issue: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub action: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ValidationStats {
    pub total: usize,
    pub valid: usize,
    pub fixed: usize,
    pub issues: usize,
}

fn is_content_part_type(t: &str) -> bool {
    CONTENT_PART_TYPES.contains(&t)
}

fn is_thinking_part_type(t: &str) -> bool {
    THINKING_PART_TYPES.contains(&t)
}

pub fn is_extended_thinking_model(model_id: &str) -> bool {
    if model_id.is_empty() {
        return false;
    }
    let lower = model_id.to_lowercase();
    if lower.contains("thinking") || lower.ends_with("-high") {
        return true;
    }
    lower.contains("claude-sonnet-4")
        || lower.contains("claude-opus-4")
        || lower.contains("claude-3")
}

pub fn has_content_parts(parts: &[MessagePart]) -> bool {
    parts.iter().any(|p| is_content_part_type(&p.part_type))
}

pub fn starts_with_thinking_block(parts: &[MessagePart]) -> bool {
    parts
        .first()
        .map(|p| is_thinking_part_type(&p.part_type))
        .unwrap_or(false)
}

pub fn find_previous_thinking_content(
    messages: &[MessageWithParts],
    current_index: usize,
) -> String {
    if current_index == 0 {
        return String::new();
    }

    for msg in messages[..current_index].iter().rev() {
        if msg.info.role != "assistant" {
            continue;
        }
        for part in &msg.parts {
            if !is_thinking_part_type(&part.part_type) {
                continue;
            }
            let thinking = part
                .thinking
                .as_deref()
                .or(part.text.as_deref())
                .unwrap_or("");
            if !thinking.trim().is_empty() {
                return thinking.to_string();
            }
        }
    }

    String::new()
}

pub fn prepend_thinking_block(message: &mut MessageWithParts, thinking_content: &str) {
    let thinking_part = MessagePart {
        part_type: "thinking".to_string(),
        id: Some(SYNTHETIC_THINKING_ID_PREFIX.to_string()),
        session_id: message.info.session_id.clone().or(Some(String::new())),
        message_id: Some(message.info.id.clone()),
        thinking: Some(thinking_content.to_string()),
        text: None,
        synthetic: Some(true),
    };

    message.parts.insert(0, thinking_part);
}

pub fn validate_message(
    message: &mut MessageWithParts,
    messages: &[MessageWithParts],
    index: usize,
    model_id: &str,
) -> ValidationResult {
    if message.info.role != "assistant" {
        return ValidationResult {
            valid: true,
            fixed: false,
            issue: None,
            action: None,
        };
    }

    if !is_extended_thinking_model(model_id) {
        return ValidationResult {
            valid: true,
            fixed: false,
            issue: None,
            action: None,
        };
    }

    if has_content_parts(&message.parts) && !starts_with_thinking_block(&message.parts) {
        let prev = find_previous_thinking_content(messages, index);
        let content = if prev.is_empty() {
            DEFAULT_THINKING_CONTENT
        } else {
            prev.as_str()
        };

        prepend_thinking_block(message, content);

        return ValidationResult {
            valid: false,
            fixed: true,
            issue: Some("Assistant message has content but no thinking block".to_string()),
            action: Some(format!(
                "Prepended synthetic thinking block: \"{}...\"",
                content.chars().take(50).collect::<String>()
            )),
        };
    }

    ValidationResult {
        valid: true,
        fixed: false,
        issue: None,
        action: None,
    }
}

pub fn validate_messages(
    messages: &mut [MessageWithParts],
    model_id: &str,
) -> Vec<ValidationResult> {
    let snapshot = messages.to_vec();
    let mut results = Vec::with_capacity(messages.len());
    for (i, msg) in messages.iter_mut().enumerate() {
        let res = validate_message(msg, &snapshot, i, model_id);
        results.push(res);
    }
    results
}

pub fn get_validation_stats(results: &[ValidationResult]) -> ValidationStats {
    ValidationStats {
        total: results.len(),
        valid: results.iter().filter(|r| r.valid && !r.fixed).count(),
        fixed: results.iter().filter(|r| r.fixed).count(),
        issues: results.iter().filter(|r| !r.valid).count(),
    }
}

fn extract_model_id_from_messages(messages: &[MessageWithParts]) -> String {
    for msg in messages.iter().rev() {
        if msg.info.role == "user" {
            return msg.info.model_id.clone().unwrap_or_default();
        }
    }
    String::new()
}

fn parse_messages_from_input(input: &HookInput) -> Option<Vec<MessageWithParts>> {
    // Prefer tool_input (transform payload), fallback to extra["messages"].
    let v = input
        .tool_input
        .as_ref()
        .or_else(|| input.extra.get("messages"));
    let v = v?;

    if v.is_array() {
        serde_json::from_value::<Vec<MessageWithParts>>(v.clone()).ok()
    } else if v.is_object() {
        v.get("messages")
            .and_then(|m| serde_json::from_value::<Vec<MessageWithParts>>(m.clone()).ok())
    } else {
        None
    }
}

fn build_modified_input(
    original: &serde_json::Value,
    messages: &[MessageWithParts],
) -> serde_json::Value {
    if original.is_array() {
        serde_json::to_value(messages).unwrap_or(serde_json::Value::Null)
    } else if original.is_object() {
        let mut obj = original.as_object().cloned().unwrap_or_default();
        obj.insert(
            "messages".to_string(),
            serde_json::to_value(messages).unwrap_or(serde_json::Value::Null),
        );
        serde_json::Value::Object(obj)
    } else {
        serde_json::json!({ "messages": messages })
    }
}

#[derive(Debug, Clone, Default)]
pub struct ThinkingBlockValidatorHook;

impl ThinkingBlockValidatorHook {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Hook for ThinkingBlockValidatorHook {
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
        let Some(mut messages) = parse_messages_from_input(input) else {
            return Ok(HookOutput::pass());
        };

        let model_id = extract_model_id_from_messages(&messages);
        if !is_extended_thinking_model(&model_id) {
            return Ok(HookOutput::pass());
        }

        let mut fixed = 0usize;
        for i in 0..messages.len() {
            if messages[i].info.role != "assistant" {
                continue;
            }
            if has_content_parts(&messages[i].parts)
                && !starts_with_thinking_block(&messages[i].parts)
            {
                let prev = find_previous_thinking_content(&messages, i);
                let content = if prev.is_empty() {
                    DEFAULT_THINKING_CONTENT
                } else {
                    prev.as_str()
                };
                prepend_thinking_block(&mut messages[i], content);
                fixed += 1;
            }
        }

        if fixed == 0 {
            return Ok(HookOutput::pass());
        }

        let original = input
            .tool_input
            .as_ref()
            .or_else(|| input.extra.get("messages"))
            .cloned()
            .unwrap_or_else(|| serde_json::json!({ "messages": [] }));

        Ok(HookOutput {
            should_continue: true,
            message: None,
            reason: None,
            modified_input: Some(build_modified_input(&original, &messages)),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_extended_thinking_model() {
        assert!(is_extended_thinking_model("claude-sonnet-4"));
        assert!(is_extended_thinking_model("claude-sonnet-4-high"));
        assert!(is_extended_thinking_model("something-thinking"));
        assert!(!is_extended_thinking_model("gpt-4"));
    }

    #[test]
    fn test_validate_message_prepends_thinking() {
        let mut messages = vec![
            MessageWithParts {
                info: MessageInfo {
                    id: "u1".to_string(),
                    role: "user".to_string(),
                    session_id: Some("s".to_string()),
                    model_id: Some("claude-sonnet-4".to_string()),
                },
                parts: vec![MessagePart {
                    part_type: "text".to_string(),
                    id: None,
                    session_id: None,
                    message_id: None,
                    thinking: None,
                    text: Some("hi".to_string()),
                    synthetic: None,
                }],
            },
            MessageWithParts {
                info: MessageInfo {
                    id: "a1".to_string(),
                    role: "assistant".to_string(),
                    session_id: Some("s".to_string()),
                    model_id: None,
                },
                parts: vec![MessagePart {
                    part_type: "tool_use".to_string(),
                    id: Some("t1".to_string()),
                    session_id: None,
                    message_id: None,
                    thinking: None,
                    text: None,
                    synthetic: None,
                }],
            },
        ];

        let model_id = extract_model_id_from_messages(&messages);
        let results = validate_messages(&mut messages, &model_id);
        let stats = get_validation_stats(&results);

        assert_eq!(stats.fixed, 1);
        assert_eq!(messages[1].parts[0].part_type, "thinking");
        assert!(messages[1].parts[0].synthetic.unwrap_or(false));
    }

    #[test]
    fn test_find_previous_thinking_content() {
        let messages = vec![
            MessageWithParts {
                info: MessageInfo {
                    id: "a0".to_string(),
                    role: "assistant".to_string(),
                    session_id: None,
                    model_id: None,
                },
                parts: vec![MessagePart {
                    part_type: "thinking".to_string(),
                    id: None,
                    session_id: None,
                    message_id: None,
                    thinking: Some("prev".to_string()),
                    text: None,
                    synthetic: None,
                }],
            },
            MessageWithParts {
                info: MessageInfo {
                    id: "a1".to_string(),
                    role: "assistant".to_string(),
                    session_id: None,
                    model_id: None,
                },
                parts: vec![],
            },
        ];

        assert_eq!(find_previous_thinking_content(&messages, 1), "prev");
    }
}
