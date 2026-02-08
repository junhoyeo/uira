use crate::types::{DiscoveredTool, McpServerConfig};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use thiserror::Error;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use tokio::sync::{oneshot, Mutex};

const DEFAULT_RPC_TIMEOUT: Duration = Duration::from_secs(20);
const MAX_MESSAGE_SIZE: usize = 8 * 1024 * 1024;

type PendingResponse = oneshot::Sender<Result<Value, McpClientError>>;
type PendingMap = HashMap<u64, PendingResponse>;
type SharedPendingMap = Arc<Mutex<PendingMap>>;

#[derive(Debug, Error)]
pub enum McpClientError {
    #[error("failed to spawn MCP server '{server}': {message}")]
    SpawnFailed { server: String, message: String },

    #[error("failed to serialize JSON-RPC message: {0}")]
    Serialization(String),

    #[error("failed to parse JSON-RPC message: {0}")]
    Parse(String),

    #[error("JSON-RPC timeout calling '{method}' on '{server}'")]
    Timeout { server: String, method: String },

    #[error("JSON-RPC transport closed for '{server}'")]
    TransportClosed { server: String },

    #[error("MCP protocol error ({code}): {message}")]
    ProtocolError { code: i64, message: String },

    #[error("invalid MCP response: {0}")]
    InvalidResponse(String),

    #[error("unknown MCP server: {server}")]
    UnknownServer { server: String },
}

#[derive(Clone)]
pub struct McpRuntimeManager {
    servers: HashMap<String, Arc<Mutex<ServerRuntime>>>,
    rpc_timeout: Duration,
}

impl McpRuntimeManager {
    pub fn new(configs: Vec<McpServerConfig>, default_cwd: PathBuf) -> Self {
        let mut servers = HashMap::new();
        for config in configs {
            servers.insert(
                config.name.clone(),
                Arc::new(Mutex::new(ServerRuntime::new(config, default_cwd.clone()))),
            );
        }

        Self {
            servers,
            rpc_timeout: DEFAULT_RPC_TIMEOUT,
        }
    }

    pub fn with_rpc_timeout(mut self, timeout: Duration) -> Self {
        self.rpc_timeout = timeout;
        self
    }

    pub async fn call_tool(
        &self,
        server_name: &str,
        tool_name: &str,
        arguments: Value,
        cwd: &Path,
    ) -> Result<Value, McpClientError> {
        let runtime = self
            .servers
            .get(server_name)
            .ok_or_else(|| McpClientError::UnknownServer {
                server: server_name.to_string(),
            })?
            .clone();

        let mut runtime = runtime.lock().await;
        runtime.default_cwd = cwd.to_path_buf();
        runtime.ensure_connected(self.rpc_timeout).await?;

        match call_tool_once(&mut runtime, tool_name, arguments.clone(), self.rpc_timeout).await {
            Ok(v) => Ok(v),
            Err(McpClientError::TransportClosed { .. }) => {
                runtime.restart(self.rpc_timeout).await?;
                call_tool_once(&mut runtime, tool_name, arguments, self.rpc_timeout).await
            }
            Err(err) => Err(err),
        }
    }
}

async fn call_tool_once(
    runtime: &mut ServerRuntime,
    tool_name: &str,
    arguments: Value,
    timeout: Duration,
) -> Result<Value, McpClientError> {
    let params = json!({"name": tool_name, "arguments": arguments});
    let response = runtime
        .connection_mut()?
        .request("tools/call", Some(params), timeout)
        .await?;
    Ok(parse_tools_call_result(response))
}

pub async fn discover_tools(
    configs: &[McpServerConfig],
    cwd: &Path,
    timeout: Duration,
) -> Result<Vec<DiscoveredTool>, McpClientError> {
    let mut discovered = Vec::new();

    for config in configs {
        let mut connection = McpConnection::spawn(config.clone(), cwd.to_path_buf()).await?;
        connection.initialize(timeout).await?;
        let tools = connection.list_tools(timeout).await?;

        for tool in tools {
            let original_name = tool
                .get("name")
                .and_then(Value::as_str)
                .ok_or_else(|| McpClientError::InvalidResponse("tool missing name".to_string()))?
                .to_string();

            let description = tool
                .get("description")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();

            let input_schema = tool
                .get("inputSchema")
                .cloned()
                .unwrap_or_else(|| json!({}));

            discovered.push(DiscoveredTool {
                server_name: config.name.clone(),
                namespaced_name: DiscoveredTool::namespaced(&config.name, &original_name),
                original_name,
                description,
                input_schema,
            });
        }

        connection.shutdown().await;
    }

    Ok(discovered)
}

