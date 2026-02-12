//! Turn validation for Anthropic API
//!
//! Merges consecutive user messages and preserves tool-role turns.

use tracing::warn;
use uira_types::{ContentBlock, Message, MessageContent, Role};

pub fn validate_anthropic_turns(messages: &[Message]) -> Vec<Message> {
    let mut result = Vec::new();
    let mut pending_user_blocks: Vec<ContentBlock> = Vec::new();

    for msg in messages {
        match msg.role {
            Role::User => {
                // Accumulate user message content
                match &msg.content {
                    MessageContent::Text(text) => {
                        pending_user_blocks.push(ContentBlock::Text { text: text.clone() });
                    }
                    MessageContent::Blocks(blocks) => {
                        pending_user_blocks.extend(blocks.clone());
                    }
                    MessageContent::ToolCalls(calls) => {
                        warn!(
                            "User message contains ToolCalls ({} calls) — converting to text",
                            calls.len()
                        );
                        for call in calls {
                            pending_user_blocks.push(ContentBlock::Text {
                                text: format!("[tool_call: {}({})]", call.name, call.input),
                            });
                        }
                    }
                }
            }
            Role::Tool => {
                if !pending_user_blocks.is_empty() {
                    result.push(Message::with_blocks(
                        Role::User,
                        pending_user_blocks.clone(),
                    ));
                    pending_user_blocks.clear();
                }

                result.push(msg.clone());
            }
            Role::Assistant => {
                // Flush any pending user messages before adding assistant message
                if !pending_user_blocks.is_empty() {
                    result.push(Message::with_blocks(
                        Role::User,
                        pending_user_blocks.clone(),
                    ));
                    pending_user_blocks.clear();
                }

                // Add assistant message as-is
                result.push(msg.clone());

                // If this assistant message has thinking blocks, it acts as a boundary
                // (already handled by flushing above)
            }
            Role::System => {
                // System and tool messages are handled separately in Anthropic API
                // Don't include them in turn validation
                continue;
            }
        }
    }

    // Flush any remaining user messages
    if !pending_user_blocks.is_empty() {
        result.push(Message::with_blocks(Role::User, pending_user_blocks));
    }

    result
}

