//! Configuration from environment variables and astrape.yml.
//!
//! The proxy uses **agent-based model routing** configured in astrape.yml:
//!
//! ```yaml
//! agents:
//!   explore:
//!     model: "opencode/gpt-5-nano"
//!   architect:
//!     model: "openai/gpt-4.1"
//! ```
//!
//! Model IDs use the format `provider/model-name` (e.g., `openai/gpt-4.1`, `gemini/gemini-2.5-pro`).
//! The provider is extracted from the model ID to determine authentication and routing.
//!
//! **Environment variables:**
//! - `PORT`: server port (default: 8787)
//! - `LITELLM_BASE_URL`: base URL of a LiteLLM proxy (default: http://localhost:4000)
//! - `REQUEST_TIMEOUT_SECS`: upstream request timeout (default: 120)

use anyhow::{Context, Result};
use serde::Deserialize;
use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct ProxyConfig {
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_agent_based_routing() {
        let mut agent_models = HashMap::new();
        agent_models.insert("explore".to_string(), "opencode/gpt-5-nano".to_string());
        agent_models.insert("architect".to_string(), "openai/gpt-4.1".to_string());

        let config = ProxyConfig {
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