struct ServerRuntime {
    config: McpServerConfig,
    default_cwd: PathBuf,
    connection: Option<McpConnection>,
}

impl ServerRuntime {
    fn new(config: McpServerConfig, default_cwd: PathBuf) -> Self {
        Self {
            config,
            default_cwd,
            connection: None,
        }
    }

    async fn ensure_connected(&mut self, timeout: Duration) -> Result<(), McpClientError> {
        if self.connection.is_some() {
            return Ok(());
        }

        self.restart(timeout).await
    }

    async fn restart(&mut self, timeout: Duration) -> Result<(), McpClientError> {
        if let Some(mut existing) = self.connection.take() {
            existing.shutdown().await;
        }

        let mut connection =
            McpConnection::spawn(self.config.clone(), self.default_cwd.clone()).await?;
        connection.initialize(timeout).await?;
        self.connection = Some(connection);
        Ok(())
    }

    fn connection_mut(&mut self) -> Result<&mut McpConnection, McpClientError> {
        self.connection
            .as_mut()
            .ok_or_else(|| McpClientError::TransportClosed {
                server: self.config.name.clone(),
            })
    }
}

struct McpConnection {
    server_name: String,
    child: Child,
    stdin: ChildStdin,
    pending: SharedPendingMap,
    next_id: AtomicU64,
}

impl McpConnection {
    async fn spawn(config: McpServerConfig, cwd: PathBuf) -> Result<Self, McpClientError> {
        let mut command = Command::new(&config.command);
        command
            .args(&config.args)
            .current_dir(cwd)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());

        for (key, value) in &config.env {
            command.env(key, value);
        }

        let mut child = command.spawn().map_err(|e| McpClientError::SpawnFailed {
            server: config.name.clone(),
            message: e.to_string(),
        })?;

        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| McpClientError::SpawnFailed {
                server: config.name.clone(),
                message: "failed to capture stdin".to_string(),
            })?;

        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| McpClientError::SpawnFailed {
                server: config.name.clone(),
                message: "failed to capture stdout".to_string(),
            })?;

        let pending = Arc::new(Mutex::new(HashMap::new()));
        spawn_stdout_loop(config.name.clone(), stdout, pending.clone());

        if let Some(stderr) = child.stderr.take() {
            spawn_stderr_loop(config.name.clone(), stderr);
        }

        Ok(Self {
            server_name: config.name,
            child,
            stdin,
            pending,
            next_id: AtomicU64::new(1),
        })
    }

    async fn initialize(&mut self, timeout: Duration) -> Result<(), McpClientError> {
        let init_result = self
            .request(
                "initialize",
                Some(json!({
                    "protocolVersion": "2024-11-05",
                    "capabilities": {"tools": {}},
                    "clientInfo": {"name": "uira", "version": env!("CARGO_PKG_VERSION")}
                })),
                timeout,
            )
            .await?;

        if init_result.get("protocolVersion").is_none() {
            return Err(McpClientError::InvalidResponse(
                "initialize response missing protocolVersion".to_string(),
            ));
        }

        self.notify("notifications/initialized", None).await
    }

    async fn list_tools(&mut self, timeout: Duration) -> Result<Vec<Value>, McpClientError> {
        let mut tools = Vec::new();
        let mut cursor: Option<String> = None;

        loop {
            let params = cursor
                .as_ref()
                .map(|c| json!({"cursor": c}))
                .or_else(|| Some(json!({})));

            let result = self.request("tools/list", params, timeout).await?;
            let page_tools = result
                .get("tools")
                .and_then(Value::as_array)
                .ok_or_else(|| {
                    McpClientError::InvalidResponse("tools/list response missing tools".to_string())
                })?;

            tools.extend(page_tools.iter().cloned());

            cursor = result
                .get("nextCursor")
                .and_then(Value::as_str)
                .map(|s| s.to_string());

            if cursor.is_none() {
                break;
            }
        }

        Ok(tools)
    }

    async fn request(
        &mut self,
        method: &str,
        params: Option<Value>,
        timeout: Duration,
    ) -> Result<Value, McpClientError> {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let request = json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params.unwrap_or_else(|| json!({}))
        });

        let payload = serde_json::to_vec(&request)
            .map_err(|e| McpClientError::Serialization(e.to_string()))?;

        let (tx, rx) = oneshot::channel();
        self.pending.lock().await.insert(id, tx);

        if let Err(e) = self.stdin.write_all(&payload).await {
            let _ = self.pending.lock().await.remove(&id);
            return Err(McpClientError::TransportClosed {
                server: format!("{} ({})", self.server_name, e),
            });
        }
        if let Err(e) = self.stdin.write_all(b"\n").await {
            let _ = self.pending.lock().await.remove(&id);
            return Err(McpClientError::TransportClosed {
                server: format!("{} ({})", self.server_name, e),
            });
        }
        if let Err(e) = self.stdin.flush().await {
            let _ = self.pending.lock().await.remove(&id);
            return Err(McpClientError::TransportClosed {
                server: format!("{} ({})", self.server_name, e),
            });
        }

        match tokio::time::timeout(timeout, rx).await {
            Ok(Ok(result)) => result,
            Ok(Err(_)) => Err(McpClientError::TransportClosed {
                server: self.server_name.clone(),
            }),
            Err(_) => {
                let _ = self.pending.lock().await.remove(&id);
                Err(McpClientError::Timeout {
                    server: self.server_name.clone(),
                    method: method.to_string(),
                })
            }
        }
    }

    async fn notify(&mut self, method: &str, params: Option<Value>) -> Result<(), McpClientError> {
        let notification = json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params.unwrap_or_else(|| json!({}))
        });

        let payload = serde_json::to_vec(&notification)
            .map_err(|e| McpClientError::Serialization(e.to_string()))?;

        self.stdin
            .write_all(&payload)
            .await
            .map_err(|_| McpClientError::TransportClosed {
                server: self.server_name.clone(),
            })?;
        self.stdin
            .write_all(b"\n")
            .await
            .map_err(|_| McpClientError::TransportClosed {
                server: self.server_name.clone(),
            })?;
        self.stdin
            .flush()
            .await
            .map_err(|_| McpClientError::TransportClosed {
                server: self.server_name.clone(),
            })?;

        Ok(())
    }

    async fn shutdown(&mut self) {
        let _ = self.stdin.shutdown().await;
        let _ = self.child.kill().await;
        let _ = self.child.wait().await;
    }
}

