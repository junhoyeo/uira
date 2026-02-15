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
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiraConfig {
    /// TUI theme name (default, dark, light, dracula, nord)
    #[serde(default = "default_tui_theme")]
    pub theme: String,

    /// Optional per-color theme overrides using hex values (e.g. "#282a36")
    #[serde(default)]
    pub theme_colors: ThemeColorOverrides,

    /// Typos command settings (AI-assisted typo checking)
    #[serde(default)]
    pub typos: TyposSettings,

    /// Diagnostics command settings (AI-assisted lint/type error fixing)
    #[serde(default)]
    pub diagnostics: DiagnosticsSettings,

    /// Comments command settings (AI-assisted comment removal/preservation)
    #[serde(default)]
    pub comments: CommentsSettings,

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

    /// Context compaction settings
    #[serde(default)]
    pub compaction: CompactionSettings,

    /// Permission rules for tool execution
    #[serde(default)]
    pub permissions: PermissionsSettings,

    /// Skills settings for loading SKILL.md files
    #[serde(default)]
    pub skills: SkillsSettings,

    /// Gateway settings for WebSocket control plane
    #[serde(default)]
    pub gateway: GatewaySettings,

    /// Channel settings for multi-channel messaging
    #[serde(default)]
    pub channels: ChannelSettings,

    /// Provider-specific settings
    #[serde(default)]
    pub providers: ProvidersSettings,
}

