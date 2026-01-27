use crate::types::{ToolError, ToolOutput};
use async_trait::async_trait;
use lsp_types::*;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::{Mutex, RwLock};

/// LSP client that communicates with language servers
pub struct LspClientImpl {
    servers: Arc<RwLock<HashMap<String, Arc<Mutex<ServerProcess>>>>>,
    root_path: PathBuf,
}

struct ServerProcess {
    child: Child,
    _next_id: i64,
}

impl LspClientImpl {
    pub fn new(root_path: PathBuf) -> Self {
        Self {
            servers: Arc::new(RwLock::new(HashMap::new())),
            root_path,
        }
    }

    async fn get_or_start_server(
        &self,
        language: &str,
    ) -> Result<Arc<Mutex<ServerProcess>>, ToolError> {
        let servers = self.servers.read().await;
        if let Some(server) = servers.get(language) {
            return Ok(Arc::clone(server));
        }
        drop(servers);

        // Start new server
        let server_config = super::servers::get_server_config(language).ok_or_else(|| {
            ToolError::ExecutionFailed {
                message: format!("No LSP server configured for language: {}", language),
            }
        })?;

        let mut cmd = Command::new(&server_config.command);
        cmd.stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());

        let child = cmd.spawn().map_err(|e| ToolError::ExecutionFailed {
            message: format!(
                "Failed to start LSP server: {}. {}",
                e, server_config.install_hint
            ),
        })?;

        let process = Arc::new(Mutex::new(ServerProcess { child, _next_id: 1 }));

        // Send initialize request
        self.send_initialize(&process).await?;

        let mut servers = self.servers.write().await;
        servers.insert(language.to_string(), Arc::clone(&process));

        Ok(process)
    }

    async fn send_initialize(&self, process: &Arc<Mutex<ServerProcess>>) -> Result<(), ToolError> {
        #[allow(deprecated)]
        let init_params = InitializeParams {
            process_id: Some(std::process::id()),
            root_uri: Some(Url::from_file_path(&self.root_path).unwrap()),
            capabilities: ClientCapabilities::default(),
            ..Default::default()
        };

        let request = json!({
            "jsonrpc": "2.0",
            "id": 0,
            "method": "initialize",
            "params": init_params,
        });

        self.send_request(process, request).await?;

        // Send initialized notification
        let notification = json!({
            "jsonrpc": "2.0",
            "method": "initialized",
            "params": {},
        });

        self.send_notification(process, notification).await?;

        Ok(())
    }

    async fn send_request(
        &self,
        process: &Arc<Mutex<ServerProcess>>,
        request: Value,
    ) -> Result<Value, ToolError> {
        let mut proc = process.lock().await;

        let content = serde_json::to_string(&request).unwrap();
        let message = format!("Content-Length: {}\r\n\r\n{}", content.len(), content);

        if let Some(stdin) = proc.child.stdin.as_mut() {
            stdin
                .write_all(message.as_bytes())
                .await
                .map_err(|e| ToolError::ExecutionFailed {
                    message: format!("Failed to write to LSP server: {}", e),
                })?;
        }

        // Read response
        if let Some(stdout) = proc.child.stdout.as_mut() {
            let mut reader = BufReader::new(stdout);
            let mut header = String::new();

            // Read Content-Length header
            reader
                .read_line(&mut header)
                .await
                .map_err(|e| ToolError::ExecutionFailed {
                    message: format!("Failed to read LSP response: {}", e),
                })?;

            let content_length = header
                .trim()
                .strip_prefix("Content-Length: ")
                .and_then(|s| s.parse::<usize>().ok())
                .ok_or_else(|| ToolError::ExecutionFailed {
                    message: "Invalid Content-Length header".to_string(),
                })?;

            // Skip empty line
            let mut empty = String::new();
            reader.read_line(&mut empty).await.ok();

            // Read content
            let mut buffer = vec![0; content_length];
            tokio::io::AsyncReadExt::read_exact(&mut reader, &mut buffer)
                .await
                .map_err(|e| ToolError::ExecutionFailed {
                    message: format!("Failed to read LSP content: {}", e),
                })?;

            let response: Value =
                serde_json::from_slice(&buffer).map_err(|e| ToolError::ExecutionFailed {
                    message: format!("Failed to parse LSP response: {}", e),
                })?;

            Ok(response)
        } else {
            Err(ToolError::ExecutionFailed {
                message: "LSP server stdout not available".to_string(),
            })
        }
    }

    async fn send_notification(
        &self,
        process: &Arc<Mutex<ServerProcess>>,
        notification: Value,
    ) -> Result<(), ToolError> {
        let mut proc = process.lock().await;

        let content = serde_json::to_string(&notification).unwrap();
        let message = format!("Content-Length: {}\r\n\r\n{}", content.len(), content);

        if let Some(stdin) = proc.child.stdin.as_mut() {
            stdin
                .write_all(message.as_bytes())
                .await
                .map_err(|e| ToolError::ExecutionFailed {
                    message: format!("Failed to write notification: {}", e),
                })?;
        }

        Ok(())
    }

    fn detect_language(&self, file_path: &str) -> Option<String> {
        let extension = std::path::Path::new(file_path)
            .extension()
            .and_then(|s| s.to_str())?;

        match extension {
            "ts" | "tsx" | "js" | "jsx" => Some("typescript".to_string()),
            "rs" => Some("rust".to_string()),
            "py" => Some("python".to_string()),
            "go" => Some("go".to_string()),
            "java" => Some("java".to_string()),
            _ => None,
        }
    }
}

