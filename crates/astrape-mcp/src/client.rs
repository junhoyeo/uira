use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use thiserror::Error;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use tokio::sync::Mutex;

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

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("Not connected")]
    NotConnected,

    #[error("Protocol error: {0}")]
    ProtocolError(String),
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

// --- JSON-RPC types for the MCP stdio transport ---

#[derive(Debug, Serialize)]
struct JsonRpcRequest {
    jsonrpc: &'static str,
    id: u64,
    method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    params: Option<Value>,
}

#[derive(Debug, Deserialize)]
struct JsonRpcResponse {
    #[allow(dead_code)]
    jsonrpc: String,
    #[allow(dead_code)]
    id: Option<u64>,
    result: Option<Value>,
    error: Option<JsonRpcError>,
}

#[derive(Debug, Deserialize)]
struct JsonRpcError {
    #[allow(dead_code)]
    code: i64,
    message: String,
    #[allow(dead_code)]
    data: Option<Value>,
}

// --- StdioMcpClient Builder ---

/// Builder for constructing a `StdioMcpClient`.
pub struct StdioMcpClientBuilder {
    command: String,
    args: Vec<String>,
    env: HashMap<String, String>,
}

impl StdioMcpClientBuilder {
    /// Set the arguments for the child process command.
    pub fn args<I, S>(mut self, args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.args = args.into_iter().map(|s| s.into()).collect();
        self
    }

    /// Add a single argument.
    pub fn arg<S: Into<String>>(mut self, arg: S) -> Self {
        self.args.push(arg.into());
        self
    }

    /// Set an environment variable for the child process.
    pub fn env<K: Into<String>, V: Into<String>>(mut self, key: K, value: V) -> Self {
        self.env.insert(key.into(), value.into());
        self
    }

    /// Set multiple environment variables.
    pub fn envs<I, K, V>(mut self, vars: I) -> Self
    where
        I: IntoIterator<Item = (K, V)>,
        K: Into<String>,
        V: Into<String>,
    {
        for (k, v) in vars {
            self.env.insert(k.into(), v.into());
        }
        self
    }

    /// Build the `StdioMcpClient`. Does not start the process yet;
    /// call `connect()` to spawn and perform the initialization handshake.
    pub fn build(self) -> StdioMcpClient {
        StdioMcpClient {
            command: self.command,
            args: self.args,
            env: self.env,
            child: None,
            stdin: Arc::new(Mutex::new(None)),
            stdout: Arc::new(Mutex::new(None)),
            request_id: AtomicU64::new(1),
            capabilities: None,
            tools: Vec::new(),
            resources: Vec::new(),
            prompts: Vec::new(),
            connected: false,
        }
    }
}

// --- StdioMcpClient ---

/// An MCP client that communicates with a server over stdio using JSON-RPC.
///
/// # Example
///
/// ```rust,no_run
/// use astrape_mcp::client::StdioMcpClient;
/// use astrape_mcp::client::McpClient;
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let mut client = StdioMcpClient::new("npx")
///     .args(["-y", "@modelcontextprotocol/server-filesystem", "/tmp"])
///     .build();
///
/// client.connect().await?;
/// let tools = client.list_tools().await?;
/// for tool in &tools {
///     println!("{}: {}", tool.name, tool.description);
/// }
/// client.disconnect().await?;
/// # Ok(())
/// # }
/// ```
pub struct StdioMcpClient {
    /// The command to run (e.g. "npx", "python", "node")
    command: String,
    /// Arguments to pass to the command
    args: Vec<String>,
    /// Environment variables for the child process
    env: HashMap<String, String>,
    /// The spawned child process
    child: Option<Child>,
    /// Stdin handle for sending JSON-RPC requests
    stdin: Arc<Mutex<Option<ChildStdin>>>,
    /// Buffered stdout reader for receiving JSON-RPC responses
    stdout: Arc<Mutex<Option<BufReader<ChildStdout>>>>,
    /// Monotonically increasing request ID
    request_id: AtomicU64,
    /// Server capabilities received during initialization
    capabilities: Option<McpCapabilities>,
    /// Cached list of tools from the server
    tools: Vec<McpTool>,
    /// Cached list of resources from the server
    resources: Vec<McpResource>,
    /// Cached list of prompts from the server
    prompts: Vec<McpPrompt>,
    /// Whether the client is connected
    connected: bool,
}

