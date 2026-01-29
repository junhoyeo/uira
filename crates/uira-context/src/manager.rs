//! Context manager for tracking and managing conversation context

use uira_protocol::{Message, TokenUsage};

use crate::{CompactionStrategy, ContextError, MessageHistory, TruncationPolicy};

/// Manages conversation context within token limits
pub struct ContextManager {
    history: MessageHistory,
    max_tokens: usize,
    truncation_policy: TruncationPolicy,
    compaction_strategy: CompactionStrategy,
    total_usage: TokenUsage,
}

impl ContextManager {
    pub fn new(max_tokens: usize) -> Self {
        Self {
            history: MessageHistory::new(),
            max_tokens,
            truncation_policy: TruncationPolicy::default(),
            compaction_strategy: CompactionStrategy::default(),
            total_usage: TokenUsage::default(),
        }
    }

    pub fn with_truncation_policy(mut self, policy: TruncationPolicy) -> Self {
        self.truncation_policy = policy;
        self
    }

    pub fn with_compaction_strategy(mut self, strategy: CompactionStrategy) -> Self {
        self.compaction_strategy = strategy;
        self
    }

    /// Add a message to the context
    pub fn add_message(&mut self, message: Message) -> Result<(), ContextError> {
        self.history.push(message);
        self.maybe_truncate()
    }

    /// Get all messages for the next prompt
    pub fn messages(&self) -> &[Message] {
        self.history.messages()
    }

    /// Get current token estimate
    pub fn current_tokens(&self) -> usize {
        self.history.estimate_tokens()
    }

    /// Get remaining token budget
    pub fn remaining_tokens(&self) -> usize {
        let current = self.current_tokens();
        self.max_tokens.saturating_sub(current)
    }

    /// Record token usage from a model response
    pub fn record_usage(&mut self, usage: TokenUsage) {
        self.total_usage += usage;
    }

    /// Get total token usage for this session
    pub fn total_usage(&self) -> &TokenUsage {
        &self.total_usage
    }

    /// Clear the context
    pub fn clear(&mut self) {
        self.history.clear();
    }

    fn maybe_truncate(&mut self) -> Result<(), ContextError> {
        while self.current_tokens() > self.max_tokens {
            match self.truncation_policy {
                TruncationPolicy::Fifo => {
                    if self.history.remove_first().is_none() {
                        break;
                    }
                }
                TruncationPolicy::KeepRecent { count } => {
                    while self.history.len() > count {
                        self.history.remove_first();
                    }
                    break;
                }
                TruncationPolicy::Summarize => {
                    // Would need model access for summarization
                    // For now, fall back to FIFO
                    if self.history.remove_first().is_none() {
                        break;
                    }
                }
                TruncationPolicy::Error => {
                    return Err(ContextError::ContextExceeded {
                        used: self.current_tokens() as u64,
                        limit: self.max_tokens as u64,
                    });
                }
            }
        }
        Ok(())
    }
}

impl Default for ContextManager {
    fn default() -> Self {
        Self::new(100_000) // 100k tokens default
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_context_manager_basics() {
        let mut manager = ContextManager::new(1000);

        manager.add_message(Message::user("Hello")).unwrap();
        manager
            .add_message(Message::assistant("Hi there!"))
            .unwrap();

        assert_eq!(manager.messages().len(), 2);
        assert!(manager.current_tokens() > 0);
        assert!(manager.remaining_tokens() > 0);
    }

    #[test]
    fn test_truncation_on_overflow() {
        let mut manager = ContextManager::new(10); // Very small limit

        manager
            .add_message(Message::user(
                "This is a long message that will cause truncation",
            ))
            .unwrap();
        manager
            .add_message(Message::user("Another long message"))
            .unwrap();

        // Should have truncated some messages
        assert!(manager.current_tokens() <= 10 || manager.messages().is_empty());
    }
}
