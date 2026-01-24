pub mod background_task;
pub mod delegate_task;
pub mod lsp;
pub mod registry;
pub mod skill;
pub mod types;

pub use lsp::{LspClient, LspClientImpl, LspServerConfig};
pub use registry::ToolRegistry;
pub use types::{ToolContent, ToolDefinition, ToolError, ToolInput, ToolOutput};

pub fn orchestration_tools() -> Vec<ToolDefinition> {
    vec![
        delegate_task::tool_definition(),
        background_task::tool_definition(),
        skill::tool_definition(),
    ]
}

pub fn orchestration_registry() -> Result<ToolRegistry, ToolError> {
    let mut registry = ToolRegistry::new();
    registry.register_many(orchestration_tools())?;
    Ok(registry)
}
