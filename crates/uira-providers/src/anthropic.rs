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
    beta_features::BetaFeatures,
    error_classify::classify_error,
    image::normalize_image_source,
    payload_log::PayloadLogger,
    response_handling::{extract_retry_after, parse_error_body},
    retry::{with_retry, RetryConfig},
    traits::ModelResult,
    traits::ResponseStream,
    turn_validation::validate_anthropic_turns,
    ModelClient, ProviderConfig, ProviderError,
};

const ANTHROPIC_VERSION: &str = "2023-06-01";
const DEFAULT_MAX_TOKENS: usize = 8192;
const MAX_SSE_BUFFER: usize = 10 * 1024 * 1024; // 10MB SSE buffer cap
const PROVIDER_NAME: &str = "anthropic";
/// Buffer time before token expiration to trigger refresh (5 minutes)
const TOKEN_REFRESH_BUFFER_SECS: i64 = 300;
/// OAuth client ID (same as Claude Code CLI)
const OAUTH_CLIENT_ID: &str = "9d1c250a-e61b-44d9-88ed-5944d1962f5e";
/// User agent to masquerade as Claude Code CLI
const OAUTH_USER_AGENT: &str = "claude-cli/2.1.2 (external, cli)";
/// Tool name prefix for OAuth requests
const TOOL_PREFIX: &str = "mcp_";
/// System prompt prefix required for OAuth (masquerade as Claude Code)
const CLAUDE_CODE_IDENTITY: &str = "You are Claude Code, Anthropic's official CLI for Claude.";

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
        // First check for API key (higher priority for reliability)
        if let Some(api_key) = &config.api_key {
            tracing::debug!("Using API key from config");
            return Ok(CredentialSource::ApiKey(api_key.clone()));
        }

        if let Ok(key) = std::env::var("ANTHROPIC_API_KEY") {
            tracing::debug!("Using API key from ANTHROPIC_API_KEY env var");
            return Ok(CredentialSource::ApiKey(SecretString::from(key)));
        }

        // Fall back to stored credentials (OAuth or stored API key)
        tracing::debug!("No API key found, checking stored credentials");
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

        Err(ProviderError::Configuration(
            "No Anthropic credentials found. Set ANTHROPIC_API_KEY or authenticate via OAuth."
                .into(),
        ))
    }

    fn is_token_expired(expires_at: Option<i64>) -> bool {
        match expires_at {
            Some(exp) => {
                let now = Utc::now().timestamp();
                // Handle both seconds and milliseconds formats
                let exp_secs = if exp > 1_000_000_000_000 {
                    exp / 1000 // Convert milliseconds to seconds
                } else {
                    exp
                };
                now >= (exp_secs - TOKEN_REFRESH_BUFFER_SECS)
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
        #[derive(Serialize)]
        struct TokenRefreshRequest<'a> {
            grant_type: &'a str,
            refresh_token: &'a str,
            client_id: &'a str,
        }

        let response = self
            .client
            .post("https://console.anthropic.com/v1/oauth/token")
            .header("Content-Type", "application/json")
            .json(&TokenRefreshRequest {
                grant_type: "refresh_token",
                refresh_token: refresh_token.expose_secret(),
                client_id: OAUTH_CLIENT_ID,
            })
            .send()
            .await
            .map_err(ProviderError::Network)?;

        if !response.status().is_success() {
            let status = response.status();
            if status.is_server_error() {
                return Err(ProviderError::Unavailable {
                    provider: "anthropic".to_string(),
                });
            }
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
            if let Err(e) = store.save() {
                tracing::error!("Failed to save credential store after token refresh: {}", e);
            }
        }

        Ok(())
    }

    async fn get_auth_headers(&self) -> Result<Vec<(&'static str, String)>, ProviderError> {
        self.refresh_token_if_needed().await?;

        let credential = self.credential.read().await;
        match &*credential {
            CredentialSource::OAuth { access_token, .. } => {
                let beta = BetaFeatures::oauth_default().to_header_value();
                Ok(vec![
                    (
                        "Authorization",
                        format!("Bearer {}", access_token.expose_secret()),
                    ),
                    ("anthropic-beta", beta),
                    ("anthropic-product", "claude-code".to_string()),
                    ("user-agent", OAUTH_USER_AGENT.to_string()),
                ])
            }
            CredentialSource::ApiKey(key) => {
                Ok(vec![("x-api-key", key.expose_secret().to_string())])
            }
        }
    }

    async fn is_using_oauth(&self) -> bool {
        let credential = self.credential.read().await;
        matches!(&*credential, CredentialSource::OAuth { .. })
    }

    fn build_request(
        &self,
        messages: &[Message],
        tools: &[ToolSpec],
        stream: bool,
        is_oauth: bool,
    ) -> Result<AnthropicRequest, ProviderError> {
        let (system, messages) = Self::extract_system(messages);

        let system_prompt = if is_oauth {
            let mut blocks = vec![SystemBlock {
                block_type: "text".to_string(),
                text: CLAUDE_CODE_IDENTITY.to_string(),
                cache_control: CacheControl {
                    cache_type: "ephemeral".to_string(),
                },
            }];
            if let Some(existing) = system {
                let sanitized = Self::sanitize_system_for_oauth(&existing);
                blocks.push(SystemBlock {
                    block_type: "text".to_string(),
                    text: sanitized,
                    cache_control: CacheControl {
                        cache_type: "ephemeral".to_string(),
                    },
                });
            }
            Some(SystemPrompt::Blocks(blocks))
        } else {
            system.map(SystemPrompt::Text)
        };

        // CRITICAL: ThinkingConfig guard - force temperature = None when thinking enabled
        let (thinking, temperature) = if self.config.enable_thinking {
            let budget = self.config.thinking_budget.unwrap_or(64_000);
            (Some(ThinkingConfig::enabled(budget)), None)
        } else {
            (None, self.config.temperature)
        };

        Ok(AnthropicRequest {
            model: self.config.model.clone(),
            max_tokens: self.config.max_tokens.unwrap_or(DEFAULT_MAX_TOKENS),
            messages: messages
                .into_iter()
                .map(|m| self.convert_message(m, is_oauth))
                .collect::<Result<Vec<_>, _>>()?,
            system: system_prompt,
            tools: if tools.is_empty() {
                None
            } else {
                Some(tools.to_vec())
            },
            stream: Some(stream),
            temperature,
            thinking,
        })
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

    fn convert_message(
        &self,
        msg: &Message,
        is_oauth: bool,
    ) -> Result<AnthropicMessage, ProviderError> {
        let role = match msg.role {
            Role::User | Role::Tool => "user",
            Role::Assistant => "assistant",
            Role::System => "user", // Should be filtered out
        };

        let maybe_prefix = |name: &str| -> String {
            if is_oauth {
                format!("{}{}", TOOL_PREFIX, name)
            } else {
                name.to_string()
            }
        };

        let content = match &msg.content {
            MessageContent::Text(text) => vec![AnthropicContent::Text { text: text.clone() }],
            MessageContent::Blocks(blocks) => {
                let mut content = Vec::with_capacity(blocks.len());
                for block in blocks {
                    let converted = match block {
                        ContentBlock::Text { text } => {
                            AnthropicContent::Text { text: text.clone() }
                        }
                        ContentBlock::ToolUse { id, name, input } => AnthropicContent::ToolUse {
                            id: id.clone(),
                            name: maybe_prefix(name),
                            input: Self::normalize_tool_input(input),
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
                            source: normalize_image_source(source)?,
                        },
                        ContentBlock::Thinking {
                            thinking,
                            signature,
                        } => AnthropicContent::Thinking {
                            thinking: thinking.clone(),
                            signature: signature.clone(),
                        },
                    };
                    content.push(converted);
                }
                content
            }
            MessageContent::ToolCalls(calls) => calls
                .iter()
                .map(|c| AnthropicContent::ToolUse {
                    id: c.id.clone(),
                    name: maybe_prefix(&c.name),
                    input: Self::normalize_tool_input(&c.input),
                })
                .collect(),
        };

        Ok(AnthropicMessage {
            role: role.to_string(),
            content,
        })
    }

    fn validated_messages_with_system(messages: &[Message]) -> Vec<Message> {
        let mut system_messages: Vec<Message> = Vec::new();
        let mut non_system: Vec<Message> = Vec::new();

        for msg in messages {
            if msg.role == Role::System {
                system_messages.push(msg.clone());
            } else {
                non_system.push(msg.clone());
            }
        }

        let validated = validate_anthropic_turns(&non_system);

        let mut result = system_messages;
        result.extend(validated);
        result
    }

    fn base_url(&self) -> &str {
        self.config
            .base_url
            .as_deref()
            .unwrap_or("https://api.anthropic.com")
    }

    fn prefix_tool_names(tools: &[ToolSpec]) -> Vec<ToolSpec> {
        tools
            .iter()
            .map(|tool| {
                let mut prefixed = tool.clone();
                prefixed.name = format!("{}{}", TOOL_PREFIX, tool.name);
                prefixed
            })
            .collect()
    }

    fn strip_tool_prefix(name: &str) -> String {
        name.strip_prefix(TOOL_PREFIX).unwrap_or(name).to_string()
    }

    fn strip_tool_prefix_from_response(response: &mut ModelResponse) {
        for block in &mut response.content {
            if let ContentBlock::ToolUse { name, .. } = block {
                *name = Self::strip_tool_prefix(name);
            }
        }
    }

    fn strip_tool_prefix_from_sse(text: &str) -> String {
        use regex::Regex;
        lazy_static::lazy_static! {
            static ref TOOL_NAME_RE: Regex = Regex::new(r#""name"\s*:\s*"mcp_([^"]+)""#).unwrap();
        }
        TOOL_NAME_RE
            .replace_all(text, r#""name": "$1""#)
            .to_string()
    }

    fn normalize_tool_input(input: &serde_json::Value) -> serde_json::Value {
        match input {
            serde_json::Value::Object(_) => input.clone(),
            serde_json::Value::Null => serde_json::Value::Object(serde_json::Map::new()),
            serde_json::Value::String(s) => {
                if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(s) {
                    if parsed.is_object() {
                        return parsed;
                    }
                    return serde_json::json!({ "value": parsed });
                }
                serde_json::json!({ "value": s })
            }
            other => serde_json::json!({ "value": other }),
        }
    }

    fn sanitize_system_for_oauth(system: &str) -> String {
        system
            .replace("OpenCode", "Claude Code")
            .replace("opencode", "Claude")
    }
}

#[async_trait]
impl ModelClient for AnthropicClient {
    async fn chat(&self, messages: &[Message], tools: &[ToolSpec]) -> ModelResult<ModelResponse> {
        let validated_messages = Self::validated_messages_with_system(messages);
        let retry_config = RetryConfig {
            max_attempts: self.config.max_retries.unwrap_or(3),
            ..RetryConfig::default()
        };
        let logger = PayloadLogger::from_env();

        with_retry(&retry_config, || async {
            let auth_headers = self.get_auth_headers().await?;
            let is_oauth = self.is_using_oauth().await;

            let tools_for_request = if is_oauth {
                Self::prefix_tool_names(tools)
            } else {
                tools.to_vec()
            };

            let request =
                self.build_request(&validated_messages, &tools_for_request, false, is_oauth)?;
            let url = if is_oauth {
                format!("{}/v1/messages?beta=true", self.base_url())
            } else {
                format!("{}/v1/messages", self.base_url())
            };

            tracing::debug!(
                "AnthropicClient::chat: base_url={}, full_url={}, is_oauth={}",
                self.base_url(),
                url,
                is_oauth
            );

            if let Ok(request_json) = serde_json::to_value(&request) {
                logger.log_request(None, PROVIDER_NAME, &self.config.model, &request_json);
            }

            let mut req = self.client.post(&url);
            for (key, value) in &auth_headers {
                req = req.header(*key, value);
            }
            let response = req.json(&request).send().await?;

            if !response.status().is_success() {
                let status = response.status().as_u16();
                let retry_after = extract_retry_after(response.headers());
                let body = parse_error_body(response).await;

                let mut error = classify_error(status, &body);
                if let ProviderError::RateLimited {
                    ref mut retry_after_ms,
                } = error
                {
                    if let Some(ra) = retry_after {
                        *retry_after_ms = ra;
                    }
                }
                return Err(error);
            }

            let api_response: AnthropicResponse = response.json().await?;
            let mut model_response = self.convert_response(api_response);

            if is_oauth {
                Self::strip_tool_prefix_from_response(&mut model_response);
            }

            Ok(model_response)
        })
        .await
    }

    async fn chat_stream(
        &self,
        messages: &[Message],
        tools: &[ToolSpec],
    ) -> ModelResult<ResponseStream> {
        let validated_messages = Self::validated_messages_with_system(messages);
        let retry_config = RetryConfig {
            max_attempts: self.config.max_retries.unwrap_or(3),
            ..RetryConfig::default()
        };
        let logger = PayloadLogger::from_env();

        let (response, is_oauth) = with_retry(&retry_config, || async {
            let auth_headers = self.get_auth_headers().await?;
            let is_oauth = self.is_using_oauth().await;

            let tools_for_request = if is_oauth {
                Self::prefix_tool_names(tools)
            } else {
                tools.to_vec()
            };

            let request =
                self.build_request(&validated_messages, &tools_for_request, true, is_oauth)?;
            let url = if is_oauth {
                format!("{}/v1/messages?beta=true", self.base_url())
            } else {
                format!("{}/v1/messages", self.base_url())
            };

            if let Ok(request_json) = serde_json::to_value(&request) {
                logger.log_request(None, PROVIDER_NAME, &self.config.model, &request_json);
            }

            let mut req = self.client.post(&url);
            for (key, value) in &auth_headers {
                req = req.header(*key, value);
            }
            let response = req.json(&request).send().await?;

            if !response.status().is_success() {
                let status = response.status().as_u16();
                let retry_after = extract_retry_after(response.headers());
                let body = parse_error_body(response).await;

                let mut error = classify_error(status, &body);
                if let ProviderError::RateLimited {
                    ref mut retry_after_ms,
                } = error
                {
                    if let Some(ra) = retry_after {
                        *retry_after_ms = ra;
                    }
                }
                return Err(error);
            }

            Ok((response, is_oauth))
        })
        .await?;

        tracing::debug!("Starting SSE stream from Anthropic API");
        let byte_stream = response.bytes_stream();
        let stream = async_stream::try_stream! {
            let mut buffer = String::new();
            let mut consecutive_parse_errors: usize = 0;
            let mut _received_message_stop = false;
            const MAX_CONSECUTIVE_PARSE_ERRORS: usize = 10;
            
            futures::pin_mut!(byte_stream);

            while let Some(result) = byte_stream.next().await {
                let bytes = match result {
                    Ok(b) => b,
                    Err(e) => {
                        // Network error mid-stream - NO RETRY per Metis decision
                        tracing::warn!("SSE network error (no retry mid-stream): {}", e);
                        yield StreamChunk::Error {
                            error: StreamError {
                                r#type: "network_error".to_string(),
                                message: e.to_string(),
                            },
                        };
                        continue;
                    }
                };
                
                let text = String::from_utf8_lossy(&bytes);
                let text = text.replace("\r\n", "\n");
                buffer.push_str(&text);

                if buffer.len() > MAX_SSE_BUFFER {
                    Err(ProviderError::StreamError(
                        "SSE buffer exceeded maximum size".to_string(),
                    ))?;
                }

                // Process complete SSE events (separated by double newline)
                while let Some(pos) = buffer.find("\n\n") {
                    let event_text = buffer[..pos].to_string();
                    buffer = buffer[pos + 2..].to_string();

                    // Parse each line in the event
                    for line in event_text.lines() {
                        let trimmed = line.trim_end_matches('\r');
                        if let Some(data) = trimmed
                            .strip_prefix("data: ")
                            .or_else(|| trimmed.strip_prefix("data:"))
                            .map(str::trim_start)
                        {
                            let data = if is_oauth {
                                Self::strip_tool_prefix_from_sse(data)
                            } else {
                                data.to_string()
                            };
                            if data == "[DONE]" {
                                _received_message_stop = true;
                                yield StreamChunk::MessageStop;
                                return;
                            }
                            match serde_json::from_str::<AnthropicStreamEvent>(&data) {
                                Ok(event) => {
                                    tracing::debug!("Anthropic SSE event: {:?}", event);
                                    
                                    // Track MessageStop event
                                    if matches!(event, AnthropicStreamEvent::MessageStop) {
                                        _received_message_stop = true;
                                    }
                                    
                                    // Reset consecutive parse errors on successful parse
                                    consecutive_parse_errors = 0;
                                    
                                    yield event.into();
                                }
                                Err(e) => {
                                    if !data.trim().is_empty() && !data.starts_with(':') {
                                        tracing::debug!("SSE parse error: {} for data: {}", e, data);
                                        
                                        // Increment consecutive parse errors
                                        consecutive_parse_errors += 1;
                                        if consecutive_parse_errors > MAX_CONSECUTIVE_PARSE_ERRORS {
                                            Err(ProviderError::StreamError(
                                                format!("Too many consecutive parse errors: {}", consecutive_parse_errors)
                                            ))?;
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // Process any remaining data in buffer
            if !buffer.is_empty() {
                for line in buffer.lines() {
                    let trimmed = line.trim_end_matches('\r');
                    if let Some(data) = trimmed
                        .strip_prefix("data: ")
                        .or_else(|| trimmed.strip_prefix("data:"))
                        .map(str::trim_start)
                    {
                        let data = if is_oauth {
                            Self::strip_tool_prefix_from_sse(data)
                        } else {
                            data.to_string()
                        };
                        if data == "[DONE]" {
                            _received_message_stop = true;
                            yield StreamChunk::MessageStop;
                            return;
                        }
                        match serde_json::from_str::<AnthropicStreamEvent>(&data) {
                            Ok(event) => {
                                // Track MessageStop event
                                if matches!(event, AnthropicStreamEvent::MessageStop) {
                                    _received_message_stop = true;
                                }
                                
                                // Reset consecutive parse errors on successful parse
                                consecutive_parse_errors = 0;
                                
                                yield event.into();
                            }
                            Err(e) => {
                                if !data.trim().is_empty() && !data.starts_with(':') {
                                    tracing::debug!("SSE parse error in remaining buffer: {} for data: {}", e, data);
                                    
                                    // Increment consecutive parse errors
                                    consecutive_parse_errors += 1;
                                    if consecutive_parse_errors > MAX_CONSECUTIVE_PARSE_ERRORS {
                                        Err(ProviderError::StreamError(
                                            format!("Too many consecutive parse errors: {}", consecutive_parse_errors)
                                        ))?;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        };

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
}

/// Extended thinking configuration for Claude models
#[derive(Debug, Clone, Serialize)]
pub struct ThinkingConfig {
    #[serde(rename = "type")]
    pub thinking_type: String,
    pub budget_tokens: u32,
}

impl ThinkingConfig {
    pub fn enabled(budget_tokens: u32) -> Self {
        Self {
            thinking_type: "enabled".to_string(),
            budget_tokens,
        }
    }
}

// API request/response types
#[derive(Debug, Serialize)]
struct AnthropicRequest {
    model: String,
    max_tokens: usize,
    messages: Vec<AnthropicMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    system: Option<SystemPrompt>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<ToolSpec>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stream: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    thinking: Option<ThinkingConfig>,
}

#[derive(Debug, Serialize)]
#[serde(untagged)]
enum SystemPrompt {
    Text(String),
    Blocks(Vec<SystemBlock>),
}

#[derive(Debug, Serialize)]
struct SystemBlock {
    #[serde(rename = "type")]
    block_type: String,
    text: String,
    cache_control: CacheControl,
}

#[derive(Debug, Serialize)]
struct CacheControl {
    #[serde(rename = "type")]
    cache_type: String,
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

#[cfg(test)]
mod tests {
    use super::{AnthropicClient, ThinkingConfig};

    #[test]
    fn normalize_tool_input_keeps_object() {
        let value = serde_json::json!({"todos": []});
        assert_eq!(AnthropicClient::normalize_tool_input(&value), value);
    }

    #[test]
    fn normalize_tool_input_converts_null_to_empty_object() {
        let value = serde_json::Value::Null;
        assert_eq!(
            AnthropicClient::normalize_tool_input(&value),
            serde_json::json!({})
        );
    }

    #[test]
    fn normalize_tool_input_parses_stringified_object() {
        let value = serde_json::Value::String("{\"a\":1}".to_string());
        assert_eq!(
            AnthropicClient::normalize_tool_input(&value),
            serde_json::json!({"a": 1})
        );
    }

    #[test]
    fn normalize_tool_input_wraps_non_object_values() {
        let value = serde_json::json!([1, 2, 3]);
        assert_eq!(
            AnthropicClient::normalize_tool_input(&value),
            serde_json::json!({"value": [1, 2, 3]})
        );
    }

    #[test]
    fn thinking_config_serializes_correctly() {
        let config = ThinkingConfig::enabled(64000);
        let json = serde_json::to_value(&config).unwrap();
        assert_eq!(
            json,
            serde_json::json!({
                "type": "enabled",
                "budget_tokens": 64000
            })
        );
    }
}
