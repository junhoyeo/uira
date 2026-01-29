//! OpenAI client implementation

use async_trait::async_trait;
use futures::StreamExt;
use reqwest::Client;
use secrecy::ExposeSecret;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use uira_protocol::{
    ContentBlock, ContentDelta, Message, MessageContent, MessageDelta, ModelResponse, Role,
    StopReason, StreamChunk, StreamMessageStart, TokenUsage, ToolSpec,
};

use crate::{
    traits::ModelResult, traits::ResponseStream, ModelClient, ProviderConfig, ProviderError,
};

const DEFAULT_MAX_TOKENS: usize = 4096;

/// OpenAI API client
pub struct OpenAIClient {
    client: Client,
    config: ProviderConfig,
}

impl OpenAIClient {
    pub fn new(config: ProviderConfig) -> Result<Self, ProviderError> {
        let api_key = config
            .api_key
            .as_ref()
            .ok_or_else(|| ProviderError::Configuration("API key required for OpenAI".into()))?;

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

    fn build_request(
        &self,
        messages: &[Message],
        tools: &[ToolSpec],
        stream: bool,
    ) -> OpenAIRequest {
        OpenAIRequest {
            model: self.config.model.clone(),
            messages: messages.iter().map(|m| self.convert_message(m)).collect(),
            max_tokens: self.config.max_tokens.unwrap_or(DEFAULT_MAX_TOKENS),
            tools: if tools.is_empty() {
                None
            } else {
                Some(
                    tools
                        .iter()
                        .map(|t| OpenAITool {
                            r#type: "function".to_string(),
                            function: OpenAIFunction {
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

    fn convert_message(&self, msg: &Message) -> OpenAIMessage {
        let role = match msg.role {
            Role::System => "system",
            Role::User => "user",
            Role::Assistant => "assistant",
            Role::Tool => "tool",
        };

        let content = match &msg.content {
            MessageContent::Text(text) => Some(text.clone()),
            MessageContent::Blocks(blocks) => {
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
                    Some(text)
                }
            }
            MessageContent::ToolCalls(_) => None,
        };

        let tool_calls = match &msg.content {
            MessageContent::ToolCalls(calls) => Some(
                calls
                    .iter()
                    .map(|c| OpenAIToolCall {
                        id: c.id.clone(),
                        r#type: "function".to_string(),
                        function: OpenAIFunctionCall {
                            name: c.name.clone(),
                            arguments: c.input.to_string(),
                        },
                    })
                    .collect(),
            ),
            _ => None,
        };

        OpenAIMessage {
            role: role.to_string(),
            content,
            tool_calls,
            tool_call_id: msg.tool_call_id.clone(),
        }
    }

    fn base_url(&self) -> &str {
        self.config
            .base_url
            .as_deref()
            .unwrap_or("https://api.openai.com")
    }

    fn convert_response(&self, response: OpenAIResponse) -> ModelResponse {
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
                    cache_read_tokens: 0,
                    cache_creation_tokens: 0,
                })
                .unwrap_or_default(),
        }
    }
}

#[async_trait]
impl ModelClient for OpenAIClient {
    async fn chat(&self, messages: &[Message], tools: &[ToolSpec]) -> ModelResult<ModelResponse> {
        let request = self.build_request(messages, tools, false);
        let url = format!("{}/v1/chat/completions", self.base_url());

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

        let api_response: OpenAIResponse = response.json().await?;
        Ok(self.convert_response(api_response))
    }

    async fn chat_stream(
        &self,
        messages: &[Message],
        tools: &[ToolSpec],
    ) -> ModelResult<ResponseStream> {
        let request = self.build_request(messages, tools, true);
        let url = format!("{}/v1/chat/completions", self.base_url());

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
        // GPT-4 supports up to 128k context
        128_000
    }

    fn model(&self) -> &str {
        &self.config.model
    }

    fn provider(&self) -> &str {
        "openai"
    }
}

impl OpenAIClient {
    fn parse_sse_event(text: &str) -> Result<StreamChunk, ProviderError> {
        // Parse OpenAI SSE format: "data: {...}" or "data: [DONE]"
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with(':') {
                continue;
            }

            if let Some(data) = line.strip_prefix("data: ") {
                if data == "[DONE]" {
                    return Ok(StreamChunk::MessageStop);
                }

                if let Ok(chunk) = serde_json::from_str::<OpenAIStreamChunk>(data) {
                    return Ok(Self::convert_stream_chunk(chunk));
                }
            }
        }

        Ok(StreamChunk::Ping)
    }

    fn convert_stream_chunk(chunk: OpenAIStreamChunk) -> StreamChunk {
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
                    cache_read_tokens: 0,
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

// API request/response types
#[derive(Debug, Serialize)]
struct OpenAIRequest {
    model: String,
    messages: Vec<OpenAIMessage>,
    max_tokens: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<OpenAITool>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stream: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
}

#[derive(Debug, Serialize)]
struct OpenAIMessage {
    role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_calls: Option<Vec<OpenAIToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_call_id: Option<String>,
}

#[derive(Debug, Serialize)]
struct OpenAITool {
    r#type: String,
    function: OpenAIFunction,
}

#[derive(Debug, Serialize)]
struct OpenAIFunction {
    name: String,
    description: String,
    parameters: serde_json::Value,
}

#[derive(Debug, Serialize, Deserialize)]
struct OpenAIToolCall {
    id: String,
    r#type: String,
    function: OpenAIFunctionCall,
}

#[derive(Debug, Serialize, Deserialize)]
struct OpenAIFunctionCall {
    name: String,
    arguments: String,
}

#[derive(Debug, Deserialize)]
struct OpenAIResponse {
    id: String,
    model: String,
    choices: Vec<OpenAIChoice>,
    usage: Option<OpenAIUsage>,
}

#[derive(Debug, Default, Deserialize)]
struct OpenAIChoice {
    message: OpenAIResponseMessage,
    finish_reason: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct OpenAIResponseMessage {
    content: Option<String>,
    tool_calls: Option<Vec<OpenAIToolCall>>,
}

#[derive(Debug, Deserialize)]
struct OpenAIUsage {
    prompt_tokens: u64,
    completion_tokens: u64,
}

// Streaming response types
#[derive(Debug, Deserialize)]
struct OpenAIStreamChunk {
    id: String,
    model: String,
    choices: Vec<OpenAIStreamChoice>,
    #[serde(default)]
    usage: Option<OpenAIUsage>,
}

#[derive(Debug, Deserialize)]
struct OpenAIStreamChoice {
    index: usize,
    delta: OpenAIStreamDelta,
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OpenAIStreamDelta {
    #[serde(default)]
    role: Option<String>,
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    tool_calls: Option<Vec<OpenAIStreamToolCall>>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct OpenAIStreamToolCall {
    index: usize,
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    r#type: Option<String>,
    #[serde(default)]
    function: Option<OpenAIStreamFunction>,
}

#[derive(Debug, Deserialize)]
struct OpenAIStreamFunction {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    arguments: Option<String>,
}