impl StdioMcpClient {
    /// Create a new builder with the given command.
    ///
    /// # Example
    /// ```rust,no_run
    /// use astrape_mcp::client::StdioMcpClient;
    ///
    /// let client = StdioMcpClient::builder("npx")
    ///     .args(["-y", "@modelcontextprotocol/server-filesystem", "/tmp"])
    ///     .build();
    /// ```
    pub fn builder<S: Into<String>>(command: S) -> StdioMcpClientBuilder {
        StdioMcpClientBuilder {
            command: command.into(),
            args: Vec::new(),
            env: HashMap::new(),
        }
    }

    /// Create a StdioMcpClient from an `McpServerConfig`.
    pub fn from_config(config: &crate::types::McpServerConfig) -> Self {
        let mut builder = Self::builder(&config.command).args(config.args.clone());
        if let Some(env) = &config.env {
            builder = builder.envs(env.clone());
        }
        builder.build()
    }

    /// Get the next request ID.
    fn next_id(&self) -> u64 {
        self.request_id.fetch_add(1, Ordering::SeqCst)
    }

    /// Send a JSON-RPC request and read the response.
    async fn send_request(&self, method: &str, params: Option<Value>) -> McpResult<Value> {
        if !self.connected {
            return Err(McpClientError::NotConnected);
        }

        let id = self.next_id();
        let request = JsonRpcRequest {
            jsonrpc: "2.0",
            id,
            method: method.to_string(),
            params,
        };

        let mut request_bytes = serde_json::to_vec(&request)?;
        request_bytes.push(b'\n');

        // Send the request
        {
            let mut stdin_guard = self.stdin.lock().await;
            let stdin = stdin_guard.as_mut().ok_or(McpClientError::NotConnected)?;
            stdin.write_all(&request_bytes).await.map_err(|e| {
                McpClientError::ConnectionError(format!("Failed to write to stdin: {}", e))
            })?;
            stdin.flush().await.map_err(|e| {
                McpClientError::ConnectionError(format!("Failed to flush stdin: {}", e))
            })?;
        }

        // Read lines until we get a valid JSON-RPC response (skip notifications)
        loop {
            let line = {
                let mut stdout_guard = self.stdout.lock().await;
                let stdout = stdout_guard.as_mut().ok_or(McpClientError::NotConnected)?;
                let mut line = String::new();
                let bytes_read = stdout.read_line(&mut line).await.map_err(|e| {
                    McpClientError::ConnectionError(format!("Failed to read from stdout: {}", e))
                })?;
                if bytes_read == 0 {
                    return Err(McpClientError::ConnectionError(
                        "Server closed stdout (EOF)".to_string(),
                    ));
                }
                line
            };

            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }

            // Try to parse as JSON-RPC response
            let response: JsonRpcResponse = serde_json::from_str(trimmed).map_err(|e| {
                McpClientError::ProtocolError(format!(
                    "Invalid JSON-RPC response: {} (raw: {})",
                    e,
                    trimmed.chars().take(200).collect::<String>()
                ))
            })?;

            // If it's a notification (no id), skip it
            if response.id.is_none() {
                continue;
            }

            // Check if the id matches (MCP servers should respond in order for stdio)
            if let Some(resp_id) = response.id {
                if resp_id != id {
                    // Out of order response; in a production client you'd buffer these.
                    // For stdio (sequential), this shouldn't happen, but handle gracefully.
                    continue;
                }
            }

            // Handle error response
            if let Some(err) = response.error {
                return Err(McpClientError::ServerError(err.message));
            }

            return Ok(response.result.unwrap_or(Value::Null));
        }
    }

    /// Perform the MCP initialization handshake.
    async fn initialize(&mut self) -> McpResult<()> {
        let init_params = serde_json::json!({
            "protocolVersion": "2024-11-05",
            "capabilities": {
                "roots": { "listChanged": true }
            },
            "clientInfo": {
                "name": "astrape",
                "version": env!("CARGO_PKG_VERSION")
            }
        });

        let result = self.send_request("initialize", Some(init_params)).await?;

        // Parse capabilities from the server's response
        let server_caps = result.get("capabilities").cloned().unwrap_or(Value::Null);
        self.capabilities = Some(McpCapabilities {
            tools: server_caps.get("tools").is_some(),
            resources: server_caps.get("resources").is_some(),
            prompts: server_caps.get("prompts").is_some(),
        });

        // Send initialized notification (no response expected)
        let notification = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "notifications/initialized",
            "params": {}
        });
        let mut notif_bytes = serde_json::to_vec(&notification)?;
        notif_bytes.push(b'\n');

        {
            let mut stdin_guard = self.stdin.lock().await;
            if let Some(stdin) = stdin_guard.as_mut() {
                stdin.write_all(&notif_bytes).await.map_err(|e| {
                    McpClientError::ConnectionError(format!(
                        "Failed to send initialized notification: {}",
                        e
                    ))
                })?;
                stdin.flush().await.map_err(|e| {
                    McpClientError::ConnectionError(format!("Failed to flush: {}", e))
                })?;
            }
        }

        Ok(())
    }
}