fn parse_tools_call_result(result: Value) -> Value {
    let is_error = result
        .get("isError")
        .and_then(Value::as_bool)
        .unwrap_or(false);

    let content = result
        .get("content")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();

    if is_error {
        return json!({
            "isError": true,
            "content": content,
        });
    }

    json!({
        "isError": false,
        "content": content,
    })
}

fn spawn_stdout_loop(server_name: String, mut stdout: ChildStdout, pending: SharedPendingMap) {
    tokio::spawn(async move {
        let mut buffer = Vec::<u8>::new();
        let mut read_buf = [0u8; 8192];

        loop {
            match stdout.read(&mut read_buf).await {
                Ok(0) => {
                    fail_all_pending(&pending, &server_name).await;
                    break;
                }
                Ok(n) => {
                    buffer.extend_from_slice(&read_buf[..n]);
                    while let Some(message_bytes) = extract_message(&mut buffer) {
                        if message_bytes.is_empty() {
                            continue;
                        }

                        let parsed: Value = match serde_json::from_slice(&message_bytes) {
                            Ok(v) => v,
                            Err(e) => {
                                tracing::warn!(
                                    server = %server_name,
                                    error = %e,
                                    "failed to parse MCP message"
                                );
                                continue;
                            }
                        };

                        let Some(id) = parsed.get("id") else {
                            continue;
                        };
                        let Some(id) = id.as_u64() else {
                            continue;
                        };

                        if let Some(error) = parsed.get("error") {
                            let code = error.get("code").and_then(Value::as_i64).unwrap_or(-32000);
                            let message = error
                                .get("message")
                                .and_then(Value::as_str)
                                .unwrap_or("unknown error")
                                .to_string();

                            if let Some(tx) = pending.lock().await.remove(&id) {
                                let _ =
                                    tx.send(Err(McpClientError::ProtocolError { code, message }));
                            }
                            continue;
                        }

                        if let Some(result) = parsed.get("result") {
                            if let Some(tx) = pending.lock().await.remove(&id) {
                                let _ = tx.send(Ok(result.clone()));
                            }
                        }
                    }
                }
                Err(_) => {
                    fail_all_pending(&pending, &server_name).await;
                    break;
                }
            }
        }
    });
}