#[allow(dead_code)]
fn has_thinking_blocks(message: &Message) -> bool {
    match &message.content {
        MessageContent::Blocks(blocks) => blocks
            .iter()
            .any(|block| matches!(block, ContentBlock::Thinking { .. })),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_merge_consecutive_user_messages() {
        let messages = vec![
            Message::user("First question"),
            Message::user("Second question"),
            Message::user("Third question"),
        ];

        let validated = validate_anthropic_turns(&messages);

        assert_eq!(validated.len(), 1);
        assert_eq!(validated[0].role, Role::User);

        // Check that all text was merged
        if let MessageContent::Blocks(blocks) = &validated[0].content {
            assert_eq!(blocks.len(), 3);
            for block in blocks {
                assert!(matches!(block, ContentBlock::Text { .. }));
            }
        } else {
            panic!("Expected Blocks content");
        }
    }

    #[test]
    fn test_alternating_messages_unchanged() {
        let messages = vec![
            Message::user("Question 1"),
            Message::assistant("Answer 1"),
            Message::user("Question 2"),
            Message::assistant("Answer 2"),
        ];

        let validated = validate_anthropic_turns(&messages);

        assert_eq!(validated.len(), 4);
        assert_eq!(validated[0].role, Role::User);
        assert_eq!(validated[1].role, Role::Assistant);
        assert_eq!(validated[2].role, Role::User);
        assert_eq!(validated[3].role, Role::Assistant);
    }

    #[test]
    fn test_merge_with_blocks() {
        let messages = vec![
            Message::with_blocks(
                Role::User,
                vec![
                    ContentBlock::Text {
                        text: "First".to_string(),
                    },
                    ContentBlock::Text {
                        text: "block".to_string(),
                    },
                ],
            ),
            Message::user("Second message"),
        ];

        let validated = validate_anthropic_turns(&messages);

        assert_eq!(validated.len(), 1);
        if let MessageContent::Blocks(blocks) = &validated[0].content {
            assert_eq!(blocks.len(), 3); // 2 from first + 1 from second
        } else {
            panic!("Expected Blocks content");
        }
    }

    #[test]
    fn test_system_messages_filtered() {
        let messages = vec![
            Message::system("System prompt"),
            Message::user("User message"),
            Message::assistant("Assistant response"),
        ];

        let validated = validate_anthropic_turns(&messages);

        // System message should be filtered out
        assert_eq!(validated.len(), 2);
        assert_eq!(validated[0].role, Role::User);
        assert_eq!(validated[1].role, Role::Assistant);
    }

    #[test]
    fn test_has_thinking_blocks() {
        let msg_with_thinking = Message::with_blocks(
            Role::Assistant,
            vec![
                ContentBlock::Text {
                    text: "Response".to_string(),
                },
                ContentBlock::Thinking {
                    thinking: "Internal reasoning".to_string(),
                    signature: None,
                },
            ],
        );

        let msg_without_thinking = Message::assistant("Just text");

        assert!(has_thinking_blocks(&msg_with_thinking));
        assert!(!has_thinking_blocks(&msg_without_thinking));
    }

    #[test]
    fn test_empty_messages() {
        let messages: Vec<Message> = vec![];
        let validated = validate_anthropic_turns(&messages);
        assert_eq!(validated.len(), 0);
    }

    #[test]
    fn test_only_assistant_messages() {
        let messages = vec![
            Message::assistant("Response 1"),
            Message::assistant("Response 2"),
        ];

        let validated = validate_anthropic_turns(&messages);

        // Assistant messages should be preserved as-is
        assert_eq!(validated.len(), 2);
        assert_eq!(validated[0].role, Role::Assistant);
        assert_eq!(validated[1].role, Role::Assistant);
    }

    #[test]
    fn test_tool_messages_preserved() {
        let messages = vec![
            Message::user("Q1"),
            Message::tool_result("tc_1", "tool output"),
            Message::assistant("A1"),
        ];

        let validated = validate_anthropic_turns(&messages);

        assert_eq!(validated.len(), 3);
        assert_eq!(validated[0].role, Role::User);
        assert_eq!(validated[1].role, Role::Tool);
        assert_eq!(validated[2].role, Role::Assistant);
        assert_eq!(validated[1].tool_call_id.as_deref(), Some("tc_1"));
        assert!(matches!(validated[1].content, MessageContent::Text(_)));
    }

    #[test]
    fn test_complex_alternation() {
        let messages = vec![
            Message::user("Q1"),
            Message::user("Q2"), // Should merge with Q1
            Message::assistant("A1"),
            Message::user("Q3"),
            Message::user("Q4"), // Should merge with Q3
            Message::user("Q5"), // Should merge with Q3, Q4
            Message::assistant("A2"),
        ];

        let validated = validate_anthropic_turns(&messages);

        assert_eq!(validated.len(), 4);
        assert_eq!(validated[0].role, Role::User); // Q1 + Q2
        assert_eq!(validated[1].role, Role::Assistant); // A1
        assert_eq!(validated[2].role, Role::User); // Q3 + Q4 + Q5
        assert_eq!(validated[3].role, Role::Assistant); // A2

        // Verify Q1+Q2 merged
        if let MessageContent::Blocks(blocks) = &validated[0].content {
            assert_eq!(blocks.len(), 2);
        } else {
            panic!("Expected Blocks content");
        }

        // Verify Q3+Q4+Q5 merged
        if let MessageContent::Blocks(blocks) = &validated[2].content {
            assert_eq!(blocks.len(), 3);
        } else {
            panic!("Expected Blocks content");
        }
    }
}
