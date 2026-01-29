//! Anthropic (Claude) client implementation

use async_trait::async_trait;
use futures::StreamExt;
use reqwest::Client;
use secrecy::ExposeSecret;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use uira_protocol::{
    ContentBlock, ContentDelta, Message, MessageContent, MessageDelta, ModelResponse, Role,
    StopReason, StreamChunk, StreamError, StreamMessageStart, TokenUsage, ToolSpec,
};

use crate::{
    traits::ModelResult, traits::ResponseStream, ModelClient, ProviderConfig, ProviderError,
};

const ANTHROPIC_VERSION: &str = "2023-06-01";
const DEFAULT_MAX_TOKENS: usize = 8192;

/// Anthropic API client
pub struct AnthropicClient {
    client: Client,
    config: ProviderConfig,
}

impl AnthropicClient {
    pub fn new(config: ProviderConfig) -> Result<Self, ProviderError> {
        let api_key = config
            .api_key
            .as_ref()
            .ok_or_else(|| ProviderError::Configuration("API key required for Anthropic".into()))?;

        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(
            "x-api-key",
            api_key
                .expose_secret()
                .parse()
                .map_err(|_| ProviderError::Configuration("Invalid API key format".into()))?,
        );
        headers.insert("anthropic-version", ANTHROPIC_VERSION.parse().unwrap());
        headers.insert("content-type", "application/json".parse().unwrap());

        let timeout = Duration::from_secs(config.timeout_seconds.unwrap_or(120));

        let client = Client::builder()
            .default_headers(headers)
            .timeout(timeout)
            .build()?;

        Ok(Self { client, config })
    }

    fn build_request(
        &self,
        messages: &[Message],
        tools: &[ToolSpec],
        stream: bool,
    ) -> AnthropicRequest {
        let (system, messages) = Self::extract_system(messages);

        AnthropicRequest {
            model: self.config.model.clone(),
            max_tokens: self.config.max_tokens.unwrap_or(DEFAULT_MAX_TOKENS),
            messages: messages
                .into_iter()
                .map(|m| self.convert_message(m))
                .collect(),
            system,
            tools: if tools.is_empty() {
                None
            } else {
                Some(tools.to_vec())
            },
            stream: Some(stream),
            temperature: self.config.temperature,
        }
    }

    fn extract_system(messages: &[Message]) -> (Option<String>, Vec<&Message>) {
        let mut system = None;
        let mut rest = Vec::new();

        for msg in messages {
            if msg.role == Role::System {
                if let MessageContent::Text(text) = &msg.content {
                    system = Some(text.clone());
                }
            } else {
                rest.push(msg);
            }
        }

        (system, rest)
    }

    fn convert_message(&self, msg: &Message) -> AnthropicMessage {
        let role = match msg.role {
            Role::User | Role::Tool => "user",
            Role::Assistant => "assistant",
            Role::System => "user", // Should be filtered out
        };

        let content = match &msg.content {
            MessageContent::Text(text) => vec![AnthropicContent::Text { text: text.clone() }],
            MessageContent::Blocks(blocks) => blocks
                .iter()
                .map(|b| match b {
                    ContentBlock::Text { text } => AnthropicContent::Text { text: text.clone() },
                    ContentBlock::ToolUse { id, name, input } => AnthropicContent::ToolUse {
                        id: id.clone(),
                        name: name.clone(),
                        input: input.clone(),
                    },
                    ContentBlock::ToolResult {
                        tool_use_id,
                        content,
                        is_error,
                    } => AnthropicContent::ToolResult {
                        tool_use_id: tool_use_id.clone(),
                        content: content.clone(),
                        is_error: *is_error,
                    },
                    ContentBlock::Image { source } => AnthropicContent::Image {
                        source: source.clone(),
                    },
                    ContentBlock::Thinking {
                        thinking,
                        signature,
                    } => AnthropicContent::Thinking {
                        thinking: thinking.clone(),
                        signature: signature.clone(),
                    },
                })
                .collect(),
            MessageContent::ToolCalls(calls) => calls
                .iter()
                .map(|c| AnthropicContent::ToolUse {
                    id: c.id.clone(),
                    name: c.name.clone(),
                    input: c.input.clone(),
                })
                .collect(),
        };

        AnthropicMessage {
            role: role.to_string(),
            content,
        }
    }

    fn base_url(&self) -> &str {
        self.config
            .base_url
            .as_deref()
            .unwrap_or("https://api.anthropic.com")
    }
}

