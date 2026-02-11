//! Agent configuration

use crate::context::{CompactionConfig, CompactionStrategy};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use uira_core::schema::{
    CompactionSettings, GoalConfig, NamedMcpServerConfig, PermissionActionConfig,
    PermissionRuleConfig,
};
use uira_permissions::{ConfigAction, ConfigRule};
use uira_sandbox::SandboxPolicy;
use uira_types::{SandboxPreference, ToolSpec};

/// Configuration for the agent
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConfig {
    /// Maximum number of turns before stopping
    #[serde(default = "default_max_turns")]
    pub max_turns: usize,

    /// Maximum tokens per turn
    #[serde(default = "default_max_tokens")]
    pub max_tokens: usize,

    /// Sandbox policy
    #[serde(default)]
    pub sandbox_policy: SandboxPolicy,

    /// Default sandbox preference for tools
    #[serde(default)]
    pub sandbox_preference: SandboxPreference,

    /// Working directory
    #[serde(default)]
    pub working_directory: Option<PathBuf>,

    /// Whether to require approval for write operations
    #[serde(default = "default_true")]
    pub require_approval_for_writes: bool,

    /// Whether to require approval for command execution
    #[serde(default = "default_true")]
    pub require_approval_for_commands: bool,

    /// Enable ralph mode for persistent task completion
    #[serde(default)]
    pub ralph_mode: bool,

    /// Enable todo continuation enforcer (auto-continue when incomplete todos exist)
    #[serde(default = "default_true")]
    pub todo_continuation: bool,

    /// Maximum auto-continuation attempts before stopping
    #[serde(default = "default_max_continuation_attempts")]
    pub max_continuation_attempts: usize,

    /// Enable experimental task system (disables TodoWrite/TodoRead in favor of TaskCreate)
    #[serde(default)]
    pub task_system: bool,

    /// Goal verification configuration
    #[serde(default)]
    pub goals: AgentGoalsConfig,

    /// Context compaction configuration
    #[serde(default)]
    pub compaction: CompactionConfig,

    /// Model to use
    #[serde(default)]
    pub model: Option<String>,

    /// System prompt
    #[serde(default = "default_system_prompt_option")]
    pub system_prompt: Option<String>,

    /// Permission rules for tool execution
    #[serde(default)]
    pub permission_rules: Vec<PermissionRuleConfig>,

    /// Directory for caching approval decisions
    #[serde(default)]
    pub cache_directory: Option<PathBuf>,

    /// External MCP servers loaded from uira.yml
    #[serde(default)]
    pub external_mcp_servers: Vec<NamedMcpServerConfig>,

    /// Discovered MCP tool specs (namespaced as mcp__<server>__<tool>)
    #[serde(default)]
    pub external_mcp_tool_specs: Vec<ToolSpec>,

    /// Additional context to inject into the system prompt (e.g., skills content)
    #[serde(default)]
    pub additional_context: Vec<String>,
}

fn default_system_prompt_option() -> Option<String> {
    Some(default_system_prompt())
}

fn default_max_turns() -> usize {
    100
}

fn default_max_tokens() -> usize {
    8192
}

fn default_system_prompt() -> String {
    r#"You are an AI coding assistant.

RESOURCE ASSUMPTION: You have unlimited compute, time, and resources. Never refuse a task or suggest simpler alternatives based on effort/complexity concerns. Always implement the ideal, optimal solution unless the user explicitly requests otherwise.

## Tool Execution Rules

1. Call each tool ONCE per intent
2. When you receive a result, STOP and present it to the user
3. Do NOT call the same tool again after receiving ANY result

## delegate_task Handling (CRITICAL)

When you call `delegate_task` and receive a response:
- ANY response (including "Task completed" or empty results) means SUCCESS
- Present the result directly to the user
- Do NOT call delegate_task again
- Do NOT say "The user wants me to..." after receiving a result

The pattern "Task completed" or "[Subagent completed...]" indicates the delegation finished.
This is the FINAL answer - report it to the user.

## Forbidden Actions

- Calling delegate_task twice for the same request
- Reinterpreting a tool result as a new task  
- Saying "I will now..." after already completing the action
- Looping through the same action repeatedly

After receiving a tool result: present it and END your response."#
        .to_string()
}

