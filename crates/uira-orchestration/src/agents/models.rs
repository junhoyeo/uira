//! Provider-specific model mapping
//!
//! Maps abstract model tiers (Opus/Sonnet/Haiku) to actual model IDs per provider.

use super::types::ModelType;
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct ProviderModels {
    pub opus: String,
    pub sonnet: String,
    pub haiku: String,
}

impl ProviderModels {
    pub fn get(&self, tier: ModelType) -> &str {
        match tier {
            ModelType::Opus => &self.opus,
            ModelType::Sonnet => &self.sonnet,
            ModelType::Haiku => &self.haiku,
            ModelType::Inherit => &self.sonnet,
        }
    }
}

impl Default for ProviderModels {
    fn default() -> Self {
        Self::anthropic()
    }
}

impl ProviderModels {
    pub fn anthropic() -> Self {
        Self {
            opus: "claude-opus-4-20250514".to_string(),
            sonnet: "claude-sonnet-4-20250514".to_string(),
            haiku: "claude-3-5-haiku-20241022".to_string(),
        }
    }

    pub fn openai() -> Self {
        Self {
            opus: "gpt-4o".to_string(),
            sonnet: "gpt-4o-mini".to_string(),
            haiku: "gpt-4o-mini".to_string(),
        }
    }

    pub fn opencode() -> Self {
        Self {
            opus: "anthropic/claude-opus-4-20250514".to_string(),
            sonnet: "anthropic/claude-sonnet-4-20250514".to_string(),
            haiku: "anthropic/claude-3-5-haiku-20241022".to_string(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ModelRegistry {
    providers: HashMap<String, ProviderModels>,
    default_provider: String,
}

impl Default for ModelRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl ModelRegistry {
    pub fn new() -> Self {
        let mut providers = HashMap::new();
        providers.insert("anthropic".to_string(), ProviderModels::anthropic());
        providers.insert("openai".to_string(), ProviderModels::openai());
        providers.insert("opencode".to_string(), ProviderModels::opencode());

        Self {
            providers,
            default_provider: "anthropic".to_string(),
        }
    }

    pub fn with_provider(mut self, name: impl Into<String>, models: ProviderModels) -> Self {
        self.providers.insert(name.into(), models);
        self
    }

    pub fn set_default_provider(&mut self, provider: impl Into<String>) {
        self.default_provider = provider.into();
    }

    pub fn resolve(&self, tier: ModelType, provider: Option<&str>) -> String {
        let provider_name = provider.unwrap_or(&self.default_provider);
        self.providers
            .get(provider_name)
            .map(|p| p.get(tier).to_string())
            .unwrap_or_else(|| {
                self.providers
                    .get(&self.default_provider)
                    .map(|p| p.get(tier).to_string())
                    .unwrap_or_else(|| tier.as_str().to_string())
            })
    }

    pub fn resolve_with_override(
        &self,
        tier: ModelType,
        provider: Option<&str>,
        model_override: Option<&str>,
    ) -> String {
        if let Some(override_model) = model_override {
            return override_model.to_string();
        }
        self.resolve(tier, provider)
    }

    pub fn get_provider(&self, name: &str) -> Option<&ProviderModels> {
        self.providers.get(name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_anthropic_models() {
        let registry = ModelRegistry::new();
        assert_eq!(
            registry.resolve(ModelType::Opus, Some("anthropic")),
            "claude-opus-4-20250514"
        );
        assert_eq!(
            registry.resolve(ModelType::Sonnet, Some("anthropic")),
            "claude-sonnet-4-20250514"
        );
    }

    #[test]
    fn test_openai_models() {
        let registry = ModelRegistry::new();
        assert_eq!(registry.resolve(ModelType::Opus, Some("openai")), "gpt-4o");
    }

    #[test]
    fn test_default_provider() {
        let registry = ModelRegistry::new();
        assert_eq!(
            registry.resolve(ModelType::Sonnet, None),
            "claude-sonnet-4-20250514"
        );
    }

    #[test]
    fn test_model_override() {
        let registry = ModelRegistry::new();
        assert_eq!(
            registry.resolve_with_override(ModelType::Opus, None, Some("custom-model")),
            "custom-model"
        );
    }
}
