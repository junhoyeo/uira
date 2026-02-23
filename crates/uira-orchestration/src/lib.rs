pub mod agents;
pub mod features;
pub mod hooks;
pub mod sdk;
pub mod tools;

pub use agents::{
    config, definitions, models, planning_pipeline, prompt_loader, prompts, registry, tier_builder,
    tool_restrictions, types,
};
pub use agents::{
    get_agent_definitions, get_agent_definitions_with_config, AgentCategory, AgentConfig,
    AgentCost, AgentFactory, AgentOverrideConfig, AgentOverrides, AgentPromptMetadata,
    AgentRegistry, DelegationTrigger, ModelRegistry, ModelTier, ModelType, OrchestratorPersonality,
    PlanningPipeline, PlanningStage, PromptLoader, PromptSource, RoutingTier, TierBuilder,
    ToolRestrictions, ToolRestrictionsRegistry,
};
pub use features::{background_agent, dynamic_prompt_builder, model_routing, uira_state};
pub use features::{
    build_default_orchestrator_prompt, build_dynamic_orchestrator_prompt,
    build_environment_context, builtin_agent_metadata, register_environment_context,
    AvailableAgent, AvailableDelegationCategory, AvailableSkill,
};
pub use features::{KeywordDetector, KeywordPattern, StateManager};
pub use hooks::{
    create_hook_event_adapter, default_hooks, GoalCheckResult, GoalRunner, Hook, HookEventAdapter, HookRegistry, MemoryCaptureAdapter,
    MemoryRecallAdapter, VerificationResult,
};
pub use sdk::{
    create_uira_session, AgentDefinitionEntry, AgentDefinitions, AgentState, AgentStatus,
    AgentTierOverride, AgentsConfig, BackgroundTask, Context7Config, ExaConfig, FeaturesConfig,
    MagicKeywordsConfig, McpServerConfig, McpServersConfig, PermissionsConfig, PluginConfig,
    QueryOptions, RoutingConfig, SdkError, SdkResult, SessionOptions, SessionState, TaskStatus,
    TierModelsConfig, UiraSession,
};
pub use tools::{
    builtin_tools, create_builtin_router, register_builtins, register_builtins_with_todos,
    register_builtins_without_todos, AgentExecutor, ApprovalCache, ApprovalCacheFile, ApprovalKey,
    AstToolProvider, BashTool, BoxedTool, CacheDecision, CachedApproval, CommentChecker,
    DelegationToolProvider, EditTool, FetchUrlTool, FunctionTool, GlobTool, GrepTool, LspClient,
    LspClientImpl, LspServerConfig, LspToolProvider, McpToolProvider, MemoryForgetTool,
    MemoryProfileTool, MemorySearchTool, MemoryStoreTool, PendingApproval, ReadTool, RunOptions,
    TodoReadTool, TodoSessionInfo, TodoStore, TodoWriteTool, Tool, ToolCallRuntime, ToolContent,
    ToolContext, ToolDefinition, ToolError, ToolFuture, ToolHandler, ToolInput, ToolOrchestrator,
    ToolOutput, ToolProvider, ToolRegistry, ToolRouter, WebSearchTool, WriteTool,
};
