//! Tool provider trait for extensible tool sources

use crate::tools::{ToolContext, ToolError};
use async_trait::async_trait;
use serde_json::Value;
use uira_core::{ToolOutput, ToolSpec};

/// A provider of tools that can be dynamically registered
#[async_trait]
pub trait ToolProvider: Send + Sync {
    /// Get tool specifications for this provider
    fn specs(&self) -> Vec<ToolSpec>;

    /// Check if this provider handles a specific tool
    fn handles(&self, name: &str) -> bool;

    /// Execute a tool provided by this provider
    async fn execute(
        &self,
        name: &str,
        input: Value,
        ctx: &ToolContext,
    ) -> Result<ToolOutput, ToolError>;
}
