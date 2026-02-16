//! CLI configuration

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use uira_core::UIRA_DIR;
use uira_types::atomic_write_secure;

/// CLI-specific configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CliConfig {
    /// Default provider
    #[serde(default)]
    pub default_provider: Option<String>,

    /// Default model
    #[serde(default)]
    pub default_model: Option<String>,

    /// API keys by provider
    #[serde(default)]
    pub api_keys: std::collections::HashMap<String, String>,

    /// Default working directory
    #[serde(default)]
    pub working_directory: Option<PathBuf>,

    /// Enable colors in output
    #[serde(default = "default_true")]
    pub colors: bool,

    /// Enable verbose output
    #[serde(default)]
    pub verbose: bool,
}

fn default_true() -> bool {
    true
}

impl Default for CliConfig {
    fn default() -> Self {
        Self {
            default_provider: None,
            default_model: None,
            api_keys: std::collections::HashMap::new(),
            working_directory: None,
            colors: true,
            verbose: false,
        }
    }
}

#[allow(dead_code)] // Public API methods
impl CliConfig {
    /// Load configuration from default locations
    pub fn load() -> Self {
        // Try to load from:
        // 1. ~/.config/uira/config.toml
        // 2. ~/.uira/config.toml
        // 3. Use defaults

        if let Some(config_dir) = dirs::config_dir() {
            let config_path = config_dir.join("uira").join("config.toml");
            if config_path.exists() {
                if let Ok(content) = std::fs::read_to_string(&config_path) {
                    if let Ok(config) = toml::from_str(&content) {
                        return config;
                    }
                }
            }
        }

        if let Some(home) = dirs::home_dir() {
            let config_path = home.join(UIRA_DIR).join("config.toml");
            if config_path.exists() {
                if let Ok(content) = std::fs::read_to_string(&config_path) {
                    if let Ok(config) = toml::from_str(&content) {
                        return config;
                    }
                }
            }
        }

        Self::default()
    }

    /// Save configuration to disk
    pub fn save(&self) -> std::io::Result<()> {
        let config_dir = dirs::config_dir()
            .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "No config dir"))?
            .join("uira");

        std::fs::create_dir_all(&config_dir)?;

        let config_path = config_dir.join("config.toml");
        let content = toml::to_string_pretty(self)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

        atomic_write_secure(&config_path, content.as_bytes())
    }

    /// Get API key for a provider
    pub fn get_api_key(&self, provider: &str) -> Option<&String> {
        self.api_keys.get(provider)
    }

    /// Set API key for a provider
    pub fn set_api_key(&mut self, provider: impl Into<String>, key: impl Into<String>) {
        self.api_keys.insert(provider.into(), key.into());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = CliConfig::default();
        assert!(config.colors);
        assert!(!config.verbose);
    }

    #[test]
    fn test_api_key_management() {
        let mut config = CliConfig::default();
        config.set_api_key("anthropic", "sk-test-key");
        assert_eq!(
            config.get_api_key("anthropic"),
            Some(&"sk-test-key".to_string())
        );
    }
}
