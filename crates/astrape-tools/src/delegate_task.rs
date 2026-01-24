use crate::types::ToolDefinition;
use serde_json::json;

pub fn tool_definition() -> ToolDefinition {
    ToolDefinition::stub(
        "delegate_task",
        "Delegate a task to another agent category (foundation only).",
        json!({
            "type": "object",
            "properties": {
                "agent": {"type": "string"},
                "prompt": {"type": "string"},
                "runInBackground": {"type": "boolean", "default": false}
            },
            "required": ["agent", "prompt"]
        }),
    )
}
