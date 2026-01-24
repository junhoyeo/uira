//! Streaming conversion (SSE).
//!
//! LiteLLM (OpenAI-compatible) streams responses as Server-Sent Events with
//! `data: {json}` lines and a terminal `data: [DONE]` marker.
//!
//! Claude Code expects Anthropic-style SSE events (`message_start`,
//! `content_block_delta`, ...). This module reads the upstream SSE and emits
//! Anthropic SSE frames as raw strings.

use crate::types::{ContentBlock, MessagesRequest, Usage};
use anyhow::{Context, Result};
use async_stream::try_stream;
use futures::{Stream, StreamExt};
use serde_json::{json, Value};

/// Convert an upstream LiteLLM/OpenAI SSE response into Anthropic SSE frames.
///
/// The returned stream yields fully formatted SSE frames:
///
/// ```text
/// event: message_start
/// data: {...}
///
/// ```
pub fn handle_streaming(
    response: reqwest::Response,
    original_request: MessagesRequest,
) -> impl Stream<Item = Result<String>> + Send {
    let model = original_request.model.clone();

    try_stream! {
        let mut buffer = String::new();

        let mut message_id: Option<String> = None;
        let mut started = false;

        // Text streaming state.
        let mut text_block_started = false;
        let mut finish_reason: Option<String> = None;
        let mut usage: Usage = Usage { input_tokens: 0, output_tokens: 0 };

        // Tool call streaming state.
        let mut tool_call_index: std::collections::HashMap<String, usize> = std::collections::HashMap::new();

        let mut stream = response.bytes_stream();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.context("failed to read upstream stream chunk")?;
            let s = String::from_utf8_lossy(&chunk);
            buffer.push_str(&s);

            while let Some((frame, rest)) = split_sse_frame(&buffer) {
                buffer = rest;

                let Some(data_str) = extract_data_line(&frame) else {
                    continue;
                };

                if data_str.trim() == "[DONE]" {
                    break;
                }

                let v: Value = serde_json::from_str(data_str)
                    .with_context(|| format!("failed to parse upstream SSE json: {}", data_str))?;

                if message_id.is_none() {
                    message_id = v.get("id").and_then(|x| x.as_str()).map(|s| s.to_string());
                }

                // Usage appears either on the final chunk (OpenAI) or in
                // streaming usage deltas when `stream_options.include_usage` is
                // enabled.
                if let Some(u) = v.get("usage") {
                    if let Some(obj) = u.as_object() {
                        if let Some(p) = obj.get("prompt_tokens").and_then(|x| x.as_u64()) {
                            usage.input_tokens = p as u32;
                        }
                        if let Some(c) = obj.get("completion_tokens").and_then(|x| x.as_u64()) {
                            usage.output_tokens = c as u32;
                        }
                    }
                }

                let choices = v.get("choices").and_then(|x| x.as_array());
                let Some(choice) = choices.and_then(|c| c.first()) else {
                    continue;
                };

                if finish_reason.is_none() {
                    finish_reason = choice.get("finish_reason").and_then(|x| x.as_str()).map(|s| s.to_string());
                }

                let delta = choice.get("delta");
                if !started {
                    started = true;
                    let mid = message_id.clone().unwrap_or_else(|| "msg_unknown".to_string());
                    yield sse_event(
                        "message_start",
                        &json!({
                            "type": "message_start",
                            "message": {
                                "id": mid,
                                "type": "message",
                                "role": "assistant",
                                "model": model,
                                "content": [],
                                "stop_reason": null,
                                "stop_sequence": null,
                                "usage": {"input_tokens": usage.input_tokens, "output_tokens": usage.output_tokens}
                            }
                        }),
                    );
                }

                // Text deltas.
                if let Some(text) = delta.and_then(|d| d.get("content")).and_then(|c| c.as_str()) {
                    if !text_block_started {
                        text_block_started = true;
                        yield sse_event(
                            "content_block_start",
                            &json!({
                                "type": "content_block_start",
                                "index": 0,
                                "content_block": {"type": "text", "text": ""}
                            }),
                        );
                    }
                    if !text.is_empty() {
                        yield sse_event(
                            "content_block_delta",
                            &json!({
                                "type": "content_block_delta",
                                "index": 0,
                                "delta": {"type": "text_delta", "text": text}
                            }),
                        );
                    }
                }

                // Tool call deltas.
                if let Some(tool_calls) = delta.and_then(|d| d.get("tool_calls")).and_then(|tc| tc.as_array()) {
                    for call in tool_calls {
                        let id = call.get("id").and_then(|x| x.as_str()).unwrap_or("toolcall_unknown");
                        let func = call.get("function");
                        let name = func.and_then(|f| f.get("name")).and_then(|x| x.as_str());
                        let args = func.and_then(|f| f.get("arguments")).and_then(|x| x.as_str()).unwrap_or("");

                        let idx = if let Some(existing) = tool_call_index.get(id) {
                            *existing
                        } else {
                            let new_idx = if text_block_started { tool_call_index.len() + 1 } else { tool_call_index.len() };
                            tool_call_index.insert(id.to_string(), new_idx);
                            let cb = ContentBlock::ToolUse {
                                id: id.to_string(),
                                name: name.unwrap_or("unknown").to_string(),
                                input: json!({}),
                            };
                            yield sse_event(
                                "content_block_start",
                                &json!({
                                    "type": "content_block_start",
                                    "index": new_idx,
                                    "content_block": cb,
                                }),
                            );
                            new_idx
                        };

                        if !args.is_empty() {
                            yield sse_event(
                                "content_block_delta",
                                &json!({
                                    "type": "content_block_delta",
                                    "index": idx,
                                    "delta": {"type": "input_json_delta", "partial_json": args}
                                }),
                            );
                        }
                    }
                }
            }
        }

        // Close any opened blocks.
        if text_block_started {
            yield sse_event(
                "content_block_stop",
                &json!({"type": "content_block_stop", "index": 0}),
            );
        }
        for (_id, idx) in tool_call_index {
            yield sse_event(
                "content_block_stop",
                &json!({"type": "content_block_stop", "index": idx}),
            );
        }

        let stop_reason = finish_reason.map(map_openai_finish_reason);
        yield sse_event(
            "message_delta",
            &json!({
                "type": "message_delta",
                "delta": {"stop_reason": stop_reason, "stop_sequence": null},
                "usage": {"output_tokens": usage.output_tokens}
            }),
        );
        yield sse_event("message_stop", &json!({"type": "message_stop"}));
    }
}

fn sse_event(event: &str, data: &Value) -> String {
    // Match the proxy's Python implementation format:
    // event: <type>\n
    // data: <json>\n\n
    format!("event: {}\ndata: {}\n\n", event, data)
}

fn map_openai_finish_reason(reason: String) -> String {
    match reason.as_str() {
        "stop" => "end_turn".to_string(),
        "length" => "max_tokens".to_string(),
        "tool_calls" => "tool_use".to_string(),
        other => other.to_string(),
    }
}

/// Split the current buffer into the first complete SSE frame and the remaining
/// buffer.
///
/// SSE frames are separated by a blank line (`\n\n`).
fn split_sse_frame(buffer: &str) -> Option<(String, String)> {
    let idx = buffer.find("\n\n")?;
    let (frame, rest) = buffer.split_at(idx + 2);
    Some((frame.to_string(), rest.to_string()))
}

fn extract_data_line(frame: &str) -> Option<&str> {
    for line in frame.lines() {
        if let Some(rest) = line.strip_prefix("data:") {
            return Some(rest.trim_start());
        }
    }
    None
}