fn spawn_stderr_loop(server_name: String, stderr: tokio::process::ChildStderr) {
    tokio::spawn(async move {
        use tokio::io::{AsyncBufReadExt, BufReader};
        let mut lines = BufReader::new(stderr).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            tracing::debug!(server = %server_name, "mcp stderr: {}", line);
        }
    });
}

async fn fail_all_pending(pending: &SharedPendingMap, server_name: &str) {
    let mut lock = pending.lock().await;
    let mut drained = HashMap::new();
    std::mem::swap(&mut *lock, &mut drained);
    drop(lock);

    for (_, tx) in drained {
        let _ = tx.send(Err(McpClientError::TransportClosed {
            server: server_name.to_string(),
        }));
    }
}

fn extract_message(buffer: &mut Vec<u8>) -> Option<Vec<u8>> {
    loop {
        while matches!(buffer.first(), Some(b'\n' | b'\r')) {
            buffer.remove(0);
        }

        if buffer.is_empty() {
            return None;
        }

        if starts_with_content_length(buffer) {
            let (header_end, delimiter_len) = find_header_end(buffer)?;
            let headers = String::from_utf8_lossy(&buffer[..header_end]);
            let mut content_length: Option<usize> = None;
            for line in headers.lines() {
                let lower = line.to_ascii_lowercase();
                if let Some(rest) = lower.strip_prefix("content-length:") {
                    content_length = rest.trim().parse::<usize>().ok();
                    break;
                }
            }

            let content_length = content_length?;
            if content_length > MAX_MESSAGE_SIZE {
                buffer.clear();
                return None;
            }
            let body_start = header_end + delimiter_len;
            if buffer.len() < body_start + content_length {
                return None;
            }

            let body = buffer[body_start..body_start + content_length].to_vec();
            buffer.drain(..body_start + content_length);
            return Some(body);
        }

        let newline_pos = buffer.iter().position(|b| *b == b'\n')?;
        let mut line = buffer[..newline_pos].to_vec();
        buffer.drain(..=newline_pos);

        while matches!(line.last(), Some(b'\r')) {
            line.pop();
        }

        if line.is_empty() {
            continue;
        }

        return Some(line);
    }
}

fn starts_with_content_length(buffer: &[u8]) -> bool {
    let prefix = b"content-length:";
    if buffer.len() < prefix.len() {
        return false;
    }

    buffer[..prefix.len()]
        .iter()
        .zip(prefix.iter())
        .all(|(a, b)| a.to_ascii_lowercase() == *b)
}

fn find_header_end(buffer: &[u8]) -> Option<(usize, usize)> {
    if let Some(pos) = find_subsequence(buffer, b"\r\n\r\n") {
        return Some((pos, 4));
    }
    if let Some(pos) = find_subsequence(buffer, b"\n\n") {
        return Some((pos, 2));
    }
    None
}

fn find_subsequence(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use tempfile::tempdir;

    #[test]
    fn decodes_newline_delimited_message() {
        let mut buffer = b"{\"jsonrpc\":\"2.0\",\"id\":1}\n".to_vec();
        let msg = extract_message(&mut buffer).unwrap();
        assert_eq!(msg, b"{\"jsonrpc\":\"2.0\",\"id\":1}".to_vec());
        assert!(buffer.is_empty());
    }

    #[test]
    fn decodes_content_length_message() {
        let body = b"{\"jsonrpc\":\"2.0\",\"id\":1}";
        let mut buffer = format!("Content-Length: {}\r\n\r\n", body.len()).into_bytes();
        buffer.extend_from_slice(body);
        let msg = extract_message(&mut buffer).unwrap();
        assert_eq!(msg, body.to_vec());
        assert!(buffer.is_empty());
    }

    #[tokio::test]
    #[ignore = "requires npm/npx and network access"]
    async fn discover_tools_from_real_filesystem_server() {
        let workspace = tempdir().unwrap();
        let server = McpServerConfig::from_command(
            "filesystem",
            format!(
                "npx -y @modelcontextprotocol/server-filesystem {}",
                workspace.path().display()
            ),
            Vec::new(),
            HashMap::new(),
        )
        .unwrap();

        let tools = discover_tools(&[server], workspace.path(), Duration::from_secs(45))
            .await
            .unwrap();
        assert!(!tools.is_empty());
        assert!(tools
            .iter()
            .any(|tool| tool.namespaced_name.starts_with("mcp__filesystem__")));
    }
}
