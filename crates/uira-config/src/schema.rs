use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Main Uira configuration
///
/// Configuration is loaded from (in priority order):
/// 1. `uira.jsonc` - JSON with comments
/// 2. `uira.json` - Standard JSON
/// 3. `uira.yml` / `uira.yaml` - YAML format
///
/// Also checks hidden variants (`.uira.*`) and `~/.config/uira/` for global config.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UiraConfig {
    /// Typos command settings (AI-assisted typo checking)
    #[serde(default)]
    pub typos: TyposSettings,

    /// OpenCode server settings
    #[serde(default)]
    pub opencode: OpencodeSettings,

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

    /// Score-based verification goals
    #[serde(default)]
    pub goals: GoalsConfig,
}

// ============================================================================
// OpenCode Configuration
// ============================================================================

/// OpenCode server settings
///
/// Configuration for connecting to OpenCode server for agent-based model routing.
///
/// # Example
///
/// ```yaml
/// opencode:
///   host: 127.0.0.1
///   port: 4096
///   timeout_secs: 120
///   auto_start: true
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpencodeSettings {
    /// Server host (default: 127.0.0.1)
    #[serde(default = "default_opencode_host")]
    pub host: String,

    /// Server port (default: 4096)
    #[serde(default = "default_opencode_port")]
    pub port: u16,

    /// Request timeout in seconds (default: 120)
    #[serde(default = "default_opencode_timeout")]
    pub timeout_secs: u64,

    /// Auto-start OpenCode server (default: true)
    #[serde(default = "default_opencode_auto_start")]
    pub auto_start: bool,
}

impl Default for OpencodeSettings {
    fn default() -> Self {
        Self {
            host: default_opencode_host(),
            port: default_opencode_port(),
            timeout_secs: default_opencode_timeout(),
            auto_start: default_opencode_auto_start(),
        }
    }
}

fn default_opencode_host() -> String {
    "127.0.0.1".to_string()
}

fn default_opencode_port() -> u16 {
    4096
}

fn default_opencode_timeout() -> u64 {
    120
}

fn default_opencode_auto_start() -> bool {
    true
}

// ============================================================================
// Typos Configuration
// ============================================================================

/// Typos command settings for AI-assisted typo checking
///
/// Configuration for the `uira typos --ai` command.
///
/// # Example
///
/// ```yaml
/// typos:
///   ai:
///     model: "anthropic/claude-sonnet-4-20250514"
///     host: "127.0.0.1"
///     port: 4096
///     disable_tools: true
///     disable_mcp: true
/// ```
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TyposSettings {
    /// AI settings for typos checking
    #[serde(default)]
    pub ai: TyposAiSettings,
}

/// AI settings for typos command
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TyposAiSettings {
    /// Model identifier (e.g., "anthropic/claude-sonnet-4-20250514")
    #[serde(default = "default_typos_model")]
    pub model: String,

    /// OpenCode server host (default: 127.0.0.1)
    #[serde(default = "default_typos_host")]
    pub host: String,

    /// OpenCode server port (default: 4096)
    #[serde(default = "default_typos_port")]
    pub port: u16,

    /// Disable built-in tools (default: true)
    #[serde(default = "default_typos_disable_tools")]
    pub disable_tools: bool,

    /// Disable MCP servers (default: true)
    #[serde(default = "default_typos_disable_mcp")]
    pub disable_mcp: bool,
}

impl Default for TyposAiSettings {
    fn default() -> Self {
        Self {
            model: default_typos_model(),
            host: default_typos_host(),
            port: default_typos_port(),
            disable_tools: default_typos_disable_tools(),
            disable_mcp: default_typos_disable_mcp(),
        }
    }
}

impl TyposAiSettings {
    /// Parse model string into (provider, model) tuple
    pub fn parse_model(&self) -> (String, String) {
        if let Some((provider, model)) = self.model.split_once('/') {
            (provider.to_string(), model.to_string())
        } else {
            ("anthropic".to_string(), self.model.clone())
        }
    }
}

fn default_typos_model() -> String {
    "anthropic/claude-sonnet-4-20250514".to_string()
}

fn default_typos_host() -> String {
    "127.0.0.1".to_string()
}

fn default_typos_port() -> u16 {
    4096
}

fn default_typos_disable_tools() -> bool {
    true
}

