use async_trait::async_trait;
use serde_json::Value;
use thiserror::Error;

use crate::types::{McpCapabilities, McpPrompt, McpResource, McpTool, McpToolResponse};

/// Error type for MCP client operations
#[derive(Debug, Error)]
pub enum McpClientError {
    #[error("Connection error: {0}")]
    ConnectionError(String),

    #[error("Tool not found: {0}")]
    ToolNotFound(String),

    #[error("Resource not found: {0}")]
    ResourceNotFound(String),

    #[error("Prompt not found: {0}")]
    PromptNotFound(String),

    #[error("Tool execution failed: {0}")]
    ToolExecutionFailed(String),

    #[error("Serialization error: {0}")]
    SerializationError(#[from] serde_json::Error),

    #[error("Invalid request: {0}")]
    InvalidRequest(String),

    #[error("Server error: {0}")]
    ServerError(String),
}

/// Result type for MCP client operations
pub type McpResult<T> = Result<T, McpClientError>;

/// Trait for MCP client implementations
#[async_trait]
pub trait McpClient: Send + Sync {
    /// Initialize the client connection
    async fn connect(&mut self) -> McpResult<()>;

    /// Close the client connection
    async fn disconnect(&mut self) -> McpResult<()>;

    /// Get the capabilities of the MCP server
    async fn get_capabilities(&self) -> McpResult<McpCapabilities>;

    /// List all available tools
    async fn list_tools(&self) -> McpResult<Vec<McpTool>>;

    /// Get a specific tool by name
    async fn get_tool(&self, name: &str) -> McpResult<McpTool>;

    /// Call a tool with the given arguments
    async fn call_tool(&self, name: &str, arguments: Value) -> McpResult<McpToolResponse>;

    /// List all available resources
    async fn list_resources(&self) -> McpResult<Vec<McpResource>>;

    /// Get a specific resource by URI
    async fn get_resource(&self, uri: &str) -> McpResult<McpResource>;

    /// Read the content of a resource
    async fn read_resource(&self, uri: &str) -> McpResult<String>;

    /// List all available prompts
    async fn list_prompts(&self) -> McpResult<Vec<McpPrompt>>;

    /// Get a specific prompt by name
    async fn get_prompt(&self, name: &str) -> McpResult<McpPrompt>;

    /// Execute a prompt with the given arguments
    async fn execute_prompt(&self, name: &str, arguments: Value) -> McpResult<String>;

    /// Check if the client is connected
    fn is_connected(&self) -> bool;
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockMcpClient {
        connected: bool,
    }

    #[async_trait]
    impl McpClient for MockMcpClient {
        async fn connect(&mut self) -> McpResult<()> {
            self.connected = true;
            Ok(())
        }

        async fn disconnect(&mut self) -> McpResult<()> {
            self.connected = false;
            Ok(())
        }

        async fn get_capabilities(&self) -> McpResult<McpCapabilities> {
            Ok(McpCapabilities {
                tools: true,
                resources: true,
                prompts: true,
            })
        }

        async fn list_tools(&self) -> McpResult<Vec<McpTool>> {
            Ok(vec![])
        }

        async fn get_tool(&self, _name: &str) -> McpResult<McpTool> {
            Err(McpClientError::ToolNotFound("test".to_string()))
        }

        async fn call_tool(&self, _name: &str, _arguments: Value) -> McpResult<McpToolResponse> {
            Ok(McpToolResponse {
                success: true,
                result: Some(Value::Null),
                error: None,
            })
        }

        async fn list_resources(&self) -> McpResult<Vec<McpResource>> {
            Ok(vec![])
        }

        async fn get_resource(&self, _uri: &str) -> McpResult<McpResource> {
            Err(McpClientError::ResourceNotFound("test".to_string()))
        }

        async fn read_resource(&self, _uri: &str) -> McpResult<String> {
            Ok("test content".to_string())
        }

        async fn list_prompts(&self) -> McpResult<Vec<McpPrompt>> {
            Ok(vec![])
        }

        async fn get_prompt(&self, _name: &str) -> McpResult<McpPrompt> {
            Err(McpClientError::PromptNotFound("test".to_string()))
        }

        async fn execute_prompt(&self, _name: &str, _arguments: Value) -> McpResult<String> {
            Ok("test result".to_string())
        }

        fn is_connected(&self) -> bool {
            self.connected
        }
    }

    #[tokio::test]
    async fn test_mock_client_connect() {
        let mut client = MockMcpClient { connected: false };
        assert!(!client.is_connected());

        client.connect().await.unwrap();
        assert!(client.is_connected());

        client.disconnect().await.unwrap();
        assert!(!client.is_connected());
    }

    #[tokio::test]
    async fn test_mock_client_capabilities() {
        let client = MockMcpClient { connected: true };
        let caps = client.get_capabilities().await.unwrap();

        assert!(caps.tools);
        assert!(caps.resources);
        assert!(caps.prompts);
    }

    #[tokio::test]
    async fn test_mock_client_tool_not_found() {
        let client = MockMcpClient { connected: true };
        let result = client.get_tool("nonexistent").await;

        assert!(result.is_err());
        match result {
            Err(McpClientError::ToolNotFound(_)) => (),
            _ => panic!("Expected ToolNotFound error"),
        }
    }

    #[tokio::test]
    async fn test_mock_client_call_tool() {
        let client = MockMcpClient { connected: true };
        let response = client
            .call_tool("test", serde_json::json!({}))
            .await
            .unwrap();

        assert!(response.success);
        assert!(response.error.is_none());
    }
}
