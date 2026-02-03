//! Permission types and enums
//!
//! Defines the core types for the permission system.

use serde::{Deserialize, Serialize};

/// Action to take when a permission rule matches
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    /// Allow the operation without asking
    #[default]
    Allow,
    /// Deny the operation
    Deny,
    /// Ask the user for approval
    Ask,
}

impl Action {
    /// Check if this action allows the operation
    pub fn is_allow(&self) -> bool {
        matches!(self, Action::Allow)
    }

    /// Check if this action denies the operation
    pub fn is_deny(&self) -> bool {
        matches!(self, Action::Deny)
    }

    /// Check if this action requires asking the user
    pub fn is_ask(&self) -> bool {
        matches!(self, Action::Ask)
    }
}

/// Permission categories for tool operations
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Permission {
    /// File read operations
    FileRead,
    /// File write/edit operations
    FileWrite,
    /// File delete operations
    FileDelete,
    /// Shell/bash command execution
    ShellExecute,
    /// Network requests
    NetworkAccess,
    /// MCP tool execution
    McpTool,
    /// Generic tool execution (catch-all)
    Tool(String),
}

impl Permission {
    /// Create a permission from a tool name
    pub fn from_tool_name(name: &str) -> Self {
        match name.to_lowercase().as_str() {
            "read" | "read_file" | "file_read" => Permission::FileRead,
            "write" | "write_file" | "file_write" | "edit" => Permission::FileWrite,
            "delete" | "remove" | "rm" => Permission::FileDelete,
            "bash" | "shell" | "exec" | "execute" => Permission::ShellExecute,
            "fetch" | "http" | "request" | "web_search" => Permission::NetworkAccess,
            name if name.starts_with("mcp_") => Permission::McpTool,
            name => Permission::Tool(name.to_string()),
        }
    }

    /// Convert permission to a string for glob matching
    pub fn as_str(&self) -> &str {
        match self {
            Permission::FileRead => "file:read",
            Permission::FileWrite => "file:write",
            Permission::FileDelete => "file:delete",
            Permission::ShellExecute => "shell:execute",
            Permission::NetworkAccess => "network:access",
            Permission::McpTool => "mcp:*",
            Permission::Tool(name) => name.as_str(),
        }
    }
}

impl std::fmt::Display for Permission {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_action_default() {
        assert_eq!(Action::default(), Action::Allow);
    }

    #[test]
    fn test_action_checks() {
        assert!(Action::Allow.is_allow());
        assert!(!Action::Allow.is_deny());
        assert!(!Action::Allow.is_ask());

        assert!(!Action::Deny.is_allow());
        assert!(Action::Deny.is_deny());
        assert!(!Action::Deny.is_ask());

        assert!(!Action::Ask.is_allow());
        assert!(!Action::Ask.is_deny());
        assert!(Action::Ask.is_ask());
    }

    #[test]
    fn test_permission_from_tool_name() {
        assert_eq!(Permission::from_tool_name("read"), Permission::FileRead);
        assert_eq!(Permission::from_tool_name("write"), Permission::FileWrite);
        assert_eq!(Permission::from_tool_name("bash"), Permission::ShellExecute);
        assert_eq!(
            Permission::from_tool_name("fetch"),
            Permission::NetworkAccess
        );
        assert_eq!(Permission::from_tool_name("mcp_lsp"), Permission::McpTool);
        assert_eq!(
            Permission::from_tool_name("custom_tool"),
            Permission::Tool("custom_tool".to_string())
        );
    }

    #[test]
    fn test_permission_as_str() {
        assert_eq!(Permission::FileRead.as_str(), "file:read");
        assert_eq!(Permission::FileWrite.as_str(), "file:write");
        assert_eq!(Permission::ShellExecute.as_str(), "shell:execute");
    }

    #[test]
    fn test_action_serialization() {
        let allow = Action::Allow;
        let json = serde_json::to_string(&allow).unwrap();
        assert_eq!(json, "\"allow\"");

        let parsed: Action = serde_json::from_str("\"deny\"").unwrap();
        assert_eq!(parsed, Action::Deny);
    }
}
