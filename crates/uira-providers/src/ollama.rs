//! Ollama client implementation for local LLMs

use async_trait::async_trait;
use futures::StreamExt;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use uira_core::{
    ContentBlock, ContentDelta, ImageSource, Message, MessageContent, MessageDelta, ModelResponse,
    Role, StopReason, StreamChunk, TokenUsage, ToolSpec,
};

use crate::{
    image::normalize_image_source, traits::ModelResult, traits::ResponseStream, ModelClient,
    ProviderConfig, ProviderError,
};

const DEFAULT_MAX_TOKENS: usize = 4096;

/// Ollama API client for local LLMs
pub struct OllamaClient {
    client: Client,
    config: ProviderConfig,
}

impl OllamaClient {
    pub fn new(config: ProviderConfig) -> Result<Self, ProviderError> {
        let timeout = Duration::from_secs(config.timeout_seconds.unwrap_or(300)); // Longer timeout for local models

        let client = Client::builder().timeout(timeout).build()?;

        Ok(Self { client, config })
    }

    fn build_request(
        &self,
        messages: &[Message],
        _tools: &[ToolSpec],
        stream: bool,
    ) -> OllamaRequest {
        OllamaRequest {
            model: self.config.model.clone(),
            messages: messages.iter().map(Self::convert_message).collect(),
            stream,
            options: Some(OllamaOptions {
                num_predict: self.config.max_tokens.unwrap_or(DEFAULT_MAX_TOKENS) as i32,
                temperature: self.config.temperature,
            }),
        }
    }

    fn convert_message(msg: &Message) -> OllamaMessage {
        let role = match msg.role {
            Role::System => "system",
            Role::User | Role::Tool => "user",
            Role::Assistant => "assistant",
        };

        let mut images = Vec::new();

        let content = match &msg.content {
            MessageContent::Text(text) => text.clone(),
            MessageContent::Blocks(blocks) => {
                let mut text_chunks = Vec::new();

                for block in blocks {
                    match block {
                        ContentBlock::Text { text } => text_chunks.push(text.clone()),
                        ContentBlock::ToolResult { content, .. } => {
                            text_chunks.push(content.clone())
                        }
                        ContentBlock::Image { source } => match normalize_image_source(source) {
                            Ok(ImageSource::Base64 { data, .. }) => images.push(data),
                            Ok(ImageSource::Url { url }) => {
                                text_chunks.push(format!("Image URL: {}", url));
                            }
                            Ok(ImageSource::FilePath { .. }) => {
                                tracing::warn!(
                                    "Skipping unresolved file path image for Ollama request"
                                );
                            }
                            Err(error) => {
                                tracing::warn!(
                                    "Skipping image attachment for Ollama request: {}",
                                    error
                                );
                            }
                        },
                        _ => {}
                    }
                }

                text_chunks.join("\n")
            }
            MessageContent::ToolCalls(calls) => calls
                .iter()
                .map(|c| format!("Tool call: {} with {}", c.name, c.input))
                .collect::<Vec<_>>()
                .join("\n"),
        };

        OllamaMessage {
            role: role.to_string(),
            content,
            images: if images.is_empty() {
                None
            } else {
                Some(images)
            },
        }
    }

    fn base_url(&self) -> &str {
        self.config
            .base_url
            .as_deref()
            .unwrap_or("http://localhost:11434")
    }

    fn convert_response(&self, response: OllamaResponse) -> ModelResponse {
        let content = vec![ContentBlock::Text {
            text: response.message.content,
        }];

        let stop_reason = if response.done {
            Some(StopReason::EndTurn)
        } else {
            None
        };

        ModelResponse {
            id: format!("ollama_{}", uuid::Uuid::new_v4()),
            model: response.model,
            content,
            stop_reason,
            usage: TokenUsage {
                input_tokens: response.prompt_eval_count.unwrap_or(0) as u64,
                output_tokens: response.eval_count.unwrap_or(0) as u64,
                cache_read_tokens: 0,
                cache_creation_tokens: 0,
            },
        }
    }

    fn parse_stream_line(line: &str) -> Result<StreamChunk, ProviderError> {
        let line = line.trim();
        if line.is_empty() {
            return Ok(StreamChunk::Ping);
        }

        if let Ok(response) = serde_json::from_str::<OllamaStreamResponse>(line) {
            return Ok(Self::convert_stream_response(response));
        }

        Ok(StreamChunk::Ping)
    }

