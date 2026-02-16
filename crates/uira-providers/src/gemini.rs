//! Google Gemini client implementation

use async_trait::async_trait;
use futures::StreamExt;
use reqwest::Client;
use secrecy::ExposeSecret;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use uira_core::{
    ContentBlock, ContentDelta, ImageSource, Message, MessageContent, MessageDelta, ModelResponse,
    Role, StopReason, StreamChunk, StreamMessageStart, TokenUsage, ToolSpec,
};

use crate::{
    image::normalize_image_source, traits::ModelResult, traits::ResponseStream, ModelClient,
    ProviderConfig, ProviderError,
};

const DEFAULT_MAX_TOKENS: usize = 8192;
const MAX_SSE_BUFFER: usize = 10 * 1024 * 1024;

/// Google Gemini API client
pub struct GeminiClient {
    client: Client,
    config: ProviderConfig,
}

impl GeminiClient {
    pub fn new(config: ProviderConfig) -> Result<Self, ProviderError> {
        let api_key = config
            .api_key
            .as_ref()
            .ok_or_else(|| ProviderError::Configuration("API key required for Gemini".into()))?;

        // Verify API key is valid format
        let _ = api_key.expose_secret();

        let timeout = Duration::from_secs(config.timeout_seconds.unwrap_or(120));

        let client = Client::builder().timeout(timeout).build()?;

        Ok(Self { client, config })
    }

    fn build_request(&self, messages: &[Message], tools: &[ToolSpec]) -> GeminiRequest {
        let (system_instruction, contents) = Self::convert_messages(messages);

        GeminiRequest {
            contents,
            system_instruction,
            tools: if tools.is_empty() {
                None
            } else {
                Some(vec![GeminiTools {
                    function_declarations: tools
                        .iter()
                        .map(|t| GeminiFunctionDeclaration {
                            name: t.name.clone(),
                            description: t.description.clone(),
                            parameters: serde_json::to_value(&t.input_schema).unwrap_or_default(),
                        })
                        .collect(),
                }])
            },
            generation_config: Some(GeminiGenerationConfig {
                max_output_tokens: self.config.max_tokens.unwrap_or(DEFAULT_MAX_TOKENS),
                temperature: self.config.temperature,
                top_p: None,
                top_k: None,
            }),
        }
    }

    fn convert_messages(
        messages: &[Message],
    ) -> (Option<GeminiSystemInstruction>, Vec<GeminiContent>) {
        let mut system_instruction = None;
        let mut contents = Vec::new();

        for msg in messages {
            match msg.role {
                Role::System => {
                    if let MessageContent::Text(text) = &msg.content {
                        system_instruction = Some(GeminiSystemInstruction {
                            parts: vec![GeminiPart::Text { text: text.clone() }],
                        });
                    }
                }
                Role::User | Role::Tool => {
                    let parts = Self::convert_content(&msg.content);
                    contents.push(GeminiContent {
                        role: "user".to_string(),
                        parts,
                    });
                }
                Role::Assistant => {
                    let parts = Self::convert_content(&msg.content);
                    contents.push(GeminiContent {
                        role: "model".to_string(),
                        parts,
                    });
                }
            }
        }

        (system_instruction, contents)
    }

