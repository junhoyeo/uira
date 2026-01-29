//! Agent configuration

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
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
            model: None,
            system_prompt: None,
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
}
