//! Token estimation and truncation policies

use serde::{Deserialize, Serialize};

/// Policy for truncating context when it exceeds the limit
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TruncationPolicy {
    /// Remove oldest messages first (FIFO)
    #[default]
    Fifo,
    /// Keep only the most recent N messages
    KeepRecent { count: usize },
    /// Summarize old messages before truncating
    Summarize,
    /// Error when context is exceeded (don't auto-truncate)
    Error,
}

impl TruncationPolicy {
    pub fn fifo() -> Self {
        Self::Fifo
    }

    pub fn keep_recent(count: usize) -> Self {
        Self::KeepRecent { count }
    }

    pub fn summarize() -> Self {
        Self::Summarize
    }

    pub fn error() -> Self {
        Self::Error
    }
}

/// Estimate tokens for a string (~4 chars per token)
#[allow(dead_code)] // Public utility function
pub fn estimate_tokens(text: &str) -> usize {
    text.len() / 4
}

/// Estimate tokens for JSON value
#[allow(dead_code)] // Public utility function
pub fn estimate_json_tokens(value: &serde_json::Value) -> usize {
    estimate_tokens(&value.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_estimate_tokens() {
        // 20 chars should be ~5 tokens
        assert_eq!(estimate_tokens("12345678901234567890"), 5);
    }

    #[test]
    fn test_truncation_policy() {
        assert_eq!(TruncationPolicy::default(), TruncationPolicy::Fifo);
    }
}
