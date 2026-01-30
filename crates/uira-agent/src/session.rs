//! Session state management

use std::path::PathBuf;
use std::sync::Arc;
use uira_context::ContextManager;
use uira_protocol::{SessionId, TokenUsage};
use uira_providers::ModelClient;
use uira_sandbox::SandboxManager;
use uira_tools::{
    create_builtin_router, AgentExecutor, AstToolProvider, DelegationToolProvider, LspToolProvider,
    ToolCallRuntime, ToolContext, ToolOrchestrator, ToolRouter,
};

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

    /// Parallel tool execution runtime
    pub parallel_runtime: ToolCallRuntime,

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
        Self::new_with_executor(config, client, None)
    }

    pub fn new_with_executor(
        config: AgentConfig,
        client: Arc<dyn ModelClient>,
        executor: Option<Arc<dyn AgentExecutor>>,
    ) -> Self {
        let cwd = config
            .working_directory
            .clone()
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());

        let mut tool_router = create_builtin_router();
        tool_router.register_provider(Arc::new(LspToolProvider::new()));
        tool_router.register_provider(Arc::new(AstToolProvider::new()));

        let delegation_provider = match executor {
            Some(exec) => DelegationToolProvider::with_executor(exec),
            None => DelegationToolProvider::new(),
        };
        tool_router.register_provider(Arc::new(delegation_provider));

        let tool_router = Arc::new(tool_router);
        let full_auto = Self::is_full_auto(&config);
        let orchestrator =
            ToolOrchestrator::new(tool_router.clone(), config.sandbox_policy.clone())
                .with_full_auto(full_auto);

        let mut context = ContextManager::new(client.max_tokens());

        if let Some(ref system_prompt) = config.system_prompt {
            if let Err(e) = context.add_message(uira_protocol::Message::system(system_prompt)) {
                tracing::warn!("Failed to add system prompt: {}", e);
            }
        }

        let parallel_runtime = ToolCallRuntime::new(tool_router.clone());

        Self {
            id: SessionId::new(),
            context,
            sandbox: SandboxManager::new(config.sandbox_policy.clone()),
            tool_router,
            orchestrator,
            parallel_runtime,
            config,
            client,
            cwd,
            turn: 0,
            usage: TokenUsage::default(),
        }
    }

    fn is_full_auto(config: &AgentConfig) -> bool {
        !config.require_approval_for_writes && !config.require_approval_for_commands
    }

    /// Create a tool context for execution
    pub fn tool_context(&self) -> ToolContext {
        ToolContext {
            cwd: self.cwd.clone(),
            session_id: self.id.to_string(),
            full_auto: Self::is_full_auto(&self.config),
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

    /// Switch to a new model client
    pub fn set_client(&mut self, client: Arc<dyn ModelClient>) {
        self.context = ContextManager::new(client.max_tokens());
        self.client = client;
    }

    /// Get tool specifications for the model API
    pub fn tool_specs(&self) -> Vec<uira_protocol::ToolSpec> {
        self.tool_router.specs()
    }
}
