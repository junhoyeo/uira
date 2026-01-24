use astrape_sdk::{AgentConfig, ModelType};

use crate::prompt_loader::PromptLoader;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelTier {
    Low,
    Medium,
    High,
}

impl ModelTier {
    pub fn suffix(&self) -> &'static str {
        match self {
            ModelTier::Low => "low",
            ModelTier::Medium => "medium",
            ModelTier::High => "high",
        }
    }

    pub fn model_type(&self) -> ModelType {
        match self {
            ModelTier::Low => ModelType::Haiku,
            ModelTier::Medium => ModelType::Sonnet,
            ModelTier::High => ModelType::Opus,
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            ModelTier::Low => "LOW",
            ModelTier::Medium => "MEDIUM",
            ModelTier::High => "HIGH",
        }
    }
}

#[derive(Debug, Clone)]
pub struct TierBuilder {
    prompt_loader: PromptLoader,
}

impl TierBuilder {
    pub fn new(prompt_loader: PromptLoader) -> Self {
        Self { prompt_loader }
    }

    pub fn build_variant(&self, base: &AgentConfig, tier: ModelTier) -> AgentConfig {
        let name = format!("{}-{}", base.name, tier.suffix());
        let prompt = self.prompt_loader.load(&name);

        // If we don't have a dedicated prompt file, fall back to base prompt
        // with tier-specific guidance prepended.
        let prompt = if prompt.contains("Prompt file not found") {
            let mut out = String::new();
            out.push_str(&format!(
                "[TIER: {} ({})]\n\n",
                tier.label(),
                tier.model_type().as_str()
            ));
            out.push_str(base.prompt.trim());
            out
        } else {
            prompt
        };

        AgentConfig {
            name,
            description: format!(
                "{} - {} tier ({})",
                base.description,
                tier.suffix(),
                tier.model_type().as_str()
            ),
            prompt,
            tools: base.tools.clone(),
            model: Some(tier.model_type()),
            default_model: Some(tier.model_type()),
            metadata: base.metadata.clone(),
        }
    }

    pub fn build_variants(&self, base: &AgentConfig, tiers: &[ModelTier]) -> Vec<AgentConfig> {
        tiers
            .iter()
            .copied()
            .map(|t| self.build_variant(base, t))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prompt_loader::PromptLoader;
    use tempfile::tempdir;

    #[test]
    fn builds_variant_with_fallback_prompt_prefix() {
        let tmp = tempdir().unwrap();
        let loader = PromptLoader::from_fs(tmp.path());
        let builder = TierBuilder::new(loader);

        let base = AgentConfig {
            name: "executor".to_string(),
            description: "Executes tasks".to_string(),
            prompt: "Base prompt".to_string(),
            tools: vec!["Read".to_string()],
            model: None,
            default_model: None,
            metadata: None,
        };

        let v = builder.build_variant(&base, ModelTier::High);
        assert_eq!(v.name, "executor-high");
        assert_eq!(v.model, Some(ModelType::Opus));
        assert!(v.prompt.contains("[TIER: HIGH"));
        assert!(v.prompt.contains("Base prompt"));
    }
}
