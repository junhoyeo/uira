//! Model client traits

use async_trait::async_trait;
use futures::Stream;
use std::pin::Pin;
use uira_types::{Message, ModelResponse, StreamChunk, ToolSpec};

use crate::ProviderError;

/// Result type for model operations
pub type ModelResult<T> = Result<T, ProviderError>;

/// Stream of response chunks
pub type ResponseStream = Pin<Box<dyn Stream<Item = Result<StreamChunk, ProviderError>> + Send>>;

/// Trait for model clients
#[async_trait]
pub trait ModelClient: Send + Sync {
    /// Send messages and get a complete response
    async fn chat(&self, messages: &[Message], tools: &[ToolSpec]) -> ModelResult<ModelResponse>;

    /// Send messages and get a streaming response
    async fn chat_stream(
        &self,
        messages: &[Message],
        tools: &[ToolSpec],
    ) -> ModelResult<ResponseStream>;

    /// Check if this client supports tool use
    fn supports_tools(&self) -> bool;

    /// Get the maximum context window size in tokens
    fn max_tokens(&self) -> usize;

    /// Get the model identifier
    fn model(&self) -> &str;

    /// Get the provider name
    fn provider(&self) -> &str;
}
