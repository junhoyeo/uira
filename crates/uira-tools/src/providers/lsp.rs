//! LSP tool provider - lazy initialization of LSP client

use crate::lsp::{LspClient, LspClientImpl};
use crate::provider::ToolProvider;
use crate::{ToolContext, ToolError};
use async_trait::async_trait;
use serde_json::Value;
use std::sync::Arc;
use tokio::sync::RwLock;
use uira_protocol::{JsonSchema, ToolSpec};

/// Convert from crate::types::ToolOutput to uira_protocol::ToolOutput
fn convert_tool_output(output: crate::types::ToolOutput) -> uira_protocol::ToolOutput {
    let content = output
        .content
        .into_iter()
        .map(|c| match c {
            crate::types::ToolContent::Text { text } => {
                uira_protocol::ToolOutputContent::Text { text }
            }
        })
        .collect();

    uira_protocol::ToolOutput { content }
}

/// Provider for LSP-based tools with lazy initialization
pub struct LspToolProvider {
    client: Arc<RwLock<Option<LspClientImpl>>>,
}

impl LspToolProvider {
    pub fn new() -> Self {
        Self {
            client: Arc::new(RwLock::new(None)),
        }
    }

    /// Get or initialize the LSP client
    async fn get_client(&self, ctx: &ToolContext) -> Result<LspClientImpl, ToolError> {
        let read_lock = self.client.read().await;
        if let Some(client) = read_lock.as_ref() {
            // Clone the cached client (it's cheap since it uses Arc internally)
            return Ok(client.clone());
        }
        drop(read_lock);

        // Initialize client
        let mut write_lock = self.client.write().await;
        if write_lock.is_none() {
            let client = LspClientImpl::new(ctx.cwd.clone());
            *write_lock = Some(client);
        }

        Ok(write_lock.as_ref().unwrap().clone())
    }
}

impl Default for LspToolProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ToolProvider for LspToolProvider {
    fn specs(&self) -> Vec<ToolSpec> {
        vec![
            ToolSpec::new(
                "lsp_goto_definition",
                "Jump to the definition of a symbol using LSP",
                JsonSchema::object()
                    .property(
                        "filePath",
                        JsonSchema::string().description("Path to the file"),
                    )
                    .property(
                        "line",
                        JsonSchema::number().description("Line number (0-indexed)"),
                    )
                    .property(
                        "character",
                        JsonSchema::number().description("Character position (0-indexed)"),
                    )
                    .required(&["filePath", "line", "character"]),
            ),
            ToolSpec::new(
                "lsp_find_references",
                "Find all references to a symbol using LSP",
                JsonSchema::object()
                    .property(
                        "filePath",
                        JsonSchema::string().description("Path to the file"),
                    )
                    .property(
                        "line",
                        JsonSchema::number().description("Line number (0-indexed)"),
                    )
                    .property(
                        "character",
                        JsonSchema::number().description("Character position (0-indexed)"),
                    )
                    .required(&["filePath", "line", "character"]),
            ),
            ToolSpec::new(
                "lsp_symbols",
                "List all symbols in a file or workspace using LSP",
                JsonSchema::object().property(
                    "filePath",
                    JsonSchema::string().description("Optional file path for document symbols"),
                ),
            ),
            ToolSpec::new(
                "lsp_diagnostics",
                "Get diagnostics (errors/warnings) for a file using LSP",
                JsonSchema::object()
                    .property(
                        "filePath",
                        JsonSchema::string().description("Path to the file"),
                    )
                    .required(&["filePath"]),
            ),
            ToolSpec::new(
                "lsp_hover",
                "Get hover information for a symbol using LSP",
                JsonSchema::object()
                    .property(
                        "filePath",
                        JsonSchema::string().description("Path to the file"),
                    )
                    .property(
                        "line",
                        JsonSchema::number().description("Line number (0-indexed)"),
                    )
                    .property(
                        "character",
                        JsonSchema::number().description("Character position (0-indexed)"),
                    )
                    .required(&["filePath", "line", "character"]),
            ),
            ToolSpec::new(
                "lsp_rename",
                "Rename a symbol across the codebase using LSP",
                JsonSchema::object()
                    .property(
                        "filePath",
                        JsonSchema::string().description("Path to the file"),
                    )
                    .property(
                        "line",
                        JsonSchema::number().description("Line number (0-indexed)"),
                    )
                    .property(
                        "character",
                        JsonSchema::number().description("Character position (0-indexed)"),
                    )
                    .property(
                        "newName",
                        JsonSchema::string().description("New name for the symbol"),
                    )
                    .required(&["filePath", "line", "character", "newName"]),
            ),
        ]
    }

    fn handles(&self, name: &str) -> bool {
        name.starts_with("lsp_")
    }

    async fn execute(
        &self,
        name: &str,
        input: Value,
        ctx: &ToolContext,
    ) -> Result<uira_protocol::ToolOutput, ToolError> {
        let client = self.get_client(ctx).await?;

        let result = match name {
            "lsp_goto_definition" => client.goto_definition(input).await,
            "lsp_find_references" => client.find_references(input).await,
            "lsp_symbols" => client.symbols(input).await,
            "lsp_diagnostics" => client.diagnostics(input).await,
            "lsp_hover" => client.hover(input).await,
            "lsp_rename" => client.rename(input).await,
            _ => {
                return Err(ToolError::NotFound {
                    name: name.to_string(),
                })
            }
        }?;

        // Convert from crate::types::ToolOutput to uira_protocol::ToolOutput
        Ok(convert_tool_output(result))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lsp_provider_handles() {
        let provider = LspToolProvider::new();
        assert!(provider.handles("lsp_goto_definition"));
        assert!(provider.handles("lsp_diagnostics"));
        assert!(!provider.handles("ast_search"));
        assert!(!provider.handles("read_file"));
    }

    #[test]
    fn test_lsp_provider_specs() {
        let provider = LspToolProvider::new();
        let specs = provider.specs();
        assert_eq!(specs.len(), 6);
        assert!(specs.iter().any(|s| s.name == "lsp_goto_definition"));
        assert!(specs.iter().any(|s| s.name == "lsp_find_references"));
        assert!(specs.iter().any(|s| s.name == "lsp_symbols"));
        assert!(specs.iter().any(|s| s.name == "lsp_diagnostics"));
        assert!(specs.iter().any(|s| s.name == "lsp_hover"));
        assert!(specs.iter().any(|s| s.name == "lsp_rename"));
    }
}