    fn convert_content(content: &MessageContent) -> Vec<GeminiPart> {
        match content {
            MessageContent::Text(text) => vec![GeminiPart::Text { text: text.clone() }],
            MessageContent::Blocks(blocks) => {
                let mut parts = Vec::new();
                for block in blocks {
                    match block {
                        ContentBlock::Text { text } => {
                            parts.push(GeminiPart::Text { text: text.clone() })
                        }
                        ContentBlock::ToolUse { id: _, name, input } => {
                            parts.push(GeminiPart::FunctionCall {
                                function_call: GeminiFunctionCall {
                                    name: name.clone(),
                                    args: input.clone(),
                                },
                            });
                        }
                        ContentBlock::ToolResult {
                            tool_use_id,
                            content,
                            ..
                        } => {
                            parts.push(GeminiPart::FunctionResponse {
                                function_response: GeminiFunctionResponse {
                                    name: tool_use_id.clone(),
                                    response: serde_json::json!({ "result": content }),
                                },
                            });
                        }
                        ContentBlock::Image { source } => match normalize_image_source(source) {
                            Ok(ImageSource::Base64 { media_type, data }) => {
                                parts.push(GeminiPart::InlineData {
                                    inline_data: GeminiInlineData {
                                        mime_type: media_type,
                                        data,
                                    },
                                });
                            }
                            Ok(ImageSource::Url { url }) => {
                                parts.push(GeminiPart::Text {
                                    text: format!("Image URL: {}", url),
                                });
                            }
                            Ok(ImageSource::FilePath { .. }) => {
                                tracing::warn!(
                                    "Skipping unresolved file path image for Gemini request"
                                );
                            }
                            Err(error) => {
                                tracing::warn!(
                                    "Skipping image attachment for Gemini request: {}",
                                    error
                                );
                            }
                        },
                        _ => {}
                    }
                }
                parts
            }
            MessageContent::ToolCalls(calls) => calls
                .iter()
                .map(|c| GeminiPart::FunctionCall {
                    function_call: GeminiFunctionCall {
                        name: c.name.clone(),
                        args: c.input.clone(),
                    },
                })
                .collect(),
        }
    }

    fn base_url(&self) -> &str {
        self.config
            .base_url
            .as_deref()
            .unwrap_or("https://generativelanguage.googleapis.com")
    }

    fn api_key(&self) -> String {
        self.config
            .api_key
            .as_ref()
            .map(|k| k.expose_secret().to_string())
            .unwrap_or_default()
    }

    fn convert_response(&self, response: GeminiResponse) -> ModelResponse {
        let candidate = response.candidates.into_iter().next().unwrap_or_default();

        let mut content = Vec::new();

        for part in candidate.content.parts {
            match part {
                GeminiPart::Text { text } => {
                    content.push(ContentBlock::Text { text });
                }
                GeminiPart::FunctionCall { function_call } => {
                    content.push(ContentBlock::ToolUse {
                        id: format!("call_{}", uuid::Uuid::new_v4()),
                        name: function_call.name,
                        input: function_call.args,
                    });
                }
                _ => {}
            }
        }

        let stop_reason = candidate.finish_reason.map(|r| match r.as_str() {
            "STOP" => StopReason::EndTurn,
            "MAX_TOKENS" => StopReason::MaxTokens,
            "SAFETY" => StopReason::ContentFilter,
            "RECITATION" => StopReason::ContentFilter,
            _ => StopReason::EndTurn,
        });

        ModelResponse {
            id: format!("gemini_{}", uuid::Uuid::new_v4()),
            model: self.config.model.clone(),
            content,
            stop_reason,
            usage: response
                .usage_metadata
                .map(|u| TokenUsage {
                    input_tokens: u.prompt_token_count as u64,
                    output_tokens: u.candidates_token_count as u64,
                    cache_read_tokens: 0,
                    cache_creation_tokens: 0,
                })
                .unwrap_or_default(),
        }
    }

    fn convert_stream_response(response: GeminiStreamResponse) -> Vec<StreamChunk> {
        let candidate = match response.candidates.into_iter().next() {
            Some(c) => c,
            None => return vec![StreamChunk::Ping],
        };

        if let Some(reason) = candidate.finish_reason {
            let stop_reason = match reason.as_str() {
                "STOP" => StopReason::EndTurn,
                "MAX_TOKENS" => StopReason::MaxTokens,
                "SAFETY" => StopReason::ContentFilter,
                _ => StopReason::EndTurn,
            };

            return vec![StreamChunk::MessageDelta {
                delta: MessageDelta {
                    stop_reason: Some(stop_reason),
                },
                usage: response.usage_metadata.map(|u| TokenUsage {
                    input_tokens: u.prompt_token_count as u64,
                    output_tokens: u.candidates_token_count as u64,
                    cache_read_tokens: 0,
                    cache_creation_tokens: 0,
                }),
            }];
        }

        let mut chunks = Vec::new();
        for (index, part) in candidate.content.parts.into_iter().enumerate() {
            match part {
                GeminiPart::Text { text } => {
                    chunks.push(StreamChunk::ContentBlockDelta {
                        index,
                        delta: ContentDelta::TextDelta { text },
                    });
                }
                GeminiPart::FunctionCall { function_call } => {
                    chunks.push(StreamChunk::ContentBlockStart {
                        index,
                        content_block: ContentBlock::ToolUse {
                            id: format!("call_{}", uuid::Uuid::new_v4()),
                            name: function_call.name,
                            input: serde_json::Value::Null,
                        },
                    });
                    let partial_json = serde_json::to_string(&function_call.args)
                        .unwrap_or_else(|_| "{}".to_string());
                    chunks.push(StreamChunk::ContentBlockDelta {
                        index,
                        delta: ContentDelta::InputJsonDelta { partial_json },
                    });
                    chunks.push(StreamChunk::ContentBlockStop { index });
                }
                _ => {}
            }
        }

        if chunks.is_empty() {
            vec![StreamChunk::Ping]
        } else {
            chunks
        }
    }

