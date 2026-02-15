//! CLI commands

use clap::{Parser, Subcommand, ValueEnum};

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub enum CliMode {
    Interactive,
    Rpc,
}

/// Uira - Native AI Coding Agent
#[derive(Parser, Debug)]
#[command(name = "uira-agent")]
#[command(author, version, about, long_about = None)]
pub struct Cli {
    /// Runtime mode (interactive, rpc)
    #[arg(long, value_enum, default_value_t = CliMode::Interactive)]
    pub mode: CliMode,

    /// Prompt to execute (interactive mode if omitted)
    #[arg(trailing_var_arg = true)]
    pub prompt: Vec<String>,

    /// Model to use (e.g., claude-sonnet-4-20250514, gpt-4o)
    #[arg(short, long)]
    pub model: Option<String>,

    /// Provider to use (anthropic, openai, ollama, opencode)
    #[arg(short, long)]
    pub provider: Option<String>,

    /// Sandbox policy (read-only, workspace-write, full-access)
    #[arg(long, default_value = "workspace-write")]
    pub sandbox: String,

    /// Run in full-auto mode (no approval prompts)
    #[arg(long)]
    pub full_auto: bool,

    /// Verbose output - show streaming events (tool calls, thinking, etc.)
    #[arg(short, long)]
    pub verbose: bool,

    /// Enable ralph mode for persistent task completion
    #[arg(long)]
    pub ralph: bool,

    /// Agent to use (e.g., architect, executor, explore, designer)
    /// Each agent has specialized prompts and a default model tier
    #[arg(short, long)]
    pub agent: Option<String>,

    /// Output format (text, json, jsonl)
    #[arg(long, default_value = "text")]
    pub output: String,

    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Execute prompt non-interactively
    Exec {
        /// The prompt to execute
        prompt: String,

        /// Output as JSON
        #[arg(long)]
        json: bool,
    },

    /// Resume a previous session
    Resume {
        /// Session ID to resume
        session_id: Option<String>,

        /// Fork from a parent session instead of resuming directly
        #[arg(long)]
        fork: bool,

        /// Number of messages to keep when forking (default: all)
        #[arg(long)]
        fork_at: Option<usize>,
    },

    /// List and manage sessions
    Sessions {
        #[command(subcommand)]
        command: SessionsCommands,
    },

    /// Authentication commands
    Auth {
        #[command(subcommand)]
        command: AuthCommands,
    },

    /// Configuration management
    Config {
        #[command(subcommand)]
        command: ConfigCommands,
    },

    /// Goal verification commands
    Goals {
        #[command(subcommand)]
        command: GoalsCommands,
    },

    /// Background task management
    Tasks {
        #[command(subcommand)]
        command: TasksCommands,
    },

    /// Generate shell completion scripts
    Completion {
        /// Target shell to generate completions for
        #[arg(value_enum)]
        shell: clap_complete::Shell,
    },

    /// Gateway server management
    Gateway {
        #[command(subcommand)]
        command: GatewayCommands,
    },

    /// Manage skills
    Skills {
        #[command(subcommand)]
        command: SkillsCommands,
    },
}

#[derive(Subcommand, Debug)]
pub enum AuthCommands {
    /// Login to a provider
    Login {
        /// Provider to login to (omit to see available providers)
        provider: Option<String>,
    },
    /// Logout from a provider
    Logout {
        /// Provider to logout from
        provider: String,
    },
    /// Show current authentication status
    Status,
}

#[derive(Subcommand, Debug)]
pub enum ConfigCommands {
    /// Show current configuration
    Show,
    /// Set a configuration value
    Set {
        /// Configuration key
        key: String,
        /// Configuration value
        value: String,
    },
    /// Get a configuration value
    Get {
        /// Configuration key
        key: String,
    },
    /// Reset configuration to defaults
    Reset,
}

#[derive(Subcommand, Debug)]
pub enum GoalsCommands {
    /// Run goal verification
    Check,
    /// List configured goals
    List,
    /// Show goal verification status
    Status,
}

#[derive(Subcommand, Debug)]
pub enum TasksCommands {
    /// List all background tasks
    List,
    /// Get task status
    Status {
        /// Task ID to check
        task_id: String,
    },
    /// Cancel a task
    Cancel {
        /// Task ID to cancel
        task_id: String,
    },
}

#[derive(Subcommand, Debug)]
pub enum SessionsCommands {
    /// List all sessions (with fork relationships)
    List {
        /// Show only recent N sessions
        #[arg(short, long, default_value = "20")]
        limit: usize,

        /// Show fork tree structure
        #[arg(long)]
        tree: bool,
    },
    /// Show session details
    Info {
        /// Session ID to inspect
        session_id: String,
    },
    /// Delete a session
    Delete {
        /// Session ID to delete
        session_id: String,
    },
}

#[derive(Subcommand, Debug)]
pub enum GatewayCommands {
    /// Start the WebSocket gateway server
    Start {
        /// Host to bind to (default: from config or 127.0.0.1)
        #[arg(long)]
        host: Option<String>,
        /// Port to bind to (default: from config or 18789)
        #[arg(long)]
        port: Option<u16>,
        /// Authentication token for WebSocket connections
        #[arg(long)]
        auth_token: Option<String>,
    },
}

#[derive(Subcommand, Debug)]
pub enum SkillsCommands {
    /// List all discovered skills
    List,
    /// Show details of a specific skill
    Show {
        /// Name of the skill to show
        name: String,
    },
    /// Install a skill from a local path
    Install {
        /// Path to the skill directory (must contain SKILL.md)
        path: String,
    },
}

impl Cli {
    pub fn get_prompt(&self) -> Option<String> {
        if self.prompt.is_empty() {
            None
        } else {
            Some(self.prompt.join(" "))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[test]
    fn defaults_to_interactive_mode() {
        let cli = Cli::parse_from(["uira-agent"]);
        assert_eq!(cli.mode, CliMode::Interactive);
    }

    #[test]
    fn parses_rpc_mode_flag() {
        let cli = Cli::parse_from(["uira-agent", "--mode", "rpc"]);
        assert_eq!(cli.mode, CliMode::Rpc);
    }
}
