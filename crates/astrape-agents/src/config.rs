use std::collections::HashMap;

use astrape_sdk::{AgentConfig, AgentOverrideConfig, AgentOverrides, ModelType};

#[derive(Debug, thiserror::Error)]
pub enum AgentConfigError {
    #[error("unknown model type: {0}")]
    UnknownModelType(String),
}

pub fn parse_model_type(s: &str) -> Result<ModelType, AgentConfigError> {
    let lower = s.to_lowercase();
    if lower.contains("haiku") {
        Ok(ModelType::Haiku)
    } else if lower.contains("sonnet") {
        Ok(ModelType::Sonnet)
    } else if lower.contains("opus") {
        Ok(ModelType::Opus)
    } else if lower.contains("inherit") {
        Ok(ModelType::Inherit)
    } else {
        Err(AgentConfigError::UnknownModelType(s.to_string()))
    }
}

pub fn merge_agent_config(base: &AgentConfig, override_cfg: &AgentOverrideConfig) -> AgentConfig {
    let mut merged = base.clone();

    if let Some(model) = override_cfg.model.as_deref() {
        if let Ok(mt) = parse_model_type(model) {
            merged.model = Some(mt);
        }
    }

    if let Some(prompt_append) = override_cfg.prompt_append.as_deref() {
        if !prompt_append.trim().is_empty() {
            merged.prompt = format!("{}\n\n{}", merged.prompt.trim_end(), prompt_append.trim());
        }
    }

    merged
}

pub fn apply_overrides(
    mut agents: HashMap<String, AgentConfig>,
    overrides: Option<&AgentOverrides>,
) -> HashMap<String, AgentConfig> {
    let Some(overrides) = overrides else {
        return agents;
    };

    for (name, override_cfg) in overrides {
        if let Some(enabled) = override_cfg.enabled {
            if !enabled {
                agents.remove(name);
                continue;
            }
        }

        if let Some(base) = agents.get(name).cloned() {
            agents.insert(name.clone(), merge_agent_config(&base, override_cfg));
        }
    }

    agents
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merge_agent_config_appends_prompt() {
        let base = AgentConfig {
            name: "a".to_string(),
            description: "".to_string(),
            prompt: "hello".to_string(),
            tools: vec![],
            model: None,
            default_model: None,
            metadata: None,
        };

        let override_cfg = AgentOverrideConfig {
            prompt_append: Some("world".to_string()),
            ..Default::default()
        };

        let merged = merge_agent_config(&base, &override_cfg);
        assert_eq!(merged.prompt, "hello\n\nworld");
    }

    #[test]
    fn apply_overrides_can_disable_agent() {
        let mut agents = HashMap::new();
        agents.insert(
            "a".to_string(),
            AgentConfig {
                name: "a".to_string(),
                description: "".to_string(),
                prompt: "".to_string(),
                tools: vec![],
                model: None,
                default_model: None,
                metadata: None,
            },
        );

        let mut overrides = AgentOverrides::new();
        overrides.insert(
            "a".to_string(),
            AgentOverrideConfig {
                enabled: Some(false),
                ..Default::default()
            },
        );

        let out = apply_overrides(agents, Some(&overrides));
        assert!(out.get("a").is_none());
    }
}