#[async_trait]
impl McpClient for StdioMcpClient {
    async fn connect(&mut self) -> McpResult<()> {
        let mut cmd = Command::new(&self.command);
        cmd.args(&self.args)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null());

        for (key, value) in &self.env {
            cmd.env(key, value);
        }

        let mut child = cmd.spawn().map_err(|e| {
            McpClientError::ConnectionError(format!("Failed to spawn '{}': {}", self.command, e))
        })?;

        let stdin = child.stdin.take().ok_or_else(|| {
            McpClientError::ConnectionError("Failed to capture child stdin".to_string())
        })?;
        let stdout = child.stdout.take().ok_or_else(|| {
            McpClientError::ConnectionError("Failed to capture child stdout".to_string())
        })?;

        self.child = Some(child);
        *self.stdin.lock().await = Some(stdin);
        *self.stdout.lock().await = Some(BufReader::new(stdout));
        self.connected = true;

        // Perform MCP initialization handshake
        self.initialize().await?;

        Ok(())
    }

    async fn disconnect(&mut self) -> McpResult<()> {
        self.connected = false;

        // Drop stdin to signal EOF to the child
        *self.stdin.lock().await = None;
        *self.stdout.lock().await = None;

        if let Some(mut child) = self.child.take() {
            // Try to kill gracefully, ignore errors (process may have already exited)
            let _ = child.kill().await;
            let _ = child.wait().await;
        }

        self.capabilities = None;
        self.tools.clear();
        self.resources.clear();
        self.prompts.clear();

        Ok(())
    }

    async fn get_capabilities(&self) -> McpResult<McpCapabilities> {
        self.capabilities
            .clone()
            .ok_or(McpClientError::NotConnected)
    }

    async fn list_tools(&self) -> McpResult<Vec<McpTool>> {
        let result = self
            .send_request("tools/list", Some(serde_json::json!({})))
            .await?;

        let tools_value = result.get("tools").cloned().unwrap_or(Value::Array(vec![]));

        let raw_tools: Vec<Value> = serde_json::from_value(tools_value).map_err(|e| {
            McpClientError::ProtocolError(format!("Invalid tools list response: {}", e))
        })?;

        let tools = raw_tools
            .into_iter()
            .map(|t| McpTool {
                name: t
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                description: t
                    .get("description")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                input_schema: t
                    .get("inputSchema")
                    .cloned()
                    .unwrap_or(serde_json::json!({"type": "object"})),
            })
            .collect();

        Ok(tools)
    }

    async fn get_tool(&self, name: &str) -> McpResult<McpTool> {
        let tools = self.list_tools().await?;
        tools
            .into_iter()
            .find(|t| t.name == name)
            .ok_or_else(|| McpClientError::ToolNotFound(name.to_string()))
    }

    async fn call_tool(&self, name: &str, arguments: Value) -> McpResult<McpToolResponse> {
        let params = serde_json::json!({
            "name": name,
            "arguments": arguments
        });

        let result = self.send_request("tools/call", Some(params)).await;

        match result {
            Ok(value) => {
                // MCP tool call responses have "content" array and optional "isError"
                let is_error = value
                    .get("isError")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);

                if is_error {
                    // Extract error text from content
                    let error_text = value
                        .get("content")
                        .and_then(|c| c.as_array())
                        .and_then(|arr| arr.first())
                        .and_then(|item| item.get("text"))
                        .and_then(|t| t.as_str())
                        .unwrap_or("Unknown error")
                        .to_string();

                    Ok(McpToolResponse {
                        success: false,
                        result: Some(value),
                        error: Some(error_text),
                    })
                } else {
                    Ok(McpToolResponse {
                        success: true,
                        result: Some(value),
                        error: None,
                    })
                }
            }
            Err(McpClientError::ServerError(msg)) => Ok(McpToolResponse {
                success: false,
                result: None,
                error: Some(msg),
            }),
            Err(e) => Err(e),
        }
    }

    async fn list_resources(&self) -> McpResult<Vec<McpResource>> {
        let result = self
            .send_request("resources/list", Some(serde_json::json!({})))
            .await?;

        let resources_value = result
            .get("resources")
            .cloned()
            .unwrap_or(Value::Array(vec![]));

        let raw_resources: Vec<Value> = serde_json::from_value(resources_value).map_err(|e| {
            McpClientError::ProtocolError(format!("Invalid resources list response: {}", e))
        })?;

        let resources = raw_resources
            .into_iter()
            .map(|r| McpResource {
                uri: r
                    .get("uri")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                mime_type: r
                    .get("mimeType")
                    .and_then(|v| v.as_str())
                    .unwrap_or("application/octet-stream")
                    .to_string(),
                description: r
                    .get("description")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string()),
            })
            .collect();

        Ok(resources)
    }

    async fn get_resource(&self, uri: &str) -> McpResult<McpResource> {
        let resources = self.list_resources().await?;
        resources
            .into_iter()
            .find(|r| r.uri == uri)
            .ok_or_else(|| McpClientError::ResourceNotFound(uri.to_string()))
    }

    async fn read_resource(&self, uri: &str) -> McpResult<String> {
        let params = serde_json::json!({
            "uri": uri
        });

        let result = self.send_request("resources/read", Some(params)).await?;

        // MCP resources/read returns {"contents": [{"uri": ..., "text": ...}]}
        let text = result
            .get("contents")
            .and_then(|c| c.as_array())
            .and_then(|arr| arr.first())
            .and_then(|item| item.get("text"))
            .and_then(|t| t.as_str())
            .unwrap_or("")
            .to_string();

        Ok(text)
    }

    async fn list_prompts(&self) -> McpResult<Vec<McpPrompt>> {
        let result = self
            .send_request("prompts/list", Some(serde_json::json!({})))
            .await?;

        let prompts_value = result
            .get("prompts")
            .cloned()
            .unwrap_or(Value::Array(vec![]));

        let raw_prompts: Vec<Value> = serde_json::from_value(prompts_value).map_err(|e| {
            McpClientError::ProtocolError(format!("Invalid prompts list response: {}", e))
        })?;

        let prompts = raw_prompts
            .into_iter()
            .map(|p| {
                let arguments = p
                    .get("arguments")
                    .and_then(|a| a.as_array())
                    .map(|arr| {
                        arr.iter()
                            .map(|arg| crate::types::PromptArgument {
                                name: arg
                                    .get("name")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("")
                                    .to_string(),
                                description: arg
                                    .get("description")
                                    .and_then(|v| v.as_str())
                                    .map(|s| s.to_string()),
                                required: arg
                                    .get("required")
                                    .and_then(|v| v.as_bool())
                                    .unwrap_or(false),
                            })
                            .collect()
                    })
                    .unwrap_or_default();

                McpPrompt {
                    name: p
                        .get("name")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                    description: p
                        .get("description")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                    arguments,
                }
            })
            .collect();

        Ok(prompts)
    }

    async fn get_prompt(&self, name: &str) -> McpResult<McpPrompt> {
        let prompts = self.list_prompts().await?;
        prompts
            .into_iter()
            .find(|p| p.name == name)
            .ok_or_else(|| McpClientError::PromptNotFound(name.to_string()))
    }

    async fn execute_prompt(&self, name: &str, arguments: Value) -> McpResult<String> {
        let params = serde_json::json!({
            "name": name,
            "arguments": arguments
        });

        let result = self.send_request("prompts/get", Some(params)).await?;

        // MCP prompts/get returns {"messages": [{"role": ..., "content": {"type": "text", "text": ...}}]}
        let text = result
            .get("messages")
            .and_then(|m| m.as_array())
            .and_then(|arr| arr.last())
            .and_then(|msg| msg.get("content"))
            .and_then(|c| {
                // content can be a string or an object with "text"
                if let Some(s) = c.as_str() {
                    Some(s.to_string())
                } else {
                    c.get("text")
                        .and_then(|t| t.as_str())
                        .map(|s| s.to_string())
                }
            })
            .unwrap_or_default();

        Ok(text)
    }

    fn is_connected(&self) -> bool {
        self.connected
    }
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
