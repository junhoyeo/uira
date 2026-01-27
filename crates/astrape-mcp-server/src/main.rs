use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::io::{self, BufRead, Write};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;

mod anthropic_client;
mod auth;
mod opencode_client;
mod opencode_server;
mod router;
mod tools;

use tools::ToolExecutor;

#[derive(Debug, Serialize)]
struct JsonRpcResponse {
    jsonrpc: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    id: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<JsonRpcError>,
}

#[derive(Debug, Serialize)]
struct JsonRpcError {
    code: i64,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<Value>,
}

#[derive(Debug, Deserialize)]
struct JsonRpcRequest {
    #[allow(dead_code)]
    jsonrpc: String,
    id: Option<Value>,
    method: String,
    params: Option<Value>,
}

struct McpServer {
    executor: Arc<RwLock<ToolExecutor>>,
}

impl McpServer {
    fn new(root_path: PathBuf) -> Self {
        Self {
            executor: Arc::new(RwLock::new(ToolExecutor::new(root_path))),
        }
    }

    fn tool_definitions(&self) -> Vec<Value> {
        vec![
            // LSP Tools
            json!({
                "name": "lsp_goto_definition",
                "description": "Find the definition location of a symbol. Supports TypeScript, Rust, Python, Go, C/C++, Java.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "filePath": {"type": "string", "description": "Absolute path to the source file"},
                        "line": {"type": "integer", "minimum": 1, "description": "Line number (1-indexed)"},
                        "character": {"type": "integer", "minimum": 0, "description": "Column position (0-indexed)"}
                    },
                    "required": ["filePath", "line", "character"]
                }
            }),
            json!({
                "name": "lsp_find_references",
                "description": "Find all references to a symbol across the workspace.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "filePath": {"type": "string", "description": "Absolute path to the source file"},
                        "line": {"type": "integer", "minimum": 1},
                        "character": {"type": "integer", "minimum": 0},
                        "includeDeclaration": {"type": "boolean", "default": true}
                    },
                    "required": ["filePath", "line", "character"]
                }
            }),
            json!({
                "name": "lsp_symbols",
                "description": "Get symbols from a file (document symbols) or search across workspace.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "filePath": {"type": "string", "description": "Path to a file in the workspace"},
                        "scope": {"type": "string", "enum": ["document", "workspace"], "default": "document"},
                        "query": {"type": "string", "description": "Search query for workspace symbols"},
                        "limit": {"type": "integer", "minimum": 1, "default": 50}
                    },
                    "required": ["filePath"]
                }
            }),
            json!({
                "name": "lsp_diagnostics",
                "description": "Get language server diagnostics (errors, warnings) for a file.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "filePath": {"type": "string"},
                        "severity": {"type": "string", "enum": ["error", "warning", "information", "hint", "all"], "default": "all"}
                    },
                    "required": ["filePath"]
                }
            }),
            json!({
                "name": "lsp_hover",
                "description": "Get type information and documentation for a symbol at a position.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "filePath": {"type": "string"},
                        "line": {"type": "integer", "minimum": 1},
                        "character": {"type": "integer", "minimum": 0}
                    },
                    "required": ["filePath", "line", "character"]
                }
            }),
            json!({
                "name": "lsp_rename",
                "description": "Rename a symbol across the workspace.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "filePath": {"type": "string"},
                        "line": {"type": "integer", "minimum": 1},
                        "character": {"type": "integer", "minimum": 0},
                        "newName": {"type": "string", "minLength": 1}
                    },
                    "required": ["filePath", "line", "character", "newName"]
                }
            }),
            // AST-grep Tools
            json!({
                "name": "ast_search",
                "description": "Search for code patterns using AST matching. Uses ast-grep syntax with meta-variables ($VAR, $$$ARGS).",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "pattern": {"type": "string", "description": "AST pattern to search for (e.g., 'console.log($MSG)')"},
                        "lang": {"type": "string", "description": "Language: javascript, typescript, python, rust, go, etc."},
                        "paths": {"type": "array", "items": {"type": "string"}, "description": "Paths to search in"},
                        "globs": {"type": "array", "items": {"type": "string"}, "description": "Glob patterns (e.g., '**/*.ts')"}
                    },
                    "required": ["pattern", "lang"]
                }
            }),
            json!({
                "name": "ast_replace",
                "description": "Replace code patterns using AST matching. Preserves structure and formatting.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "pattern": {"type": "string", "description": "AST pattern to match"},
                        "rewrite": {"type": "string", "description": "Replacement pattern (can use captured meta-variables)"},
                        "lang": {"type": "string"},
                        "paths": {"type": "array", "items": {"type": "string"}},
                        "globs": {"type": "array", "items": {"type": "string"}},
                        "dryRun": {"type": "boolean", "default": true, "description": "Preview changes without applying"}
                    },
                    "required": ["pattern", "rewrite", "lang"]
                }
            }),
            // Agent Delegation Tool - multi-provider model routing
            json!({
                "name": "delegate_task",
                "description": "Delegate a task to a specialized agent with multi-provider model routing via OpenCode. \
                    Routes to the configured model for that agent (e.g., librarian -> opencode/big-pickle). \
                    Returns the agent's response.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "agent": {
                            "type": "string",
                            "description": "Agent name (e.g., 'librarian', 'explore', 'architect'). Must match an agent configured in astrape.yml"
                        },
                        "prompt": {
                            "type": "string",
                            "description": "The task/prompt for the agent to execute"
                        },
                        "model": {
                            "type": "string",
                            "description": "Override model (sonnet, opus, haiku). If not specified, uses the agent's configured default"
                        },
                        "allowedTools": {
                            "type": "array",
                            "items": {"type": "string"},
                            "description": "List of tools to allow (e.g., ['Read', 'Glob', 'Grep']). Defaults to agent's configured tools"
                        },
                        "maxTurns": {
                            "type": "integer",
                            "minimum": 1,
                            "description": "Maximum number of turns before stopping. Default: 10"
                        },
                        "runInBackground": {
                            "type": "boolean",
                            "default": false,
                            "description": "If true, runs the agent in the background and returns a task_id immediately. Use background_output to get results."
                        }
                    },
                    "required": ["agent", "prompt"]
                }
            }),
            // Background Task Tools
            json!({
                "name": "background_output",
                "description": "Get the output from a background task. Returns immediately if complete, otherwise shows current status.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "taskId": {
                            "type": "string",
                            "description": "The task ID returned from delegate_task with runInBackground=true"
                        },
                        "block": {
                            "type": "boolean",
                            "default": false,
                            "description": "If true, blocks until the task completes (max 120s timeout)"
                        },
                        "timeout": {
                            "type": "integer",
                            "minimum": 1,
                            "description": "Timeout in seconds when blocking. Default: 120"
                        }
                    },
                    "required": ["taskId"]
                }
            }),
            json!({
                "name": "background_cancel",
                "description": "Cancel a running background task or all background tasks.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "taskId": {
                            "type": "string",
                            "description": "The task ID to cancel"
                        },
                        "all": {
                            "type": "boolean",
                            "default": false,
                            "description": "If true, cancels ALL running background tasks"
                        }
                    }
                }
            }),
        ]
    }

    async fn handle_request(&self, request: JsonRpcRequest) -> JsonRpcResponse {
        match request.method.as_str() {
            "initialize" => self.handle_initialize(request.id).await,
            "notifications/initialized" => {
                // No response needed for notifications
                JsonRpcResponse {
                    jsonrpc: "2.0",
                    id: None,
                    result: None,
                    error: None,
                }
            }
            "tools/list" => self.handle_tools_list(request.id),
            "tools/call" => self.handle_tools_call(request.id, request.params).await,
            "resources/list" => self.handle_resources_list(request.id),
            "prompts/list" => self.handle_prompts_list(request.id),
            _ => JsonRpcResponse {
                jsonrpc: "2.0",
                id: request.id,
                result: None,
                error: Some(JsonRpcError {
                    code: -32601,
                    message: format!("Method not found: {}", request.method),
                    data: None,
                }),
            },
        }
    }

    async fn handle_initialize(&self, id: Option<Value>) -> JsonRpcResponse {
        JsonRpcResponse {
            jsonrpc: "2.0",
            id,
            result: Some(json!({
                "protocolVersion": "2024-11-05",
                "capabilities": {
                    "tools": {}
                },
                "serverInfo": {
                    "name": "astrape-mcp",
                    "version": env!("CARGO_PKG_VERSION")
                }
            })),
            error: None,
        }
    }

    fn handle_tools_list(&self, id: Option<Value>) -> JsonRpcResponse {
        JsonRpcResponse {
            jsonrpc: "2.0",
            id,
            result: Some(json!({
                "tools": self.tool_definitions()
            })),
            error: None,
        }
    }

    async fn handle_tools_call(&self, id: Option<Value>, params: Option<Value>) -> JsonRpcResponse {
        let params = match params {
            Some(p) => p,
            None => {
                return JsonRpcResponse {
                    jsonrpc: "2.0",
                    id,
                    result: None,
                    error: Some(JsonRpcError {
                        code: -32602,
                        message: "Missing params".to_string(),
                        data: None,
                    }),
                };
            }
        };

        let tool_name = params.get("name").and_then(|v| v.as_str()).unwrap_or("");
        let arguments = params.get("arguments").cloned().unwrap_or(json!({}));

        let executor = self.executor.read().await;
        match executor.execute(tool_name, arguments).await {
            Ok(result) => JsonRpcResponse {
                jsonrpc: "2.0",
                id,
                result: Some(json!({
                    "content": [{
                        "type": "text",
                        "text": result
                    }]
                })),
                error: None,
            },
            Err(e) => JsonRpcResponse {
                jsonrpc: "2.0",
                id,
                result: Some(json!({
                    "content": [{
                        "type": "text",
                        "text": e
                    }],
                    "isError": true
                })),
                error: None,
            },
        }
    }

    fn handle_resources_list(&self, id: Option<Value>) -> JsonRpcResponse {
        JsonRpcResponse {
            jsonrpc: "2.0",
            id,
            result: Some(json!({
                "resources": []
            })),
            error: None,
        }
    }

    fn handle_prompts_list(&self, id: Option<Value>) -> JsonRpcResponse {
        JsonRpcResponse {
            jsonrpc: "2.0",
            id,
            result: Some(json!({
                "prompts": []
            })),
            error: None,
        }
    }
}

