use anyhow::Result;
use astrape_config::{load_config, AstrapeConfig, ProxySettings};
use std::collections::HashMap;
use std::env;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct ProxyConfig {
    pub port: u16,
    pub litellm_base_url: String,
    pub request_timeout_secs: u64,
    pub auto_start: bool,
    pub health_endpoint: String,
    pub enable_logging: bool,
    pub max_connections: u32,
    pub agent_models: HashMap<String, String>,
}

impl From<&AstrapeConfig> for ProxyConfig {
    fn from(config: &AstrapeConfig) -> Self {
        let mut agent_models = HashMap::new();

        agent_models.insert("librarian".to_string(), "opencode/big-pickle".to_string());

        for (agent_name, agent_config) in &config.agents.agents {
            if let Some(model) = &agent_config.model {
                agent_models.insert(agent_name.clone(), model.clone());
            }
        }

        Self {
            port: env_override("ASTRAPE_PROXY_PORT", config.proxy.port),
            litellm_base_url: env_override_str(
                "ASTRAPE_PROXY_LITELLM_BASE_URL",
                config.proxy.litellm_base_url.clone(),
            ),
            request_timeout_secs: env_override(
                "ASTRAPE_PROXY_TIMEOUT_SECS",
                config.proxy.request_timeout_secs,
            ),
            auto_start: config.proxy.auto_start,
            health_endpoint: config.proxy.health_endpoint.clone(),
            enable_logging: config.proxy.enable_logging,
            max_connections: config.proxy.max_connections,
            agent_models,
        }
    }
}

fn env_override<T: std::str::FromStr>(var: &str, default: T) -> T {
    env::var(var)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

fn env_override_str(var: &str, default: String) -> String {
    env::var(var).unwrap_or(default)
}

impl Default for ProxyConfig {
    fn default() -> Self {
        let proxy_settings = ProxySettings::default();
        let mut agent_models = HashMap::new();
        agent_models.insert("librarian".to_string(), "opencode/big-pickle".to_string());

        Self {
            port: env_override("ASTRAPE_PROXY_PORT", proxy_settings.port),
            litellm_base_url: env_override_str(
                "ASTRAPE_PROXY_LITELLM_BASE_URL",
                proxy_settings.litellm_base_url,
            ),
            request_timeout_secs: env_override(
                "ASTRAPE_PROXY_TIMEOUT_SECS",
                proxy_settings.request_timeout_secs,
            ),
            auto_start: proxy_settings.auto_start,
            health_endpoint: proxy_settings.health_endpoint,
            enable_logging: proxy_settings.enable_logging,
            max_connections: proxy_settings.max_connections,
            agent_models,
        }
    }
}

impl ProxyConfig {
    pub fn from_astrape_config(path: Option<impl Into<PathBuf>>) -> Result<Self> {
        let config = if let Some(p) = path {
            astrape_config::load_config_from_file(&p.into())?.config
        } else {
            load_config(None).unwrap_or_default()
        };

        Ok(Self::from(&config))
    }

    pub fn load() -> Self {
        Self::from_astrape_config(None::<PathBuf>).unwrap_or_default()
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
        agent_models.insert("librarian".to_string(), "opencode/big-pickle".to_string());

        let config = ProxyConfig {
            port: 8787,
            litellm_base_url: "http://localhost:4000".to_string(),
            request_timeout_secs: 120,
            auto_start: true,
            health_endpoint: "/health".to_string(),
            enable_logging: false,
            max_connections: 100,
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
        assert_eq!(
            config.get_model_for_agent("librarian"),
            Some("opencode/big-pickle")
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

    #[test]
    fn test_default_has_librarian() {
        let config = ProxyConfig::default();
        assert_eq!(
            config.get_model_for_agent("librarian"),
            Some("opencode/big-pickle")
        );
    }

    #[test]
    fn test_default_proxy_settings() {
        let config = ProxyConfig::default();
        assert_eq!(config.port, 8787);
        assert_eq!(config.litellm_base_url, "http://localhost:4000");
        assert_eq!(config.request_timeout_secs, 120);
        assert!(config.auto_start);
        assert_eq!(config.health_endpoint, "/health");
        assert!(!config.enable_logging);
        assert_eq!(config.max_connections, 100);
    }

    #[test]
    fn test_from_astrape_config() {
        use astrape_config::schema::{AgentConfig, AgentSettings};

        let mut agents = HashMap::new();
        agents.insert(
            "explore".to_string(),
            AgentConfig {
                model: Some("custom/model".to_string()),
                settings: HashMap::new(),
            },
        );

        let astrape_config = AstrapeConfig {
            agents: AgentSettings { agents },
            proxy: ProxySettings {
                port: 9000,
                litellm_base_url: "http://custom:5000".to_string(),
                request_timeout_secs: 60,
                auto_start: false,
                health_endpoint: "/healthz".to_string(),
                enable_logging: true,
                max_connections: 50,
            },
            ..Default::default()
        };

        let config = ProxyConfig::from(&astrape_config);
        assert_eq!(config.port, 9000);
        assert_eq!(config.litellm_base_url, "http://custom:5000");
        assert_eq!(config.request_timeout_secs, 60);
        assert!(!config.auto_start);
        assert!(config.enable_logging);
        assert_eq!(config.get_model_for_agent("explore"), Some("custom/model"));
    }
}
