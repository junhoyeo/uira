use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};

use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

use crate::error::{SdkError, SdkResult};

static REQUEST_ID: AtomicU64 = AtomicU64::new(1);

fn next_request_id() -> String {
    REQUEST_ID.fetch_add(1, Ordering::SeqCst).to_string()
}

#[derive(Debug, Serialize)]
struct BridgeRequest {
    id: String,
    method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    params: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
struct BridgeResponse {
    id: String,
    #[serde(default)]
    result: Option<serde_json::Value>,
    #[serde(default)]
    error: Option<BridgeError>,
    #[serde(default)]
    stream: Option<bool>,
    #[serde(default)]
    data: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
struct BridgeError {
    #[allow(dead_code)]
    code: i32,
    message: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct QueryParams {
    pub prompt: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub options: Option<BridgeQueryOptions>,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct BridgeQueryOptions {
    #[serde(rename = "systemPrompt", skip_serializing_if = "Option::is_none")]
    pub system_prompt: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agents: Option<HashMap<String, AgentDef>>,
    #[serde(rename = "mcpServers", skip_serializing_if = "Option::is_none")]
    pub mcp_servers: Option<HashMap<String, McpServerDef>>,
    #[serde(rename = "allowedTools", skip_serializing_if = "Option::is_none")]
    pub allowed_tools: Option<Vec<String>>,
    #[serde(rename = "permissionMode", skip_serializing_if = "Option::is_none")]
    pub permission_mode: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AgentDef {
    pub description: String,
    pub prompt: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct McpServerDef {
    pub command: String,
    pub args: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub env: Option<HashMap<String, String>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct StreamMessage {
    #[serde(rename = "type")]
    pub message_type: Option<String>,
    pub content: Option<serde_json::Value>,
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

pub struct SdkBridge {
    process: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
}

impl SdkBridge {
    pub fn new() -> SdkResult<Self> {
        Self::with_bridge_path(None)
    }

    pub fn with_bridge_path(bridge_path: Option<&str>) -> SdkResult<Self> {
        let default_path = Self::find_bridge_path()?;
        let path = bridge_path.unwrap_or(&default_path);

        let mut process = Command::new("node")
            .arg(path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .map_err(|e| SdkError::Bridge(format!("Failed to spawn bridge process: {}", e)))?;

        let stdin = process
            .stdin
            .take()
            .ok_or_else(|| SdkError::Bridge("Failed to get stdin handle".to_string()))?;

        let stdout = process
            .stdout
            .take()
            .ok_or_else(|| SdkError::Bridge("Failed to get stdout handle".to_string()))?;

        Ok(Self {
            process,
            stdin,
            stdout: BufReader::new(stdout),
        })
    }

    fn find_bridge_path() -> SdkResult<String> {
        let candidates = [
            "./packages/bridge/dist/index.js",
            "../packages/bridge/dist/index.js",
            "../../packages/bridge/dist/index.js",
        ];

        for candidate in &candidates {
            if std::path::Path::new(candidate).exists() {
                return Ok(candidate.to_string());
            }
        }

        Err(SdkError::Bridge(
            "Could not find bridge. Run 'npm run build' in packages/bridge/ directory".to_string(),
        ))
    }

    pub fn ping(&mut self) -> SdkResult<bool> {
        let request = BridgeRequest {
            id: next_request_id(),
            method: "ping".to_string(),
            params: None,
        };

        self.send_request(&request)?;
        let response = self.read_response()?;

        match response.result {
            Some(result) => Ok(result
                .get("pong")
                .and_then(|v| v.as_bool())
                .unwrap_or(false)),
            None => {
                if let Some(error) = response.error {
                    Err(SdkError::Bridge(error.message))
                } else {
                    Ok(false)
                }
            }
        }
    }

    pub fn query(
        &mut self,
        params: QueryParams,
    ) -> SdkResult<mpsc::Receiver<SdkResult<StreamMessage>>> {
        let (tx, rx) = mpsc::channel(100);

        let request = BridgeRequest {
            id: next_request_id(),
            method: "query".to_string(),
            params: Some(serde_json::to_value(&params).map_err(|e| {
                SdkError::Serialization(format!("Failed to serialize params: {}", e))
            })?),
        };

        self.send_request(&request)?;

        let request_id = request.id.clone();

        loop {
            let response = self.read_response()?;

            if response.id != request_id {
                continue;
            }

            if let Some(error) = response.error {
                let _ = tx.blocking_send(Err(SdkError::Bridge(error.message)));
                break;
            }

            if response.stream == Some(true) {
                if let Some(data) = response.data {
                    let msg: StreamMessage =
                        serde_json::from_value(data).unwrap_or_else(|_| StreamMessage {
                            message_type: None,
                            content: None,
                            extra: HashMap::new(),
                        });
                    let _ = tx.blocking_send(Ok(msg));
                }
            } else if response.result.is_some() {
                break;
            }
        }

        Ok(rx)
    }

    fn send_request(&mut self, request: &BridgeRequest) -> SdkResult<()> {
        let json = serde_json::to_string(request)
            .map_err(|e| SdkError::Serialization(format!("Failed to serialize request: {}", e)))?;

        writeln!(self.stdin, "{}", json)
            .map_err(|e| SdkError::Bridge(format!("Failed to write to bridge: {}", e)))?;

        self.stdin
            .flush()
            .map_err(|e| SdkError::Bridge(format!("Failed to flush to bridge: {}", e)))?;

        Ok(())
    }

    fn read_response(&mut self) -> SdkResult<BridgeResponse> {
        let mut line = String::new();
        self.stdout
            .read_line(&mut line)
            .map_err(|e| SdkError::Bridge(format!("Failed to read from bridge: {}", e)))?;

        serde_json::from_str(&line).map_err(|e| {
            SdkError::Serialization(format!("Failed to parse response: {} (line: {})", e, line))
        })
    }
}

impl Drop for SdkBridge {
    fn drop(&mut self) {
        let _ = self.process.kill();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_next_request_id() {
        let id1 = next_request_id();
        let id2 = next_request_id();
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_query_params_serialize() {
        let params = QueryParams {
            prompt: "Hello".to_string(),
            options: Some(BridgeQueryOptions {
                system_prompt: Some("You are helpful".to_string()),
                ..Default::default()
            }),
        };

        let json = serde_json::to_string(&params).unwrap();
        assert!(json.contains("Hello"));
        assert!(json.contains("systemPrompt"));
    }
}
