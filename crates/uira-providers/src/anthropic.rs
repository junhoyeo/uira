//! Anthropic (Claude) client implementation

use async_trait::async_trait;
use chrono::Utc;
use futures::StreamExt;
use reqwest::Client;
use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{Mutex, RwLock};
use uira_auth::CredentialStore;
use uira_protocol::{
    ContentBlock, ContentDelta, Message, MessageContent, MessageDelta, ModelResponse, Role,
    StopReason, StreamChunk, StreamError, StreamMessageStart, TokenUsage, ToolSpec,
};

use crate::{
    traits::ModelResult, traits::ResponseStream, ModelClient, ProviderConfig, ProviderError,
};

const ANTHROPIC_VERSION: &str = "2023-06-01";
const DEFAULT_MAX_TOKENS: usize = 8192;
const PROVIDER_NAME: &str = "anthropic";
/// Buffer time before token expiration to trigger refresh (5 minutes)
const TOKEN_REFRESH_BUFFER_SECS: i64 = 300;

/// Credential source for the client
#[derive(Debug, Clone)]
enum CredentialSource {
    /// OAuth credentials from credential store
    OAuth {
        access_token: SecretString,
        refresh_token: Option<SecretString>,
        expires_at: Option<i64>,
    },
    /// API key from config or environment variable
    ApiKey(SecretString),
}

/// Anthropic API client
pub struct AnthropicClient {
    client: Client,
    config: ProviderConfig,
    /// Current credential source (wrapped in RwLock for token refresh)
    credential: Arc<RwLock<CredentialSource>>,
    refresh_lock: Mutex<()>,
}

impl AnthropicClient {
    pub fn new(config: ProviderConfig) -> Result<Self, ProviderError> {
        let credential = Self::load_credential(&config)?;

        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert("anthropic-version", ANTHROPIC_VERSION.parse().unwrap());
        headers.insert("content-type", "application/json".parse().unwrap());

        let timeout = Duration::from_secs(config.timeout_seconds.unwrap_or(120));

        let client = Client::builder()
            .default_headers(headers)
            .timeout(timeout)
            .build()?;

        Ok(Self {
            client,
            config,
            credential: Arc::new(RwLock::new(credential)),
            refresh_lock: Mutex::new(()),
        })
    }

    fn load_credential(config: &ProviderConfig) -> Result<CredentialSource, ProviderError> {
        if let Ok(store) = CredentialStore::load() {
            if let Some(cred) = store.get(PROVIDER_NAME) {
                use uira_auth::secrecy::ExposeSecret as AuthExposeSecret;
                use uira_auth::StoredCredential;
                match cred {
                    StoredCredential::OAuth {
                        access_token,
                        refresh_token,
                        expires_at,
                    } => {
                        return Ok(CredentialSource::OAuth {
                            access_token: SecretString::from(
                                access_token.expose_secret().to_string(),
                            ),
                            refresh_token: refresh_token
                                .as_ref()
                                .map(|t| SecretString::from(t.expose_secret().to_string())),
                            expires_at: *expires_at,
                        });
                    }
                    StoredCredential::ApiKey { key } => {
                        return Ok(CredentialSource::ApiKey(SecretString::from(
                            key.expose_secret().to_string(),
                        )));
                    }
                }
            }
        }

        if let Some(api_key) = &config.api_key {
            return Ok(CredentialSource::ApiKey(api_key.clone()));
        }

        if let Ok(key) = std::env::var("ANTHROPIC_API_KEY") {
            return Ok(CredentialSource::ApiKey(SecretString::from(key)));
        }

        Err(ProviderError::Configuration(
            "No Anthropic credentials found. Set ANTHROPIC_API_KEY or authenticate via OAuth."
                .into(),
        ))
    }

    fn is_token_expired(expires_at: Option<i64>) -> bool {
        match expires_at {
            Some(exp) => {
                let now = Utc::now().timestamp();
                now >= (exp - TOKEN_REFRESH_BUFFER_SECS)
            }
            None => false,
        }
    }

