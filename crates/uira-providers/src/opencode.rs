//! OpenCode Zen API client implementation
//!
//! OpenCode Zen is a hosted API service at https://opencode.ai/zen/v1 that provides
//! access to various AI models through an OpenAI-compatible API.
//!
//! This client calls the Zen API directly without requiring a local OpenCode server.

use async_trait::async_trait;
use reqwest::Client;
use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use uira_protocol::{
    ContentBlock, ContentDelta, Message, MessageContent, MessageDelta, ModelResponse, Role,
    StopReason, StreamChunk, StreamMessageStart, TokenUsage, ToolSpec,
};

use crate::{
    image::image_source_to_data_url, traits::ModelResult, traits::ResponseStream, ModelClient,
    ProviderConfig, ProviderError,
};

const OPENCODE_ZEN_BASE_URL: &str = "https://opencode.ai/zen/v1";
const DEFAULT_MAX_TOKENS: usize = 8192;
const MAX_SSE_BUFFER: usize = 10 * 1024 * 1024;
const PROVIDER_NAME: &str = "opencode";

/// OpenCode Zen API client
///
/// Calls the hosted OpenCode Zen API directly using OpenAI-compatible format.
/// Supports models like: glm-4.7, qwen3-coder, claude-opus-4-1, big-pickle, gpt-5-nano, etc.
pub struct OpenCodeClient {
    client: Client,
    config: ProviderConfig,
}

impl OpenCodeClient {
    pub fn new(config: ProviderConfig) -> Result<Self, ProviderError> {
        let api_key = Self::get_api_key(&config)?;

        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(
            "Authorization",
            format!("Bearer {}", api_key.expose_secret())
                .parse()
                .map_err(|_| ProviderError::Configuration("Invalid API key format".into()))?,
        );
        headers.insert("content-type", "application/json".parse().unwrap());

        let timeout = Duration::from_secs(config.timeout_seconds.unwrap_or(120));

        let client = Client::builder()
            .default_headers(headers)
            .timeout(timeout)
            .build()?;

        Ok(Self { client, config })
    }

    fn get_api_key(config: &ProviderConfig) -> Result<SecretString, ProviderError> {
        // Check config first
        if let Some(api_key) = &config.api_key {
            return Ok(api_key.clone());
        }

        // Check environment variable
        if let Ok(key) = std::env::var("OPENCODE_API_KEY") {
            return Ok(SecretString::from(key));
        }

        // OpenCode Zen has some free models that work with "public" key
        Ok(SecretString::from("public".to_string()))
    }

    /// Extract the model ID from the full model string.
    /// Input: "opencode/gpt-5-nano" or "gpt-5-nano"
    /// Output: "gpt-5-nano"
    fn extract_model_id(model: &str) -> &str {
        model.strip_prefix("opencode/").unwrap_or(model)
    }

    fn build_request(
        &self,
        messages: &[Message],
        tools: &[ToolSpec],
        stream: bool,
    ) -> OpenCodeRequest {
        let model_id = Self::extract_model_id(&self.config.model);

        OpenCodeRequest {
            model: model_id.to_string(),
            messages: messages.iter().map(|m| self.convert_message(m)).collect(),
            max_tokens: self.config.max_tokens.unwrap_or(DEFAULT_MAX_TOKENS),
            tools: if tools.is_empty() {
                None
            } else {
                Some(
                    tools
                        .iter()
                        .map(|t| OpenCodeTool {
                            r#type: "function".to_string(),
                            function: OpenCodeFunction {
                                name: t.name.clone(),
                                description: t.description.clone(),
                                parameters: serde_json::to_value(&t.input_schema).unwrap(),
                            },
                        })
                        .collect(),
                )
            },
            stream: Some(stream),
            temperature: self.config.temperature,
        }
    }

    fn convert_message(&self, msg: &Message) -> OpenCodeMessage {
        let role = match msg.role {
            Role::System => "system",
            Role::User => "user",
            Role::Assistant => "assistant",
            Role::Tool => "tool",
        };

        let content = match &msg.content {
            MessageContent::Text(text) => Some(OpenCodeMessageContent::Text(text.clone())),
            MessageContent::Blocks(blocks) => {
                if msg.role == Role::User {
                    let parts = Self::convert_user_blocks_to_parts(blocks);
                    if parts.is_empty() {
                        None
                    } else {
                        Some(OpenCodeMessageContent::Parts(parts))
                    }
                } else {
                    let text: String = blocks
                        .iter()
                        .filter_map(|b| {
                            if let ContentBlock::Text { text } = b {
                                Some(text.as_str())
                            } else {
                                None
                            }
                        })
                        .collect::<Vec<_>>()
                        .join("");
                    if text.is_empty() {
                        None
                    } else {
                        Some(OpenCodeMessageContent::Text(text))
                    }
                }
            }
            MessageContent::ToolCalls(_) => None,
        };

        let tool_calls = match &msg.content {
            MessageContent::ToolCalls(calls) => Some(
                calls
                    .iter()
                    .map(|c| OpenCodeToolCall {
                        id: c.id.clone(),
                        r#type: "function".to_string(),
                        function: OpenCodeFunctionCall {
                            name: c.name.clone(),
                            arguments: c.input.to_string(),
                        },
                    })
                    .collect(),
            ),
            _ => None,
        };

        OpenCodeMessage {
            role: role.to_string(),
            content,
            tool_calls,
            tool_call_id: msg.tool_call_id.clone(),
        }
    }