impl Default for UiraConfig {
    fn default() -> Self {
        Self {
            theme: default_tui_theme(),
            theme_colors: ThemeColorOverrides::default(),
            typos: TyposSettings::default(),
            diagnostics: DiagnosticsSettings::default(),
            comments: CommentsSettings::default(),
            opencode: OpencodeSettings::default(),
            mcp: McpSettings::default(),
            agents: AgentSettings::default(),
            hooks: HooksConfig::default(),
            ai_hooks: None,
            goals: GoalsConfig::default(),
            compaction: CompactionSettings::default(),
            permissions: PermissionsSettings::default(),
            skills: SkillsSettings::default(),
            gateway: GatewaySettings::default(),
            channels: ChannelSettings::default(),
            providers: ProvidersSettings::default(),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ThemeColorOverrides {
    #[serde(default)]
    pub bg: Option<String>,

    #[serde(default)]
    pub fg: Option<String>,

    #[serde(default)]
    pub accent: Option<String>,

    #[serde(default)]
    pub error: Option<String>,

    #[serde(default)]
    pub warning: Option<String>,

    #[serde(default)]
    pub success: Option<String>,

    #[serde(default)]
    pub borders: Option<String>,
}

fn default_tui_theme() -> String {
    "default".to_string()
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
/// The AI workflow uses an embedded agent with full tool access.
///
/// # Example
///
/// ```yaml
/// typos:
///   ai:
///     model: "anthropic/claude-sonnet-4-20250514"
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
}

impl Default for TyposAiSettings {
    fn default() -> Self {
        Self {
            model: default_typos_model(),
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

// ============================================================================
// Diagnostics Configuration
// ============================================================================

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DiagnosticsSettings {
    #[serde(default)]
    pub ai: DiagnosticsAiSettings,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiagnosticsAiSettings {
    #[serde(default = "default_diagnostics_model")]
    pub model: String,

    #[serde(default = "default_diagnostics_severity")]
    pub severity: String,

    #[serde(default = "default_diagnostics_confidence")]
    pub confidence_threshold: f64,

    #[serde(default = "default_diagnostics_languages")]
    pub languages: Vec<String>,
}

impl Default for DiagnosticsAiSettings {
    fn default() -> Self {
        Self {
            model: default_diagnostics_model(),
            severity: default_diagnostics_severity(),
            confidence_threshold: default_diagnostics_confidence(),
            languages: default_diagnostics_languages(),
        }
    }
}

impl DiagnosticsAiSettings {
    pub fn parse_model(&self) -> (String, String) {
        if let Some((provider, model)) = self.model.split_once('/') {
            (provider.to_string(), model.to_string())
        } else {
            ("anthropic".to_string(), self.model.clone())
        }
    }
}

fn default_diagnostics_model() -> String {
    "anthropic/claude-sonnet-4-20250514".to_string()
}

fn default_diagnostics_severity() -> String {
    "error".to_string()
}

fn default_diagnostics_confidence() -> f64 {
    0.8
}

fn default_diagnostics_languages() -> Vec<String> {
    vec![
        "js".to_string(),
        "ts".to_string(),
        "tsx".to_string(),
        "jsx".to_string(),
    ]
}

// ============================================================================
// Comments Configuration
// ============================================================================

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CommentsSettings {
    #[serde(default)]
    pub ai: CommentsAiSettings,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommentsAiSettings {
    #[serde(default = "default_comments_model")]
    pub model: String,

    #[serde(default = "default_comments_pragma")]
    pub pragma_format: String,

    #[serde(default = "default_comments_docstrings")]
    pub include_docstrings: bool,
}

impl Default for CommentsAiSettings {
    fn default() -> Self {
        Self {
            model: default_comments_model(),
            pragma_format: default_comments_pragma(),
            include_docstrings: default_comments_docstrings(),
        }
    }
}

impl CommentsAiSettings {
    pub fn parse_model(&self) -> (String, String) {
        if let Some((provider, model)) = self.model.split_once('/') {
            (provider.to_string(), model.to_string())
        } else {
            ("anthropic".to_string(), self.model.clone())
        }
    }
}

fn default_comments_model() -> String {
    "anthropic/claude-sonnet-4-20250514".to_string()
}

fn default_comments_pragma() -> String {
    "@uira-allow".to_string()
}

fn default_comments_docstrings() -> bool {
    false
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
    #[serde(default, deserialize_with = "deserialize_mcp_servers")]
    pub servers: Vec<NamedMcpServerConfig>,
}

impl McpSettings {
    pub fn get(&self, name: &str) -> Option<&McpServerConfig> {
        self.servers
            .iter()
            .find(|server| server.name == name)
            .map(|server| &server.config)
    }

    pub fn contains_key(&self, name: &str) -> bool {
        self.get(name).is_some()
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NamedMcpServerConfig {
    pub name: String,
    #[serde(flatten)]
    pub config: McpServerConfig,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
enum McpServersRepr {
    List(Vec<NamedMcpServerConfig>),
    Map(HashMap<String, McpServerConfig>),
}

fn deserialize_mcp_servers<'de, D>(deserializer: D) -> Result<Vec<NamedMcpServerConfig>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let repr = McpServersRepr::deserialize(deserializer)?;
    let mut servers = match repr {
        McpServersRepr::List(list) => list,
        McpServersRepr::Map(map) => map
            .into_iter()
            .map(|(name, config)| NamedMcpServerConfig { name, config })
            .collect(),
    };

    servers.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(servers)
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

// ============================================================================
// Compaction Configuration
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompactionSettings {
    #[serde(default = "default_compaction_enabled")]
    pub enabled: bool,

    #[serde(default = "default_compaction_threshold")]
    pub threshold: f64,

    #[serde(default = "default_protected_tokens")]
    pub protected_tokens: usize,

    #[serde(default = "default_compaction_strategy")]
    pub strategy: String,

    #[serde(default)]
    pub summarization_model: Option<String>,
}

impl Default for CompactionSettings {
    fn default() -> Self {
        Self {
            enabled: default_compaction_enabled(),
            threshold: default_compaction_threshold(),
            protected_tokens: default_protected_tokens(),
            strategy: default_compaction_strategy(),
            summarization_model: None,
        }
    }
}

fn default_compaction_enabled() -> bool {
    true
}

fn default_compaction_threshold() -> f64 {
    0.8
}

fn default_protected_tokens() -> usize {
    40_000
}

fn default_compaction_strategy() -> String {
    "summarize".to_string()
}

// ============================================================================
// Providers Configuration
// ============================================================================

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProvidersSettings {
    #[serde(default)]
    pub anthropic: AnthropicProviderSettings,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AnthropicProviderSettings {
    #[serde(default)]
    pub payload_log: PayloadLogSettings,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PayloadLogSettings {
    #[serde(default)]
    pub enabled: bool,

    #[serde(default)]
    pub path: Option<String>,
}

// ============================================================================
// Permissions Configuration
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PermissionsSettings {
    #[serde(default)]
    pub rules: Vec<PermissionRuleConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionRuleConfig {
    #[serde(default)]
    pub name: Option<String>,

    pub permission: String,

    pub pattern: String,

    pub action: PermissionActionConfig,

    #[serde(default)]
    pub comment: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum PermissionActionConfig {
    #[default]
    Allow,
    Deny,
    Ask,
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

// ============================================================================
// Skills Configuration
// ============================================================================

/// Skills settings for loading SKILL.md instruction files
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillsSettings {
    /// Whether skills loading is enabled
    #[serde(default)]
    pub enabled: bool,

    /// Directories to scan for SKILL.md files
    #[serde(default = "default_skills_paths")]
    pub paths: Vec<String>,

    /// List of active skill names to load
    #[serde(default)]
    pub active: Vec<String>,
}

impl Default for SkillsSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            paths: default_skills_paths(),
            active: Vec::new(),
        }
    }
}

fn default_skills_paths() -> Vec<String> {
    vec!["~/.uira/skills".to_string(), ".uira/skills".to_string()]
}

// ============================================================================
// Gateway Configuration
// ============================================================================

/// Gateway settings for the WebSocket control plane
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GatewaySettings {
    /// Whether the gateway is enabled
    #[serde(default)]
    pub enabled: bool,

    /// Host to bind the gateway server
    #[serde(default = "default_gateway_host")]
    pub host: String,

    /// Port to bind the gateway server
    #[serde(default = "default_gateway_port")]
    pub port: u16,

    /// Maximum number of concurrent sessions
    #[serde(default = "default_max_sessions")]
    pub max_sessions: usize,

    /// Default model for gateway sessions
    #[serde(default = "default_gateway_model")]
    pub model: String,

    /// Default provider for gateway sessions
    #[serde(default = "default_gateway_provider")]
    pub provider: String,

    /// Optional authentication token for gateway access
    #[serde(default)]
    pub auth_token: Option<String>,

    /// Idle timeout in seconds before a session is cleaned up
    #[serde(default = "default_idle_timeout")]
    pub idle_timeout_secs: Option<u64>,

    /// Working directory for gateway-spawned sessions
    #[serde(default)]
    pub working_directory: Option<String>,
}

impl Default for GatewaySettings {
    fn default() -> Self {
        Self {
            enabled: false,
            host: default_gateway_host(),
            port: default_gateway_port(),
            max_sessions: default_max_sessions(),
            model: default_gateway_model(),
            provider: default_gateway_provider(),
            auth_token: None,
            idle_timeout_secs: default_idle_timeout(),
            working_directory: None,
        }
    }
}

fn default_gateway_host() -> String {
    "127.0.0.1".to_string()
}

fn default_gateway_port() -> u16 {
    18789
}

fn default_max_sessions() -> usize {
    10
}

fn default_gateway_model() -> String {
    "claude-sonnet-4-20250514".to_string()
}

fn default_gateway_provider() -> String {
    "anthropic".to_string()
}

fn default_idle_timeout() -> Option<u64> {
    Some(1800)
}

// ============================================================================
// Channel Configuration
// ============================================================================

/// Channel settings for multi-channel messaging
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ChannelSettings {
    #[serde(default)]
    pub telegram: Option<TelegramChannelConfig>,

    #[serde(default)]
    pub telegram_accounts: Vec<TelegramChannelConfig>,

    #[serde(default)]
    pub slack: Option<SlackChannelConfig>,

    #[serde(default)]
    pub slack_accounts: Vec<SlackChannelConfig>,
}

fn default_account_id() -> String {
    "default".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelegramChannelConfig {
    #[serde(default = "default_account_id")]
    pub account_id: String,

    pub bot_token: String,

    #[serde(default)]
    pub allowed_users: Vec<String>,

    #[serde(default)]
    pub active_skills: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlackChannelConfig {
    #[serde(default = "default_account_id")]
    pub account_id: String,

    pub bot_token: String,

    pub app_token: String,

    #[serde(default)]
    pub allowed_channels: Vec<String>,

    #[serde(default)]
    pub active_skills: Vec<String>,
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
        let hook: HookConfig = serde_yaml_ng::from_str(yaml).unwrap();
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
        let config: UiraConfig = serde_yaml_ng::from_str(yaml).unwrap();
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
        let goal: GoalConfig = serde_yaml_ng::from_str(yaml).unwrap();
        assert_eq!(goal.name, "pixel-match");
        assert_eq!(goal.workspace, Some(".uira/goals/pixel-match/".to_string()));
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
        let config: GoalsConfig = serde_yaml_ng::from_str(yaml).unwrap();
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
        let config: UiraConfig = serde_yaml_ng::from_str(yaml).unwrap();
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
        let goal: GoalConfig = serde_yaml_ng::from_str(yaml).unwrap();
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

    #[test]
    fn test_diagnostics_settings_defaults() {
        let settings = DiagnosticsSettings::default();
        assert_eq!(settings.ai.model, "anthropic/claude-sonnet-4-20250514");
        assert_eq!(settings.ai.severity, "error");
        assert!((settings.ai.confidence_threshold - 0.8).abs() < 0.01);
        assert_eq!(settings.ai.languages, vec!["js", "ts", "tsx", "jsx"]);
    }

    #[test]
    fn test_diagnostics_settings_parse() {
        let yaml = r#"
ai:
  model: openai/gpt-4o
  severity: warning
  confidence_threshold: 0.9
  languages:
    - ts
    - rust
"#;
        let settings: DiagnosticsSettings = serde_yaml_ng::from_str(yaml).unwrap();
        assert_eq!(settings.ai.model, "openai/gpt-4o");
        assert_eq!(settings.ai.severity, "warning");
        assert!((settings.ai.confidence_threshold - 0.9).abs() < 0.01);
        assert_eq!(settings.ai.languages, vec!["ts", "rust"]);

        let (provider, model) = settings.ai.parse_model();
        assert_eq!(provider, "openai");
        assert_eq!(model, "gpt-4o");
    }

    #[test]
    fn test_comments_settings_defaults() {
        let settings = CommentsSettings::default();
        assert_eq!(settings.ai.model, "anthropic/claude-sonnet-4-20250514");
        assert_eq!(settings.ai.pragma_format, "@uira-allow");
        assert!(!settings.ai.include_docstrings);
    }

    #[test]
    fn test_comments_settings_parse() {
        let yaml = r#"
ai:
  model: anthropic/claude-opus-4
  pragma_format: "@allow-comment"
  include_docstrings: true
"#;
        let settings: CommentsSettings = serde_yaml_ng::from_str(yaml).unwrap();
        assert_eq!(settings.ai.model, "anthropic/claude-opus-4");
        assert_eq!(settings.ai.pragma_format, "@allow-comment");
        assert!(settings.ai.include_docstrings);
    }

    #[test]
    fn test_full_config_with_diagnostics_comments() {
        let yaml = r#"
typos:
  ai:
    model: anthropic/claude-sonnet-4-20250514

diagnostics:
  ai:
    severity: error
    languages:
      - ts
      - tsx

comments:
  ai:
    include_docstrings: true
"#;
        let config: UiraConfig = serde_yaml_ng::from_str(yaml).unwrap();
        assert_eq!(config.diagnostics.ai.severity, "error");
        assert_eq!(config.diagnostics.ai.languages, vec!["ts", "tsx"]);
        assert!(config.comments.ai.include_docstrings);
    }

    #[test]
    fn test_permissions_settings_defaults() {
        let settings = PermissionsSettings::default();
        assert!(settings.rules.is_empty());
    }

    #[test]
    fn test_permissions_settings_parse() {
        let yaml = r#"
rules:
  - permission: "file:read"
    pattern: "**"
    action: allow
  - permission: "file:write"
    pattern: "src/**"
    action: allow
    name: "allow-src-writes"
  - permission: "shell:execute"
    pattern: "**"
    action: ask
"#;
        let settings: PermissionsSettings = serde_yaml_ng::from_str(yaml).unwrap();
        assert_eq!(settings.rules.len(), 3);
        assert_eq!(settings.rules[0].permission, "file:read");
        assert_eq!(settings.rules[0].action, PermissionActionConfig::Allow);
        assert_eq!(settings.rules[1].name, Some("allow-src-writes".to_string()));
        assert_eq!(settings.rules[2].action, PermissionActionConfig::Ask);
    }

    #[test]
    fn test_full_config_with_permissions() {
        let yaml = r#"
permissions:
  rules:
    - permission: "file:write"
      pattern: "**/.env*"
      action: deny
"#;
        let config: UiraConfig = serde_yaml_ng::from_str(yaml).unwrap();
        assert_eq!(config.permissions.rules.len(), 1);
        assert_eq!(
            config.permissions.rules[0].action,
            PermissionActionConfig::Deny
        );
    }

    #[test]
    fn test_mcp_servers_deserialize_from_list() {
        let yaml = r#"
mcp:
   servers:
     - name: filesystem
       command: npx -y @anthropic/mcp-server-filesystem /tmp
     - name: github
       command: npx -y @anthropic/mcp-server-github
       env:
         GITHUB_TOKEN: ${GITHUB_TOKEN}
"#;

        let config: UiraConfig = serde_yaml_ng::from_str(yaml).unwrap();
        assert_eq!(config.mcp.servers.len(), 2);
        assert!(config.mcp.contains_key("filesystem"));
        assert!(config.mcp.contains_key("github"));
        assert_eq!(
            config.mcp.get("filesystem").unwrap().command,
            "npx -y @anthropic/mcp-server-filesystem /tmp"
        );
    }

    #[test]
    fn test_mcp_servers_deserialize_from_map_legacy_format() {
        let yaml = r#"
mcp:
   servers:
     context7:
       command: npx
       args: ["-y", "@upstash/context7-mcp"]
     exa:
       command: npx
       args: ["-y", "exa-mcp-server"]
"#;

        let config: UiraConfig = serde_yaml_ng::from_str(yaml).unwrap();
        assert_eq!(config.mcp.servers.len(), 2);
        assert!(config.mcp.contains_key("context7"));
        assert!(config.mcp.contains_key("exa"));
        assert_eq!(
            config.mcp.get("context7").unwrap().args,
            vec!["-y".to_string(), "@upstash/context7-mcp".to_string()]
        );
    }

    #[test]
    fn test_tui_theme_defaults() {
        let config = UiraConfig::default();
        assert_eq!(config.theme, "default");
        assert!(config.theme_colors.bg.is_none());
        assert!(config.theme_colors.accent.is_none());
    }

    #[test]
    fn test_tui_theme_parsing() {
        let yaml = r##"
theme: dracula
theme_colors:
    accent: "#ff79c6"
    borders: "#6272a4"
"##;

        let config: UiraConfig = serde_yaml_ng::from_str(yaml).unwrap();
        assert_eq!(config.theme, "dracula");
        assert_eq!(config.theme_colors.accent, Some("#ff79c6".to_string()));
        assert_eq!(config.theme_colors.borders, Some("#6272a4".to_string()));
    }

    #[test]
    fn test_skills_settings_defaults() {
        let settings = SkillsSettings::default();
        assert!(!settings.enabled);
        assert_eq!(settings.paths.len(), 2);
        assert!(settings.active.is_empty());
    }

    #[test]
    fn test_gateway_settings_defaults() {
        let settings = GatewaySettings::default();
        assert!(!settings.enabled);
        assert_eq!(settings.host, "127.0.0.1");
        assert_eq!(settings.port, 18789);
        assert_eq!(settings.max_sessions, 10);
        assert_eq!(settings.model, "claude-sonnet-4-20250514");
        assert_eq!(settings.provider, "anthropic");
        assert!(settings.auth_token.is_none());
        assert_eq!(settings.idle_timeout_secs, Some(1800));
        assert!(settings.working_directory.is_none());
    }

    #[test]
    fn test_channel_settings_defaults() {
        let settings = ChannelSettings::default();
        assert!(settings.telegram.is_none());
        assert!(settings.slack.is_none());
    }

    #[test]
    fn test_full_config_with_skills() {
        let yaml = r#"
skills:
  enabled: true
  paths:
    - "~/.uira/skills"
    - "./project-skills"
  active:
    - coding-agent
    - debugger
"#;
        let config: UiraConfig = serde_yaml_ng::from_str(yaml).unwrap();
        assert!(config.skills.enabled);
        assert_eq!(config.skills.paths.len(), 2);
        assert_eq!(config.skills.active, vec!["coding-agent", "debugger"]);
    }

    #[test]
    fn test_full_config_with_gateway() {
        let yaml = r#"
gateway:
  enabled: true
  host: "0.0.0.0"
  port: 9000
  max_sessions: 20
"#;
        let config: UiraConfig = serde_yaml_ng::from_str(yaml).unwrap();
        assert!(config.gateway.enabled);
        assert_eq!(config.gateway.host, "0.0.0.0");
        assert_eq!(config.gateway.port, 9000);
        assert_eq!(config.gateway.max_sessions, 20);
    }

    #[test]
    fn test_full_config_with_channels() {
        let yaml = r#"
channels:
  telegram:
    bot_token: "123456:ABC-DEF"
    allowed_users:
      - "user123"
  slack:
    bot_token: "xoxb-test"
    app_token: "xapp-test"
    allowed_channels:
      - "C12345"
"#;
        let config: UiraConfig = serde_yaml_ng::from_str(yaml).unwrap();
        let tg = config.channels.telegram.unwrap();
        assert_eq!(tg.bot_token, "123456:ABC-DEF");
        assert_eq!(tg.allowed_users, vec!["user123"]);
        let slack = config.channels.slack.unwrap();
        assert_eq!(slack.bot_token, "xoxb-test");
        assert_eq!(slack.app_token, "xapp-test");
        assert_eq!(slack.allowed_channels, vec!["C12345"]);
    }
}
