pub mod approval_cache;
pub mod builtins;
pub mod comment_hook;
pub mod lsp;
pub mod orchestrator;
pub mod parallel;
pub mod provider;
pub mod providers;
pub mod registry;
pub mod router;
pub mod traits;
pub mod types;

pub use approval_cache::{
    ApprovalCache, ApprovalCacheFile, ApprovalKey, CacheDecision, CachedApproval,
};
pub use builtins::{
    builtin_tools, create_builtin_router, register_builtins, register_builtins_with_todos,
    register_builtins_without_todos, BashTool, EditTool, FetchUrlTool, GlobTool, GrepTool,
    ReadTool, TodoReadTool, TodoSessionInfo, TodoStore, TodoWriteTool, WebSearchTool, WriteTool,
};
pub use comment_hook::CommentChecker;
pub use lsp::{LspClient, LspClientImpl, LspServerConfig};
pub use orchestrator::{PendingApproval, RunOptions, ToolOrchestrator};
pub use parallel::ToolCallRuntime;
pub use provider::ToolProvider;
pub use providers::{AgentExecutor, AstToolProvider, DelegationToolProvider, LspToolProvider};
pub use registry::ToolRegistry;
pub use router::ToolRouter;
pub use traits::{BoxedTool, FunctionTool, Tool, ToolContext, ToolFuture, ToolHandler};
pub use types::{ToolContent, ToolDefinition, ToolError, ToolInput, ToolOutput};