    fn convert_user_blocks_to_parts(blocks: &[ContentBlock]) -> Vec<OpenCodeContentPart> {
        let mut parts = Vec::new();

        for block in blocks {
            match block {
                ContentBlock::Text { text } => {
                    parts.push(OpenCodeContentPart::Text { text: text.clone() });
                }
                ContentBlock::Image { source } => match image_source_to_data_url(source) {
                    Ok(url) => {
                        parts.push(OpenCodeContentPart::ImageUrl {
                            image_url: OpenCodeImageUrl { url },
                        });
                    }
                    Err(error) => {
                        tracing::warn!("Skipping image attachment for OpenCode request: {}", error);
                    }
                },
                ContentBlock::ToolResult { content, .. } => {
                    parts.push(OpenCodeContentPart::Text {
                        text: content.clone(),
                    });
                }
                _ => {}
            }
        }

        parts
    }

    fn base_url(&self) -> &str {
        self.config
            .base_url
            .as_deref()
            .unwrap_or(OPENCODE_ZEN_BASE_URL)
    }

    fn convert_response(&self, response: OpenCodeResponse) -> ModelResponse {
        let choice = response.choices.into_iter().next().unwrap_or_default();

        let mut content = Vec::new();

        if let Some(text) = choice.message.content {
            content.push(ContentBlock::Text { text });
        }

        if let Some(tool_calls) = choice.message.tool_calls {
            for tc in tool_calls {
                let input: serde_json::Value =
                    serde_json::from_str(&tc.function.arguments).unwrap_or_default();
                content.push(ContentBlock::ToolUse {
                    id: tc.id,
                    name: tc.function.name,
                    input,
                });
            }
        }

        let stop_reason = choice.finish_reason.map(|r| match r.as_str() {
            "stop" => StopReason::EndTurn,
            "length" => StopReason::MaxTokens,
            "tool_calls" => StopReason::ToolUse,
            "content_filter" => StopReason::ContentFilter,
            _ => StopReason::EndTurn,
        });

        ModelResponse {
            id: response.id,
            model: response.model,
            content,
            stop_reason,
            usage: response
                .usage
                .map(|u| TokenUsage {
                    input_tokens: u.prompt_tokens,
                    output_tokens: u.completion_tokens,
                    cache_read_tokens: u.cache_read_tokens.unwrap_or(0),
                    cache_creation_tokens: 0,
                })
                .unwrap_or_default(),
        }
    }
}

#[async_trait]
impl ModelClient for OpenCodeClient {
    async fn chat(&self, messages: &[Message], tools: &[ToolSpec]) -> ModelResult<ModelResponse> {
        let request = self.build_request(messages, tools, false);
        let url = format!("{}/chat/completions", self.base_url());

        let response = self.client.post(&url).json(&request).send().await?;

        if !response.status().is_success() {
            let status = response.status();

            if status.as_u16() == 429 {
                // Parse Retry-After header if present, otherwise default to 60s
                let retry_after_ms = response
                    .headers()
                    .get("retry-after")
                    .and_then(|v| v.to_str().ok())
                    .and_then(|s| s.parse::<u64>().ok())
                    .map(|secs| secs * 1000)
                    .unwrap_or(60000);
                return Err(ProviderError::RateLimited { retry_after_ms });
            }

            if status.is_server_error() {
                return Err(ProviderError::Unavailable {
                    provider: "opencode".to_string(),
                });
            }

            let body = response.text().await.unwrap_or_default();
            return Err(ProviderError::InvalidResponse(format!(
                "OpenCode Zen API error {}: {}",
                status, body
            )));
        }

        let api_response: OpenCodeResponse = response.json().await?;
        Ok(self.convert_response(api_response))
    }

