//! Provider configuration

use secrecy::SecretString;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use uira_core::Provider;

const DEFAULT_ANTHROPIC_MODEL: &str = "claude-sonnet-4-20250514";
const DEFAULT_OPENAI_MODEL: &str = "gpt-4o";
const DEFAULT_FRIENDLI_MODEL: &str = "MiniMaxAI/MiniMax-M2.5";

/// FriendliAI endpoint type configuration
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FriendliEndpointType {
    /// Serverless endpoints - pay-per-use with automatic scaling
    #[default]
    Serverless,
    /// Dedicated endpoints - reserved capacity with consistent performance
    Dedicated,
}

/// FriendliAI-specific configuration options
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FriendliAIConfig {
    /// API token for authentication
    /// Can be provided directly, loaded from file, or fallback to FRIENDLI_TOKEN env var
    #[serde(skip)]
    pub token: Option<SecretString>,

    /// Path to file containing the API token
    /// If provided, the token will be read from this file
    pub token_file: Option<PathBuf>,

    /// Endpoint type (serverless or dedicated)
    pub endpoint_type: FriendliEndpointType,

    /// Custom endpoint URL (overrides endpoint_type if provided)
    pub custom_endpoint: Option<String>,

    /// Model identifier to use
    /// Examples: "zai-org/GLM-5", "MiniMaxAI/MiniMax-M2.5", "meta-llama/Llama-3-8B-Instruct"
    pub model: Option<String>,

    /// Enable reasoning mode for models that support it
    /// When enabled, the model will show its reasoning process
    pub enable_reasoning: Option<bool>,

    /// Enable thinking mode via chat template
    /// This allows models to "think" before responding
    pub enable_thinking: Option<bool>,

    /// Parse and return reasoning content separately
    /// Useful for models that include reasoning in their output
    pub parse_reasoning: Option<bool>,

    /// Sampling temperature (0.0 to 2.0)
    /// Controls randomness - lower values are more focused and deterministic
    pub temperature: Option<f32>,

    /// Top-p nucleus sampling parameter (0.0 to 1.0)
    /// Controls diversity by limiting token choices to top probability mass
    pub top_p: Option<f32>,

    /// Top-k sampling parameter
    /// Limits token choices to the k most likely tokens
    pub top_k: Option<u32>,

    /// Maximum tokens to generate in the response
    pub max_tokens: Option<usize>,

    /// Repetition penalty (typically 1.0 to 1.2)
    /// Higher values reduce repetition
    pub repetition_penalty: Option<f32>,

    /// Frequency penalty (-2.0 to 2.0)
    /// Positive values discourage repeating tokens
    pub frequency_penalty: Option<f32>,

    /// Presence penalty (-2.0 to 2.0)
    /// Positive values encourage talking about new topics
    pub presence_penalty: Option<f32>,

    /// Request timeout in seconds for API calls
    pub request_timeout: Option<u64>,

    /// Connection timeout in seconds
    pub connect_timeout: Option<u64>,

    /// Maximum number of retry attempts for failed requests
    pub max_retries: Option<u32>,

    /// Additional chat template kwargs
    /// Platform-specific parameters passed to the chat template
    pub chat_template_kwargs: Option<std::collections::HashMap<String, serde_json::Value>>,
}

impl FriendliAIConfig {
    /// Create a new FriendliAI configuration with default values
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the API token directly
    pub fn with_token(mut self, token: impl Into<String>) -> Self {
        self.token = Some(SecretString::from(token.into()));
        self
    }

    /// Set the path to a file containing the API token
    pub fn with_token_file(mut self, path: impl Into<PathBuf>) -> Self {
        self.token_file = Some(path.into());
        self
    }

    /// Set the endpoint type
    pub fn with_endpoint_type(mut self, endpoint_type: FriendliEndpointType) -> Self {
        self.endpoint_type = endpoint_type;
        self
    }

    /// Set a custom endpoint URL
    pub fn with_custom_endpoint(mut self, url: impl Into<String>) -> Self {
        self.custom_endpoint = Some(url.into());
        self
    }

