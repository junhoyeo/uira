//! Model client builder

use std::sync::Arc;
use uira_protocol::Provider;

use crate::{
    AnthropicClient, GeminiClient, OllamaClient, OpenAIClient, ProviderConfig, ProviderError,
};

/// Builder for creating model clients
pub struct ModelClientBuilder {
    config: ProviderConfig,
}

impl ModelClientBuilder {
    pub fn new() -> Self {
        Self {
            config: ProviderConfig::default(),
        }
    }

    pub fn with_config(mut self, config: ProviderConfig) -> Self {
        self.config = config;
        self
    }

    pub fn build(self) -> Result<Arc<dyn crate::ModelClient>, ProviderError> {
        match self.config.provider {
            Provider::Anthropic => Ok(Arc::new(AnthropicClient::new(self.config)?)),
            Provider::OpenAI => Ok(Arc::new(OpenAIClient::new(self.config)?)),
            Provider::Google => Ok(Arc::new(GeminiClient::new(self.config)?)),
            Provider::Ollama => Ok(Arc::new(OllamaClient::new(self.config)?)),
            Provider::OpenRouter => {
                // OpenRouter uses OpenAI-compatible API
                let mut config = self.config;
                config.base_url = Some("https://openrouter.ai/api".to_string());
                Ok(Arc::new(OpenAIClient::new(config)?))
            }
            Provider::Custom => {
                // Custom provider - try OpenAI-compatible API
                Ok(Arc::new(OpenAIClient::new(self.config)?))
            }
        }
    }
}

impl Default for ModelClientBuilder {
    fn default() -> Self {
        Self::new()
    }
}
