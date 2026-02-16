//! LSP tool provider - lazy initialization of LSP client

use crate::tools::lsp::{LspClient, LspClientImpl};
use crate::tools::provider::ToolProvider;
use crate::tools::{ToolContext, ToolError};
use async_trait::async_trait;
use serde_json::Value;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;
use uira_core::{JsonSchema, ToolSpec};

/// Convert from crate::tools::types::ToolOutput to uira_core::ToolOutput
fn convert_tool_output(output: crate::tools::types::ToolOutput) -> uira_core::ToolOutput {
    let content = output
        .content
        .into_iter()
        .map(|c| match c {
            crate::tools::types::ToolContent::Text { text } => {
                uira_core::ToolOutputContent::Text { text }
            }
        })
        .collect();

    uira_core::ToolOutput { content }
}

/// Provider for LSP-based tools with lazy initialization
pub struct LspToolProvider {
    client: Arc<RwLock<Option<(PathBuf, LspClientImpl)>>>,
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
        if let Some((cwd, client)) = read_lock.as_ref() {
            if cwd == &ctx.cwd {
                // Clone the cached client (it's cheap since it uses Arc internally)
                return Ok(client.clone());
            }
        }
        drop(read_lock);

        // Initialize client
        let mut write_lock = self.client.write().await;
        if let Some((cwd, client)) = write_lock.as_ref() {
            if cwd == &ctx.cwd {
                return Ok(client.clone());
            }
        }

        let client = LspClientImpl::new(ctx.cwd.clone());
        *write_lock = Some((ctx.cwd.clone(), client.clone()));

        Ok(client)
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
                        JsonSchema::number().description("Line number (1-indexed)"),
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
                        JsonSchema::number().description("Line number (1-indexed)"),
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
                JsonSchema::object()
                    .property(
                        "filePath",
                        JsonSchema::string().description("Path to the file"),
                    )
                    .property(
                        "scope",
                        JsonSchema::string().description("document or workspace"),
                    )
                    .property(
                        "query",
                        JsonSchema::string().description("Query for workspace symbol search"),
                    )
                    .required(&["filePath"]),
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
                        JsonSchema::number().description("Line number (1-indexed)"),
                    )
                    .property(
                        "character",
                        JsonSchema::number().description("Character position (0-indexed)"),
                    )
                    .required(&["filePath", "line", "character"]),
            ),
            ToolSpec::new(
                "lsp_prepare_rename",
                "Check if rename is valid using LSP prepareRename",
                JsonSchema::object()
                    .property(
                        "filePath",
                        JsonSchema::string().description("Path to the file"),
                    )
                    .property(
                        "line",
                        JsonSchema::number().description("Line number (1-indexed)"),
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
                        JsonSchema::number().description("Line number (1-indexed)"),
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
        matches!(
            name,
            "lsp_goto_definition"
                | "lsp_find_references"
                | "lsp_symbols"
                | "lsp_diagnostics"
                | "lsp_hover"
                | "lsp_prepare_rename"
                | "lsp_rename"
        )
    }

    async fn execute(
        &self,
        name: &str,
        input: Value,
        ctx: &ToolContext,
    ) -> Result<uira_core::ToolOutput, ToolError> {
        let client = self.get_client(ctx).await?;

        let result = match name {
            "lsp_goto_definition" => client.goto_definition(input).await,
            "lsp_find_references" => client.find_references(input).await,
            "lsp_symbols" => client.symbols(input).await,
            "lsp_diagnostics" => client.diagnostics(input).await,
            "lsp_hover" => client.hover(input).await,
            "lsp_prepare_rename" => client.prepare_rename(input).await,
            "lsp_rename" => client.rename(input).await,
            _ => {
                return Err(ToolError::NotFound {
                    name: name.to_string(),
                })
            }
        }?;

        // Convert from crate::tools::types::ToolOutput to uira_core::ToolOutput
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
        assert_eq!(specs.len(), 7);
        assert!(specs.iter().any(|s| s.name == "lsp_goto_definition"));
        assert!(specs.iter().any(|s| s.name == "lsp_find_references"));
        assert!(specs.iter().any(|s| s.name == "lsp_symbols"));
        assert!(specs.iter().any(|s| s.name == "lsp_diagnostics"));
        assert!(specs.iter().any(|s| s.name == "lsp_hover"));
        assert!(specs.iter().any(|s| s.name == "lsp_prepare_rename"));
        assert!(specs.iter().any(|s| s.name == "lsp_rename"));
    }
}
