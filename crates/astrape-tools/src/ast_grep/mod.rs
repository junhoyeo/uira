pub mod tools;

use crate::types::ToolDefinition;

pub fn tool_definitions() -> Vec<ToolDefinition> {
    tools::tool_definitions()
}
