//! Session state management

use std::path::PathBuf;
use std::sync::Arc;
use uira_context::ContextManager;
use uira_protocol::{SessionId, TokenUsage};
use uira_providers::ModelClient;
use uira_sandbox::SandboxManager;
use uira_tools::{create_builtin_router, ToolContext, ToolOrchestrator, ToolRouter};

use crate::AgentConfig;

/// Session holds all session-wide state
pub struct Session {
    /// Unique session identifier
    pub id: SessionId,

    /// Agent configuration
    pub config: AgentConfig,

    /// Context manager for conversation history
    pub context: ContextManager,

    /// Sandbox manager
    pub sandbox: SandboxManager,

    /// Tool router
    pub tool_router: Arc<ToolRouter>,

    /// Tool orchestrator
    pub orchestrator: ToolOrchestrator,

    /// Model client
    pub client: Arc<dyn ModelClient>,

    /// Working directory
    pub cwd: PathBuf,

    /// Current turn number
    pub turn: usize,

    /// Total token usage
    pub usage: TokenUsage,
}

impl Session {
    pub fn new(config: AgentConfig, client: Arc<dyn ModelClient>) -> Self {
        let cwd = config
            .working_directory
            .clone()
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());

        let tool_router = Arc::new(create_builtin_router());
        let full_auto =
            !config.require_approval_for_writes && !config.require_approval_for_commands;
        let orchestrator =
            ToolOrchestrator::new(tool_router.clone(), config.sandbox_policy.clone())
                .with_full_auto(full_auto);

        Self {
            id: SessionId::new(),
            context: ContextManager::new(client.max_tokens()),
            sandbox: SandboxManager::new(config.sandbox_policy.clone()),
            tool_router,
            orchestrator,
            config,
            client,
            cwd,
            turn: 0,
            usage: TokenUsage::default(),
        }
    }

    /// Create a tool context for execution
    pub fn tool_context(&self) -> ToolContext {
        ToolContext {
            cwd: self.cwd.clone(),
            session_id: self.id.to_string(),
            full_auto: !self.config.require_approval_for_writes
                && !self.config.require_approval_for_commands,
            env: std::collections::HashMap::new(),
        }
    }

    /// Start a new turn
    pub fn start_turn(&mut self) -> usize {
        self.turn += 1;
        self.turn
    }

    /// Record usage for a turn
    pub fn record_usage(&mut self, usage: TokenUsage) {
        self.usage += usage.clone();
        self.context.record_usage(usage);
    }

    /// Check if max turns exceeded
    pub fn is_max_turns_exceeded(&self) -> bool {
        self.turn >= self.config.max_turns
    }
}
