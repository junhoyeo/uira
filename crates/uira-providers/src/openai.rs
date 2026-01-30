//! OpenAI client implementation with OAuth (Codex) support

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
    StopReason, StreamChunk, StreamMessageStart, TokenUsage, ToolSpec,
};

use crate::{
    traits::ModelResult, traits::ResponseStream, ModelClient, ProviderConfig, ProviderError,
};

const DEFAULT_MAX_TOKENS: usize = 4096;
const PROVIDER_NAME: &str = "openai";
const TOKEN_REFRESH_BUFFER_SECS: i64 = 300;
const OPENAI_OAUTH_TOKEN_URL: &str = "https://auth.openai.com/oauth/token";
const CODEX_CLIENT_ID: &str = "app_EMoamEEZ73f0CkXaXp7hrann";

#[derive(Debug, Clone)]
enum CredentialSource {
    OAuth {
        access_token: SecretString,
        refresh_token: Option<SecretString>,
        expires_at: Option<i64>,
        account_id: Option<String>,
    },
    ApiKey(SecretString),
}

/// OpenAI API client with OAuth (Codex) support
pub struct OpenAIClient {
    client: Client,
    config: ProviderConfig,
    credential: Arc<RwLock<CredentialSource>>,
    refresh_lock: Mutex<()>,
}

impl OpenAIClient {
    pub fn new(config: ProviderConfig) -> Result<Self, ProviderError> {
        let credential = Self::load_credential(&config)?;

        let mut headers = reqwest::header::HeaderMap::new();
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
                            account_id: None,
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

        if let Ok(key) = std::env::var("OPENAI_API_KEY") {
            return Ok(CredentialSource::ApiKey(SecretString::from(key)));
        }

        Err(ProviderError::Configuration(
            "No OpenAI credentials found. Set OPENAI_API_KEY or authenticate via OAuth.".into(),
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
            .post(OPENAI_OAUTH_TOKEN_URL)
            .form(&[
                ("grant_type", "refresh_token"),
                ("refresh_token", refresh_token.expose_secret()),
                ("client_id", CODEX_CLIENT_ID),
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
            id_token: Option<String>,
        }

        let token_response: TokenResponse = response.json().await.map_err(|e| {
            ProviderError::InvalidResponse(format!("Invalid token response: {}", e))
        })?;

        let expires_at = token_response
            .expires_in
            .map(|exp| Utc::now().timestamp() + exp);

        let account_id = token_response
            .id_token
            .as_ref()
            .and_then(|t| Self::extract_account_id_from_jwt(t));

        let old_account_id = {
            let cred = self.credential.read().await;
            if let CredentialSource::OAuth { account_id, .. } = &*cred {
                account_id.clone()
            } else {
                None
            }
        };

        let mut credential = self.credential.write().await;
        *credential = CredentialSource::OAuth {
            access_token: SecretString::from(token_response.access_token.clone()),
            refresh_token: token_response
                .refresh_token
                .clone()
                .map(SecretString::from)
                .or_else(|| Some(refresh_token.clone())),
            expires_at,
            account_id: account_id.or(old_account_id),
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
                        .map(uira_auth::secrecy::SecretString::from)
                        .or_else(|| {
                            Some(uira_auth::secrecy::SecretString::from(
                                old_refresh_token_str,
                            ))
                        }),
                    expires_at,
                },
            );
            let _ = store.save();
        }

        Ok(())
    }

    fn extract_account_id_from_jwt(token: &str) -> Option<String> {
        let parts: Vec<&str> = token.split('.').collect();
        if parts.len() != 3 {
            return None;
        }

        use base64::Engine;
        let payload = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(parts[1])
            .ok()?;

        #[derive(Deserialize)]
        struct JwtClaims {
            chatgpt_account_id: Option<String>,
            #[serde(rename = "https://api.openai.com/auth")]
            auth_claim: Option<AuthClaim>,
            organizations: Option<Vec<OrgClaim>>,
        }

        #[derive(Deserialize)]
        struct AuthClaim {
            chatgpt_account_id: Option<String>,
        }

        #[derive(Deserialize)]
        struct OrgClaim {
            id: String,
        }

        let claims: JwtClaims = serde_json::from_slice(&payload).ok()?;

        claims
            .chatgpt_account_id
            .or_else(|| claims.auth_claim.and_then(|a| a.chatgpt_account_id))
            .or_else(|| {
                claims
                    .organizations
                    .and_then(|orgs| orgs.first().map(|o| o.id.clone()))
            })
    }

    async fn get_auth_headers(&self) -> Result<Vec<(String, String)>, ProviderError> {
        self.refresh_token_if_needed().await?;

        let credential = self.credential.read().await;
        match &*credential {
            CredentialSource::OAuth {
                access_token,
                account_id,
                ..
            } => {
                let mut headers = vec![(
                    "Authorization".to_string(),
                    format!("Bearer {}", access_token.expose_secret()),
                )];

                if let Some(acc_id) = account_id {
                    headers.push(("ChatGPT-Account-Id".to_string(), acc_id.clone()));
                }

                Ok(headers)
            }
            CredentialSource::ApiKey(key) => Ok(vec![(
                "Authorization".to_string(),
                format!("Bearer {}", key.expose_secret()),
            )]),
        }
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
        let auth_headers = self.get_auth_headers().await?;
        let request = self.build_request(messages, tools, false);
        let url = format!("{}/v1/chat/completions", self.base_url());

        let mut req_builder = self.client.post(&url).json(&request);
        for (key, value) in auth_headers {
            req_builder = req_builder.header(&key, &value);
        }

        let response = req_builder.send().await?;

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
        let auth_headers = self.get_auth_headers().await?;
        let request = self.build_request(messages, tools, true);
        let url = format!("{}/v1/chat/completions", self.base_url());

        let mut req_builder = self.client.post(&url).json(&request);
        for (key, value) in auth_headers {
            req_builder = req_builder.header(&key, &value);
        }

        let response = req_builder.send().await?;

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
        128_000
    }

    fn model(&self) -> &str {
        &self.config.model
    }

    fn provider(&self) -> &str {
        PROVIDER_NAME
    }
}

impl OpenAIClient {
    fn parse_sse_event(text: &str) -> Result<StreamChunk, ProviderError> {
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

        if let Some(content) = choice.delta.content {
            if !content.is_empty() {
                return StreamChunk::ContentBlockDelta {
                    index: choice.index,
                    delta: ContentDelta::TextDelta { text: content },
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
