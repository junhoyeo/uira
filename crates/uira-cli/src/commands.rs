//! CLI commands

use clap::{Parser, Subcommand};

/// Uira - Native AI Coding Agent
#[derive(Parser, Debug)]
#[command(name = "uira")]
#[command(author, version, about, long_about = None)]
pub struct Cli {
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

impl Cli {
    pub fn get_prompt(&self) -> Option<String> {
        if self.prompt.is_empty() {
            None
        } else {
            Some(self.prompt.join(" "))
        }
    }
}
