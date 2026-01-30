//! Compaction strategies for context management

use serde::{Deserialize, Serialize};

/// Strategy for compacting old context
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompactionStrategy {
    /// No compaction, just truncate
    #[default]
    None,
    /// Summarize old messages into a single system message
    Summarize {
        /// Target token count for the summary
        target_tokens: usize,
    },
    /// Keep only tool results and key decisions
    KeepDecisions,
}

impl CompactionStrategy {
    pub fn summarize(target_tokens: usize) -> Self {
        Self::Summarize { target_tokens }
    }

    pub fn requires_model(&self) -> bool {
        matches!(self, Self::Summarize { .. })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compaction_strategy() {
        let strategy = CompactionStrategy::summarize(1000);
        assert!(strategy.requires_model());

        let none = CompactionStrategy::None;
        assert!(!none.requires_model());
    }
}
