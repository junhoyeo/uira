//! MCP (Model Context Protocol) server configuration
//!
//! Types for configuring MCP servers that extend Claude's capabilities.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// MCP server configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerConfig {
    /// Command to run the MCP server
    pub command: String,
    /// Arguments to pass to the command
    pub args: Vec<String>,
    /// Environment variables
    #[serde(skip_serializing_if = "Option::is_none")]
    pub env: Option<HashMap<String, String>>,
}

impl McpServerConfig {
    /// Create a new MCP server config
    pub fn new(command: impl Into<String>, args: Vec<String>) -> Self {
        Self {
            command: command.into(),
            args,
            env: None,
        }
    }

    /// Add environment variables
    pub fn with_env(mut self, env: HashMap<String, String>) -> Self {
        self.env = Some(env);
        self
    }
}

/// Create Context7 MCP server config
pub fn context7_server() -> McpServerConfig {
    McpServerConfig::new(
        "npx",
        vec!["-y".to_string(), "@upstash/context7-mcp".to_string()],
    )
}

/// Create Exa MCP server config
pub fn exa_server(api_key: String) -> McpServerConfig {
    let mut env = HashMap::new();
    env.insert("EXA_API_KEY".to_string(), api_key);

    McpServerConfig::new("npx", vec!["-y".to_string(), "exa-mcp-server".to_string()]).with_env(env)
}

/// Create GitHub MCP server config
pub fn github_server(token: String) -> McpServerConfig {
    let mut env = HashMap::new();
    env.insert("GITHUB_PERSONAL_ACCESS_TOKEN".to_string(), token);

    McpServerConfig::new(
        "npx",
        vec![
            "-y".to_string(),
            "@modelcontextprotocol/server-github".to_string(),
        ],
    )
    .with_env(env)
}

/// Create filesystem MCP server config
pub fn filesystem_server(allowed_dirs: Vec<String>) -> McpServerConfig {
    let mut args = vec![
        "-y".to_string(),
        "@modelcontextprotocol/server-filesystem".to_string(),
    ];
    args.extend(allowed_dirs);

    McpServerConfig::new("npx", args)
}

/// Default MCP servers configuration
pub fn get_default_mcp_servers(
    exa_api_key: Option<String>,
    enable_context7: bool,
) -> HashMap<String, McpServerConfig> {
    let mut servers = HashMap::new();

    if enable_context7 {
        servers.insert("context7".to_string(), context7_server());
    }

    if let Some(api_key) = exa_api_key {
        servers.insert("exa".to_string(), exa_server(api_key));
    }

    servers
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_context7_server() {
        let server = context7_server();
        assert_eq!(server.command, "npx");
        assert!(server.args.contains(&"@upstash/context7-mcp".to_string()));
    }

    #[test]
    fn test_exa_server_with_env() {
        let server = exa_server("test-key".to_string());
        assert!(server.env.is_some());
        let env = server.env.unwrap();
        assert_eq!(env.get("EXA_API_KEY"), Some(&"test-key".to_string()));
    }

    #[test]
    fn test_default_mcp_servers() {
        let servers = get_default_mcp_servers(None, true);
        assert!(servers.contains_key("context7"));
        assert!(!servers.contains_key("exa"));

        let servers_with_exa = get_default_mcp_servers(Some("key".to_string()), true);
        assert!(servers_with_exa.contains_key("exa"));
    }
}
