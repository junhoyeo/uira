//! Context manager for tracking and managing conversation context

use uira_protocol::{ContentBlock, Message, MessageContent, Role, TokenUsage};

use crate::{
    CompactionConfig, CompactionResult, CompactionStrategy, ContextError, MessageHistory,
    PruningStrategy, TokenMonitor, TruncationPolicy,
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

const CHARS_PER_TOKEN_ESTIMATE: usize = 4;
const MIN_SUMMARY_TOKENS: usize = 128;
const SUMMARY_LINE_CHAR_LIMIT: usize = 180;
const SUMMARY_END_MARKER: &str = "[End Summary]";

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

    pub fn with_compaction_config(mut self, config: CompactionConfig) -> Self {
        self.token_monitor = self
            .token_monitor
            .with_threshold(config.threshold)
            .with_protected_tokens(config.protected_tokens);
        self.compaction_strategy = if config.enabled {
            config.strategy
        } else {
            CompactionStrategy::None
        };
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

        if self.needs_compaction() {
            let _ = self.compact();
        }

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
        let strategy_used = self.compaction_strategy.clone();

        let messages_pruned = match strategy_used {
            CompactionStrategy::None => return None,
            CompactionStrategy::Prune => self.prune_old_messages(),
            CompactionStrategy::Summarize { target_tokens } => {
                self.summarize_old_messages(target_tokens, false)
            }
            CompactionStrategy::Hybrid {
                prune_first,
                target_tokens,
            } => self.summarize_old_messages(target_tokens, prune_first),
        };

        let tokens_after = self.current_tokens();
        let messages_after = self.history.len();

        Some(CompactionResult {
            tokens_before,
            tokens_after,
            messages_removed: messages_before.saturating_sub(messages_after),
            messages_pruned,
            strategy_used,
        })
    }

    fn prune_old_messages(&mut self) -> usize {
        let mut messages = self.history.messages().to_vec();
        let protected_count = self.protected_recent_count(&messages);
        if messages.len() <= protected_count {
            return 0;
        }

        self.pruning_strategy
            .prune_messages(&mut messages, protected_count);
        self.history = MessageHistory::from_messages(messages);
        self.history.len().saturating_sub(protected_count)
    }

    fn summarize_old_messages(&mut self, target_tokens: usize, prune_first: bool) -> usize {
        let messages = self.history.messages().to_vec();
        let protected_count = self.protected_recent_count(&messages);
        if messages.len() <= protected_count {
            return 0;
        }

        let split_at = messages.len() - protected_count;
        let mut older = messages[..split_at].to_vec();
        let recent = messages[split_at..].to_vec();

        if prune_first {
            self.pruning_strategy.prune_messages(&mut older, 0);
        }

        let mut preserved_prefix = Vec::new();
        let mut summary_candidates = Vec::new();

        for message in older {
            if message.role == Role::System || Self::is_summary_message(&message) {
                preserved_prefix.push(message);
            } else {
                summary_candidates.push(message);
            }
        }

        let Some(summary_message) = self.build_summary_message(&summary_candidates, target_tokens)
        else {
            return 0;
        };

        let mut compacted = preserved_prefix;
        compacted.push(summary_message);
        compacted.extend(recent);
        self.history = MessageHistory::from_messages(compacted);

        summary_candidates.len()
    }

    fn protected_recent_count(&self, messages: &[Message]) -> usize {
        let by_count = self.protected_message_count.min(messages.len());
        let protected_tokens = self.token_monitor.protected_tokens();

        if protected_tokens == 0 {
            return by_count;
        }

        let mut by_tokens = 0;
        let mut token_total = 0;

        for message in messages.iter().rev() {
            if token_total >= protected_tokens {
                break;
            }
            token_total += message.estimate_tokens();
            by_tokens += 1;
        }

        let protected = by_tokens.max(by_count).min(messages.len());

        // If we're already over the compaction threshold, ensure that at least
        // one message is eligible for pruning/summarization.
        //
        // This prevents large `protected_tokens` defaults from fully shielding
        // short histories, which would otherwise make compaction a no-op.
        let current_tokens: usize = messages.iter().map(|m| m.estimate_tokens()).sum();
        let needs_compaction = self.token_monitor.needs_compaction(current_tokens);
        if needs_compaction && protected == messages.len() && by_count < messages.len() {
            return by_count;
        }

        protected
    }

    fn build_summary_message(&self, messages: &[Message], target_tokens: usize) -> Option<Message> {
        if messages.is_empty() {
            return None;
        }

        let target_tokens = target_tokens.max(MIN_SUMMARY_TOKENS);
        let max_chars = target_tokens.saturating_mul(CHARS_PER_TOKEN_ESTIMATE);
        if max_chars == 0 {
            return None;
        }

        let mut summary = format!("[Session Summary - {} messages]\n", messages.len());
        let mut lines_added = 0;

        for message in messages {
            let Some(line) = Self::summary_line_for_message(message) else {
                continue;
            };

            let entry = format!("- {}\n", line);
            if summary.len() + entry.len() + SUMMARY_END_MARKER.len() > max_chars {
                break;
            }

            summary.push_str(&entry);
            lines_added += 1;
        }

        if lines_added == 0 {
            return None;
        }

        summary.push_str(SUMMARY_END_MARKER);
        Some(Message::assistant(summary))
    }

    fn summary_line_for_message(message: &Message) -> Option<String> {
        let text = Self::extract_message_text(message);
        let normalized = Self::normalize_whitespace(&text);
        if normalized.is_empty() {
            return None;
        }

        let role_label = match message.role {
            Role::System => "System",
            Role::User => "User",
            Role::Assistant => "Assistant",
            Role::Tool => "Tool",
        };

        let excerpt = Self::truncate_chars(&normalized, SUMMARY_LINE_CHAR_LIMIT);
        Some(format!("{}: {}", role_label, excerpt))
    }

    fn extract_message_text(message: &Message) -> String {
        match &message.content {
            MessageContent::Text(text) => text.clone(),
            MessageContent::Blocks(blocks) => blocks
                .iter()
                .filter_map(|block| match block {
                    ContentBlock::Text { text } => Some(text.clone()),
                    ContentBlock::ToolUse { name, .. } => Some(format!("tool_use {}", name)),
                    ContentBlock::ToolResult { content, .. } => Some(content.clone()),
                    ContentBlock::Image { .. } => Some("[image content]".to_string()),
                    ContentBlock::Thinking { .. } => None,
                })
                .collect::<Vec<_>>()
                .join(" "),
            MessageContent::ToolCalls(calls) => calls
                .iter()
                .map(|call| format!("tool_call {} {}", call.name, call.input))
                .collect::<Vec<_>>()
                .join(" "),
        }
    }

    fn normalize_whitespace(text: &str) -> String {
        text.split_whitespace().collect::<Vec<_>>().join(" ")
    }

    fn truncate_chars(text: &str, max_chars: usize) -> String {
        let total_chars = text.chars().count();
        if total_chars <= max_chars {
            return text.to_string();
        }

        if max_chars <= 3 {
            return "...".to_string();
        }

        let truncated = text.chars().take(max_chars - 3).collect::<String>();
        format!("{}...", truncated)
    }

    fn is_summary_message(message: &Message) -> bool {
        match &message.content {
            MessageContent::Text(text) => text.starts_with("[Session Summary - "),
            _ => false,
        }
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
                    let before = self.current_tokens();
                    let _ = self.compact();

                    if self.current_tokens() < before {
                        continue;
                    }

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

    #[test]
    fn test_summarize_compaction_keeps_recent_messages_verbatim() {
        let mut manager = ContextManager::new(2_000)
            .with_threshold(0.01)
            .with_compaction_strategy(CompactionStrategy::summarize(256))
            .with_protected_message_count(2);

        manager
            .add_message(Message::system("Follow the project coding conventions."))
            .unwrap();
        manager
            .add_message(Message::user(
                "Implement authentication and make sure all routes are protected from unauthorized access.",
            ))
            .unwrap();
        manager
            .add_message(Message::assistant(
                "Implemented middleware and validated token checks for protected endpoints.",
            ))
            .unwrap();
        manager
            .add_message(Message::user("Recent message A should remain verbatim"))
            .unwrap();
        manager
            .add_message(Message::assistant(
                "Recent message B should remain verbatim",
            ))
            .unwrap();

        let _ = manager.compact();

        let messages = manager.messages();
        assert!(messages
            .iter()
            .any(|message| matches!(message.content.as_text(), Some(text) if text.starts_with("[Session Summary - "))));
        assert_eq!(
            messages[messages.len() - 2].content.as_text(),
            Some("Recent message A should remain verbatim")
        );
        assert_eq!(
            messages[messages.len() - 1].content.as_text(),
            Some("Recent message B should remain verbatim")
        );
    }

    #[test]
    fn test_add_message_auto_compacts_with_summarize_strategy() {
        let mut manager = ContextManager::new(2_000)
            .with_threshold(0.01)
            .with_compaction_strategy(CompactionStrategy::summarize(256))
            .with_protected_message_count(1);

        manager
            .add_message(Message::system("You are a coding assistant."))
            .unwrap();

        manager
            .add_message(Message::user(
                "Analyze the failing tests, identify root causes, and propose a concrete patch plan.",
            ))
            .unwrap();
        manager
            .add_message(Message::assistant(
                "Root causes identified in parser edge cases and stale fixture setup.",
            ))
            .unwrap();
        manager
            .add_message(Message::user(
                "Apply fixes and keep the implementation easy to review and verify.",
            ))
            .unwrap();

        let messages = manager.messages();
        assert!(messages
            .iter()
            .any(|message| matches!(message.content.as_text(), Some(text) if text.starts_with("[Session Summary - "))));
    }
}