#[async_trait]
impl ModelClient for AnthropicClient {
    async fn chat(&self, messages: &[Message], tools: &[ToolSpec]) -> ModelResult<ModelResponse> {
        let request = self.build_request(messages, tools, false);
        let url = format!("{}/v1/messages", self.base_url());

        let response = self.client.post(&url).json(&request).send().await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();

            if status.as_u16() == 429 {
                return Err(ProviderError::RateLimited {
                    retry_after_ms: 60000,
                });
            }

            return Err(ProviderError::InvalidResponse(format!(
                "API error {}: {}",
                status, body
            )));
        }

        let api_response: AnthropicResponse = response.json().await?;
        Ok(self.convert_response(api_response))
    }

    async fn chat_stream(
        &self,
        messages: &[Message],
        tools: &[ToolSpec],
    ) -> ModelResult<ResponseStream> {
        let request = self.build_request(messages, tools, true);
        let url = format!("{}/v1/messages", self.base_url());

        let response = self.client.post(&url).json(&request).send().await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(ProviderError::InvalidResponse(format!(
                "API error {}: {}",
                status, body
            )));
        }

        let stream = response.bytes_stream().map(|result| match result {
            Ok(bytes) => {
                let text = String::from_utf8_lossy(&bytes);
                Self::parse_sse_event(&text)
            }
            Err(e) => Err(ProviderError::StreamError(e.to_string())),
        });

        Ok(Box::pin(stream))
    }

    fn supports_tools(&self) -> bool {
        true
    }

    fn max_tokens(&self) -> usize {
        // Claude 3 models support up to 200k context
        200_000
    }

    fn model(&self) -> &str {
        &self.config.model
    }

    fn provider(&self) -> &str {
        "anthropic"
    }
}

impl AnthropicClient {
    fn convert_response(&self, response: AnthropicResponse) -> ModelResponse {
        let content = response
            .content
            .into_iter()
            .map(|c| match c {
                AnthropicContent::Text { text } => ContentBlock::Text { text },
                AnthropicContent::ToolUse { id, name, input } => {
                    ContentBlock::ToolUse { id, name, input }
                }
                AnthropicContent::ToolResult {
                    tool_use_id,
                    content,
                    is_error,
                } => ContentBlock::ToolResult {
                    tool_use_id,
                    content,
                    is_error,
                },
                AnthropicContent::Image { source } => ContentBlock::Image { source },
                AnthropicContent::Thinking {
                    thinking,
                    signature,
                } => ContentBlock::Thinking {
                    thinking,
                    signature,
                },
            })
            .collect();

        let stop_reason = response.stop_reason.map(|r| match r.as_str() {
            "end_turn" => StopReason::EndTurn,
            "max_tokens" => StopReason::MaxTokens,
            "stop_sequence" => StopReason::StopSequence,
            "tool_use" => StopReason::ToolUse,
            _ => StopReason::EndTurn,
        });

        ModelResponse {
            id: response.id,
            model: response.model,
            content,
            stop_reason,
            usage: TokenUsage {
                input_tokens: response.usage.input_tokens,
                output_tokens: response.usage.output_tokens,
                cache_read_tokens: response.usage.cache_read_input_tokens.unwrap_or(0),
                cache_creation_tokens: response.usage.cache_creation_input_tokens.unwrap_or(0),
            },
        }
    }

    fn parse_sse_event(text: &str) -> Result<StreamChunk, ProviderError> {
        // Simple SSE parser - in production would use eventsource-stream
        for line in text.lines() {
            if let Some(data) = line.strip_prefix("data: ") {
                if data == "[DONE]" {
                    return Ok(StreamChunk::MessageStop);
                }

                if let Ok(event) = serde_json::from_str::<AnthropicStreamEvent>(data) {
                    return Ok(event.into());
                }
            }
        }

        Ok(StreamChunk::Ping)
    }
}

// API request/response types
#[derive(Debug, Serialize)]
struct AnthropicRequest {
    model: String,
    max_tokens: usize,
    messages: Vec<AnthropicMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    system: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<ToolSpec>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stream: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
}

#[derive(Debug, Serialize)]
struct AnthropicMessage {
    role: String,
    content: Vec<AnthropicContent>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum AnthropicContent {
    Text {
        text: String,
    },
    ToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
    },
    ToolResult {
        tool_use_id: String,
        content: String,
        #[serde(default)]
        is_error: bool,
    },
    Image {
        source: uira_protocol::ImageSource,
    },
    Thinking {
        thinking: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        signature: Option<String>,
    },
}

#[derive(Debug, Deserialize)]
struct AnthropicResponse {
    id: String,
    model: String,
    content: Vec<AnthropicContent>,
    stop_reason: Option<String>,
    usage: AnthropicUsage,
}

