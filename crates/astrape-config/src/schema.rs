use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Main Astrape configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AstrapeConfig {
    /// AI model settings
    #[serde(default)]
    pub ai: AiSettings,

    /// MCP (Model Context Protocol) settings
    #[serde(default)]
    pub mcp: McpSettings,

    /// Agent settings
    #[serde(default)]
    pub agents: AgentSettings,

    /// Git hooks configuration
    #[serde(default)]
    pub hooks: HooksConfig,

    /// AI hooks for typos checking and other workflows
    #[serde(default)]
    pub ai_hooks: Option<AiHooksConfig>,
}

/// AI model configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiSettings {
    /// Model identifier (e.g., "anthropic/claude-sonnet-4-20250514")
    #[serde(default = "default_model")]
    pub model: String,

    /// Temperature for model responses (0.0 - 1.0)
    #[serde(default = "default_temperature")]
    pub temperature: f32,

    /// Opencode server host
    #[serde(default = "default_host")]
    pub host: String,

    /// Opencode server port
    #[serde(default = "default_port")]
    pub port: u16,

    /// Disable built-in tools
    #[serde(default = "default_disable_tools")]
    pub disable_tools: bool,

    /// Disable MCP servers
    #[serde(default = "default_disable_mcp")]
    pub disable_mcp: bool,
}

impl Default for AiSettings {
    fn default() -> Self {
        Self {
            model: default_model(),
            temperature: default_temperature(),
            host: default_host(),
            port: default_port(),
            disable_tools: default_disable_tools(),
            disable_mcp: default_disable_mcp(),
        }
    }
}

/// MCP (Model Context Protocol) settings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpSettings {
    /// Enabled MCP servers
    #[serde(default)]
    pub servers: HashMap<String, McpServerConfig>,
}

impl Default for McpSettings {
    fn default() -> Self {
        Self {
            servers: HashMap::new(),
        }
    }
}

/// Individual MCP server configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerConfig {
    /// Command to run the server
    pub command: String,

    /// Arguments for the command
    #[serde(default)]
    pub args: Vec<String>,

    /// Environment variables
    #[serde(default)]
    pub env: HashMap<String, String>,
}

/// Agent settings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentSettings {
    /// Individual agent configurations
    #[serde(default)]
    pub agents: HashMap<String, AgentConfig>,
}

impl Default for AgentSettings {
    fn default() -> Self {
        Self {
            agents: HashMap::new(),
        }
    }
}

/// Individual agent configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConfig {
    /// Agent model override (uses ai.model if not specified)
    pub model: Option<String>,

    /// Agent-specific settings
    #[serde(default)]
    pub settings: HashMap<String, serde_json::Value>,
}

/// Git hooks configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HooksConfig {
    /// Pre-commit hook configuration
    #[serde(default)]
    pub pre_commit: Option<HookConfig>,

    /// Post-commit hook configuration
    #[serde(default)]
    pub post_commit: Option<HookConfig>,

    /// Pre-push hook configuration
    #[serde(default)]
    pub pre_push: Option<HookConfig>,
}

impl Default for HooksConfig {
    fn default() -> Self {
        Self {
            pre_commit: None,
            post_commit: None,
            pre_push: None,
        }
    }
}

/// Individual hook configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookConfig {
    /// Run commands in parallel
    #[serde(default)]
    pub parallel: bool,

    /// Commands to execute
    #[serde(default)]
    pub commands: Vec<HookCommand>,
}

/// Individual hook command
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookCommand {
    /// Command name
    pub name: String,

    /// Command to run
    pub run: String,

    /// Glob pattern for files to match
    #[serde(default)]
    pub glob: Option<String>,

    /// Stop execution on failure
    #[serde(default)]
    pub on_fail: Option<String>,
}

/// AI hooks configuration for typos checking and other workflows
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiHooksConfig {
    /// Pre-check hook
    #[serde(default)]
    pub pre_check: Option<Vec<AiHookCommand>>,

    /// Post-check hook
    #[serde(default)]
    pub post_check: Option<Vec<AiHookCommand>>,

    /// Pre-AI hook
    #[serde(default)]
    pub pre_ai: Option<Vec<AiHookCommand>>,

    /// Post-AI hook
    #[serde(default)]
    pub post_ai: Option<Vec<AiHookCommand>>,

    /// Pre-fix hook
    #[serde(default)]
    pub pre_fix: Option<Vec<AiHookCommand>>,

    /// Post-fix hook
    #[serde(default)]
    pub post_fix: Option<Vec<AiHookCommand>>,
}

/// AI hook command
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiHookCommand {
    /// Glob matcher for files
    #[serde(default)]
    pub matcher: Option<String>,

    /// Command to run
    pub run: String,

    /// Stop execution on failure
    #[serde(default)]
    pub on_fail: Option<String>,
}

// Default value functions
fn default_model() -> String {
    "anthropic/claude-sonnet-4-20250514".to_string()
}

fn default_temperature() -> f32 {
    0.7
}

fn default_host() -> String {
    "127.0.0.1".to_string()
}

fn default_port() -> u16 {
    4096
}

fn default_disable_tools() -> bool {
    true
}

fn default_disable_mcp() -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_ai_settings() {
        let ai = AiSettings::default();
        assert_eq!(ai.model, "anthropic/claude-sonnet-4-20250514");
        assert_eq!(ai.temperature, 0.7);
        assert_eq!(ai.host, "127.0.0.1");
        assert_eq!(ai.port, 4096);
        assert!(ai.disable_tools);
        assert!(ai.disable_mcp);
    }

    #[test]
    fn test_deserialize_ai_settings() {
        let yaml = r#"
model: anthropic/claude-opus-4-1
temperature: 0.5
host: localhost
port: 8080
disable_tools: false
disable_mcp: false
"#;
        let ai: AiSettings = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(ai.model, "anthropic/claude-opus-4-1");
        assert_eq!(ai.temperature, 0.5);
        assert_eq!(ai.host, "localhost");
        assert_eq!(ai.port, 8080);
        assert!(!ai.disable_tools);
        assert!(!ai.disable_mcp);
    }

    #[test]
    fn test_deserialize_hook_config() {
        let yaml = r#"
parallel: true
commands:
  - name: fmt
    run: cargo fmt --check
  - name: clippy
    run: cargo clippy -- -D warnings
    glob: "**/*.rs"
"#;
        let hook: HookConfig = serde_yaml::from_str(yaml).unwrap();
        assert!(hook.parallel);
        assert_eq!(hook.commands.len(), 2);
        assert_eq!(hook.commands[0].name, "fmt");
        assert_eq!(hook.commands[1].glob, Some("**/*.rs".to_string()));
    }

    #[test]
    fn test_deserialize_full_config() {
        let yaml = r#"
ai:
  model: anthropic/claude-sonnet-4-20250514
  temperature: 0.7

hooks:
  pre_commit:
    parallel: true
    commands:
      - name: fmt
        run: cargo fmt --check
  post_commit:
    parallel: false
    commands:
      - name: auto-push
        run: git push origin HEAD
"#;
        let config: AstrapeConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.ai.model, "anthropic/claude-sonnet-4-20250514");
        assert!(config.hooks.pre_commit.is_some());
        assert!(config.hooks.post_commit.is_some());
    }
}
