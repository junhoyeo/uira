//! Provider error types

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
}

impl ProviderError {
    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            Self::RateLimited { .. } | Self::Network(_) | Self::Unavailable { .. }
        )
    }

    pub fn retry_after_ms(&self) -> Option<u64> {
        match self {
            Self::RateLimited { retry_after_ms } => Some(*retry_after_ms),
            _ => None,
        }
    }
}
