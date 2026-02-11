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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_truncation_policy() {
        assert_eq!(TruncationPolicy::default(), TruncationPolicy::Fifo);
    }
}
