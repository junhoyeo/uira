//! Anthropic <-> LiteLLM translation.
//!
//! This module implements the core format conversions:
//! - Anthropic `/v1/messages` request -> OpenAI/LiteLLM `chat/completions` JSON
//! - OpenAI/LiteLLM `chat/completions` response -> Anthropic `/v1/messages`

use crate::types::{ContentBlock, MessagesRequest, MessagesResponse, Role, ToolChoice, Usage};
use anyhow::{Context, Result};
use serde_json::{json, Value};

/// Maximum token limit used to keep OpenAI/Gemini backends from rejecting overly
/// large `max_tokens` values.
pub const OPENAI_COMPAT_MAX_TOKENS_CAP: u32 = 16_384;

/// Convert an Anthropic Messages request into an OpenAI/LiteLLM
/// `chat/completions` request payload.
pub fn convert_anthropic_to_litellm(req: &MessagesRequest) -> Result<Value> {
    let mut out_messages: Vec<Value> = Vec::new();

    if let Some(system) = &req.system {
        let system_text = system.to_plaintext();
        if !system_text.is_empty() {
            out_messages.push(json!({"role": "system", "content": system_text}));
        }
    }

    for msg in &req.messages {
        let role_str = match msg.role {
            Role::User => "user",
            Role::Assistant => "assistant",
        };

        // Tool results are represented as separate `role: tool` messages in
        // OpenAI format.
        let mut pending_user_parts: Vec<Value> = Vec::new();
        let mut pending_user_text = String::new();

        let blocks = msg.content.as_blocks();
        let mut tool_calls: Vec<Value> = Vec::new();

        for block in blocks {
            match block {
                ContentBlock::Text { text } => {
                    if pending_user_parts.is_empty() {
                        pending_user_text.push_str(&text);
                    } else {
                        pending_user_parts.push(json!({"type": "text", "text": text}));
                    }
                }
                ContentBlock::Image { source } => {
                    // Switch to multi-part content.
                    if pending_user_parts.is_empty() && !pending_user_text.is_empty() {
                        pending_user_parts.push(json!({"type": "text", "text": pending_user_text}));
                        pending_user_text = String::new();
                    }
                    let url = format!("data:{};base64,{}", source.media_type, source.data);
                    pending_user_parts
                        .push(json!({"type": "image_url", "image_url": {"url": url}}));
                }
                ContentBlock::ToolUse { id, name, input } => {
                    // Only meaningful for assistant messages.
                    let args = serde_json::to_string(&input)
                        .with_context(|| "failed to serialize tool_use input")?;
                    tool_calls.push(json!({
                        "id": id,
                        "type": "function",
                        "function": {"name": name, "arguments": args}
                    }));
                }
                ContentBlock::ToolResult {
                    tool_use_id,
                    content,
                    ..
                } => {
                    // Flush any pending user content first.
                    if msg.role == Role::User {
                        if !pending_user_parts.is_empty() {
                            out_messages
                                .push(json!({"role": role_str, "content": pending_user_parts}));
                            pending_user_parts = Vec::new();
                        } else if !pending_user_text.is_empty() {
                            out_messages
                                .push(json!({"role": role_str, "content": pending_user_text}));
                            pending_user_text = String::new();
                        }
                    }

                    out_messages.push(json!({
                        "role": "tool",
                        "tool_call_id": tool_use_id,
                        "content": content.to_plaintext(),
                    }));
                }
            }
        }

        // Flush remaining content for the message.
        let content_val = if !pending_user_parts.is_empty() {
            Value::Array(pending_user_parts)
        } else if !pending_user_text.is_empty() {
            Value::String(pending_user_text)
        } else {
            Value::Null
        };

        let mut msg_obj = json!({"role": role_str});
        if content_val != Value::Null {
            msg_obj
                .as_object_mut()
                .expect("json object")
                .insert("content".to_string(), content_val);
        }
        if !tool_calls.is_empty() {
            msg_obj
                .as_object_mut()
                .expect("json object")
                .insert("tool_calls".to_string(), Value::Array(tool_calls));
        }

        // Avoid emitting empty assistant messages unless they carry tool calls.
        let has_content = msg_obj.get("content").is_some();
        let has_tool_calls = msg_obj.get("tool_calls").is_some();
        if has_content || has_tool_calls {
            out_messages.push(msg_obj);
        }
    }

    let is_openai_compat = req.model.starts_with("openai/") || req.model.starts_with("gemini/");
    let max_tokens = if is_openai_compat {
        req.max_tokens.min(OPENAI_COMPAT_MAX_TOKENS_CAP)
    } else {
        req.max_tokens
    };

    let mut out = json!({
        "model": req.model,
        "messages": out_messages,
        "max_tokens": max_tokens,
    });

    if let Some(stream) = req.stream {
        out.as_object_mut()
            .expect("json object")
            .insert("stream".to_string(), Value::Bool(stream));
        if stream {
            out.as_object_mut()
                .expect("json object")
                .insert("stream_options".to_string(), json!({"include_usage": true}));
        }
    }
    if let Some(t) = req.temperature {
        out["temperature"] = json!(t);
    }
    if let Some(tp) = req.top_p {
        out["top_p"] = json!(tp);
    }
    if let Some(ss) = &req.stop_sequences {
        out["stop"] = json!(ss);
    }

    // Tools
    if let Some(tools) = &req.tools {
        let mapped_tools: Vec<Value> = tools
            .iter()
            .map(|t| {
                let mut schema = t.input_schema.clone();
                if req.model.starts_with("gemini/") {
                    clean_gemini_schema(&mut schema);
                }
                json!({
                    "type": "function",
                    "function": {
                        "name": t.name,
                        "description": t.description,
                        "parameters": schema,
                    }
                })
            })
            .collect();
        out["tools"] = Value::Array(mapped_tools);
    }

    if let Some(choice) = &req.tool_choice {
        out["tool_choice"] = match choice {
            ToolChoice::Auto => json!("auto"),
            ToolChoice::Any => json!("required"),
            ToolChoice::Tool { name } => json!({
                "type": "function",
                "function": {"name": name}
            }),
        };
    }

    Ok(out)
}

