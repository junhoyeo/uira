//! Built-in tools for the Uira agent
//!
//! These are the core tools that the agent uses to interact with the filesystem
//! and execute commands.

mod bash;
mod edit;
mod glob;
mod grep;
pub mod memory;
mod read;
pub mod todo;
mod web_search;
mod write;

pub use bash::BashTool;
pub use edit::EditTool;
pub use glob::GlobTool;
pub use grep::GrepTool;
pub use memory::{MemoryForgetTool, MemoryProfileTool, MemorySearchTool, MemoryStoreTool};
pub use read::ReadTool;
pub use todo::{TodoReadTool, TodoSessionInfo, TodoStore, TodoWriteTool};
pub use web_search::{CodeSearchTool, FetchUrlTool, GrepAppTool, WebSearchTool};
pub use write::WriteTool;

use crate::tools::{BoxedTool, ToolRouter};
use std::sync::Arc;

pub fn register_builtins(router: &mut ToolRouter) {
    router.register(BashTool::new());
    router.register(ReadTool::new());
    router.register(WriteTool::new());
    router.register(EditTool::new());
    router.register(GlobTool::new());
    router.register(GrepTool::new());
    router.register(WebSearchTool::new());
    router.register(FetchUrlTool::new());
    router.register(CodeSearchTool::new());
    router.register(GrepAppTool::new());
}

pub fn register_builtins_with_todos(router: &mut ToolRouter, store: TodoStore) {
    register_builtins(router);
    router.register(TodoWriteTool::new(store.clone()));
    router.register(TodoReadTool::new(store));
}

/// Register builtins without todo tools (when task_system is enabled).
/// Ported from oh-my-opencode's tasks-todowrite-disabler hook which blocks
/// TodoWrite/TodoRead when the experimental task system is active.
pub fn register_builtins_without_todos(router: &mut ToolRouter) {
    register_builtins(router);
}

pub fn create_builtin_router() -> ToolRouter {
    let mut router = ToolRouter::new();
    register_builtins(&mut router);
    router
}

pub fn builtin_tools() -> Vec<BoxedTool> {
    vec![
        Arc::new(BashTool::new()),
        Arc::new(ReadTool::new()),
        Arc::new(WriteTool::new()),
        Arc::new(EditTool::new()),
        Arc::new(GlobTool::new()),
        Arc::new(GrepTool::new()),
        Arc::new(WebSearchTool::new()),
        Arc::new(FetchUrlTool::new()),
        Arc::new(CodeSearchTool::new()),
        Arc::new(GrepAppTool::new()),
    ]
}
