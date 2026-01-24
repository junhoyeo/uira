use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Config {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ai: Option<AiConfig>,

    #[serde(flatten)]
    pub hooks: HashMap<String, HookConfig>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct AiConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct HookConfig {
    #[serde(default)]
    pub parallel: bool,

    #[serde(default)]
    pub commands: Vec<Command>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Command {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,

    pub run: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub glob: Option<String>,

    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub stage_fixed: bool,
}

impl Config {
    pub fn from_file<P: AsRef<Path>>(path: P) -> anyhow::Result<Self> {
        let content = fs::read_to_string(path)?;
        let config: Config = serde_yaml::from_str(&content)?;
        Ok(config)
    }

    pub fn default_config() -> Self {
        let mut hooks = HashMap::new();

        let pre_commit = HookConfig {
            parallel: true,
            commands: vec![Command {
                name: Some("lint".to_string()),
                run: "astrape lint {staged_files}".to_string(),
                glob: Some("**/*.{js,ts,jsx,tsx}".to_string()),
                stage_fixed: false,
            }],
        };

        let post_commit = HookConfig {
            parallel: false,
            commands: vec![Command {
                name: Some("auto-push".to_string()),
                run: "git push origin HEAD".to_string(),
                glob: None,
                stage_fixed: false,
            }],
        };

        hooks.insert("pre-commit".to_string(), pre_commit);
        hooks.insert("post-commit".to_string(), post_commit);

        Config { ai: None, hooks }
    }

    pub fn to_yaml(&self) -> anyhow::Result<String> {
        Ok(serde_yaml::to_string(self)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = Config::default_config();
        assert!(config.hooks.contains_key("pre-commit"));
        assert!(config.hooks.contains_key("post-commit"));

        let pre_commit = &config.hooks["pre-commit"];
        assert!(pre_commit.parallel);
        assert_eq!(pre_commit.commands.len(), 1);

        let post_commit = &config.hooks["post-commit"];
        assert!(!post_commit.parallel);
        assert_eq!(post_commit.commands.len(), 1);
    }

    #[test]
    fn test_yaml_roundtrip() {
        let config = Config::default_config();
        let yaml = config.to_yaml().unwrap();
        let parsed: Config = serde_yaml::from_str(&yaml).unwrap();

        assert_eq!(config.hooks.len(), parsed.hooks.len());
    }
}
