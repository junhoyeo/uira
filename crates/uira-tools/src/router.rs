//! Tool router for dispatching tool calls

use std::collections::HashMap;
use std::sync::Arc;
use uira_types::ToolOutput;

use crate::provider::ToolProvider;
use crate::{BoxedTool, Tool, ToolContext, ToolError};

/// Router for dispatching tool calls to the appropriate tool
pub struct ToolRouter {
    tools: HashMap<String, BoxedTool>,
    providers: Vec<Arc<dyn ToolProvider>>,
}

impl ToolRouter {
    pub fn new() -> Self {
        Self {
            tools: HashMap::new(),
            providers: Vec::new(),
        }
    }

    /// Register a tool
    pub fn register(&mut self, tool: impl Tool + 'static) {
        let name = tool.name().to_string();
        self.tools.insert(name, Arc::new(tool));
    }

    /// Register a boxed tool
    pub fn register_boxed(&mut self, tool: BoxedTool) {
        let name = tool.name().to_string();
        self.tools.insert(name, tool);
    }

    /// Register a tool provider
    pub fn register_provider(&mut self, provider: Arc<dyn ToolProvider>) {
        self.providers.push(provider);
    }

    /// Get a tool by name
    pub fn get(&self, name: &str) -> Option<&BoxedTool> {
        self.tools.get(name)
    }

    /// Check if a tool exists
    pub fn has(&self, name: &str) -> bool {
        self.tools.contains_key(name)
    }

    /// Check if a tool supports parallel execution
    pub fn tool_supports_parallel(&self, name: &str) -> bool {
        self.tools
            .get(name)
            .map(|t| t.supports_parallel())
            .unwrap_or(false)
    }

    /// Dispatch a tool call
    pub async fn dispatch(
        &self,
        name: &str,
        input: serde_json::Value,
        ctx: &ToolContext,
    ) -> Result<ToolOutput, ToolError> {
        // First, try direct tools
        if let Some(tool) = self.tools.get(name) {
            return tool.execute(input, ctx).await;
        }

        // Then, check providers
        for provider in &self.providers {
            if provider.handles(name) {
                return provider.execute(name, input, ctx).await;
            }
        }

        Err(ToolError::NotFound {
            name: name.to_string(),
        })
    }

    /// Get all tool names
    pub fn names(&self) -> Vec<&str> {
        let mut names: Vec<&str> = self.tools.keys().map(String::as_str).collect();
        names.sort_unstable();
        names
    }

    /// Get tool count
    pub fn len(&self) -> usize {
        self.tools.len()
    }

    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.tools.is_empty()
    }

    /// Get tool specifications for model API
    pub fn specs(&self) -> Vec<uira_types::ToolSpec> {
        let mut specs: Vec<uira_types::ToolSpec> = self
            .tools
            .values()
            .map(|t| uira_types::ToolSpec::new(t.name(), t.description(), t.schema()))
            .collect();

        // Add provider specs
        for provider in &self.providers {
            specs.extend(provider.specs());
        }

        specs
    }
}

impl Default for ToolRouter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::FunctionTool;
    use serde_json::json;
    use uira_types::JsonSchema;

    #[tokio::test]
    async fn test_router_dispatch() {
        let mut router = ToolRouter::new();

        router.register(FunctionTool::new(
            "echo",
            "Echo input",
            JsonSchema::object(),
            |input: serde_json::Value| async move { Ok(ToolOutput::text(input.to_string())) },
        ));

        assert!(router.has("echo"));
        assert!(!router.has("nonexistent"));

        let ctx = ToolContext::default();
        let result = router
            .dispatch("echo", json!({"msg": "hello"}), &ctx)
            .await
            .unwrap();
        assert!(result.as_text().unwrap().contains("hello"));
    }

    #[tokio::test]
    async fn test_router_missing_tool() {
        let router = ToolRouter::new();
        let ctx = ToolContext::default();
        let err = router
            .dispatch("missing", json!({}), &ctx)
            .await
            .unwrap_err();
        assert!(matches!(err, ToolError::NotFound { .. }));
    }
}
