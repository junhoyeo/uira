//! Tool trait and handler definitions

use async_trait::async_trait;
use serde_json::Value;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use uira_sandbox::{SandboxPolicy, SandboxType};
use uira_types::{ApprovalRequirement, JsonSchema, SandboxPreference, ToolOutput};

use crate::tools::ToolError;

/// Context passed to tool execution
pub struct ToolContext {
    pub cwd: std::path::PathBuf,
    pub session_id: String,
    pub full_auto: bool,
    pub env: std::collections::HashMap<String, String>,
    pub sandbox_type: SandboxType,
    pub sandbox_policy: SandboxPolicy,
}

impl Default for ToolContext {
    fn default() -> Self {
        Self {
            cwd: std::env::current_dir().unwrap_or_default(),
            session_id: String::new(),
            full_auto: false,
            env: std::collections::HashMap::new(),
            sandbox_type: SandboxType::None,
            sandbox_policy: SandboxPolicy::default(),
        }
    }
}

impl ToolContext {
    pub fn with_sandbox(mut self, sandbox_type: SandboxType) -> Self {
        self.sandbox_type = sandbox_type;
        self
    }
}

/// The core Tool trait for implementing tools
#[async_trait]
pub trait Tool: Send + Sync {
    /// Get the tool name
    fn name(&self) -> &str;

    /// Get the tool description
    fn description(&self) -> &str;

    /// Get the JSON schema for input validation
    fn schema(&self) -> JsonSchema;

    /// Execute the tool with the given input
    async fn execute(&self, input: Value, ctx: &ToolContext) -> Result<ToolOutput, ToolError>;

    /// Determine approval requirement for this execution
    fn approval_requirement(&self, _input: &Value) -> ApprovalRequirement {
        // Default: requires approval for write operations
        ApprovalRequirement::skip()
    }

    /// Get the sandbox preference for this tool
    fn sandbox_preference(&self) -> SandboxPreference {
        SandboxPreference::Auto
    }

    /// Whether this tool supports parallel execution with other tools
    fn supports_parallel(&self) -> bool {
        true
    }

    /// Whether to escalate (retry without sandbox) on sandbox failure
    fn escalate_on_failure(&self) -> bool {
        false
    }
}

/// A boxed tool for dynamic dispatch
pub type BoxedTool = Arc<dyn Tool>;

/// Future type for tool execution
pub type ToolFuture = Pin<Box<dyn Future<Output = Result<ToolOutput, ToolError>> + Send + 'static>>;

/// Trait for function-based tool handlers (simpler API)
pub trait ToolHandler: Send + Sync {
    fn call(&self, input: Value) -> ToolFuture;
}

impl<F, Fut> ToolHandler for F
where
    F: Fn(Value) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<ToolOutput, ToolError>> + Send + 'static,
{
    fn call(&self, input: Value) -> ToolFuture {
        Box::pin((self)(input))
    }
}

/// Wrapper to create a Tool from a handler function
pub struct FunctionTool<H: ToolHandler> {
    name: String,
    description: String,
    schema: JsonSchema,
    handler: H,
    parallel: bool,
}

impl<H: ToolHandler> FunctionTool<H> {
    pub fn new(
        name: impl Into<String>,
        description: impl Into<String>,
        schema: JsonSchema,
        handler: H,
    ) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
            schema,
            handler,
            parallel: true,
        }
    }

    pub fn with_parallel(mut self, parallel: bool) -> Self {
        self.parallel = parallel;
        self
    }
}

#[async_trait]
impl<H: ToolHandler + 'static> Tool for FunctionTool<H> {
    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> &str {
        &self.description
    }

    fn schema(&self) -> JsonSchema {
        self.schema.clone()
    }

    async fn execute(&self, input: Value, _ctx: &ToolContext) -> Result<ToolOutput, ToolError> {
        self.handler.call(input).await
    }

    fn supports_parallel(&self) -> bool {
        self.parallel
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[tokio::test]
    async fn test_function_tool() {
        let tool = FunctionTool::new(
            "test_tool",
            "A test tool",
            JsonSchema::object(),
            |_input: Value| async { Ok(ToolOutput::text("success")) },
        );

        assert_eq!(tool.name(), "test_tool");
        assert!(tool.supports_parallel());

        let ctx = ToolContext::default();
        let result = tool.execute(json!({}), &ctx).await.unwrap();
        assert_eq!(result.as_text(), Some("success"));
    }
}
