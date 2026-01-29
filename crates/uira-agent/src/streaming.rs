//! Stream controller for newline-gated buffering (Codex pattern)
//!
//! This module implements the StreamController which processes streaming
//! chunks from the model, buffering text until newlines arrive before
//! committing lines for display.

use uira_protocol::{ContentBlock, ContentDelta, ModelResponse, StreamChunk, TokenUsage};

/// Controller for processing streaming model responses
///
/// Implements the Codex newline-gated streaming pattern:
/// - Buffer text until `\n` arrives, then commit the line
/// - Tool JSON accumulates until block stop
/// - Drain any partial lines on message stop
pub struct StreamController {
    /// Buffered text not yet committed (no newline yet)
    pending_text: String,

    /// Committed lines ready for display
    committed_lines: Vec<String>,

    /// Whether pending_text has been included in a finalized content block
    /// This prevents double-counting in into_response()
    pending_in_block: bool,

    /// Tool JSON being accumulated
    tool_json_buffer: String,

    /// Current tool ID being accumulated
    tool_id: Option<String>,

    /// Current tool name being accumulated
    tool_name: Option<String>,

    /// Thinking text being accumulated
    thinking_buffer: String,

    /// Thinking signature
    thinking_signature: Option<String>,

    /// Current block index
    current_block_index: Option<usize>,

    /// Whether current block is text
    is_text_block: bool,

    /// Final content blocks
    content_blocks: Vec<ContentBlock>,

    /// Message ID from stream start
    message_id: Option<String>,

    /// Model from stream start
    model: Option<String>,

    /// Token usage
    usage: TokenUsage,

    /// Whether stream has finished
    finished: bool,
}

impl StreamController {
    /// Create a new stream controller
    pub fn new() -> Self {
        Self {
            pending_text: String::new(),
            committed_lines: Vec::new(),
            pending_in_block: false,
            tool_json_buffer: String::new(),
            tool_id: None,
            tool_name: None,
            thinking_buffer: String::new(),
            thinking_signature: None,
            current_block_index: None,
            is_text_block: false,
            content_blocks: Vec::new(),
            message_id: None,
            model: None,
            usage: TokenUsage::default(),
            finished: false,
        }
    }