fn default_typos_disable_mcp() -> bool {
    true
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

/// Agent settings - a map of agent names to their configurations
///
/// In YAML/JSON:
/// ```yaml
/// agents:
///   explore:
///     model: "opencode/gpt-5-nano"
///   librarian:
///     model: "opencode/big-pickle"
/// ```
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AgentSettings {
    #[serde(flatten)]
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

// ============================================================================
// Goal Verification Configuration
// ============================================================================

/// Goal configuration for score-based verification
///
/// Goals define measurable success criteria that can be checked during
/// agent loops (ralph, ultrawork). Each goal runs a command that outputs
/// a score (0-100), and the goal passes when the score meets the target.
///
/// # Example
///
/// ```yaml
/// goals:
///   - name: pixel-match
///     workspace: .uira/goals/pixel-match/
///     command: bun run check.ts
///     target: 99.9
///
///   - name: test-coverage
///     command: bun run coverage --json | jq '.total'
///     target: 80
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoalConfig {
    /// Unique name for this goal
    pub name: String,

    /// Working directory for the command (relative to project root)
    /// If specified, the command runs inside this directory.
    /// Useful for isolating goal-specific files (reference images, scripts, etc.)
    #[serde(default)]
    pub workspace: Option<String>,

    /// Command to execute that outputs a score (0-100) to stdout
    /// The command must:
    /// - Exit with code 0 on success
    /// - Print a single number (0-100) to stdout
    /// - Use stderr for logging/debug output
    pub command: String,

    /// Target score threshold (0-100) to consider the goal passed
    pub target: f64,

    /// Optional timeout in seconds for the command (default: 60)
    #[serde(default = "default_goal_timeout")]
    pub timeout_secs: u64,

    /// Whether this goal is enabled (default: true)
    #[serde(default = "default_goal_enabled")]
    pub enabled: bool,

    /// Optional description of what this goal measures
    #[serde(default)]
    pub description: Option<String>,
}

fn default_goal_timeout() -> u64 {
    60
}

fn default_goal_enabled() -> bool {
    true
}

/// Goals configuration section
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoalsConfig {
    /// List of goal definitions
    #[serde(default)]
    pub goals: Vec<GoalConfig>,

    /// Check interval in seconds for continuous verification (default: 30)
    #[serde(default = "default_check_interval")]
    pub check_interval_secs: u64,

    /// Maximum iterations before giving up (default: 100)
    #[serde(default = "default_max_iterations")]
    pub max_iterations: u32,

    /// Whether to run goals automatically at end of each iteration (default: true)
    #[serde(default = "default_auto_verify")]
    pub auto_verify: bool,
}

impl Default for GoalsConfig {
    fn default() -> Self {
        Self {
            goals: Vec::new(),
            check_interval_secs: default_check_interval(),
            max_iterations: default_max_iterations(),
            auto_verify: default_auto_verify(),
        }
    }
}

fn default_check_interval() -> u64 {
    30
}

fn default_max_iterations() -> u32 {
    100
}

fn default_auto_verify() -> bool {
    true
}

// Default value functions

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

#[cfg(test)]
mod tests {
    use super::*;

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
        let config: UiraConfig = serde_yaml::from_str(yaml).unwrap();
        assert!(config.hooks.pre_commit.is_some());
        assert!(config.hooks.post_commit.is_some());
    }

    #[test]
    fn test_deserialize_goal_config() {
        let yaml = r#"
name: pixel-match
workspace: .uira/goals/pixel-match/
command: bun run check.ts
target: 99.9
"#;
        let goal: GoalConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(goal.name, "pixel-match");
        assert_eq!(
            goal.workspace,
            Some(".uira/goals/pixel-match/".to_string())
        );
        assert_eq!(goal.command, "bun run check.ts");
        assert!((goal.target - 99.9).abs() < 0.01);
        assert_eq!(goal.timeout_secs, 60);
        assert!(goal.enabled);
    }

    #[test]
    fn test_deserialize_goals_config() {
        let yaml = r#"
goals:
  - name: pixel-match
    workspace: .uira/goals/pixel-match/
    command: bun run check.ts
    target: 99.9
  - name: test-coverage
    command: "bun run coverage --json | jq '.total'"
    target: 80
    enabled: false
check_interval_secs: 15
max_iterations: 50
"#;
        let config: GoalsConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.goals.len(), 2);
        assert_eq!(config.goals[0].name, "pixel-match");
        assert_eq!(config.goals[1].name, "test-coverage");
        assert!(!config.goals[1].enabled);
        assert_eq!(config.check_interval_secs, 15);
        assert_eq!(config.max_iterations, 50);
    }

    #[test]
    fn test_deserialize_config_with_goals() {
        let yaml = r#"
typos:
  ai:
    model: anthropic/claude-sonnet-4-20250514

goals:
  goals:
    - name: lighthouse-perf
      command: ./scripts/lighthouse-check.sh
      target: 90
"#;
        let config: UiraConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.goals.goals.len(), 1);
        assert_eq!(config.goals.goals[0].name, "lighthouse-perf");
        assert!((config.goals.goals[0].target - 90.0).abs() < 0.01);
    }

    #[test]
    fn test_goal_config_defaults() {
        let yaml = r#"
name: simple
command: echo 100
target: 100
"#;
        let goal: GoalConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(goal.timeout_secs, 60);
        assert!(goal.enabled);
        assert!(goal.workspace.is_none());
        assert!(goal.description.is_none());
    }

    #[test]
    fn test_goals_config_defaults() {
        let config = GoalsConfig::default();
        assert!(config.goals.is_empty());
        assert_eq!(config.check_interval_secs, 30);
        assert_eq!(config.max_iterations, 100);
        assert!(config.auto_verify);
    }
}
