//! Provider configuration

use secrecy::SecretString;
use uira_types::Provider;

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
}

impl Default for ProviderConfig {
    fn default() -> Self {
        Self {
            provider: Provider::Anthropic,
            api_key: None,
            base_url: None,
            model: "claude-sonnet-4-20250514".to_string(),
            max_tokens: None,
            temperature: None,
            timeout_seconds: Some(120),
        }
    }
}

impl ProviderConfig {
    pub fn anthropic(api_key: impl Into<String>) -> Self {
        Self {
            provider: Provider::Anthropic,
            api_key: Some(SecretString::from(api_key.into())),
            base_url: Some("https://api.anthropic.com".to_string()),
            model: "claude-sonnet-4-20250514".to_string(),
            ..Default::default()
        }
    }

    pub fn openai(api_key: impl Into<String>) -> Self {
        Self {
            provider: Provider::OpenAI,
            api_key: Some(SecretString::from(api_key.into())),
            base_url: Some("https://api.openai.com".to_string()),
            model: "gpt-4o".to_string(),
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
}