#[derive(Debug, Deserialize)]
struct AnthropicUsage {
    input_tokens: u64,
    output_tokens: u64,
    #[serde(default)]
    cache_read_input_tokens: Option<u64>,
    #[serde(default)]
    cache_creation_input_tokens: Option<u64>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum AnthropicStreamEvent {
    MessageStart {
        message: AnthropicStreamMessage,
    },
    ContentBlockStart {
        index: usize,
        content_block: AnthropicContent,
    },
    ContentBlockDelta {
        index: usize,
        delta: AnthropicDelta,
    },
    ContentBlockStop {
        index: usize,
    },
    MessageDelta {
        delta: AnthropicMessageDelta,
        usage: Option<AnthropicUsage>,
    },
    MessageStop,
    Ping,
    Error {
        error: AnthropicError,
    },
}

#[derive(Debug, Deserialize)]
struct AnthropicStreamMessage {
    id: String,
    model: String,
    #[serde(default)]
    usage: Option<AnthropicUsage>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
#[allow(clippy::enum_variant_names)]
enum AnthropicDelta {
    TextDelta { text: String },
    InputJsonDelta { partial_json: String },
    ThinkingDelta { thinking: String },
    SignatureDelta { signature: String },
}

#[derive(Debug, Deserialize)]
struct AnthropicMessageDelta {
    stop_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AnthropicError {
    r#type: String,
    message: String,
}

impl From<AnthropicStreamEvent> for StreamChunk {
    fn from(event: AnthropicStreamEvent) -> Self {
        match event {
            AnthropicStreamEvent::MessageStart { message } => StreamChunk::MessageStart {
                message: StreamMessageStart {
                    id: message.id,
                    model: message.model,
                    usage: message
                        .usage
                        .map(|u| TokenUsage {
                            input_tokens: u.input_tokens,
                            output_tokens: u.output_tokens,
                            cache_read_tokens: u.cache_read_input_tokens.unwrap_or(0),
                            cache_creation_tokens: u.cache_creation_input_tokens.unwrap_or(0),
                        })
                        .unwrap_or_default(),
                },
            },
            AnthropicStreamEvent::ContentBlockStart {
                index,
                content_block,
            } => {
                let block = match content_block {
                    AnthropicContent::Text { text } => ContentBlock::Text { text },
                    AnthropicContent::ToolUse { id, name, input } => {
                        ContentBlock::ToolUse { id, name, input }
                    }
                    AnthropicContent::ToolResult {
                        tool_use_id,
                        content,
                        is_error,
                    } => ContentBlock::ToolResult {
                        tool_use_id,
                        content,
                        is_error,
                    },
                    AnthropicContent::Image { source } => ContentBlock::Image { source },
                    AnthropicContent::Thinking {
                        thinking,
                        signature,
                    } => ContentBlock::Thinking {
                        thinking,
                        signature,
                    },
                };
                StreamChunk::ContentBlockStart {
                    index,
                    content_block: block,
                }
            }
            AnthropicStreamEvent::ContentBlockDelta { index, delta } => {
                let content_delta = match delta {
                    AnthropicDelta::TextDelta { text } => ContentDelta::TextDelta { text },
                    AnthropicDelta::InputJsonDelta { partial_json } => {
                        ContentDelta::InputJsonDelta { partial_json }
                    }
                    AnthropicDelta::ThinkingDelta { thinking } => {
                        ContentDelta::ThinkingDelta { thinking }
                    }
                    AnthropicDelta::SignatureDelta { signature } => {
                        ContentDelta::SignatureDelta { signature }
                    }
                };
                StreamChunk::ContentBlockDelta {
                    index,
                    delta: content_delta,
                }
            }
            AnthropicStreamEvent::ContentBlockStop { index } => {
                StreamChunk::ContentBlockStop { index }
            }
            AnthropicStreamEvent::MessageDelta { delta, usage } => StreamChunk::MessageDelta {
                delta: MessageDelta {
                    stop_reason: delta.stop_reason.map(|r| match r.as_str() {
                        "end_turn" => StopReason::EndTurn,
                        "max_tokens" => StopReason::MaxTokens,
                        "stop_sequence" => StopReason::StopSequence,
                        "tool_use" => StopReason::ToolUse,
                        _ => StopReason::EndTurn,
                    }),
                },
                usage: usage.map(|u| TokenUsage {
                    input_tokens: u.input_tokens,
                    output_tokens: u.output_tokens,
                    cache_read_tokens: u.cache_read_input_tokens.unwrap_or(0),
                    cache_creation_tokens: u.cache_creation_input_tokens.unwrap_or(0),
                }),
            },
            AnthropicStreamEvent::MessageStop => StreamChunk::MessageStop,
            AnthropicStreamEvent::Ping => StreamChunk::Ping,
            AnthropicStreamEvent::Error { error } => StreamChunk::Error {
                error: StreamError {
                    r#type: error.r#type,
                    message: error.message,
                },
            },
        }
    }
}
