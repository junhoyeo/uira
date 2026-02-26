use async_trait::async_trait;
use reqwest::Client;
use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Duration;
use uira_core::{
    ContentBlock, ContentDelta, Message, MessageContent, MessageDelta, ModelResponse, Role,
    StopReason, StreamChunk, StreamMessageStart, TokenUsage, ToolSpec,
};

use crate::{
    image::image_source_to_data_url, traits::ModelResult, traits::ResponseStream, ModelClient,
    ProviderConfig, ProviderError,
};

const FRIENDLI_SERVERLESS_BASE_URL: &str = "https://api.friendli.ai/serverless/v1";
const FRIENDLI_DEDICATED_BASE_URL: &str = "https://api.friendli.ai/dedicated/v1";
const DEFAULT_MAX_TOKENS: usize = 8192;
const MAX_SSE_BUFFER: usize = 10 * 1024 * 1024;
const PROVIDER_NAME: &str = "friendliai";

#[derive(Debug, Clone, Copy)]
pub enum ModelType {
    Serverless,
    Dedicated,
}

pub struct FriendliClient {
    client: Client,
    config: ProviderConfig,
    model_type: ModelType,
}

impl FriendliClient {
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

        let model_type = Self::resolve_model_type(&config);

        Ok(Self {
            client,
            config,
            model_type,
        })
    }

    fn get_api_key(config: &ProviderConfig) -> Result<SecretString, ProviderError> {
        if let Some(api_key) = &config.api_key {
            return Ok(api_key.clone());
        }

        if let Ok(key) = std::env::var("FRIENDLI_TOKEN") {
            return Ok(SecretString::from(key));
        }

        Err(ProviderError::Configuration(
            "FRIENDLI_TOKEN environment variable not set".into(),
        ))
    }

    fn resolve_model_type(config: &ProviderConfig) -> ModelType {
        if let Some(base_url) = config.base_url.as_deref() {
            if base_url.contains("/dedicated/") {
                return ModelType::Dedicated;
            }
        }

        ModelType::Serverless
    }

    fn extract_model_id(model: &str) -> &str {
        model.strip_prefix("friendliai/").unwrap_or(model)
    }

    fn build_request(
        &self,
        messages: &[Message],
        tools: &[ToolSpec],
        stream: bool,
    ) -> FriendliRequest {
        let model_id = Self::extract_model_id(&self.config.model);

        FriendliRequest {
            model: model_id.to_string(),
            messages: messages.iter().map(|m| self.convert_message(m)).collect(),
            max_tokens: self.config.max_tokens.unwrap_or(DEFAULT_MAX_TOKENS),
            tools: if tools.is_empty() {
                None
            } else {
                Some(
                    tools
                        .iter()
                        .map(|t| FriendliTool {
                            r#type: "function".to_string(),
                            function: FriendliFunction {
                                name: t.name.clone(),
                                description: t.description.clone(),
                                parameters: serde_json::to_value(&t.input_schema).unwrap(),
                            },
                        })
                        .collect(),
                )
            },
            tool_choice: if tools.is_empty() {
                None
            } else {
                Some(serde_json::Value::String("auto".to_string()))
            },
            stream: Some(stream),
            temperature: self.config.temperature,
            chat_template_kwargs: None,
        }
    }

    fn convert_message(&self, msg: &Message) -> FriendliMessage {
        let role = match msg.role {
            Role::System => "system",
            Role::User => "user",
            Role::Assistant => "assistant",
            Role::Tool => "tool",
        };

        let content = match &msg.content {
            MessageContent::Text(text) => Some(FriendliMessageContent::Text(text.clone())),
            MessageContent::Blocks(blocks) => {
                if msg.role == Role::User {
                    let parts = Self::convert_user_blocks_to_parts(blocks);
                    if parts.is_empty() {
                        None
                    } else {
                        Some(FriendliMessageContent::Parts(parts))
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
                        Some(FriendliMessageContent::Text(text))
                    }
                }
            }
            MessageContent::ToolCalls(_) => None,
        };

        let tool_calls = match &msg.content {
            MessageContent::ToolCalls(calls) => Some(
                calls
                    .iter()
                    .map(|c| FriendliToolCall {
                        id: c.id.clone(),
                        r#type: "function".to_string(),
                        function: FriendliFunctionCall {
                            name: c.name.clone(),
                            arguments: c.input.to_string(),
                        },
                    })
                    .collect(),
            ),
            _ => None,
        };

        FriendliMessage {
            role: role.to_string(),
            content,
            tool_calls,
            tool_call_id: msg.tool_call_id.clone(),
        }
    }

    fn convert_user_blocks_to_parts(blocks: &[ContentBlock]) -> Vec<FriendliContentPart> {
        let mut parts = Vec::new();

        for block in blocks {
            match block {
                ContentBlock::Text { text } => {
                    parts.push(FriendliContentPart::Text { text: text.clone() });
                }
                ContentBlock::Image { source } => match image_source_to_data_url(source) {
                    Ok(url) => {
                        parts.push(FriendliContentPart::ImageUrl {
                            image_url: FriendliImageUrl { url },
                        });
                    }
                    Err(error) => {
                        tracing::warn!(
                            "Skipping image attachment for FriendliAI request: {}",
                            error
                        );
                    }
                },
                ContentBlock::ToolResult { content, .. } => {
                    parts.push(FriendliContentPart::Text {
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
            .unwrap_or(match self.model_type {
                ModelType::Serverless => FRIENDLI_SERVERLESS_BASE_URL,
                ModelType::Dedicated => FRIENDLI_DEDICATED_BASE_URL,
            })
    }

    fn convert_response(&self, response: FriendliResponse) -> ModelResponse {
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

                if let Ok(chunk) = serde_json::from_str::<FriendliStreamChunk>(data) {
                    return Some(Self::convert_stream_chunk(chunk));
                }
            }
        }

        None
    }

    fn convert_stream_chunk(chunk: FriendliStreamChunk) -> StreamChunk {
        let choice = match chunk.choices.into_iter().next() {
            Some(c) => c,
            None => return StreamChunk::Ping,
        };

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

        if let Some(content) = choice.delta.content {
            if !content.is_empty() {
                return StreamChunk::ContentBlockDelta {
                    index: choice.index,
                    delta: ContentDelta::TextDelta { text: content },
                };
            }
        }

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

        if let Some(tool_calls) = choice.delta.tool_calls {
            for tc in tool_calls {
                if let Some(func) = tc.function {
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

#[async_trait]
impl ModelClient for FriendliClient {
    async fn chat(&self, messages: &[Message], tools: &[ToolSpec]) -> ModelResult<ModelResponse> {
        let request = self.build_request(messages, tools, false);
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
                    provider: PROVIDER_NAME.to_string(),
                });
            }

            let body = response.text().await.unwrap_or_default();
            return Err(ProviderError::InvalidResponse(format!(
                "FriendliAI API error {}: {}",
                status, body
            )));
        }

        let api_response: FriendliResponse = response.json().await?;
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
                    provider: PROVIDER_NAME.to_string(),
                });
            }

            let body = response.text().await.unwrap_or_default();
            return Err(ProviderError::InvalidResponse(format!(
                "FriendliAI API error {}: {}",
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

    async fn render_request(
        &self,
        messages: &[Message],
        tools: &[ToolSpec],
    ) -> ModelResult<String> {
        let request = self.build_request(messages, tools, false);
        let mut body = serde_json::to_value(&request).map_err(|e| {
            ProviderError::InvalidResponse(format!("Failed to serialize render request: {}", e))
        })?;

        if let Some(map) = body.as_object_mut() {
            map.remove("stream");
            map.remove("tool_choice");
        }

        let url = format!("{}/chat/render", self.base_url());
        let response = self.client.post(&url).json(&body).send().await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(ProviderError::InvalidResponse(format!(
                "FriendliAI render API error {}: {}",
                status, body
            )));
        }

        let render_response: FriendliRenderResponse = response.json().await?;
        Ok(render_response.text)
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

#[derive(Debug, Serialize)]
struct FriendliRequest {
    model: String,
    messages: Vec<FriendliMessage>,
    max_tokens: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<FriendliTool>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_choice: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stream: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    chat_template_kwargs: Option<HashMap<String, bool>>,
}

#[derive(Debug, Serialize)]
#[serde(untagged)]
enum FriendliMessageContent {
    Text(String),
    Parts(Vec<FriendliContentPart>),
}

#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum FriendliContentPart {
    Text { text: String },
    ImageUrl { image_url: FriendliImageUrl },
}

#[derive(Debug, Serialize)]
struct FriendliImageUrl {
    url: String,
}

#[derive(Debug, Serialize)]
struct FriendliMessage {
    role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    content: Option<FriendliMessageContent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_calls: Option<Vec<FriendliToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_call_id: Option<String>,
}

#[derive(Debug, Serialize)]
struct FriendliTool {
    r#type: String,
    function: FriendliFunction,
}

#[derive(Debug, Serialize)]
struct FriendliFunction {
    name: String,
    description: String,
    parameters: serde_json::Value,
}

#[derive(Debug, Serialize, Deserialize)]
struct FriendliToolCall {
    id: String,
    r#type: String,
    function: FriendliFunctionCall,
}

#[derive(Debug, Serialize, Deserialize)]
struct FriendliFunctionCall {
    name: String,
    arguments: String,
}

#[derive(Debug, Deserialize)]
struct FriendliResponse {
    id: String,
    model: String,
    choices: Vec<FriendliChoice>,
    usage: Option<FriendliUsage>,
}

#[derive(Debug, Default, Deserialize)]
struct FriendliChoice {
    message: FriendliResponseMessage,
    finish_reason: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct FriendliResponseMessage {
    content: Option<String>,
    tool_calls: Option<Vec<FriendliToolCall>>,
}

#[derive(Debug, Deserialize)]
struct FriendliUsage {
    prompt_tokens: u64,
    completion_tokens: u64,
    #[serde(default)]
    cache_read_tokens: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct FriendliStreamChunk {
    id: String,
    model: String,
    choices: Vec<FriendliStreamChoice>,
    #[serde(default)]
    usage: Option<FriendliUsage>,
}

#[derive(Debug, Deserialize)]
struct FriendliStreamChoice {
    index: usize,
    delta: FriendliStreamDelta,
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct FriendliStreamDelta {
    #[serde(default)]
    role: Option<String>,
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    reasoning_content: Option<String>,
    #[serde(default)]
    tool_calls: Option<Vec<FriendliStreamToolCall>>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct FriendliStreamToolCall {
    index: usize,
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    r#type: Option<String>,
    #[serde(default)]
    function: Option<FriendliStreamFunction>,
}

#[derive(Debug, Deserialize)]
struct FriendliStreamFunction {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    arguments: Option<String>,
}

#[derive(Debug, Deserialize)]
struct FriendliRenderResponse {
    text: String,
}