    async fn refresh_token_if_needed(&self) -> Result<(), ProviderError> {
        let might_need_refresh = {
            let credential = self.credential.read().await;

            if let CredentialSource::OAuth { expires_at, .. } = &*credential {
                Self::is_token_expired(*expires_at)
            } else {
                false
            }
        };

        if !might_need_refresh {
            return Ok(());
        }

        let _guard = self.refresh_lock.lock().await;

        let (needs_refresh, refresh_token_opt) = {
            let credential = self.credential.read().await;

            if let CredentialSource::OAuth {
                refresh_token,
                expires_at,
                ..
            } = &*credential
            {
                (Self::is_token_expired(*expires_at), refresh_token.clone())
            } else {
                (false, None)
            }
        };

        if needs_refresh {
            if let Some(refresh_token) = refresh_token_opt {
                self.do_token_refresh(&refresh_token).await?;
            } else {
                return Err(ProviderError::Configuration(
                    "OAuth token expired and no refresh token available".into(),
                ));
            }
        }

        Ok(())
    }

    async fn do_token_refresh(&self, refresh_token: &SecretString) -> Result<(), ProviderError> {
        let response = self
            .client
            .post("https://console.anthropic.com/v1/oauth/token")
            .form(&[
                ("grant_type", "refresh_token"),
                ("refresh_token", refresh_token.expose_secret()),
            ])
            .send()
            .await
            .map_err(ProviderError::Network)?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(ProviderError::Configuration(format!(
                "Token refresh failed ({}): {}",
                status, body
            )));
        }

        #[derive(Deserialize)]
        struct TokenResponse {
            access_token: String,
            refresh_token: Option<String>,
            expires_in: Option<i64>,
        }

        let token_response: TokenResponse = response.json().await.map_err(|e| {
            ProviderError::InvalidResponse(format!("Invalid token response: {}", e))
        })?;

        let expires_at = token_response
            .expires_in
            .map(|exp| Utc::now().timestamp() + exp);

        let mut credential = self.credential.write().await;
        *credential = CredentialSource::OAuth {
            access_token: SecretString::from(token_response.access_token.clone()),
            refresh_token: token_response
                .refresh_token
                .clone()
                .map(SecretString::from)
                .or_else(|| Some(refresh_token.clone())),
            expires_at,
        };

        let old_refresh_token_str = refresh_token.expose_secret().to_string();
        if let Ok(mut store) = CredentialStore::load() {
            use uira_auth::StoredCredential;
            store.insert(
                PROVIDER_NAME.to_string(),
                StoredCredential::OAuth {
                    access_token: uira_auth::secrecy::SecretString::from(
                        token_response.access_token,
                    ),
                    refresh_token: token_response
                        .refresh_token
                        .clone()
                        .map(uira_auth::secrecy::SecretString::from)
                        .or_else(|| {
                            Some(uira_auth::secrecy::SecretString::from(
                                old_refresh_token_str.clone(),
                            ))
                        }),
                    expires_at,
                },
            );
            let _ = store.save();
        }

        Ok(())
    }

    async fn get_auth_headers(&self) -> Result<Vec<(&'static str, String)>, ProviderError> {
        self.refresh_token_if_needed().await?;

        let credential = self.credential.read().await;
        match &*credential {
            CredentialSource::OAuth { access_token, .. } => Ok(vec![(
                "Authorization",
                format!("Bearer {}", access_token.expose_secret()),
            )]),
            CredentialSource::ApiKey(key) => {
                Ok(vec![("x-api-key", key.expose_secret().to_string())])
            }
        }
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
        let auth_headers = self.get_auth_headers().await?;
        let request = self.build_request(messages, tools, false);
        let url = format!("{}/v1/messages", self.base_url());

        let mut req = self.client.post(&url);
        for (key, value) in auth_headers {
            req = req.header(key, value);
        }
        let response = req.json(&request).send().await?;

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
        let auth_headers = self.get_auth_headers().await?;
        let request = self.build_request(messages, tools, true);
        let url = format!("{}/v1/messages", self.base_url());

        let mut req = self.client.post(&url);
        for (key, value) in auth_headers {
            req = req.header(key, value);
        }
        let response = req.json(&request).send().await?;

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
