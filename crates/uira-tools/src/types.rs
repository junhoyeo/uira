use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

pub type ToolInput = Value;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ToolOutput {
    pub content: Vec<ToolContent>,
}

impl ToolOutput {
    pub fn text(text: impl Into<String>) -> Self {
        Self {
            content: vec![ToolContent::Text { text: text.into() }],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum ToolContent {
    Text { text: String },
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ToolError {
    #[error("invalid input: {message}")]
    InvalidInput { message: String },

    #[error("tool already registered: {name}")]
    AlreadyRegistered { name: String },

    #[error("tool not found: {name}")]
    NotFound { name: String },

    #[error("not implemented: {name}")]
    NotImplemented { name: String },

    #[error("execution failed: {message}")]
    ExecutionFailed { message: String },

    #[error("sandbox denied: {message}")]
    SandboxDenied { message: String, retryable: bool },

    #[error("permission denied: {message}")]
    PermissionDenied { message: String },
}

impl ToolError {
    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            ToolError::SandboxDenied {
                retryable: true,
                ..
            }
        )
    }

    pub fn sandbox_denied(message: impl Into<String>) -> Self {
        ToolError::SandboxDenied {
            message: message.into(),
            retryable: true,
        }
    }

    pub fn sandbox_denied_final(message: impl Into<String>) -> Self {
        ToolError::SandboxDenied {
            message: message.into(),
            retryable: false,
        }
    }
}

pub type ToolFuture = Pin<Box<dyn Future<Output = Result<ToolOutput, ToolError>> + Send + 'static>>;

pub trait ToolHandler: Send + Sync {
    fn call(&self, input: ToolInput) -> ToolFuture;
}

impl<F, Fut> ToolHandler for F
where
    F: Fn(ToolInput) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<ToolOutput, ToolError>> + Send + 'static,
{
    fn call(&self, input: ToolInput) -> ToolFuture {
        Box::pin((self)(input))
    }
}

#[derive(Clone)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub input_schema: Value,
    pub handler: Arc<dyn ToolHandler>,
}

impl fmt::Debug for ToolDefinition {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ToolDefinition")
            .field("name", &self.name)
            .field("description", &self.description)
            .field("input_schema", &self.input_schema)
            .finish_non_exhaustive()
    }
}

impl ToolDefinition {
    pub fn new(
        name: impl Into<String>,
        description: impl Into<String>,
        input_schema: Value,
        handler: Arc<dyn ToolHandler>,
    ) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
            input_schema,
            handler,
        }
    }

    pub fn stub(
        name: impl Into<String>,
        description: impl Into<String>,
        input_schema: Value,
    ) -> Self {
        let name = name.into();
        let handler_name = name.clone();
        Self {
            name,
            description: description.into(),
            input_schema,
            handler: Arc::new(move |_input: ToolInput| {
                let handler_name = handler_name.clone();
                async move { Err(ToolError::NotImplemented { name: handler_name }) }
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn tool_output_text_serializes_as_expected() {
        let out = ToolOutput::text("hello");
        let v = serde_json::to_value(out).unwrap();
        assert_eq!(v, json!({"content": [{"type": "text", "text": "hello"}]}));
    }

    #[tokio::test]
    async fn stub_tool_returns_not_implemented() {
        let def = ToolDefinition::stub(
            "stub",
            "stub",
            json!({"type": "object", "properties": {}, "required": []}),
        );
        let err = def.handler.call(json!({})).await.unwrap_err();
        assert_eq!(
            err,
            ToolError::NotImplemented {
                name: "stub".to_string()
            }
        );
    }
}
