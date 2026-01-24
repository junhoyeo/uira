use crate::types::ToolDefinition;
use serde_json::json;

pub fn tool_definition() -> ToolDefinition {
    ToolDefinition::stub(
        "background_task",
        "Manage background tasks (launch/output/cancel) (foundation only).",
        json!({
            "type": "object",
            "properties": {
                "action": {"type": "string", "enum": ["launch", "output", "cancel"]},
                "taskId": {"type": "string"},
                "block": {"type": "boolean"},
                "timeout": {"type": "integer", "minimum": 0}
            },
            "required": ["action"]
        }),
    )
}
