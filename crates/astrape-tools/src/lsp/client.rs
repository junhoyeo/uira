use crate::types::{ToolError, ToolOutput};
use serde_json::Value;
use std::future::Future;
use std::pin::Pin;

pub type LspResultFuture = Pin<Box<dyn Future<Output = Result<ToolOutput, ToolError>> + Send + 'static>>;

pub trait LspClient: Send + Sync {
    fn goto_definition(&self, _params: Value) -> LspResultFuture {
        Box::pin(async { Err(ToolError::ExecutionFailed { message: "LSP client not configured".to_string() }) })
    }

    fn find_references(&self, _params: Value) -> LspResultFuture {
        Box::pin(async { Err(ToolError::ExecutionFailed { message: "LSP client not configured".to_string() }) })
    }

    fn symbols(&self, _params: Value) -> LspResultFuture {
        Box::pin(async { Err(ToolError::ExecutionFailed { message: "LSP client not configured".to_string() }) })
    }

    fn diagnostics(&self, _params: Value) -> LspResultFuture {
        Box::pin(async { Err(ToolError::ExecutionFailed { message: "LSP client not configured".to_string() }) })
    }

    fn prepare_rename(&self, _params: Value) -> LspResultFuture {
        Box::pin(async { Err(ToolError::ExecutionFailed { message: "LSP client not configured".to_string() }) })
    }

    fn rename(&self, _params: Value) -> LspResultFuture {
        Box::pin(async { Err(ToolError::ExecutionFailed { message: "LSP client not configured".to_string() }) })
    }
}
