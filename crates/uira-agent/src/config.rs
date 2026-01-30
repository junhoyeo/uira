//! Agent configuration

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use uira_config::schema::GoalConfig;
use uira_protocol::SandboxPreference;
use uira_sandbox::SandboxPolicy;

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

    /// Goal verification configuration
    #[serde(default)]
    pub goals: AgentGoalsConfig,

    /// Model to use
    #[serde(default)]
    pub model: Option<String>,

    /// System prompt
    #[serde(default)]
    pub system_prompt: Option<String>,
}

fn default_max_turns() -> usize {
    100
}

fn default_max_tokens() -> usize {
    8192
}

fn default_system_prompt() -> String {
    r#"You are an AI coding assistant.

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

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            max_turns: default_max_turns(),
            max_tokens: default_max_tokens(),
            sandbox_policy: SandboxPolicy::default(),
            sandbox_preference: SandboxPreference::default(),
            working_directory: None,
            require_approval_for_writes: true,
            require_approval_for_commands: true,
            ralph_mode: false,
            goals: AgentGoalsConfig::default(),
            model: None,
            system_prompt: Some(default_system_prompt()),
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