    /// Push a stream chunk, returns newly committed lines
    pub fn push(&mut self, chunk: StreamChunk) -> Vec<String> {
        match chunk {
            StreamChunk::MessageStart { message } => {
                self.message_id = Some(message.id);
                self.model = Some(message.model);
                self.usage = message.usage;
                vec![]
            }

            StreamChunk::ContentBlockStart {
                index,
                content_block,
            } => {
                self.current_block_index = Some(index);
                match &content_block {
                    ContentBlock::Text { .. } => {
                        self.is_text_block = true;
                    }
                    ContentBlock::ToolUse { id, name, .. } => {
                        self.is_text_block = false;
                        self.tool_id = Some(id.clone());
                        self.tool_name = Some(name.clone());
                        self.tool_json_buffer.clear();
                    }
                    ContentBlock::Thinking { .. } => {
                        self.is_text_block = false;
                        self.thinking_buffer.clear();
                        self.thinking_signature = None;
                    }
                    _ => {}
                }
                vec![]
            }

            StreamChunk::ContentBlockDelta { delta, .. } => match delta {
                ContentDelta::TextDelta { text } => self.push_text(&text),
                ContentDelta::InputJsonDelta { partial_json } => {
                    self.tool_json_buffer.push_str(&partial_json);
                    vec![]
                }
                ContentDelta::ThinkingDelta { thinking } => {
                    self.thinking_buffer.push_str(&thinking);
                    vec![]
                }
                ContentDelta::SignatureDelta { signature } => {
                    if let Some(ref mut sig) = self.thinking_signature {
                        sig.push_str(&signature);
                    } else {
                        self.thinking_signature = Some(signature);
                    }
                    vec![]
                }
            },

            StreamChunk::ContentBlockStop { .. } => {
                self.finalize_current_block();
                self.current_block_index = None;
                vec![]
            }

            StreamChunk::MessageDelta { usage, .. } => {
                if let Some(u) = usage {
                    self.usage = u;
                }
                vec![]
            }

            StreamChunk::MessageStop => {
                self.finished = true;
                self.drain_pending()
            }

            StreamChunk::Ping => vec![],

            StreamChunk::Error { error } => {
                tracing::error!("Stream error: {} - {}", error.r#type, error.message);
                vec![]
            }
        }
    }

    /// Push text, committing on newlines (Codex pattern)
    fn push_text(&mut self, text: &str) -> Vec<String> {
        let mut new_lines = Vec::new();

        for ch in text.chars() {
            if ch == '\n' {
                // Commit the pending line
                let line = std::mem::take(&mut self.pending_text);
                self.committed_lines.push(line.clone());
                new_lines.push(line);
            } else {
                self.pending_text.push(ch);
            }
        }

        new_lines
    }

    /// Drain any remaining partial line (for UI streaming)
    /// Note: This returns the pending text for display but doesn't add it to
    /// committed_lines since it's already been included in the finalized content block.
    fn drain_pending(&mut self) -> Vec<String> {
        if self.pending_text.is_empty() {
            vec![]
        } else {
            let line = std::mem::take(&mut self.pending_text);
            // Don't add to committed_lines - it's already in the content block
            // from finalize_current_block(). Just return for UI streaming.
            vec![line]
        }
    }

    /// Finalize the current content block
    /// Note: pending_text is included in the content block but NOT cleared,
    /// so it can still be returned by drain_pending() on MessageStop for UI streaming.
    fn finalize_current_block(&mut self) {
        // Finalize text block
        if self.is_text_block {
            // Include both committed lines AND pending text in the content block
            let mut text = self.committed_lines.join("\n");
            if !self.pending_text.is_empty() {
                if !text.is_empty() {
                    text.push('\n');
                }
                text.push_str(&self.pending_text);
                // Mark that pending_text is now included in the block
                // so into_response() won't double-count it
                self.pending_in_block = true;
            }

            if !text.is_empty() {
                self.content_blocks.push(ContentBlock::Text { text });
            }
            self.committed_lines.clear();
            self.is_text_block = false;
            return;
        }

        // Finalize tool block
        if let (Some(id), Some(name)) = (self.tool_id.take(), self.tool_name.take()) {
            let input = if self.tool_json_buffer.is_empty() {
                serde_json::Value::Null
            } else {
                serde_json::from_str(&self.tool_json_buffer).unwrap_or_else(|e| {
                    tracing::warn!("Failed to parse tool JSON: {}", e);
                    serde_json::Value::Null
                })
            };
            self.content_blocks
                .push(ContentBlock::ToolUse { id, name, input });
            self.tool_json_buffer.clear();
            return;
        }

        // Finalize thinking block
        if !self.thinking_buffer.is_empty() {
            self.content_blocks.push(ContentBlock::Thinking {
                thinking: std::mem::take(&mut self.thinking_buffer),
                signature: self.thinking_signature.take(),
            });
        }
    }

    /// Get all committed lines so far
    pub fn committed_lines(&self) -> &[String] {
        &self.committed_lines
    }

    /// Get pending (uncommitted) text
    pub fn pending_text(&self) -> &str {
        &self.pending_text
    }

    /// Check if the stream has finished
    pub fn is_finished(&self) -> bool {
        self.finished
    }

    /// Get current accumulated text (committed + pending)
    pub fn current_text(&self) -> String {
        let mut text = self.committed_lines.join("\n");
        if !self.pending_text.is_empty() {
            if !text.is_empty() {
                text.push('\n');
            }
            text.push_str(&self.pending_text);
        }
        text
    }

    /// Build final response from accumulated data
    pub fn into_response(mut self) -> ModelResponse {
        // Ensure any in-progress block is finalized
        if self.current_block_index.is_some() {
            self.finalize_current_block();
        }

        // Handle any remaining text that wasn't part of a finalized block
        // Only add if pending_text wasn't already included in a block
        if !self.committed_lines.is_empty()
            || (!self.pending_text.is_empty() && !self.pending_in_block)
        {
            let mut text = self.committed_lines.join("\n");
            if !self.pending_text.is_empty() && !self.pending_in_block {
                if !text.is_empty() {
                    text.push('\n');
                }
                text.push_str(&self.pending_text);
            }
            if !text.is_empty() {
                self.content_blocks.push(ContentBlock::Text { text });
            }
        }

        ModelResponse {
            id: self.message_id.unwrap_or_default(),
            model: self.model.unwrap_or_default(),
            content: self.content_blocks,
            stop_reason: None,
            usage: self.usage,
        }
    }

    /// Get the accumulated usage
    pub fn usage(&self) -> &TokenUsage {
        &self.usage
    }
}

impl Default for StreamController {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uira_protocol::StreamMessageStart;

