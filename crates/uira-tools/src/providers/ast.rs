//! AST tool provider - stub implementation for future tree-sitter integration

use crate::provider::ToolProvider;
use crate::{ToolContext, ToolError};
use async_trait::async_trait;
use serde_json::Value;
use uira_protocol::{JsonSchema, ToolOutput, ToolSpec};

/// Provider for AST-based code manipulation tools
pub struct AstToolProvider;

impl AstToolProvider {
    pub fn new() -> Self {
        Self
    }
}

impl Default for AstToolProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ToolProvider for AstToolProvider {
    fn specs(&self) -> Vec<ToolSpec> {
        vec![
            ToolSpec::new(
                "ast_search",
                "Search code using AST patterns (tree-sitter)",
                JsonSchema::object()
                    .property(
                        "pattern",
                        JsonSchema::string().description("Tree-sitter query pattern"),
                    )
                    .property(
                        "filePath",
                        JsonSchema::string().description("Optional file path to search in"),
                    )
                    .property(
                        "language",
                        JsonSchema::string()
                            .description("Programming language (rust, typescript, etc.)"),
                    )
                    .required(&["pattern", "language"]),
            ),
            ToolSpec::new(
                "ast_replace",
                "Replace code using AST transformations",
                JsonSchema::object()
                    .property(
                        "filePath",
                        JsonSchema::string().description("Path to the file"),
                    )
                    .property(
                        "pattern",
                        JsonSchema::string().description("Tree-sitter query pattern to match"),
                    )
                    .property(
                        "replacement",
                        JsonSchema::string().description("Replacement code"),
                    )
                    .property(
                        "language",
                        JsonSchema::string().description("Programming language"),
                    )
                    .required(&["filePath", "pattern", "replacement", "language"]),
            ),
        ]
    }

    fn handles(&self, name: &str) -> bool {
        matches!(name, "ast_search" | "ast_replace")
    }

    async fn execute(
        &self,
        name: &str,
        _input: Value,
        _ctx: &ToolContext,
    ) -> Result<ToolOutput, ToolError> {
        // Stub implementation - will be implemented with tree-sitter integration
        Err(ToolError::NotImplemented {
            name: name.to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ast_provider_handles() {
        let provider = AstToolProvider::new();
        assert!(provider.handles("ast_search"));
        assert!(provider.handles("ast_replace"));
        assert!(!provider.handles("lsp_goto_definition"));
        assert!(!provider.handles("read_file"));
    }

    #[test]
    fn test_ast_provider_specs() {
        let provider = AstToolProvider::new();
        let specs = provider.specs();
        assert_eq!(specs.len(), 2);
        assert!(specs.iter().any(|s| s.name == "ast_search"));
        assert!(specs.iter().any(|s| s.name == "ast_replace"));
    }

    #[tokio::test]
    async fn test_ast_provider_not_implemented() {
        let provider = AstToolProvider::new();
        let ctx = ToolContext::default();
        let result = provider
            .execute("ast_search", serde_json::json!({}), &ctx)
            .await;
        assert!(matches!(result, Err(ToolError::NotImplemented { .. })));
    }
}