    async fn chat_stream(
        &self,
        messages: &[Message],
        tools: &[ToolSpec],
    ) -> ModelResult<ResponseStream> {
        use async_stream::stream;
        use futures::StreamExt;

        let request = self.build_request(messages, tools, true);
        let url = format!("{}/chat/completions", self.base_url());

        let response = self.client.post(&url).json(&request).send().await?;

        if !response.status().is_success() {
            let status = response.status();

            if status.as_u16() == 429 {
                let retry_after_ms = response
                    .headers()
                    .get("retry-after")
                    .and_then(|v| v.to_str().ok())
                    .and_then(|s| s.parse::<u64>().ok())
                    .map(|secs| secs * 1000)
                    .unwrap_or(60000);
                return Err(ProviderError::RateLimited { retry_after_ms });
            }

            if status.is_server_error() {
                return Err(ProviderError::Unavailable {
                    provider: "opencode".to_string(),
                });
            }

            let body = response.text().await.unwrap_or_default();
            return Err(ProviderError::InvalidResponse(format!(
                "OpenCode Zen API error {}: {}",
                status, body
            )));
        }

        let mut byte_stream = response.bytes_stream();

        let stream = stream! {
            let mut buffer = String::new();

            while let Some(result) = byte_stream.next().await {
                match result {
                    Ok(bytes) => {
                        let text = String::from_utf8_lossy(&bytes).replace("\r\n", "\n");
                        buffer.push_str(&text);

                        if buffer.len() > MAX_SSE_BUFFER {
                            yield Err(ProviderError::StreamError(
                                "SSE buffer exceeded maximum size".to_string(),
                            ));
                            return;
                        }

                        while let Some(pos) = buffer.find("\n\n") {
                            let event = buffer[..pos].to_string();
                            buffer = buffer[pos + 2..].to_string();

                            if let Some(chunk) = Self::parse_sse_line(&event) {
                                let is_stop = matches!(&chunk, StreamChunk::MessageStop);
                                let has_stop_reason = matches!(
                                    &chunk,
                                    StreamChunk::MessageDelta {
                                        delta: MessageDelta {
                                            stop_reason: Some(_)
                                        },
                                        ..
                                    }
                                );
                                yield Ok(chunk);
                                if is_stop {
                                    return;
                                }
                                if has_stop_reason {
                                    yield Ok(StreamChunk::MessageStop);
                                    return;
                                }
                            }
                        }
                    }
                    Err(e) => {
                        yield Err(ProviderError::StreamError(e.to_string()));
                    }
                }
            }

            if !buffer.trim().is_empty() {
                if let Some(chunk) = Self::parse_sse_line(&buffer) {
                    yield Ok(chunk);
                }
            }
        };

        Ok(Box::pin(stream))
    }

    fn supports_tools(&self) -> bool {
        true
    }

    fn max_tokens(&self) -> usize {
        self.config.max_tokens.unwrap_or(DEFAULT_MAX_TOKENS)
    }

    fn model(&self) -> &str {
        &self.config.model
    }

    fn provider(&self) -> &str {
        PROVIDER_NAME
    }
}

impl OpenCodeClient {
    fn parse_sse_line(event: &str) -> Option<StreamChunk> {
        for line in event.lines() {
            let trimmed = line.trim_end_matches('\r');
            if trimmed.is_empty() || trimmed.starts_with(':') {
                continue;
            }

            if let Some(data) = trimmed
                .strip_prefix("data: ")
                .or_else(|| trimmed.strip_prefix("data:"))
                .map(str::trim_start)
            {
                if data == "[DONE]" {
                    return Some(StreamChunk::MessageStop);
                }

                if let Ok(chunk) = serde_json::from_str::<OpenCodeStreamChunk>(data) {
                    return Some(Self::convert_stream_chunk(chunk));
                }
            }
        }

        None
    }