    /// Set the model to use
    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model = Some(model.into());
        self
    }

    /// Enable reasoning mode
    pub fn with_reasoning(mut self, enabled: bool) -> Self {
        self.enable_reasoning = Some(enabled);
        self
    }

    /// Enable thinking mode
    pub fn with_thinking(mut self, enabled: bool) -> Self {
        self.enable_thinking = Some(enabled);
        self
    }

    /// Enable reasoning parsing
    pub fn with_parse_reasoning(mut self, enabled: bool) -> Self {
        self.parse_reasoning = Some(enabled);
        self
    }

    /// Set temperature
    pub fn with_temperature(mut self, temperature: f32) -> Self {
        self.temperature = Some(temperature.clamp(0.0, 2.0));
        self
    }

    /// Set top-p
    pub fn with_top_p(mut self, top_p: f32) -> Self {
        self.top_p = Some(top_p.clamp(0.0, 1.0));
        self
    }

    /// Set top-k
    pub fn with_top_k(mut self, top_k: u32) -> Self {
        self.top_k = Some(top_k);
        self
    }

    /// Set max tokens
    pub fn with_max_tokens(mut self, max_tokens: usize) -> Self {
        self.max_tokens = Some(max_tokens);
        self
    }

    /// Set request timeout
    pub fn with_request_timeout(mut self, seconds: u64) -> Self {
        self.request_timeout = Some(seconds);
        self
    }

    /// Set max retries
    pub fn with_max_retries(mut self, retries: u32) -> Self {
        self.max_retries = Some(retries);
        self
    }

    /// Add a chat template kwarg
    pub fn with_chat_template_kwarg(
        mut self,
        key: impl Into<String>,
        value: serde_json::Value,
    ) -> Self {
        if self.chat_template_kwargs.is_none() {
            self.chat_template_kwargs = Some(std::collections::HashMap::new());
        }
        if let Some(ref mut kwargs) = self.chat_template_kwargs {
            kwargs.insert(key.into(), value);
        }
        self
    }

    /// Get the resolved API token
    /// Returns token in priority order: direct token, token from file, FRIENDLI_TOKEN env var
    pub fn get_token(&self) -> Result<SecretString, String> {
        // Priority 1: Direct token
        if let Some(ref token) = self.token {
            return Ok(token.clone());
        }

        // Priority 2: Token from file
        if let Some(ref path) = self.token_file {
            return std::fs::read_to_string(path)
                .map_err(|e| format!("Failed to read token file {}: {}", path.display(), e))
                .map(|content| SecretString::from(content.trim()));
        }

        // Priority 3: Environment variable (backward compatibility)
        std::env::var("FRIENDLI_TOKEN")
            .map_err(|_| "No API token found. Provide token directly, via file, or set FRIENDLI_TOKEN environment variable".to_string())
            .map(SecretString::from)
    }

    /// Get the resolved base URL for API requests
    pub fn get_base_url(&self) -> String {
        if let Some(ref custom) = self.custom_endpoint {
            return custom.clone();
        }

        match self.endpoint_type {
            FriendliEndpointType::Serverless => "https://api.friendli.ai/serverless/v1".to_string(),
            FriendliEndpointType::Dedicated => "https://api.friendli.ai/dedicated/v1".to_string(),
        }
    }

    /// Validate the configuration
    pub fn validate(&self) -> Result<(), String> {
        // Validate token availability
        self.get_token()
            .map_err(|e| format!("Token validation failed: {}", e))?;

        // Validate temperature range
        if let Some(temp) = self.temperature {
            if !(0.0..=2.0).contains(&temp) {
                return Err(format!(
                    "Temperature must be between 0.0 and 2.0, got {}",
                    temp
                ));
            }
        }

        // Validate top_p range
        if let Some(top_p) = self.top_p {
            if !(0.0..=1.0).contains(&top_p) {
                return Err(format!("top_p must be between 0.0 and 1.0, got {}", top_p));
            }
        }

        // Validate penalty ranges
        for (name, value) in [
            ("frequency_penalty", self.frequency_penalty),
            ("presence_penalty", self.presence_penalty),
        ] {
            if let Some(penalty) = value {
                if !(-2.0..=2.0).contains(&penalty) {
                    return Err(format!(
                        "{} must be between -2.0 and 2.0, got {}",
                        name, penalty
                    ));
                }
            }
        }

        // Validate repetition_penalty
        if let Some(rep_penalty) = self.repetition_penalty {
            if !(0.0..=2.0).contains(&rep_penalty) {
                return Err(format!(
                    "repetition_penalty must be between 0.0 and 2.0, got {}",
                    rep_penalty
                ));
            }
        }

        // Validate timeouts
        if let Some(timeout) = self.request_timeout {
            if timeout == 0 {
                return Err("request_timeout must be greater than 0".to_string());
            }
        }

        if let Some(timeout) = self.connect_timeout {
            if timeout == 0 {
                return Err("connect_timeout must be greater than 0".to_string());
            }
        }

        Ok(())
    }
}

