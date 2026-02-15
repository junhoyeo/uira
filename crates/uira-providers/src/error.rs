//! Provider error types
//!
//! All variants are actively produced by error classifiers:
//! - `PaymentRequired`: HTTP 402 status or billing patterns (Anthropic, OpenAI)
//! - `Timeout`: Timeout patterns (Anthropic, OpenAI)
//! - `MessageOrderingConflict`: Message role alternation violations (Anthropic)
//! - `ToolCallInputMissing`: Missing tool input fields (Anthropic)
//! - `ImageError`: Image dimension/size issues (Anthropic)
//!
//! See `anthropic/error_classify.rs` and `openai/error_classify.rs` for classifier implementations.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum ProviderError {
    #[error("authentication failed: {0}")]
    AuthenticationFailed(String),

    #[error("rate limit exceeded: retry after {retry_after_ms}ms")]
    RateLimited { retry_after_ms: u64 },

    #[error("context window exceeded: {used} tokens used, {limit} limit")]
    ContextExceeded { used: u64, limit: u64 },

    #[error("content filtered: {reason}")]
    ContentFiltered { reason: String },

    #[error("model not found: {model}")]
    ModelNotFound { model: String },

    #[error("network error: {0}")]
    Network(#[from] reqwest::Error),

    #[error("invalid response: {0}")]
    InvalidResponse(String),

    #[error("stream error: {0}")]
    StreamError(String),

    #[error("configuration error: {0}")]
    Configuration(String),

    #[error("provider unavailable: {provider}")]
    Unavailable { provider: String },

    /// Payment required - billing or quota issues (HTTP 402)
    #[error("payment required: {message}")]
    PaymentRequired { message: String },

    /// Request timeout - network or server timeout
    #[error("timeout: {message}")]
    Timeout { message: String },

    /// Image processing error - dimension or size issues
    #[error("image error: {message}")]
    ImageError { message: String },

    /// Message ordering conflict - role alternation violation
    #[error("message ordering conflict: messages must alternate between user and assistant")]
    MessageOrderingConflict,

    /// Tool call input missing - required tool input not provided
    #[error("tool call input missing: tool_use block must have input field")]
    ToolCallInputMissing,
}

impl ProviderError {
    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            Self::RateLimited { .. }
                | Self::Network(_)
                | Self::Unavailable { .. }
                | Self::Timeout { .. }
        )
    }

    pub fn retry_after_ms(&self) -> Option<u64> {
        match self {
            Self::RateLimited { retry_after_ms } => Some(*retry_after_ms),
            _ => None,
        }
    }
}
