pub mod client;
pub mod servers;
pub mod utils;

use crate::types::ToolDefinition;
use serde_json::json;

pub fn tool_definitions() -> Vec<ToolDefinition> {
    vec![
        lsp_goto_definition(),
        lsp_find_references(),
        lsp_symbols(),
        lsp_diagnostics(),
        lsp_prepare_rename(),
        lsp_rename(),
    ]
}

fn lsp_goto_definition() -> ToolDefinition {
    ToolDefinition::stub(
        "lsp_goto_definition",
        "Find the definition location of a symbol.",
        json!({
            "type": "object",
            "properties": {
                "filePath": {"type": "string", "description": "Path to the source file"},
                "line": {"type": "integer", "minimum": 1},
                "character": {"type": "integer", "minimum": 0}
            },
            "required": ["filePath", "line", "character"]
        }),
    )
}

fn lsp_find_references() -> ToolDefinition {
    ToolDefinition::stub(
        "lsp_find_references",
        "Find references to a symbol across the workspace.",
        json!({
            "type": "object",
            "properties": {
                "filePath": {"type": "string"},
                "line": {"type": "integer", "minimum": 1},
                "character": {"type": "integer", "minimum": 0},
                "includeDeclaration": {"type": "boolean"}
            },
            "required": ["filePath", "line", "character"]
        }),
    )
}

fn lsp_symbols() -> ToolDefinition {
    ToolDefinition::stub(
        "lsp_symbols",
        "Get symbols from a file or search across the workspace.",
        json!({
            "type": "object",
            "properties": {
                "filePath": {"type": "string", "description": "A file in the workspace"},
                "scope": {"type": "string", "enum": ["document", "workspace"], "default": "document"},
                "query": {"type": "string"},
                "limit": {"type": "integer", "minimum": 1}
            },
            "required": ["filePath"]
        }),
    )
}

fn lsp_diagnostics() -> ToolDefinition {
    ToolDefinition::stub(
        "lsp_diagnostics",
        "Get language server diagnostics for a file.",
        json!({
            "type": "object",
            "properties": {
                "filePath": {"type": "string"},
                "severity": {"type": "string", "enum": ["error", "warning", "information", "hint", "all"], "default": "all"}
            },
            "required": ["filePath"]
        }),
    )
}

fn lsp_prepare_rename() -> ToolDefinition {
    ToolDefinition::stub(
        "lsp_prepare_rename",
        "Check whether rename is valid for a symbol.",
        json!({
            "type": "object",
            "properties": {
                "filePath": {"type": "string"},
                "line": {"type": "integer", "minimum": 1},
                "character": {"type": "integer", "minimum": 0}
            },
            "required": ["filePath", "line", "character"]
        }),
    )
}

fn lsp_rename() -> ToolDefinition {
    ToolDefinition::stub(
        "lsp_rename",
        "Rename a symbol across the workspace.",
        json!({
            "type": "object",
            "properties": {
                "filePath": {"type": "string"},
                "line": {"type": "integer", "minimum": 1},
                "character": {"type": "integer", "minimum": 0},
                "newName": {"type": "string", "minLength": 1}
            },
            "required": ["filePath", "line", "character", "newName"]
        }),
    )
}