#[tokio::main]
async fn main() {
    // Initialize tracing to stderr (stdout is for JSON-RPC)
    tracing_subscriber::fmt()
        .with_writer(io::stderr)
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(tracing::Level::WARN.into()),
        )
        .init();

    let root_path = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let server = McpServer::new(root_path);

    let stdin = io::stdin();
    let mut stdout = io::stdout();

    for line in stdin.lock().lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => break,
        };

        if line.trim().is_empty() {
            continue;
        }

        let request: JsonRpcRequest = match serde_json::from_str(&line) {
            Ok(r) => r,
            Err(e) => {
                let error_response = JsonRpcResponse {
                    jsonrpc: "2.0",
                    id: None,
                    result: None,
                    error: Some(JsonRpcError {
                        code: -32700,
                        message: format!("Parse error: {}", e),
                        data: None,
                    }),
                };
                let _ = writeln!(
                    stdout,
                    "{}",
                    serde_json::to_string(&error_response).unwrap()
                );
                let _ = stdout.flush();
                continue;
            }
        };

        let response = server.handle_request(request).await;

        // Only send response if it has an id (not a notification)
        if response.id.is_some() || response.error.is_some() {
            let _ = writeln!(stdout, "{}", serde_json::to_string(&response).unwrap());
            let _ = stdout.flush();
        }
    }
}
