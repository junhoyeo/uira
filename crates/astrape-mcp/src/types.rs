use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Configuration for an MCP server
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerConfig {
    /// Name of the MCP server
    pub name: String,
    /// Command to execute the server
    pub command: String,
    /// Arguments to pass to the command
    pub args: Vec<String>,
    /// Environment variables for the server process
    pub env: Option<HashMap<String, String>>,
}

/// Represents an MCP tool that can be called
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpTool {
    /// Name of the tool
    pub name: String,
    /// Description of what the tool does
    pub description: String,
    /// JSON schema for the tool's input parameters
    pub input_schema: serde_json::Value,
}

/// Represents an MCP resource that can be accessed
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpResource {
    /// URI of the resource
    pub uri: String,
    /// MIME type of the resource
    pub mime_type: String,
    /// Description of the resource
    pub description: Option<String>,
}

/// Represents an MCP prompt template
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpPrompt {
    /// Name of the prompt
    pub name: String,
    /// Description of the prompt
    pub description: String,
    /// Arguments the prompt accepts
    pub arguments: Vec<PromptArgument>,
}

/// Argument for a prompt template
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptArgument {
    /// Name of the argument
    pub name: String,
    /// Description of the argument
    pub description: Option<String>,
    /// Whether the argument is required
    pub required: bool,
}

/// Response from calling an MCP tool
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpToolResponse {
    /// Whether the tool call was successful
    pub success: bool,
    /// The result of the tool call
    pub result: Option<serde_json::Value>,
    /// Error message if the call failed
    pub error: Option<String>,
}

/// Capabilities of an MCP server
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpCapabilities {
    /// Whether the server supports tools
    pub tools: bool,
    /// Whether the server supports resources
    pub resources: bool,
    /// Whether the server supports prompts
    pub prompts: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mcp_server_config_serialization() {
        let config = McpServerConfig {
            name: "test-server".to_string(),
            command: "mcp-server".to_string(),
            args: vec!["--port".to_string(), "8080".to_string()],
            env: None,
        };

        let json = serde_json::to_string(&config).unwrap();
        let deserialized: McpServerConfig = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.name, "test-server");
        assert_eq!(deserialized.command, "mcp-server");
        assert_eq!(deserialized.args.len(), 2);
    }

    #[test]
    fn test_mcp_tool_creation() {
        let tool = McpTool {
            name: "calculator".to_string(),
            description: "A simple calculator tool".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "operation": { "type": "string" },
                    "a": { "type": "number" },
                    "b": { "type": "number" }
                }
            }),
        };

        assert_eq!(tool.name, "calculator");
        assert!(tool.input_schema.is_object());
    }

    #[test]
    fn test_mcp_resource_creation() {
        let resource = McpResource {
            uri: "file:///path/to/resource".to_string(),
            mime_type: "text/plain".to_string(),
            description: Some("A test resource".to_string()),
        };

        assert_eq!(resource.uri, "file:///path/to/resource");
        assert_eq!(resource.mime_type, "text/plain");
        assert!(resource.description.is_some());
    }

    #[test]
    fn test_mcp_prompt_creation() {
        let prompt = McpPrompt {
            name: "summarize".to_string(),
            description: "Summarize text".to_string(),
            arguments: vec![PromptArgument {
                name: "text".to_string(),
                description: Some("Text to summarize".to_string()),
                required: true,
            }],
        };

        assert_eq!(prompt.name, "summarize");
        assert_eq!(prompt.arguments.len(), 1);
        assert!(prompt.arguments[0].required);
    }

    #[test]
    fn test_mcp_tool_response() {
        let response = McpToolResponse {
            success: true,
            result: Some(serde_json::json!({"answer": 42})),
            error: None,
        };

        assert!(response.success);
        assert!(response.result.is_some());
        assert!(response.error.is_none());
    }

    #[test]
    fn test_mcp_capabilities() {
        let capabilities = McpCapabilities {
            tools: true,
            resources: true,
            prompts: false,
        };

        assert!(capabilities.tools);
        assert!(capabilities.resources);
        assert!(!capabilities.prompts);
    }
}
