pub mod ast_grep;
pub mod background_task;
pub mod delegate_task;
pub mod lsp;
pub mod registry;
pub mod session_manager;
pub mod skill;
pub mod types;

pub use registry::ToolRegistry;
pub use types::{ToolContent, ToolDefinition, ToolError, ToolInput, ToolOutput};

pub fn builtin_tools() -> Vec<ToolDefinition> {
    let mut tools = Vec::new();
    tools.extend(lsp::tool_definitions());
    tools.extend(ast_grep::tool_definitions());
    tools.push(delegate_task::tool_definition());
    tools.push(background_task::tool_definition());
    tools.push(session_manager::tool_definition());
    tools.push(skill::tool_definition());
    tools
}

pub fn builtin_registry() -> Result<ToolRegistry, ToolError> {
    let mut registry = ToolRegistry::new();
    registry.register_many(builtin_tools())?;
    Ok(registry)
}