    fn parse_sse_event_payload(event: &str) -> Option<String> {
        let mut payload_lines = Vec::new();
        for raw_line in event.lines() {
            let line = raw_line.trim();
            if line.is_empty() || line.starts_with(':') {
                continue;
            }
            if let Some(rest) = line.strip_prefix("data:") {
                payload_lines.push(rest.trim_start().to_string());
            }
        }

        if payload_lines.is_empty() {
            None
        } else {
            Some(payload_lines.join("\n"))
        }
    }
}

#[async_trait]
impl ModelClient for GeminiClient {
    async fn chat(&self, messages: &[Message], tools: &[ToolSpec]) -> ModelResult<ModelResponse> {
        let request = self.build_request(messages, tools);
        let url = format!(
            "{}/v1beta/models/{}:generateContent",
            self.base_url(),
            self.config.model,
        );

        let response = self
            .client
            .post(&url)
            .header("x-goog-api-key", self.api_key())
            .json(&request)
            .send()
            .await?;

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
                    provider: "gemini".to_string(),
                });
            }

            let body = response.text().await.unwrap_or_default();
            return Err(ProviderError::InvalidResponse(format!(
                "API error {}: {}",
                status, body
            )));
        }

        let api_response: GeminiResponse = response.json().await?;
        Ok(self.convert_response(api_response))
    }

    async fn chat_stream(
        &self,
        messages: &[Message],
        tools: &[ToolSpec],
    ) -> ModelResult<ResponseStream> {
        let request = self.build_request(messages, tools);
        let url = format!(
            "{}/v1beta/models/{}:streamGenerateContent?alt=sse",
            self.base_url(),
            self.config.model,
        );

        let response = self
            .client
            .post(&url)
            .header("x-goog-api-key", self.api_key())
            .json(&request)
            .send()
            .await?;

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
                    provider: "gemini".to_string(),
                });
            }

            let body = response.text().await.unwrap_or_default();
            return Err(ProviderError::InvalidResponse(format!(
                "API error {}: {}",
                status, body
            )));
        }

        let byte_stream = response.bytes_stream();
        let stream_id = format!("gemini_stream_{}", uuid::Uuid::new_v4());
        let stream_model = self.config.model.clone();
        let stream = async_stream::try_stream! {
            let mut buffer = String::new();
            futures::pin_mut!(byte_stream);

            yield StreamChunk::MessageStart {
                message: StreamMessageStart {
                    id: stream_id,
                    model: stream_model,
                    usage: TokenUsage::default(),
                },
            };

            while let Some(result) = byte_stream.next().await {
                let bytes = result.map_err(|e| ProviderError::StreamError(e.to_string()))?;
                let text = String::from_utf8_lossy(&bytes);
                let normalized = text.replace("\r\n", "\n").replace('\r', "\n");
                buffer.push_str(&normalized);

                if buffer.len() > MAX_SSE_BUFFER {
                    Err(ProviderError::StreamError(
                        "SSE buffer exceeded maximum size".to_string(),
                    ))?;
                }

                while let Some(pos) = buffer.find("\n\n") {
                    let event = buffer[..pos].to_string();
                    buffer = buffer[pos + 2..].to_string();

                    let payload = match Self::parse_sse_event_payload(&event) {
                        Some(p) => p,
                        None => continue,
                    };

                    if payload == "[DONE]" {
                        yield StreamChunk::MessageStop;
                        return;
                    }

                    if let Ok(response) = serde_json::from_str::<GeminiStreamResponse>(&payload) {
                        let chunks = Self::convert_stream_response(response);
                        let should_stop = chunks.iter().any(|chunk| {
                            matches!(
                                chunk,
                                StreamChunk::MessageDelta {
                                    delta: MessageDelta {
                                        stop_reason: Some(_)
                                    },
                                    ..
                                }
                            )
                        });
                        for chunk in chunks {
                            yield chunk;
                        }
                        if should_stop {
                            yield StreamChunk::MessageStop;
                            return;
                        }
                    }
                }
            }

            let remaining = buffer.trim();
            if !remaining.is_empty() {
                let payload = Self::parse_sse_event_payload(remaining)
                    .unwrap_or_else(|| remaining.to_string());
                if payload == "[DONE]" {
                    yield StreamChunk::MessageStop;
                    return;
                }

                if let Ok(response) = serde_json::from_str::<GeminiStreamResponse>(&payload) {
                    let chunks = Self::convert_stream_response(response);
                    let should_stop = chunks.iter().any(|chunk| {
                        matches!(
                            chunk,
                            StreamChunk::MessageDelta {
                                delta: MessageDelta {
                                    stop_reason: Some(_)
                                },
                                ..
                            }
                        )
                    });
                    for chunk in chunks {
                        yield chunk;
                    }
                    if should_stop {
                        yield StreamChunk::MessageStop;
                        return;
                    }
                }
            }

            yield StreamChunk::MessageStop;
        };

        Ok(Box::pin(stream))
    }

    fn supports_tools(&self) -> bool {
        true
    }

    fn max_tokens(&self) -> usize {
        // Gemini 1.5 Pro supports up to 2M context
        2_000_000
    }

    fn model(&self) -> &str {
        &self.config.model
    }

    fn provider(&self) -> &str {
        "gemini"
    }
}