/// Configuration for a model provider
#[derive(Clone)]
pub struct ProviderConfig {
    pub provider: Provider,
    pub api_key: Option<SecretString>,
    pub base_url: Option<String>,
    pub model: String,
    pub max_tokens: Option<usize>,
    pub temperature: Option<f32>,
    pub timeout_seconds: Option<u64>,
    /// Maximum number of retry attempts for failed requests (default: 3)
    pub max_retries: Option<u32>,
    /// Enable extended thinking mode for supported models (default: false)
    pub enable_thinking: bool,
    /// Token budget for thinking when enabled
    pub thinking_budget: Option<u32>,
    /// FriendliAI-specific configuration
    pub friendliai: Option<FriendliAIConfig>,
}

impl Default for ProviderConfig {
    fn default() -> Self {
        Self {
            provider: Provider::Anthropic,
            api_key: None,
            base_url: None,
            model: DEFAULT_ANTHROPIC_MODEL.to_string(),
            max_tokens: None,
            temperature: None,
            timeout_seconds: Some(120),
            max_retries: Some(3),
            enable_thinking: false,
            thinking_budget: None,
            friendliai: None,
        }
    }
}

impl ProviderConfig {
    pub fn anthropic(api_key: impl Into<String>) -> Self {
        Self {
            provider: Provider::Anthropic,
            api_key: Some(SecretString::from(api_key.into())),
            base_url: Some("https://api.anthropic.com".to_string()),
            model: DEFAULT_ANTHROPIC_MODEL.to_string(),
            ..Default::default()
        }
    }

    pub fn openai(api_key: impl Into<String>) -> Self {
        Self {
            provider: Provider::OpenAI,
            api_key: Some(SecretString::from(api_key.into())),
            base_url: Some("https://api.openai.com".to_string()),
            model: DEFAULT_OPENAI_MODEL.to_string(),
            ..Default::default()
        }
    }

    pub fn ollama(model: impl Into<String>) -> Self {
        Self {
            provider: Provider::Ollama,
            api_key: None,
            base_url: Some("http://localhost:11434".to_string()),
            model: model.into(),
            ..Default::default()
        }
    }

    pub fn friendliai(token: impl Into<String>) -> Self {
        Self {
            provider: Provider::FriendliAI,
            api_key: Some(SecretString::from(token.into())),
            base_url: Some("https://api.friendli.ai/serverless/v1".to_string()),
            model: DEFAULT_FRIENDLI_MODEL.to_string(),
            ..Default::default()
        }
    }

    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model = model.into();
        self
    }

    pub fn with_base_url(mut self, url: impl Into<String>) -> Self {
        self.base_url = Some(url.into());
        self
    }

    pub fn with_max_tokens(mut self, max_tokens: usize) -> Self {
        self.max_tokens = Some(max_tokens);
        self
    }

    pub fn with_temperature(mut self, temperature: f32) -> Self {
        self.temperature = Some(temperature);
        self
    }

    pub fn with_max_retries(mut self, max_retries: u32) -> Self {
        self.max_retries = Some(max_retries);
        self
    }

    pub fn with_thinking(mut self, budget: u32) -> Self {
        self.enable_thinking = true;
        self.thinking_budget = Some(budget);
        self
    }
}