#[async_trait]
pub trait LspClient: Send + Sync {
    async fn goto_definition(&self, params: Value) -> Result<ToolOutput, ToolError>;
    async fn find_references(&self, params: Value) -> Result<ToolOutput, ToolError>;
    async fn symbols(&self, params: Value) -> Result<ToolOutput, ToolError>;
    async fn diagnostics(&self, params: Value) -> Result<ToolOutput, ToolError>;
    async fn prepare_rename(&self, params: Value) -> Result<ToolOutput, ToolError>;
    async fn rename(&self, params: Value) -> Result<ToolOutput, ToolError>;
    async fn hover(&self, params: Value) -> Result<ToolOutput, ToolError>;
}

#[async_trait]
impl LspClient for LspClientImpl {
    async fn goto_definition(&self, params: Value) -> Result<ToolOutput, ToolError> {
        let file_path = params["filePath"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidInput {
                message: "Missing filePath parameter".to_string(),
            })?;

        let line = params["line"]
            .as_u64()
            .ok_or_else(|| ToolError::InvalidInput {
                message: "Missing line parameter".to_string(),
            })? as u32;

        let character = params["character"]
            .as_u64()
            .ok_or_else(|| ToolError::InvalidInput {
                message: "Missing character parameter".to_string(),
            })? as u32;

        let language =
            self.detect_language(file_path)
                .ok_or_else(|| ToolError::ExecutionFailed {
                    message: "Could not detect language from file extension".to_string(),
                })?;

        let server = self.get_or_start_server(&language).await?;

        let position = super::utils::to_lsp_position(line, character);
        let request = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "textDocument/definition",
            "params": {
                "textDocument": {
                    "uri": format!("file://{}", file_path),
                },
                "position": {
                    "line": position.0,
                    "character": position.1,
                },
            },
        });

        let response = self.send_request(&server, request).await?;

        Ok(ToolOutput::text(
            serde_json::to_string_pretty(&response["result"]).unwrap(),
        ))
    }

    async fn find_references(&self, params: Value) -> Result<ToolOutput, ToolError> {
        let file_path = params["filePath"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidInput {
                message: "Missing filePath parameter".to_string(),
            })?;

        let line = params["line"]
            .as_u64()
            .ok_or_else(|| ToolError::InvalidInput {
                message: "Missing line parameter".to_string(),
            })? as u32;

        let character = params["character"]
            .as_u64()
            .ok_or_else(|| ToolError::InvalidInput {
                message: "Missing character parameter".to_string(),
            })? as u32;

        let include_declaration = params["includeDeclaration"].as_bool().unwrap_or(true);

        let language =
            self.detect_language(file_path)
                .ok_or_else(|| ToolError::ExecutionFailed {
                    message: "Could not detect language from file extension".to_string(),
                })?;

        let server = self.get_or_start_server(&language).await?;

        let position = super::utils::to_lsp_position(line, character);
        let request = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "textDocument/references",
            "params": {
                "textDocument": {
                    "uri": format!("file://{}", file_path),
                },
                "position": {
                    "line": position.0,
                    "character": position.1,
                },
                "context": {
                    "includeDeclaration": include_declaration,
                },
            },
        });

        let response = self.send_request(&server, request).await?;

        Ok(ToolOutput::text(
            serde_json::to_string_pretty(&response["result"]).unwrap(),
        ))
    }

    async fn symbols(&self, params: Value) -> Result<ToolOutput, ToolError> {
        let file_path = params["filePath"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidInput {
                message: "Missing filePath parameter".to_string(),
            })?;

        let scope = params["scope"].as_str().unwrap_or("document");

        let language =
            self.detect_language(file_path)
                .ok_or_else(|| ToolError::ExecutionFailed {
                    message: "Could not detect language from file extension".to_string(),
                })?;

        let server = self.get_or_start_server(&language).await?;

        let (method, request_params) = if scope == "workspace" {
            let query = params["query"].as_str().unwrap_or("");
            (
                "workspace/symbol",
                json!({
                    "query": query,
                }),
            )
        } else {
            (
                "textDocument/documentSymbol",
                json!({
                    "textDocument": {
                        "uri": format!("file://{}", file_path),
                    },
                }),
            )
        };

        let request = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": method,
            "params": request_params,
        });

        let response = self.send_request(&server, request).await?;

        Ok(ToolOutput::text(
            serde_json::to_string_pretty(&response["result"]).unwrap(),
        ))
    }

    async fn diagnostics(&self, params: Value) -> Result<ToolOutput, ToolError> {
        let file_path = params["filePath"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidInput {
                message: "Missing filePath parameter".to_string(),
            })?;

        let language =
            self.detect_language(file_path)
                .ok_or_else(|| ToolError::ExecutionFailed {
                    message: "Could not detect language from file extension".to_string(),
                })?;

        let server = self.get_or_start_server(&language).await?;

        // Open document to get diagnostics
        let notification = json!({
            "jsonrpc": "2.0",
            "method": "textDocument/didOpen",
            "params": {
                "textDocument": {
                    "uri": format!("file://{}", file_path),
                    "languageId": language,
                    "version": 1,
                    "text": tokio::fs::read_to_string(file_path).await.unwrap_or_default(),
                },
            },
        });

        self.send_notification(&server, notification).await?;

        // Note: In a real implementation, we'd wait for publishDiagnostics notifications
        // For now, returning a placeholder
        Ok(ToolOutput::text(
            "Diagnostics requested. Language server will publish diagnostics asynchronously.",
        ))
    }

    async fn prepare_rename(&self, params: Value) -> Result<ToolOutput, ToolError> {
        let file_path = params["filePath"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidInput {
                message: "Missing filePath parameter".to_string(),
            })?;

        let line = params["line"]
            .as_u64()
            .ok_or_else(|| ToolError::InvalidInput {
                message: "Missing line parameter".to_string(),
            })? as u32;

        let character = params["character"]
            .as_u64()
            .ok_or_else(|| ToolError::InvalidInput {
                message: "Missing character parameter".to_string(),
            })? as u32;

        let language =
            self.detect_language(file_path)
                .ok_or_else(|| ToolError::ExecutionFailed {
                    message: "Could not detect language from file extension".to_string(),
                })?;

        let server = self.get_or_start_server(&language).await?;

        let position = super::utils::to_lsp_position(line, character);
        let request = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "textDocument/prepareRename",
            "params": {
                "textDocument": {
                    "uri": format!("file://{}", file_path),
                },
                "position": {
                    "line": position.0,
                    "character": position.1,
                },
            },
        });

        let response = self.send_request(&server, request).await?;

        Ok(ToolOutput::text(
            serde_json::to_string_pretty(&response["result"]).unwrap(),
        ))
    }

    async fn rename(&self, params: Value) -> Result<ToolOutput, ToolError> {
        let file_path = params["filePath"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidInput {
                message: "Missing filePath parameter".to_string(),
            })?;

        let line = params["line"]
            .as_u64()
            .ok_or_else(|| ToolError::InvalidInput {
                message: "Missing line parameter".to_string(),
            })? as u32;

        let character = params["character"]
            .as_u64()
            .ok_or_else(|| ToolError::InvalidInput {
                message: "Missing character parameter".to_string(),
            })? as u32;

        let new_name = params["newName"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidInput {
                message: "Missing newName parameter".to_string(),
            })?;

        let language =
            self.detect_language(file_path)
                .ok_or_else(|| ToolError::ExecutionFailed {
                    message: "Could not detect language from file extension".to_string(),
                })?;

        let server = self.get_or_start_server(&language).await?;

        let position = super::utils::to_lsp_position(line, character);
        let request = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "textDocument/rename",
            "params": {
                "textDocument": {
                    "uri": format!("file://{}", file_path),
                },
                "position": {
                    "line": position.0,
                    "character": position.1,
                },
                "newName": new_name,
            },
        });

        let response = self.send_request(&server, request).await?;

        Ok(ToolOutput::text(
            serde_json::to_string_pretty(&response["result"]).unwrap(),
        ))
    }

    async fn hover(&self, params: Value) -> Result<ToolOutput, ToolError> {
        let file_path = params["filePath"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidInput {
                message: "Missing filePath parameter".to_string(),
            })?;

        let line = params["line"]
            .as_u64()
            .ok_or_else(|| ToolError::InvalidInput {
                message: "Missing line parameter".to_string(),
            })? as u32;

        let character = params["character"]
            .as_u64()
            .ok_or_else(|| ToolError::InvalidInput {
                message: "Missing character parameter".to_string(),
            })? as u32;

        let language =
            self.detect_language(file_path)
                .ok_or_else(|| ToolError::ExecutionFailed {
                    message: "Could not detect language from file extension".to_string(),
                })?;

        let server = self.get_or_start_server(&language).await?;

        let position = super::utils::to_lsp_position(line, character);
        let request = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "textDocument/hover",
            "params": {
                "textDocument": {
                    "uri": format!("file://{}", file_path),
                },
                "position": {
                    "line": position.0,
                    "character": position.1,
                },
            },
        });

        let response = self.send_request(&server, request).await?;

        Ok(ToolOutput::text(
            serde_json::to_string_pretty(&response["result"]).unwrap(),
        ))
    }
}
