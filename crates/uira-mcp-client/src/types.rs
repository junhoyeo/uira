use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct McpServerConfig {
    pub name: String,
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: HashMap<String, String>,
}

impl McpServerConfig {
    pub fn from_command(
        name: impl Into<String>,
        command_or_line: impl Into<String>,
        args: Vec<String>,
        env: HashMap<String, String>,
    ) -> Result<Self, String> {
        let name = name.into();
        let command_or_line = command_or_line.into();

        if !args.is_empty() {
            return Ok(Self {
                name,
                command: command_or_line,
                args,
                env,
            });
        }

        let mut parts = shlex::split(&command_or_line)
            .ok_or_else(|| format!("failed to parse MCP command: {command_or_line}"))?;

        if parts.is_empty() {
            return Err("MCP command is empty".to_string());
        }

        let command = parts.remove(0);
        Ok(Self {
            name,
            command,
            args: parts,
            env,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DiscoveredTool {
    pub server_name: String,
    pub original_name: String,
    pub namespaced_name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
}

impl DiscoveredTool {
    pub fn namespaced(server_name: &str, tool_name: &str) -> String {
        format!("mcp__{server_name}__{tool_name}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_command_line_when_args_are_empty() {
        let cfg = McpServerConfig::from_command(
            "filesystem",
            "npx -y @anthropic/mcp-server-filesystem /tmp",
            Vec::new(),
            HashMap::new(),
        )
        .unwrap();

        assert_eq!(cfg.command, "npx");
        assert_eq!(
            cfg.args,
            vec![
                "-y".to_string(),
                "@anthropic/mcp-server-filesystem".to_string(),
                "/tmp".to_string()
            ]
        );
    }
}
