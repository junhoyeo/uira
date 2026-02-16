use crate::tools::provider::ToolProvider;
use crate::tools::{ToolContext, ToolError};
use async_trait::async_trait;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use uira_core::schema::NamedMcpServerConfig;
use uira_mcp_client::{McpRuntimeManager, McpServerConfig};
use uira_core::{ToolOutput, ToolSpec};

#[derive(Debug, Clone)]
struct ToolRoute {
    server_name: String,
    tool_name: String,
}

pub struct McpToolProvider {
    runtime: McpRuntimeManager,
    specs: Vec<ToolSpec>,
    routes: Arc<RwLock<HashMap<String, ToolRoute>>>,
}

impl McpToolProvider {
    pub fn new(
        servers: Vec<NamedMcpServerConfig>,
        specs: Vec<ToolSpec>,
        default_cwd: std::path::PathBuf,
    ) -> Result<Self, ToolError> {
        let runtime_configs = servers
            .into_iter()
            .map(|server| {
                McpServerConfig::from_command(
                    server.name,
                    server.config.command,
                    server.config.args,
                    server.config.env,
                )
                .map_err(|message| ToolError::InvalidInput { message })
            })
            .collect::<Result<Vec<_>, _>>()?;

        let routes = specs
            .iter()
            .filter_map(|spec| {
                parse_namespaced_tool_name(&spec.name).map(|route| (spec.name.clone(), route))
            })
            .collect::<HashMap<_, _>>();

        Ok(Self {
            runtime: McpRuntimeManager::new(runtime_configs, default_cwd)
                .with_rpc_timeout(Duration::from_secs(20)),
            specs,
            routes: Arc::new(RwLock::new(routes)),
        })
    }
}

#[async_trait]
impl ToolProvider for McpToolProvider {
    fn specs(&self) -> Vec<ToolSpec> {
        self.specs.clone()
    }

    fn handles(&self, name: &str) -> bool {
        name.starts_with("mcp__")
    }

    async fn execute(
        &self,
        name: &str,
        input: Value,
        ctx: &ToolContext,
    ) -> Result<ToolOutput, ToolError> {
        let route =
            self.routes
                .read()
                .await
                .get(name)
                .cloned()
                .ok_or_else(|| ToolError::NotFound {
                    name: name.to_string(),
                })?;

        let result = self
            .runtime
            .call_tool(&route.server_name, &route.tool_name, input, &ctx.cwd)
            .await
            .map_err(|e| ToolError::ExecutionFailed {
                message: e.to_string(),
            })?;

        let content = result
            .get("content")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();

        let is_error = result
            .get("isError")
            .and_then(Value::as_bool)
            .unwrap_or(false);

        let text = content
            .iter()
            .filter_map(|entry| {
                if entry.get("type").and_then(Value::as_str) == Some("text") {
                    entry
                        .get("text")
                        .and_then(Value::as_str)
                        .map(|v| v.to_string())
                } else {
                    None
                }
            })
            .collect::<Vec<_>>()
            .join("\n");

        if is_error {
            return Err(ToolError::ExecutionFailed { message: text });
        }

        if text.is_empty() {
            Ok(ToolOutput::json(result))
        } else {
            Ok(ToolOutput::text(text))
        }
    }
}

fn parse_namespaced_tool_name(name: &str) -> Option<ToolRoute> {
    let without_prefix = name.strip_prefix("mcp__")?;
    let (server_name, tool_name) = without_prefix.split_once("__")?;
    Some(ToolRoute {
        server_name: server_name.to_string(),
        tool_name: tool_name.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_namespaced_name() {
        let route = parse_namespaced_tool_name("mcp__filesystem__read_file").unwrap();
        assert_eq!(route.server_name, "filesystem");
        assert_eq!(route.tool_name, "read_file");
    }
}