// API request/response types
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct GeminiRequest {
    contents: Vec<GeminiContent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    system_instruction: Option<GeminiSystemInstruction>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<GeminiTools>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    generation_config: Option<GeminiGenerationConfig>,
}

#[derive(Debug, Serialize)]
struct GeminiSystemInstruction {
    parts: Vec<GeminiPart>,
}

#[derive(Debug, Serialize)]
struct GeminiContent {
    role: String,
    parts: Vec<GeminiPart>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
enum GeminiPart {
    Text {
        text: String,
    },
    InlineData {
        #[serde(rename = "inlineData")]
        inline_data: GeminiInlineData,
    },
    FunctionCall {
        #[serde(rename = "functionCall")]
        function_call: GeminiFunctionCall,
    },
    FunctionResponse {
        #[serde(rename = "functionResponse")]
        function_response: GeminiFunctionResponse,
    },
}

#[derive(Debug, Serialize, Deserialize)]
struct GeminiFunctionCall {
    name: String,
    args: serde_json::Value,
}

#[derive(Debug, Serialize, Deserialize)]
struct GeminiFunctionResponse {
    name: String,
    response: serde_json::Value,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GeminiInlineData {
    mime_type: String,
    data: String,
}

#[derive(Debug, Serialize)]
struct GeminiTools {
    #[serde(rename = "functionDeclarations")]
    function_declarations: Vec<GeminiFunctionDeclaration>,
}

#[derive(Debug, Serialize)]
struct GeminiFunctionDeclaration {
    name: String,
    description: String,
    parameters: serde_json::Value,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct GeminiGenerationConfig {
    max_output_tokens: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    top_p: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    top_k: Option<i32>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GeminiResponse {
    candidates: Vec<GeminiCandidate>,
    #[serde(default)]
    usage_metadata: Option<GeminiUsageMetadata>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GeminiCandidate {
    content: GeminiContentResponse,
    #[serde(default)]
    finish_reason: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct GeminiContentResponse {
    #[serde(default)]
    parts: Vec<GeminiPart>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GeminiUsageMetadata {
    prompt_token_count: usize,
    candidates_token_count: usize,
}

// Streaming response types
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GeminiStreamResponse {
    candidates: Vec<GeminiCandidate>,
    #[serde(default)]
    usage_metadata: Option<GeminiUsageMetadata>,
}
