use crate::types::ToolDefinition;
use serde_json::json;

pub fn tool_definition() -> ToolDefinition {
    ToolDefinition::stub(
        "session_manager",
        "Session management tools (list/read/search/info) (foundation only).",
        json!({
            "type": "object",
            "properties": {
                "action": {"type": "string", "enum": ["list", "read", "search", "info"]},
                "sessionId": {"type": "string"},
                "query": {"type": "string"},
                "limit": {"type": "integer", "minimum": 1}
            },
            "required": ["action"]
        }),
    )
}