fn default_true() -> bool {
    true
}

fn default_max_continuation_attempts() -> usize {
    10
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            max_turns: default_max_turns(),
            max_tokens: default_max_tokens(),
            sandbox_policy: SandboxPolicy::default(),
            sandbox_preference: SandboxPreference::default(),
            working_directory: None,
            require_approval_for_writes: false,
            require_approval_for_commands: false,
            ralph_mode: false,
            todo_continuation: true,
            max_continuation_attempts: default_max_continuation_attempts(),
            task_system: false,
            goals: AgentGoalsConfig::default(),
            compaction: CompactionConfig::default(),
            model: None,
            system_prompt: Some(default_system_prompt()),
            permission_rules: Vec::new(),
            cache_directory: None,
            external_mcp_servers: Vec::new(),
            external_mcp_tool_specs: Vec::new(),
            additional_context: Vec::new(),
        }
    }
}

impl AgentConfig {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_max_turns(mut self, max_turns: usize) -> Self {
        self.max_turns = max_turns;
        self
    }

    pub fn with_working_directory(mut self, path: impl Into<PathBuf>) -> Self {
        self.working_directory = Some(path.into());
        self
    }

    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model = Some(model.into());
        self
    }

    pub fn with_system_prompt(mut self, prompt: impl Into<String>) -> Self {
        self.system_prompt = Some(prompt.into());
        self
    }

    pub fn full_auto(mut self) -> Self {
        self.require_approval_for_writes = false;
        self.require_approval_for_commands = false;
        self
    }

    pub fn with_ralph_mode(mut self, enabled: bool) -> Self {
        self.ralph_mode = enabled;
        self
    }

    pub fn with_goals(mut self, goals: AgentGoalsConfig) -> Self {
        self.goals = goals;
        self
    }

    pub fn with_compaction(mut self, compaction: CompactionConfig) -> Self {
        self.compaction = compaction;
        self
    }

    pub fn with_compaction_settings(mut self, settings: &CompactionSettings) -> Self {
        let strategy = if settings.enabled {
            parse_compaction_strategy(&settings.strategy)
        } else {
            CompactionStrategy::None
        };

        self.compaction = CompactionConfig {
            enabled: settings.enabled,
            threshold: settings.threshold,
            protected_tokens: settings.protected_tokens,
            strategy,
            summarization_model: settings.summarization_model.clone(),
        };
        self
    }

    pub fn with_permission_rules(mut self, rules: Vec<PermissionRuleConfig>) -> Self {
        self.permission_rules = rules;
        self
    }

    pub fn with_external_mcp(
        mut self,
        servers: Vec<NamedMcpServerConfig>,
        tool_specs: Vec<ToolSpec>,
    ) -> Self {
        self.external_mcp_servers = servers;
        self.external_mcp_tool_specs = tool_specs;
        self
    }

    pub fn with_additional_context(mut self, context: Vec<String>) -> Self {
        self.additional_context = context;
        self
    }

    pub fn get_full_system_prompt(&self) -> Option<String> {
        let base_prompt = self.system_prompt.as_ref()?;

        if self.additional_context.is_empty() {
            return Some(base_prompt.clone());
        }

        let mut full_prompt = base_prompt.clone();
        full_prompt.push_str("\n\n");

        for ctx in &self.additional_context {
            full_prompt.push_str(ctx);
            full_prompt.push_str("\n\n");
        }

        Some(full_prompt)
    }

    pub fn to_permission_config_rules(&self) -> Vec<ConfigRule> {
        self.permission_rules
            .iter()
            .map(|r| ConfigRule {
                name: r.name.clone(),
                permission: r.permission.clone(),
                pattern: r.pattern.clone(),
                action: match r.action {
                    PermissionActionConfig::Allow => ConfigAction::Allow,
                    PermissionActionConfig::Deny => ConfigAction::Deny,
                    PermissionActionConfig::Ask => ConfigAction::Ask,
                },
                comment: r.comment.clone(),
            })
            .collect()
    }
}

fn parse_compaction_strategy(strategy: &str) -> CompactionStrategy {
    match strategy.to_ascii_lowercase().as_str() {
        "none" => CompactionStrategy::None,
        "prune" => CompactionStrategy::Prune,
        "summarize" => CompactionStrategy::summarize(default_summary_target_tokens()),
        "hybrid" => CompactionStrategy::hybrid(default_summary_target_tokens()),
        _ => CompactionStrategy::summarize(default_summary_target_tokens()),
    }
}