/// Convert a LiteLLM/OpenAI chat completion response into an Anthropic
/// Messages response.
pub fn convert_litellm_to_anthropic(
    resp: Value,
    _original_req: &MessagesRequest,
) -> Result<MessagesResponse> {
    let id = resp
        .get("id")
        .and_then(|v| v.as_str())
        .unwrap_or("msg_unknown")
        .to_string();

    let model = resp
        .get("model")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
        .to_string();

    let choice = resp
        .get("choices")
        .and_then(|v| v.as_array())
        .and_then(|arr| arr.first())
        .context("missing choices[0]")?;

    let finish_reason = choice
        .get("finish_reason")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let msg = choice
        .get("message")
        .context("missing choices[0].message")?;
    let content = msg.get("content");
    let tool_calls = msg.get("tool_calls");

    let mut out_blocks: Vec<ContentBlock> = Vec::new();

    if let Some(c) = content {
        if let Some(s) = c.as_str() {
            if !s.is_empty() {
                out_blocks.push(ContentBlock::Text {
                    text: s.to_string(),
                });
            }
        }
    }

    if let Some(tc) = tool_calls.and_then(|v| v.as_array()) {
        for call in tc {
            let id = call
                .get("id")
                .and_then(|v| v.as_str())
                .unwrap_or("toolcall_unknown")
                .to_string();
            let func = call
                .get("function")
                .context("tool_calls[].function missing")?;
            let name = func
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown")
                .to_string();
            let args_str = func
                .get("arguments")
                .and_then(|v| v.as_str())
                .unwrap_or("{}");
            let input: Value =
                serde_json::from_str(args_str).unwrap_or_else(|_| json!({"raw": args_str}));

            out_blocks.push(ContentBlock::ToolUse { id, name, input });
        }
    }

    let (input_tokens, output_tokens) = resp
        .get("usage")
        .and_then(|u| u.as_object())
        .map(|u| {
            let prompt = u.get("prompt_tokens").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
            let completion = u
                .get("completion_tokens")
                .and_then(|v| v.as_u64())
                .unwrap_or(0) as u32;
            (prompt, completion)
        })
        .unwrap_or((0, 0));

    Ok(MessagesResponse {
        id,
        kind: "message".to_string(),
        role: Role::Assistant,
        content: out_blocks,
        model,
        stop_reason: finish_reason.map(map_openai_finish_reason),
        stop_sequence: None,
        usage: Usage {
            input_tokens,
            output_tokens,
        },
    })
}

