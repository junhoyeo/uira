//! Compaction strategies for context management

use serde::{Deserialize, Serialize};
use uira_protocol::{ContentBlock, Message, Role};

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompactionStrategy {
    #[default]
    None,
    Prune,
    Summarize {
        target_tokens: usize,
    },
    Hybrid {
        prune_first: bool,
        target_tokens: usize,
    },
}

impl CompactionStrategy {
    pub fn summarize(target_tokens: usize) -> Self {
        Self::Summarize { target_tokens }
    }

    pub fn hybrid(target_tokens: usize) -> Self {
        Self::Hybrid {
            prune_first: true,
            target_tokens,
        }
    }

    pub fn requires_model(&self) -> bool {
        matches!(self, Self::Summarize { .. } | Self::Hybrid { .. })
    }
}

#[derive(Debug, Clone, Default)]
pub struct PruningStrategy {
    pub truncate_tool_outputs: bool,
    pub max_tool_output_tokens: usize,
    pub remove_thinking_blocks: bool,
    pub preserve_recent_count: usize,
}

impl PruningStrategy {
    pub fn new() -> Self {
        Self {
            truncate_tool_outputs: true,
            max_tool_output_tokens: 500,
            remove_thinking_blocks: true,
            preserve_recent_count: 10,
        }
    }

    pub fn prune_messages(&self, messages: &mut [Message], protected_count: usize) {
        let total = messages.len();
        if total <= protected_count {
            return;
        }

        let prune_end = total.saturating_sub(protected_count);

        for msg in messages.iter_mut().take(prune_end) {
            self.prune_message(msg);
        }
    }

    fn prune_message(&self, message: &mut Message) {
        if message.role == Role::Tool && self.truncate_tool_outputs {
            self.truncate_tool_output(message);
        }

        if self.remove_thinking_blocks {
            self.remove_thinking(message);
        }
    }

    fn truncate_tool_output(&self, message: &mut Message) {
        use uira_protocol::MessageContent;

        match &mut message.content {
            MessageContent::Text(text) => {
                if text.len() > self.max_tool_output_tokens * 4 {
                    let truncated = format!(
                        "{}... [truncated {} chars]",
                        &text[..self.max_tool_output_tokens * 4],
                        text.len() - self.max_tool_output_tokens * 4
                    );
                    *text = truncated;
                }
            }
            MessageContent::Blocks(blocks) => {
                for block in blocks.iter_mut() {
                    if let ContentBlock::ToolResult { content, .. } = block {
                        if content.len() > self.max_tool_output_tokens * 4 {
                            let truncated = format!(
                                "{}... [truncated {} chars]",
                                &content[..self.max_tool_output_tokens * 4],
                                content.len() - self.max_tool_output_tokens * 4
                            );
                            *content = truncated;
                        }
                    }
                }
            }
            _ => {}
        }
    }

    fn remove_thinking(&self, message: &mut Message) {
        use uira_protocol::MessageContent;

        if let MessageContent::Blocks(blocks) = &mut message.content {
            blocks.retain(|block| !matches!(block, ContentBlock::Thinking { .. }));
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompactionConfig {
    pub enabled: bool,
    pub threshold: f64,
    pub protected_tokens: usize,
    pub strategy: CompactionStrategy,
    pub summarization_model: Option<String>,
}

impl Default for CompactionConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            threshold: 0.8,
            protected_tokens: 40_000,
            strategy: CompactionStrategy::Prune,
            summarization_model: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct CompactionResult {
    pub tokens_before: usize,
    pub tokens_after: usize,
    pub messages_removed: usize,
    pub messages_pruned: usize,
    pub strategy_used: CompactionStrategy,
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

        let prune = CompactionStrategy::Prune;
        assert!(!prune.requires_model());
    }

    #[test]
    fn test_pruning_strategy_default() {
        let strategy = PruningStrategy::new();
        assert!(strategy.truncate_tool_outputs);
        assert!(strategy.remove_thinking_blocks);
        assert_eq!(strategy.max_tool_output_tokens, 500);
    }

    #[test]
    fn test_compaction_config_default() {
        let config = CompactionConfig::default();
        assert!(config.enabled);
        assert!((config.threshold - 0.8).abs() < 0.01);
        assert_eq!(config.protected_tokens, 40_000);
    }
}