    fn convert_stream_chunk(chunk: OpenCodeStreamChunk) -> StreamChunk {
        let choice = match chunk.choices.into_iter().next() {
            Some(c) => c,
            None => return StreamChunk::Ping,
        };

        // Handle finish reason (message complete)
        if let Some(reason) = choice.finish_reason {
            let stop_reason = match reason.as_str() {
                "stop" => StopReason::EndTurn,
                "length" => StopReason::MaxTokens,
                "tool_calls" => StopReason::ToolUse,
                "content_filter" => StopReason::ContentFilter,
                _ => StopReason::EndTurn,
            };

            return StreamChunk::MessageDelta {
                delta: MessageDelta {
                    stop_reason: Some(stop_reason),
                },
                usage: chunk.usage.map(|u| TokenUsage {
                    input_tokens: u.prompt_tokens,
                    output_tokens: u.completion_tokens,
                    cache_read_tokens: u.cache_read_tokens.unwrap_or(0),
                    cache_creation_tokens: 0,
                }),
            };
        }

        // Handle content delta
        if let Some(content) = choice.delta.content {
            if !content.is_empty() {
                return StreamChunk::ContentBlockDelta {
                    index: choice.index,
                    delta: ContentDelta::TextDelta { text: content },
                };
            }
        }

        // Handle reasoning_content as thinking (for models like Kimi)
        if let Some(reasoning) = choice.delta.reasoning_content {
            if !reasoning.is_empty() {
                return StreamChunk::ContentBlockDelta {
                    index: choice.index,
                    delta: ContentDelta::ThinkingDelta {
                        thinking: reasoning,
                    },
                };
            }
        }

        // Handle tool calls
        if let Some(tool_calls) = choice.delta.tool_calls {
            for tc in tool_calls {
                if let Some(func) = tc.function {
                    // Tool call start (has id and name)
                    if let (Some(id), Some(name)) = (tc.id, func.name) {
                        return StreamChunk::ContentBlockStart {
                            index: tc.index,
                            content_block: ContentBlock::ToolUse {
                                id,
                                name,
                                input: serde_json::Value::Object(serde_json::Map::new()),
                            },
                        };
                    }

                    // Tool call argument delta
                    if let Some(arguments) = func.arguments {
                        if !arguments.is_empty() {
                            return StreamChunk::ContentBlockDelta {
                                index: tc.index,
                                delta: ContentDelta::InputJsonDelta {
                                    partial_json: arguments,
                                },
                            };
                        }
                    }
                }
            }
        }

        // Handle role (message start)
        if choice.delta.role.is_some() {
            return StreamChunk::MessageStart {
                message: StreamMessageStart {
                    id: chunk.id,
                    model: chunk.model,
                    usage: TokenUsage::default(),
                },
            };
        }

        StreamChunk::Ping
    }
}

#[derive(Debug, Serialize)]
struct OpenCodeRequest {
    model: String,
    messages: Vec<OpenCodeMessage>,
    max_tokens: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<OpenCodeTool>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stream: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
}

#[derive(Debug, Serialize)]
#[serde(untagged)]
enum OpenCodeMessageContent {
    Text(String),
    Parts(Vec<OpenCodeContentPart>),
}

#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum OpenCodeContentPart {
    Text { text: String },
    ImageUrl { image_url: OpenCodeImageUrl },
}

#[derive(Debug, Serialize)]
struct OpenCodeImageUrl {
    url: String,
}

#[derive(Debug, Serialize)]
struct OpenCodeMessage {
    role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    content: Option<OpenCodeMessageContent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_calls: Option<Vec<OpenCodeToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_call_id: Option<String>,
}

#[derive(Debug, Serialize)]
struct OpenCodeTool {
    r#type: String,
    function: OpenCodeFunction,
}

#[derive(Debug, Serialize)]
struct OpenCodeFunction {
    name: String,
    description: String,
    parameters: serde_json::Value,
}

#[derive(Debug, Serialize, Deserialize)]
struct OpenCodeToolCall {
    id: String,
    r#type: String,
    function: OpenCodeFunctionCall,
}

#[derive(Debug, Serialize, Deserialize)]
struct OpenCodeFunctionCall {
    name: String,
    arguments: String,
}

#[derive(Debug, Deserialize)]
struct OpenCodeResponse {
    id: String,
    model: String,
    choices: Vec<OpenCodeChoice>,
    usage: Option<OpenCodeUsage>,
}

#[derive(Debug, Default, Deserialize)]
struct OpenCodeChoice {
    message: OpenCodeResponseMessage,
    finish_reason: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct OpenCodeResponseMessage {
    content: Option<String>,
    tool_calls: Option<Vec<OpenCodeToolCall>>,
}

#[derive(Debug, Deserialize)]
struct OpenCodeUsage {
    prompt_tokens: u64,
    completion_tokens: u64,
    #[serde(default)]
    cache_read_tokens: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct OpenCodeStreamChunk {
    id: String,
    model: String,
    choices: Vec<OpenCodeStreamChoice>,
    #[serde(default)]
    usage: Option<OpenCodeUsage>,
}

#[derive(Debug, Deserialize)]
struct OpenCodeStreamChoice {
    index: usize,
    delta: OpenCodeStreamDelta,
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OpenCodeStreamDelta {
    #[serde(default)]
    role: Option<String>,
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    reasoning_content: Option<String>,
    #[serde(default)]
    tool_calls: Option<Vec<OpenCodeStreamToolCall>>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct OpenCodeStreamToolCall {
    index: usize,
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    r#type: Option<String>,
    #[serde(default)]
    function: Option<OpenCodeStreamFunction>,
}

#[derive(Debug, Deserialize)]
struct OpenCodeStreamFunction {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    arguments: Option<String>,
}
