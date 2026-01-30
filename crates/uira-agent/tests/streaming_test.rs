//! Tests for streaming controller

use uira_agent::StreamController;
use uira_protocol::{ContentBlock, ContentDelta, StreamChunk, StreamMessageStart, TokenUsage};

fn make_message_start() -> StreamChunk {
    StreamChunk::MessageStart {
        message: StreamMessageStart {
            id: "msg_test".to_string(),
            model: "test-model".to_string(),
            usage: TokenUsage::default(),
        },
    }
}

fn make_text_block_start(index: usize) -> StreamChunk {
    StreamChunk::ContentBlockStart {
        index,
        content_block: ContentBlock::Text {
            text: String::new(),
        },
    }
}

fn make_text_delta(text: &str) -> StreamChunk {
    StreamChunk::ContentBlockDelta {
        index: 0,
        delta: ContentDelta::TextDelta {
            text: text.to_string(),
        },
    }
}

fn make_block_stop(index: usize) -> StreamChunk {
    StreamChunk::ContentBlockStop { index }
}

#[test]
fn test_newline_gated_buffering() {
    let mut controller = StreamController::new();

    // Start stream
    assert!(controller.push(make_message_start()).is_empty());
    assert!(controller.push(make_text_block_start(0)).is_empty());

    // Push partial text without newline - should buffer
    let lines = controller.push(make_text_delta("Hello"));
    assert!(lines.is_empty(), "No newline, should buffer");
    assert_eq!(controller.pending_text(), "Hello");

    // Push text with newline - should commit
    let lines = controller.push(make_text_delta(" world\n"));
    assert_eq!(lines, vec!["Hello world"]);
    assert!(controller.pending_text().is_empty());

    // Push multiple lines at once
    let lines = controller.push(make_text_delta("Line 1\nLine 2\nPartial"));
    assert_eq!(lines, vec!["Line 1", "Line 2"]);
    assert_eq!(controller.pending_text(), "Partial");
}

#[test]
fn test_drain_on_message_stop() {
    let mut controller = StreamController::new();

    controller.push(make_message_start());
    controller.push(make_text_block_start(0));
    controller.push(make_text_delta("Partial without newline"));
    controller.push(make_block_stop(0));

    // Message stop should drain pending text
    let lines = controller.push(StreamChunk::MessageStop);
    assert_eq!(lines, vec!["Partial without newline"]);
    assert!(controller.is_finished());
}

#[test]
fn test_multiple_blocks() {
    let mut controller = StreamController::new();

    controller.push(make_message_start());

    // First text block
    controller.push(make_text_block_start(0));
    controller.push(make_text_delta("First block\n"));
    controller.push(make_block_stop(0));

    // Second text block
    controller.push(StreamChunk::ContentBlockStart {
        index: 1,
        content_block: ContentBlock::Text {
            text: String::new(),
        },
    });
    controller.push(StreamChunk::ContentBlockDelta {
        index: 1,
        delta: ContentDelta::TextDelta {
            text: "Second block".to_string(),
        },
    });
    controller.push(make_block_stop(1));

    controller.push(StreamChunk::MessageStop);

    let response = controller.into_response();
    assert_eq!(response.content.len(), 2);
}

#[test]
fn test_tool_json_accumulation() {
    let mut controller = StreamController::new();

    controller.push(make_message_start());

    // Tool block
    controller.push(StreamChunk::ContentBlockStart {
        index: 0,
        content_block: ContentBlock::ToolUse {
            id: "tc_test".to_string(),
            name: "read_file".to_string(),
            input: serde_json::Value::Null,
        },
    });

    // Accumulate JSON in parts
    controller.push(StreamChunk::ContentBlockDelta {
        index: 0,
        delta: ContentDelta::InputJsonDelta {
            partial_json: r#"{"path""#.to_string(),
        },
    });

    controller.push(StreamChunk::ContentBlockDelta {
        index: 0,
        delta: ContentDelta::InputJsonDelta {
            partial_json: r#": "/tmp/test.txt"}"#.to_string(),
        },
    });

    controller.push(make_block_stop(0));
    controller.push(StreamChunk::MessageStop);

    let response = controller.into_response();

    assert_eq!(response.content.len(), 1);
    if let ContentBlock::ToolUse { id, name, input } = &response.content[0] {
        assert_eq!(id, "tc_test");
        assert_eq!(name, "read_file");
        assert_eq!(input["path"], "/tmp/test.txt");
    } else {
        panic!("Expected ToolUse block");
    }
}

#[test]
fn test_mixed_text_and_tool() {
    let mut controller = StreamController::new();

    controller.push(make_message_start());

    // Text block first
    controller.push(make_text_block_start(0));
    controller.push(make_text_delta("Let me read that file.\n"));
    controller.push(make_block_stop(0));

    // Then tool block
    controller.push(StreamChunk::ContentBlockStart {
        index: 1,
        content_block: ContentBlock::ToolUse {
            id: "tc_1".to_string(),
            name: "read_file".to_string(),
            input: serde_json::Value::Null,
        },
    });
    controller.push(StreamChunk::ContentBlockDelta {
        index: 1,
        delta: ContentDelta::InputJsonDelta {
            partial_json: r#"{"path": "/tmp/test.txt"}"#.to_string(),
        },
    });
    controller.push(make_block_stop(1));

    controller.push(StreamChunk::MessageStop);

    let response = controller.into_response();

    assert_eq!(response.content.len(), 2);
    assert!(matches!(response.content[0], ContentBlock::Text { .. }));
    assert!(matches!(response.content[1], ContentBlock::ToolUse { .. }));
}