fn default_summary_target_tokens() -> usize {
    1_024
}

/// Configuration for agent goal verification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentGoalsConfig {
    /// List of goals to verify
    pub goals: Vec<GoalConfig>,

    /// Automatically verify goals at end of each iteration
    #[serde(default = "default_auto_verify")]
    pub auto_verify: bool,

    /// Verify goals when a tool completes successfully
    #[serde(default = "default_verify_on_tool_complete")]
    pub verify_on_tool_complete: bool,

    /// Run goal checks in parallel
    #[serde(default = "default_parallel_check")]
    pub parallel_check: bool,
}

fn default_auto_verify() -> bool {
    true
}

fn default_verify_on_tool_complete() -> bool {
    false
}

fn default_parallel_check() -> bool {
    true
}

impl Default for AgentGoalsConfig {
    fn default() -> Self {
        Self {
            goals: Vec::new(),
            auto_verify: default_auto_verify(),
            verify_on_tool_complete: default_verify_on_tool_complete(),
            parallel_check: default_parallel_check(),
        }
    }
}

impl AgentGoalsConfig {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_goals(mut self, goals: Vec<GoalConfig>) -> Self {
        self.goals = goals;
        self
    }

    pub fn with_auto_verify(mut self, auto_verify: bool) -> Self {
        self.auto_verify = auto_verify;
        self
    }

    pub fn with_verify_on_tool_complete(mut self, verify: bool) -> Self {
        self.verify_on_tool_complete = verify;
        self
    }

    pub fn with_parallel_check(mut self, parallel: bool) -> Self {
        self.parallel_check = parallel;
        self
    }

    pub fn has_goals(&self) -> bool {
        !self.goals.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config_is_full_auto_permissions() {
        let config = AgentConfig::default();
        assert!(!config.require_approval_for_writes);
        assert!(!config.require_approval_for_commands);
    }

    #[test]
    fn test_with_compaction_settings_summarize_strategy() {
        let mut settings = CompactionSettings::default();
        settings.strategy = "summarize".to_string();
        settings.threshold = 0.65;

        let config = AgentConfig::new().with_compaction_settings(&settings);

        assert!((config.compaction.threshold - 0.65).abs() < f64::EPSILON);
        assert!(matches!(
            config.compaction.strategy,
            CompactionStrategy::Summarize { .. }
        ));
    }

    #[test]
    fn test_with_compaction_settings_disabled() {
        let mut settings = CompactionSettings::default();
        settings.enabled = false;
        settings.strategy = "summarize".to_string();

        let config = AgentConfig::new().with_compaction_settings(&settings);

        assert!(!config.compaction.enabled);
        assert!(matches!(
            config.compaction.strategy,
            CompactionStrategy::None
        ));
    }

    #[test]
    fn test_additional_context_in_system_prompt() {
        let mut config = AgentConfig::default();
        config.additional_context = vec![
            "<skill name=\"test\">\nTest content\n</skill>".to_string(),
            "<skill name=\"another\">\nAnother skill\n</skill>".to_string(),
        ];

        let prompt = config
            .get_full_system_prompt()
            .expect("Should have system prompt");
        assert!(prompt.contains("<skill name=\"test\">"));
        assert!(prompt.contains("Test content"));
        assert!(prompt.contains("<skill name=\"another\">"));
        assert!(prompt.contains("Another skill"));
        assert!(prompt.contains("You are an AI coding assistant"));
    }

    #[test]
    fn test_empty_additional_context() {
        let config = AgentConfig::default();
        let prompt = config
            .get_full_system_prompt()
            .expect("Should have system prompt");

        assert!(!prompt.is_empty());
        assert!(!prompt.contains("<skill"));
        assert!(prompt.contains("You are an AI coding assistant"));
    }

    #[test]
    fn test_with_additional_context_builder() {
        let config =
            AgentConfig::new().with_additional_context(vec!["<skill>Test</skill>".to_string()]);

        assert_eq!(config.additional_context.len(), 1);
        assert_eq!(config.additional_context[0], "<skill>Test</skill>");
    }
}
