//! Configuration from environment variables and astrape.yml.
//!
//! The proxy supports two configuration modes:
//!
//! 1. **Agent-based mapping** (from astrape.yml):
//!    - Reads `agents.{agent_name}.model` from astrape.yml
//!    - Example: `agents.explore.model = "opencode/gpt-5-nano"`
//!
//! 2. **Environment variables** (fallback):
//!    - `PREFERRED_PROVIDER`: `openai` (default) or `google`
//!    - `BIG_MODEL`: mapped target for "sonnet"-class requests
//!    - `SMALL_MODEL`: mapped target for "haiku"-class requests
//!    - `PORT`: server port (default: 8787)
//!    - `LITELLM_BASE_URL`: base URL of a LiteLLM proxy (default: http://localhost:4000)
//!    - `REQUEST_TIMEOUT_SECS`: upstream request timeout (default: 120)

use anyhow::{Context, Result};
use serde::Deserialize;
use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct ProxyConfig {
    pub preferred_provider: String,
    pub big_model: String,
    pub small_model: String,
    pub port: u16,
    pub litellm_base_url: String,
    pub request_timeout_secs: u64,
    pub agent_models: HashMap<String, String>,
}

#[derive(Debug, Deserialize)]
struct AstrapeYamlConfig {
    #[serde(default)]
    agents: HashMap<String, AgentConfig>,
}

#[derive(Debug, Deserialize)]
struct AgentConfig {
    model: Option<String>,
}

impl Default for ProxyConfig {
    fn default() -> Self {
        Self {
            preferred_provider: env::var("PREFERRED_PROVIDER")
                .unwrap_or_else(|_| "openai".to_string()),
            big_model: env::var("BIG_MODEL").unwrap_or_else(|_| "gpt-4.1".to_string()),
            small_model: env::var("SMALL_MODEL").unwrap_or_else(|_| "gpt-4.1-mini".to_string()),
            port: env::var("PORT")
                .ok()
                .and_then(|p| p.parse().ok())
                .unwrap_or(8787),
            litellm_base_url: env::var("LITELLM_BASE_URL")
                .unwrap_or_else(|_| "http://localhost:4000".to_string()),
            request_timeout_secs: env::var("REQUEST_TIMEOUT_SECS")
                .ok()
                .and_then(|p| p.parse().ok())
                .unwrap_or(120),
            agent_models: HashMap::new(),
        }
    }
}

impl ProxyConfig {
    pub fn from_yaml_file(path: impl Into<PathBuf>) -> Result<Self> {
        let path = path.into();
        let content = fs::read_to_string(&path)
            .with_context(|| format!("Failed to read astrape.yml from {:?}", path))?;

        let yaml_config: AstrapeYamlConfig =
            serde_yaml::from_str(&content).with_context(|| "Failed to parse astrape.yml")?;

        let mut agent_models = HashMap::new();
        for (agent_name, agent_config) in yaml_config.agents {
            if let Some(model) = agent_config.model {
                agent_models.insert(agent_name, model);
            }
        }

        Ok(Self {
            agent_models,
            ..Self::default()
        })
    }

    pub fn get_model_for_agent(&self, agent_name: &str) -> Option<&str> {
        self.agent_models.get(agent_name).map(|s| s.as_str())
    }

    pub fn resolve_model_for_agent(&self, agent_name: &str, fallback_model: &str) -> String {
        self.get_model_for_agent(agent_name)
            .unwrap_or(fallback_model)
            .to_string()
    }

    pub fn litellm_base_url_trimmed(&self) -> String {
        self.litellm_base_url.trim_end_matches('/').to_string()
    }

    pub fn map_model(&self, model: &str) -> String {
        let clean = model
            .strip_prefix("anthropic/")
            .or_else(|| model.strip_prefix("openai/"))
            .or_else(|| model.strip_prefix("gemini/"))
            .unwrap_or(model);

        let lower = clean.to_lowercase();
        if lower.contains("haiku") {
            if self.preferred_provider == "google" {
                format!("gemini/{}", self.small_model)
            } else {
                format!("openai/{}", self.small_model)
            }
        } else if lower.contains("sonnet") {
            if self.preferred_provider == "google" {
                format!("gemini/{}", self.big_model)
            } else {
                format!("openai/{}", self.big_model)
            }
        } else {
            model.to_string()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_map_model_haiku_openai() {
        let config = ProxyConfig {
            preferred_provider: "openai".to_string(),
            big_model: "gpt-4.1".to_string(),
            small_model: "gpt-4.1-mini".to_string(),
            port: 8787,
            litellm_base_url: "http://localhost:4000".to_string(),
            request_timeout_secs: 120,
            agent_models: HashMap::new(),
        };

        assert_eq!(config.map_model("claude-3-haiku"), "openai/gpt-4.1-mini");
        assert_eq!(
            config.map_model("anthropic/claude-3-haiku"),
            "openai/gpt-4.1-mini"
        );
    }

    #[test]
    fn test_map_model_sonnet_google() {
        let config = ProxyConfig {
            preferred_provider: "google".to_string(),
            big_model: "gemini-2.5-pro".to_string(),
            small_model: "gemini-2.5-flash".to_string(),
            port: 8787,
            litellm_base_url: "http://localhost:4000".to_string(),
            request_timeout_secs: 120,
            agent_models: HashMap::new(),
        };

        assert_eq!(config.map_model("claude-3-sonnet"), "gemini/gemini-2.5-pro");
    }

    #[test]
    fn test_agent_based_mapping() {
        let mut agent_models = HashMap::new();
        agent_models.insert("explore".to_string(), "opencode/gpt-5-nano".to_string());
        agent_models.insert("architect".to_string(), "openai/gpt-4.1".to_string());

        let config = ProxyConfig {
            preferred_provider: "openai".to_string(),
            big_model: "gpt-4.1".to_string(),
            small_model: "gpt-4.1-mini".to_string(),
            port: 8787,
            litellm_base_url: "http://localhost:4000".to_string(),
            request_timeout_secs: 120,
            agent_models,
        };

        assert_eq!(
            config.get_model_for_agent("explore"),
            Some("opencode/gpt-5-nano")
        );
        assert_eq!(
            config.get_model_for_agent("architect"),
            Some("openai/gpt-4.1")
        );
        assert_eq!(config.get_model_for_agent("nonexistent"), None);

        assert_eq!(
            config.resolve_model_for_agent("explore", "fallback"),
            "opencode/gpt-5-nano"
        );
        assert_eq!(
            config.resolve_model_for_agent("nonexistent", "fallback"),
            "fallback"
        );
    }
}