#[test]
fn test_thinking_block() {
    let mut controller = StreamController::new();

    controller.push(make_message_start());

    // Thinking block
    controller.push(StreamChunk::ContentBlockStart {
        index: 0,
        content_block: ContentBlock::Thinking {
            thinking: String::new(),
            signature: None,
        },
    });

    controller.push(StreamChunk::ContentBlockDelta {
        index: 0,
        delta: ContentDelta::ThinkingDelta {
            thinking: "I need to analyze this carefully...".to_string(),
        },
    });

    controller.push(StreamChunk::ContentBlockDelta {
        index: 0,
        delta: ContentDelta::SignatureDelta {
            signature: "sig_123".to_string(),
        },
    });

    controller.push(make_block_stop(0));
    controller.push(StreamChunk::MessageStop);

    let response = controller.into_response();

    assert_eq!(response.content.len(), 1);
    if let ContentBlock::Thinking {
        thinking,
        signature,
    } = &response.content[0]
    {
        assert!(thinking.contains("analyze"));
        assert_eq!(signature.as_deref(), Some("sig_123"));
    } else {
        panic!("Expected Thinking block");
    }
}

#[test]
fn test_current_text() {
    let mut controller = StreamController::new();

    controller.push(make_message_start());
    controller.push(make_text_block_start(0));

    controller.push(make_text_delta("Line 1\n"));
    controller.push(make_text_delta("Line 2\n"));
    controller.push(make_text_delta("Partial"));

    let current = controller.current_text();
    assert_eq!(current, "Line 1\nLine 2\nPartial");
}

#[test]
fn test_into_response() {
    let mut controller = StreamController::new();

    controller.push(make_message_start());
    controller.push(make_text_block_start(0));
    controller.push(make_text_delta("Hello, world!"));
    controller.push(make_block_stop(0));
    controller.push(StreamChunk::MessageStop);

    let response = controller.into_response();

    assert_eq!(response.id, "msg_test");
    assert_eq!(response.model, "test-model");
    assert_eq!(response.content.len(), 1);

    if let ContentBlock::Text { text } = &response.content[0] {
        assert_eq!(text, "Hello, world!");
    } else {
        panic!("Expected Text block");
    }
}

#[test]
fn test_empty_stream() {
    let mut controller = StreamController::new();

    controller.push(make_message_start());
    controller.push(StreamChunk::MessageStop);

    assert!(controller.is_finished());

    let response = controller.into_response();
    assert!(response.content.is_empty());
}

#[test]
fn test_usage_tracking() {
    let mut controller = StreamController::new();

    // Initial usage
    controller.push(StreamChunk::MessageStart {
        message: StreamMessageStart {
            id: "msg_1".to_string(),
            model: "test".to_string(),
            usage: TokenUsage {
                input_tokens: 50,
                output_tokens: 0,
                ..Default::default()
            },
        },
    });

    // Final usage in message delta
    controller.push(StreamChunk::MessageDelta {
        delta: uira_protocol::MessageDelta { stop_reason: None },
        usage: Some(TokenUsage {
            input_tokens: 100,
            output_tokens: 50,
            cache_read_tokens: 10,
            cache_creation_tokens: 5,
        }),
    });

    controller.push(StreamChunk::MessageStop);

    let usage = controller.usage();
    assert_eq!(usage.input_tokens, 100);
    assert_eq!(usage.output_tokens, 50);
    assert_eq!(usage.cache_read_tokens, 10);
}

#[test]
fn test_ping_handling() {
    let mut controller = StreamController::new();

    controller.push(make_message_start());
    controller.push(make_text_block_start(0));

    // Ping should be ignored
    let lines = controller.push(StreamChunk::Ping);
    assert!(lines.is_empty());

    controller.push(make_text_delta("Hello\n"));
    controller.push(make_block_stop(0));
    controller.push(StreamChunk::MessageStop);

    let response = controller.into_response();
    assert_eq!(response.text(), "Hello");
}

#[test]
fn test_error_handling() {
    let mut controller = StreamController::new();

    controller.push(make_message_start());
    controller.push(make_text_block_start(0));
    controller.push(make_text_delta("Some text\n"));

    // Error chunk - should be logged but not break stream
    controller.push(StreamChunk::Error {
        error: uira_protocol::StreamError {
            r#type: "test_error".to_string(),
            message: "Test error message".to_string(),
        },
    });

    controller.push(make_block_stop(0));
    controller.push(StreamChunk::MessageStop);

    // Should still produce a valid response
    let response = controller.into_response();
    assert_eq!(response.text(), "Some text");
}

#[test]
fn test_committed_lines_access() {
    let mut controller = StreamController::new();

    controller.push(make_message_start());
    controller.push(make_text_block_start(0));

    controller.push(make_text_delta("Line 1\n"));
    controller.push(make_text_delta("Line 2\n"));

    let committed = controller.committed_lines();
    assert_eq!(committed, &["Line 1", "Line 2"]);
}

#[test]
fn test_character_by_character_streaming() {
    let mut controller = StreamController::new();

    controller.push(make_message_start());
    controller.push(make_text_block_start(0));

    // Simulate character-by-character streaming
    for c in "Hello\nWorld".chars() {
        let lines = controller.push(make_text_delta(&c.to_string()));
        if c == '\n' {
            assert_eq!(lines.len(), 1);
            assert_eq!(lines[0], "Hello");
        } else if lines.is_empty() {
            // Expected for non-newline characters
        }
    }

    controller.push(make_block_stop(0));
    controller.push(StreamChunk::MessageStop);

    let response = controller.into_response();
    assert_eq!(response.text(), "Hello\nWorld");
}
