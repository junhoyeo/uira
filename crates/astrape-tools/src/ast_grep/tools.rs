use crate::types::ToolDefinition;
use serde_json::json;

pub fn tool_definitions() -> Vec<ToolDefinition> {
    vec![ast_grep_search(), ast_grep_replace()]
}

fn ast_grep_search() -> ToolDefinition {
    ToolDefinition::stub(
        "ast_grep_search",
        "Search for code patterns using AST matching.",
        json!({
            "type": "object",
            "properties": {
                "pattern": {"type": "string"},
                "lang": {"type": "string"},
                "paths": {"type": "array", "items": {"type": "string"}},
                "globs": {"type": "array", "items": {"type": "string"}},
                "context": {"type": "integer", "minimum": 0}
            },
            "required": ["pattern", "lang"]
        }),
    )
}

fn ast_grep_replace() -> ToolDefinition {
    ToolDefinition::stub(
        "ast_grep_replace",
        "Replace code patterns using AST matching.",
        json!({
            "type": "object",
            "properties": {
                "pattern": {"type": "string"},
                "rewrite": {"type": "string"},
                "lang": {"type": "string"},
                "paths": {"type": "array", "items": {"type": "string"}},
                "globs": {"type": "array", "items": {"type": "string"}},
                "dryRun": {"type": "boolean", "default": true}
            },
            "required": ["pattern", "rewrite", "lang"]
        }),
    )
}