    fn make_message_start() -> StreamChunk {
        StreamChunk::MessageStart {
            message: StreamMessageStart {
                id: "msg_123".to_string(),
                model: "claude-3".to_string(),
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

        // Push partial text - no newline, should buffer
        let lines = controller.push(make_text_delta("Hello"));
        assert!(lines.is_empty());
        assert_eq!(controller.pending_text(), "Hello");

        // Push more with newline - should commit
        let lines = controller.push(make_text_delta(" world\n"));
        assert_eq!(lines, vec!["Hello world"]);
        assert!(controller.pending_text().is_empty());

        // Push multiple lines at once
        let lines = controller.push(make_text_delta("Line 1\nLine 2\nLine 3"));
        assert_eq!(lines, vec!["Line 1", "Line 2"]);
        assert_eq!(controller.pending_text(), "Line 3");
    }

    #[test]
    fn test_drain_on_message_stop() {
        let mut controller = StreamController::new();

        controller.push(make_message_start());
        controller.push(make_text_block_start(0));
        controller.push(make_text_delta("Partial line without newline"));
        controller.push(make_block_stop(0));

        // Message stop should drain pending
        let lines = controller.push(StreamChunk::MessageStop);
        assert_eq!(lines, vec!["Partial line without newline"]);
    }

    #[test]
    fn test_tool_json_accumulation() {
        let mut controller = StreamController::new();

        controller.push(make_message_start());

        // Start tool block
        controller.push(StreamChunk::ContentBlockStart {
            index: 0,
            content_block: ContentBlock::ToolUse {
                id: "tc_123".to_string(),
                name: "read_file".to_string(),
                input: serde_json::Value::Null,
            },
        });

        // Accumulate JSON
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
            assert_eq!(id, "tc_123");
            assert_eq!(name, "read_file");
            assert_eq!(input["path"], "/tmp/test.txt");
        } else {
            panic!("Expected ToolUse block");
        }
    }

    #[test]
    fn test_into_response() {
        let mut controller = StreamController::new();

        controller.push(make_message_start());
        controller.push(make_text_block_start(0));
        controller.push(make_text_delta("Hello\nWorld"));
        controller.push(make_block_stop(0));
        controller.push(StreamChunk::MessageStop);

        let response = controller.into_response();

        assert_eq!(response.id, "msg_123");
        assert_eq!(response.model, "claude-3");
        assert_eq!(response.content.len(), 1);

        if let ContentBlock::Text { text } = &response.content[0] {
            assert_eq!(text, "Hello\nWorld");
        } else {
            panic!("Expected Text block");
        }
    }

    #[test]
    fn test_current_text() {
        let mut controller = StreamController::new();

        controller.push(make_message_start());
        controller.push(make_text_block_start(0));
        controller.push(make_text_delta("Line 1\nLine 2\nPartial"));

        let text = controller.current_text();
        assert_eq!(text, "Line 1\nLine 2\nPartial");
    }

    #[test]
    fn test_empty_stream() {
        let mut controller = StreamController::new();

        controller.push(make_message_start());
        controller.push(StreamChunk::MessageStop);

        let response = controller.into_response();
        assert!(response.content.is_empty());
    }
}
