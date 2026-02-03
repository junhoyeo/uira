//! Context manager for tracking and managing conversation context

use uira_protocol::{Message, TokenUsage};

use crate::{
    CompactionResult, CompactionStrategy, ContextError, MessageHistory, PruningStrategy,
    TokenMonitor, TruncationPolicy,
};

pub struct ContextManager {
    history: MessageHistory,
    max_tokens: usize,
    truncation_policy: TruncationPolicy,
    compaction_strategy: CompactionStrategy,
    pruning_strategy: PruningStrategy,
    token_monitor: TokenMonitor,
    total_usage: TokenUsage,
    protected_message_count: usize,
}

impl ContextManager {
    pub fn new(max_tokens: usize) -> Self {
        Self {
            history: MessageHistory::new(),
            max_tokens,
            truncation_policy: TruncationPolicy::default(),
            compaction_strategy: CompactionStrategy::default(),
            pruning_strategy: PruningStrategy::new(),
            token_monitor: TokenMonitor::new(max_tokens),
            total_usage: TokenUsage::default(),
            protected_message_count: 10,
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

    pub fn with_pruning_strategy(mut self, strategy: PruningStrategy) -> Self {
        self.pruning_strategy = strategy;
        self
    }

    pub fn with_threshold(mut self, threshold: f64) -> Self {
        self.token_monitor = self.token_monitor.with_threshold(threshold);
        self
    }

    pub fn with_protected_tokens(mut self, protected_tokens: usize) -> Self {
        self.token_monitor = self.token_monitor.with_protected_tokens(protected_tokens);
        self
    }

    pub fn with_protected_message_count(mut self, count: usize) -> Self {
        self.protected_message_count = count;
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

    pub fn clear(&mut self) {
        self.history.clear();
    }

    pub fn needs_compaction(&self) -> bool {
        self.token_monitor.needs_compaction(self.current_tokens())
    }

    pub fn compact(&mut self) -> Option<CompactionResult> {
        let tokens_before = self.current_tokens();

        if !self.needs_compaction() {
            return None;
        }

        let messages_before = self.history.len();
        let mut messages_pruned = 0;

        match &self.compaction_strategy {
            CompactionStrategy::None => return None,
            CompactionStrategy::Prune => {
                let mut messages = self.history.messages().to_vec();
                self.pruning_strategy
                    .prune_messages(&mut messages, self.protected_message_count);
                messages_pruned = messages_before;
                self.history = MessageHistory::from_messages(messages);
            }
            CompactionStrategy::Summarize { .. } => {
                tracing::debug!("summarize compaction requires external model, skipping");
                return None;
            }
            CompactionStrategy::Hybrid { prune_first, .. } => {
                if *prune_first {
                    let mut messages = self.history.messages().to_vec();
                    self.pruning_strategy
                        .prune_messages(&mut messages, self.protected_message_count);
                    messages_pruned = messages_before;
                    self.history = MessageHistory::from_messages(messages);
                }
            }
        }

        let tokens_after = self.current_tokens();
        let messages_after = self.history.len();

        Some(CompactionResult {
            tokens_before,
            tokens_after,
            messages_removed: messages_before.saturating_sub(messages_after),
            messages_pruned,
            strategy_used: self.compaction_strategy.clone(),
        })
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
                    if self.current_tokens() <= self.max_tokens {
                        break;
                    }
                    if self.history.remove_first().is_none() {
                        break;
                    }
                }
                TruncationPolicy::Summarize => {
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
        Self::new(100_000)
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