/// Remove schema features that Gemini's tool schema parser tends to reject.
///
/// LiteLLM forwards `tools[].function.parameters` to the provider. Gemini is
/// stricter than OpenAI and commonly rejects some JSON Schema keys.
pub fn clean_gemini_schema(schema: &mut Value) {
    let Some(obj) = schema.as_object_mut() else {
        return;
    };

    // Keys that frequently cause provider-side schema validation failures.
    const STRIP_KEYS: &[&str] = &[
        "$schema",
        "title",
        "default",
        "examples",
        "additionalProperties",
        "patternProperties",
    ];

    for k in STRIP_KEYS {
        obj.remove(*k);
    }

    for (_k, v) in obj.iter_mut() {
        match v {
            Value::Object(_) => clean_gemini_schema(v),
            Value::Array(arr) => {
                for item in arr {
                    clean_gemini_schema(item);
                }
            }
            _ => {}
        }
    }
}

fn map_openai_finish_reason(reason: String) -> String {
    match reason.as_str() {
        "stop" => "end_turn".to_string(),
        "length" => "max_tokens".to_string(),
        "tool_calls" => "tool_use".to_string(),
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{Content, Message, SystemContent};

    #[test]
    fn anthropic_to_litellm_includes_system_as_first_message() {
        let req = MessagesRequest {
            model: "openai/gpt-4.1-mini".to_string(),
            system: Some(SystemContent::String("sys".to_string())),
            messages: vec![Message {
                role: Role::User,
                content: Content::String("hi".to_string()),
            }],
            max_tokens: 10,
            stream: None,
            temperature: None,
            top_p: None,
            top_k: None,
            stop_sequences: None,
            tools: None,
            tool_choice: None,
            thinking: None,
            metadata: None,
        };

        let out = convert_anthropic_to_litellm(&req).unwrap();
        let msgs = out.get("messages").unwrap().as_array().unwrap();
        assert_eq!(msgs[0].get("role").unwrap(), "system");
        assert_eq!(msgs[0].get("content").unwrap(), "sys");
    }

    #[test]
    fn litellm_to_anthropic_converts_tool_calls() {
        let resp = json!({
            "id": "chatcmpl_123",
            "model": "openai/gpt-4.1-mini",
            "choices": [{
                "finish_reason": "tool_calls",
                "message": {
                    "role": "assistant",
                    "content": null,
                    "tool_calls": [{
                        "id": "call_1",
                        "type": "function",
                        "function": {"name": "do", "arguments": "{\"x\":1}"}
                    }]
                }
            }],
            "usage": {"prompt_tokens": 3, "completion_tokens": 2}
        });

        let req = MessagesRequest {
            model: "openai/gpt-4.1-mini".to_string(),
            system: None,
            messages: vec![],
            max_tokens: 10,
            stream: None,
            temperature: None,
            top_p: None,
            top_k: None,
            stop_sequences: None,
            tools: None,
            tool_choice: None,
            thinking: None,
            metadata: None,
        };

        let out = convert_litellm_to_anthropic(resp, &req).unwrap();
        assert_eq!(out.stop_reason.as_deref(), Some("tool_use"));
        assert!(matches!(out.content[0], ContentBlock::ToolUse { .. }));
    }
}