    fn convert_stream_response(response: OllamaStreamResponse) -> StreamChunk {
        if response.done {
            return StreamChunk::MessageDelta {
                delta: MessageDelta {
                    stop_reason: Some(StopReason::EndTurn),
                },
                usage: Some(TokenUsage {
                    input_tokens: response.prompt_eval_count.unwrap_or(0) as u64,
                    output_tokens: response.eval_count.unwrap_or(0) as u64,
                    cache_read_tokens: 0,
                    cache_creation_tokens: 0,
                }),
            };
        }

        if let Some(msg) = response.message {
            if !msg.content.is_empty() {
                return StreamChunk::ContentBlockDelta {
                    index: 0,
                    delta: ContentDelta::TextDelta { text: msg.content },
                };
            }
        }

        StreamChunk::Ping
    }
}

#[async_trait]
impl ModelClient for OllamaClient {
    async fn chat(&self, messages: &[Message], tools: &[ToolSpec]) -> ModelResult<ModelResponse> {
        let request = self.build_request(messages, tools, false);
        let url = format!("{}/api/chat", self.base_url());

        let response = self.client.post(&url).json(&request).send().await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(ProviderError::InvalidResponse(format!(
                "API error {}: {}",
                status, body
            )));
        }

        let api_response: OllamaResponse = response.json().await?;
        Ok(self.convert_response(api_response))
    }

    async fn chat_stream(
        &self,
        messages: &[Message],
        tools: &[ToolSpec],
    ) -> ModelResult<ResponseStream> {
        let request = self.build_request(messages, tools, true);
        let url = format!("{}/api/chat", self.base_url());

        let response = self.client.post(&url).json(&request).send().await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(ProviderError::InvalidResponse(format!(
                "API error {}: {}",
                status, body
            )));
        }

        let stream = response.bytes_stream().flat_map(|result| {
            let chunks: Vec<Result<StreamChunk, ProviderError>> = match result {
                Ok(bytes) => {
                    let text = String::from_utf8_lossy(&bytes);
                    text.lines()
                        .filter_map(|line| match Self::parse_stream_line(line) {
                            Ok(chunk) if !matches!(chunk, StreamChunk::Ping) => Some(Ok(chunk)),
                            Ok(_) => None,
                            Err(e) => Some(Err(e)),
                        })
                        .collect()
                }
                Err(e) => vec![Err(ProviderError::StreamError(e.to_string()))],
            };
            futures::stream::iter(chunks)
        });

        Ok(Box::pin(stream))
    }

    fn supports_tools(&self) -> bool {
        // Ollama has limited tool support depending on the model
        // Some models like llama3.1 support tools
        false
    }

    fn max_tokens(&self) -> usize {
        // Depends on the model, but most support at least 4k
        self.config.max_tokens.unwrap_or(4096)
    }

    fn model(&self) -> &str {
        &self.config.model
    }

    fn provider(&self) -> &str {
        "ollama"
    }
}

// API request/response types
#[derive(Debug, Serialize)]
struct OllamaRequest {
    model: String,
    messages: Vec<OllamaMessage>,
    stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    options: Option<OllamaOptions>,
}

#[derive(Debug, Serialize)]
struct OllamaMessage {
    role: String,
    content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    images: Option<Vec<String>>,
}

#[derive(Debug, Serialize)]
struct OllamaOptions {
    num_predict: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
}

#[derive(Debug, Deserialize)]
struct OllamaResponse {
    model: String,
    message: OllamaResponseMessage,
    done: bool,
    #[serde(default)]
    prompt_eval_count: Option<usize>,
    #[serde(default)]
    eval_count: Option<usize>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct OllamaResponseMessage {
    role: String,
    content: String,
}

// Streaming response
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct OllamaStreamResponse {
    model: String,
    #[serde(default)]
    message: Option<OllamaStreamMessage>,
    done: bool,
    #[serde(default)]
    prompt_eval_count: Option<usize>,
    #[serde(default)]
    eval_count: Option<usize>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct OllamaStreamMessage {
    role: String,
    content: String,
}
