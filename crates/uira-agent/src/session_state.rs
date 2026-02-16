//! Session state management

use crate::context::ContextManager;
use std::path::PathBuf;
use std::sync::Arc;
use uira_orchestration::{
    register_builtins_with_todos, AgentExecutor, ApprovalCache, AstToolProvider,
    DelegationToolProvider, LspToolProvider, McpToolProvider, TodoStore, ToolCallRuntime,
    ToolContext, ToolOrchestrator, ToolRouter,
};
use uira_permissions::build_evaluator_from_rules;
use uira_providers::ModelClient;
use uira_sandbox::SandboxManager;
use uira_core::{MessageId, SessionId, TokenUsage};

use crate::AgentConfig;

/// Session holds all session-wide state
pub struct Session {
    /// Unique session identifier
    pub id: SessionId,

    /// Parent session ID (for forked sessions)
    pub parent_id: Option<SessionId>,

    /// Message ID where the fork occurred
    pub forked_from_message: Option<MessageId>,

    /// Number of child forks from this session
    pub fork_count: u32,

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

    pub todo_store: TodoStore,

    pub cwd: PathBuf,

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

        let todo_persist_dir = dirs::home_dir().map(|h| h.join(".uira").join("todos"));
        let todo_store = match todo_persist_dir {
            Some(dir) => TodoStore::new().with_persistence(dir),
            None => TodoStore::new(),
        };

        let mut tool_router = ToolRouter::new();
        if config.task_system {
            tracing::warn!(
                "task_system enabled but no replacement task tool is registered yet; keeping TodoWrite/TodoRead enabled"
            );
        }
        register_builtins_with_todos(&mut tool_router, todo_store.clone());
        tool_router.register_provider(Arc::new(LspToolProvider::new()));
        tool_router.register_provider(Arc::new(AstToolProvider::new()));

        if !config.external_mcp_servers.is_empty() {
            match McpToolProvider::new(
                config.external_mcp_servers.clone(),
                config.external_mcp_tool_specs.clone(),
                cwd.clone(),
            ) {
                Ok(provider) => tool_router.register_provider(Arc::new(provider)),
                Err(e) => tracing::warn!(error = %e, "failed to initialize MCP tool provider"),
            }
        }

        let delegation_provider = match executor {
            Some(exec) => DelegationToolProvider::with_executor(exec),
            None => DelegationToolProvider::new(),
        };
        tool_router.register_provider(Arc::new(delegation_provider));

        let tool_router = Arc::new(tool_router);
        let full_auto = Self::is_full_auto(&config);
        let mut orchestrator =
            ToolOrchestrator::new(tool_router.clone(), config.sandbox_policy.clone())
                .with_full_auto(full_auto);

        if !config.permission_rules.is_empty() {
            let config_rules = config.to_permission_config_rules();
            match build_evaluator_from_rules(config_rules) {
                Ok(evaluator) => {
                    orchestrator = orchestrator.with_permission_evaluator(evaluator);
                    tracing::debug!(
                        rule_count = config.permission_rules.len(),
                        "permission_evaluator_wired"
                    );
                }
                Err(e) => {
                    tracing::warn!(error = %e, "failed to build permission evaluator, using defaults");
                }
            }
        }

        let session_id = SessionId::new();
        let mut approval_cache = ApprovalCache::new(session_id.to_string());
        if let Some(ref cache_dir) = config.cache_directory {
            approval_cache = approval_cache.with_persistence(cache_dir.clone());
        }
        orchestrator = orchestrator.with_approval_cache(approval_cache);
        tracing::debug!("approval_cache_wired");

        let mut context = ContextManager::new(client.max_tokens())
            .with_compaction_config(config.compaction.clone());

        if let Some(system_prompt) = config.get_full_system_prompt() {
            if let Err(e) = context.add_message(uira_core::Message::system(&system_prompt)) {
                tracing::warn!("Failed to add system prompt: {}", e);
            }
        }

        let parallel_runtime = ToolCallRuntime::new(tool_router.clone());

        Self {
            id: session_id,
            parent_id: None,
            forked_from_message: None,
            fork_count: 0,
            context,
            sandbox: SandboxManager::new(config.sandbox_policy.clone()),
            todo_store,
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

    pub fn tool_context(&self) -> ToolContext {
        let sandbox_type = if self.config.sandbox_policy.is_restrictive() {
            uira_sandbox::SandboxType::Native
        } else {
            uira_sandbox::SandboxType::None
        };

        ToolContext {
            cwd: self.cwd.clone(),
            session_id: self.id.to_string(),
            full_auto: Self::is_full_auto(&self.config),
            env: std::collections::HashMap::new(),
            sandbox_type,
            sandbox_policy: self.config.sandbox_policy.clone(),
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

        if self.context.needs_compaction() {
            if let Some(result) = self.context.compact() {
                tracing::info!(
                    tokens_before = result.tokens_before,
                    tokens_after = result.tokens_after,
                    messages_removed = result.messages_removed,
                    "context_compacted"
                );
            }
        }
    }

    /// Check if max turns exceeded
    pub fn is_max_turns_exceeded(&self) -> bool {
        self.turn >= self.config.max_turns
    }

    /// Switch to a new model client
    pub fn set_client(&mut self, client: Arc<dyn ModelClient>) {
        self.context = ContextManager::new(client.max_tokens())
            .with_compaction_config(self.config.compaction.clone());
        self.client = client;
    }

    /// Get tool specifications for the model API
    pub fn tool_specs(&self) -> Vec<uira_core::ToolSpec> {
        self.tool_router.specs()
    }

    /// Fork this session at the current point
    ///
    /// Creates a new session with copied context. The new session inherits
    /// all messages and configuration from this session.
    pub fn fork(&mut self) -> Self {
        self.fork_count += 1;

        let mut forked = Self::new_with_executor(self.config.clone(), self.client.clone(), None);

        forked.parent_id = Some(self.id.clone());
        forked.forked_from_message = None;

        for msg in self.context.messages().to_vec() {
            let _ = forked.context.add_message(msg);
        }

        forked
    }

    /// Fork this session, keeping only messages up to a certain count
    pub fn fork_at_message(&mut self, message_count: usize) -> Self {
        self.fork_count += 1;

        let mut forked = Self::new_with_executor(self.config.clone(), self.client.clone(), None);

        forked.parent_id = Some(self.id.clone());
        forked.forked_from_message = Some(MessageId::new());

        let messages: Vec<_> = self
            .context
            .messages()
            .iter()
            .take(message_count)
            .cloned()
            .collect();
        for msg in messages {
            let _ = forked.context.add_message(msg);
        }

        forked
    }

    pub fn is_fork(&self) -> bool {
        self.parent_id.is_some()
    }

    pub fn generate_fork_title(&self, base_title: &str) -> String {
        format!("{} (fork #{})", base_title, self.fork_count.max(1))
    }

    pub async fn save_approval_cache(&self) {
        if let Some(cache) = self.orchestrator.approval_cache() {
            let cache = cache.read().await;
            if let Err(e) = cache.save() {
                tracing::warn!(error = %e, "failed to save approval cache");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_id_different() {
        let id1 = SessionId::new();
        let id2 = SessionId::new();
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_message_id_different() {
        let id1 = MessageId::new();
        let id2 = MessageId::new();
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_message_id_prefix() {
        let id = MessageId::new();
        assert!(id.0.starts_with("msg_"));
    }
}
