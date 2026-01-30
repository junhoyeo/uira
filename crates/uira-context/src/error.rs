//! Context error types

use thiserror::Error;

#[derive(Debug, Error)]
pub enum ContextError {
    #[error("context window exceeded: {used} tokens used, {limit} limit")]
    ContextExceeded { used: u64, limit: u64 },

    #[error("compaction failed: {0}")]
    CompactionFailed(String),

    #[error("invalid message: {0}")]
    InvalidMessage(String),

    #[error("history empty")]
    HistoryEmpty,
}
