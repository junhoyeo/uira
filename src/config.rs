use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Config {
    #[serde(flatten)]
    pub hooks: HashMap<String, HookConfig>,
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
    pub name: Option<String>,

    pub run: String,

    pub glob: Option<String>,

    #[serde(default)]
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

        hooks.insert("pre-commit".to_string(), pre_commit);

        Config { hooks }
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

        let pre_commit = &config.hooks["pre-commit"];
        assert!(pre_commit.parallel);
        assert_eq!(pre_commit.commands.len(), 1);
    }

    #[test]
    fn test_yaml_roundtrip() {
        let config = Config::default_config();
        let yaml = config.to_yaml().unwrap();
        let parsed: Config = serde_yaml::from_str(&yaml).unwrap();

        assert_eq!(config.hooks.len(), parsed.hooks.len());
    }
}
