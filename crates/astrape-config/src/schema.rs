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

/// OMC (Oh-My-ClaudeCode) configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OmcConfig {
    /// Default execution mode: "ultrawork" or "ecomode"
    #[serde(default = "default_execution_mode", rename = "defaultExecutionMode")]
    pub default_execution_mode: String,

    /// HUD (Heads-Up Display) settings
    #[serde(default)]
    pub hud: HudConfig,

    /// Agent tier preferences
    #[serde(default)]
    pub agent_tiers: AgentTierPreferences,

    /// Plugin settings
    #[serde(default)]
    pub plugins: PluginSettings,
}

impl Default for OmcConfig {
    fn default() -> Self {
        Self {
            default_execution_mode: default_execution_mode(),
            hud: HudConfig::default(),
            agent_tiers: AgentTierPreferences::default(),
            plugins: PluginSettings::default(),
        }
    }
}

/// HUD configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HudConfig {
    /// HUD preset: "minimal", "standard", "verbose"
    #[serde(default = "default_hud_preset")]
    pub preset: String,

    /// Enabled HUD elements
    #[serde(default)]
    pub elements: Vec<String>,

    /// Thresholds for warnings and alerts
    #[serde(default)]
    pub thresholds: HudThresholds,
}

impl Default for HudConfig {
    fn default() -> Self {
        Self {
            preset: default_hud_preset(),
            elements: default_hud_elements(),
            thresholds: HudThresholds::default(),
        }
    }
}

/// HUD thresholds
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HudThresholds {
    /// Token usage warning threshold (percentage)
    #[serde(default = "default_token_warning_threshold")]
    pub token_warning: f32,

    /// Token usage critical threshold (percentage)
    #[serde(default = "default_token_critical_threshold")]
    pub token_critical: f32,

    /// Task count warning threshold
    #[serde(default = "default_task_warning_threshold")]
    pub task_warning: usize,
}

impl Default for HudThresholds {
    fn default() -> Self {
        Self {
            token_warning: default_token_warning_threshold(),
            token_critical: default_token_critical_threshold(),
            task_warning: default_task_warning_threshold(),
        }
    }
}

/// Agent tier preferences
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentTierPreferences {
    /// Default tier for analysis tasks: "low", "medium", "high"
    #[serde(default = "default_tier")]
    pub analysis: String,

    /// Default tier for execution tasks: "low", "medium", "high"
    #[serde(default = "default_tier")]
    pub execution: String,

    /// Default tier for search tasks: "low", "medium", "high"
    #[serde(default = "default_tier")]
    pub search: String,

    /// Default tier for design tasks: "low", "medium", "high"
    #[serde(default = "default_tier")]
    pub design: String,
}

impl Default for AgentTierPreferences {
    fn default() -> Self {
        Self {
            analysis: default_tier(),
            execution: default_tier(),
            search: default_tier(),
            design: default_tier(),
        }
    }
}

/// Plugin settings
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PluginSettings {
    /// Enabled plugins
    #[serde(default)]
    pub enabled: Vec<String>,

    /// Plugin-specific configuration
    #[serde(default)]
    pub config: HashMap<String, serde_json::Value>,
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
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct McpSettings {
    /// Enabled MCP servers
    #[serde(default)]
    pub servers: HashMap<String, McpServerConfig>,
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
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AgentSettings {
    /// Individual agent configurations
    #[serde(default)]
    pub agents: HashMap<String, AgentConfig>,
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
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
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

fn default_execution_mode() -> String {
    "ultrawork".to_string()
}

fn default_hud_preset() -> String {
    "standard".to_string()
}

fn default_hud_elements() -> Vec<String> {
    vec![
        "mode".to_string(),
        "tokens".to_string(),
        "tasks".to_string(),
        "agents".to_string(),
    ]
}

fn default_token_warning_threshold() -> f32 {
    70.0
}

fn default_token_critical_threshold() -> f32 {
    90.0
}

fn default_task_warning_threshold() -> usize {
    10
}

fn default_tier() -> String {
    "medium".to_string()
}

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
