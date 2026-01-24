use crate::types::ToolDefinition;
use serde_json::json;

pub fn tool_definition() -> ToolDefinition {
    ToolDefinition::stub(
        "skill",
        "Load a skill and return its instructions (foundation only).",
        json!({
            "type": "object",
            "properties": {"name": {"type": "string"}},
            "required": ["name"]
        }),
    )
}
