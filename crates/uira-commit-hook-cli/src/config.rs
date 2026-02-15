use crate::hooks::OnFail;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Config {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ai_hooks: Option<serde_yaml_ng::Value>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub theme: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub theme_colors: Option<ThemeColorOverrides>,

    #[serde(flatten)]
    pub hooks: HashMap<String, HookConfig>,
}

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct ThemeColorOverrides {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bg: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fg: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub accent: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub warning: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub success: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub borders: Option<String>,
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

    /// Behavior when command fails: "stop" (default), "warn", or "continue"
    #[serde(default = "default_on_fail")]
    pub on_fail: OnFail,
}

fn default_on_fail() -> OnFail {
    OnFail::Stop
}

impl Config {
    pub fn from_file<P: AsRef<Path>>(path: P) -> anyhow::Result<Self> {
        let content = fs::read_to_string(path)?;
        let config: Config = serde_yaml_ng::from_str(&content)?;
        Ok(config)
    }

    pub fn default_config() -> Self {
        let mut hooks = HashMap::new();

        let pre_commit = HookConfig {
            parallel: true,
            commands: vec![Command {
                name: Some("lint".to_string()),
                run: "uira-commit-hook-cli lint {staged_files}".to_string(),
                glob: Some("**/*.{js,ts,jsx,tsx}".to_string()),
                stage_fixed: false,
                on_fail: default_on_fail(),
            }],
        };

        let post_commit = HookConfig {
            parallel: false,
            commands: vec![Command {
                name: Some("auto-push".to_string()),
                run: "git push origin HEAD".to_string(),
                glob: None,
                stage_fixed: false,
                on_fail: default_on_fail(),
            }],
        };

        hooks.insert("pre-commit".to_string(), pre_commit);
        hooks.insert("post-commit".to_string(), post_commit);

        Config {
            ai_hooks: None,
            theme: None,
            theme_colors: None,
            hooks,
        }
    }

    pub fn to_yaml(&self) -> anyhow::Result<String> {
        Ok(serde_yaml_ng::to_string(self)?)
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
        let parsed: Config = serde_yaml_ng::from_str(&yaml).unwrap();

        assert_eq!(config.hooks.len(), parsed.hooks.len());
    }

    #[test]
    fn test_parse_theme_fields_without_hook_collision() {
        let yaml = r##"
theme: dracula
theme_colors:
  accent: "#ff79c6"

pre-commit:
  commands:
    - run: cargo fmt --check
"##;

        let parsed: Config = serde_yaml_ng::from_str(yaml).unwrap();
        assert_eq!(parsed.theme.as_deref(), Some("dracula"));
        assert_eq!(
            parsed
                .theme_colors
                .as_ref()
                .and_then(|c| c.accent.as_deref()),
            Some("#ff79c6")
        );
        assert!(parsed.hooks.contains_key("pre-commit"));
        assert!(!parsed.hooks.contains_key("theme_colors"));
    }

    #[test]
    fn test_ai_hooks_block_not_flattened_into_hooks() {
        let yaml = r#"
ai_hooks:
  pre-check:
    - run: echo old

pre-commit:
  commands:
    - run: cargo fmt --check
"#;

        let parsed: Config = serde_yaml_ng::from_str(yaml).unwrap();
        assert!(parsed.ai_hooks.is_some());
        assert!(parsed.hooks.contains_key("pre-commit"));
        assert!(!parsed.hooks.contains_key("ai_hooks"));
    }
}
