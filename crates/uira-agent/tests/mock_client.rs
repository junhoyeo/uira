//! Mock model client for integration testing
//!
//! Provides a MockModelClient that can be configured with queued responses
//! and tracks all messages sent to it.

use async_trait::async_trait;
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use uira_protocol::{
    ContentBlock, Message, ModelResponse, StopReason, StreamChunk, TokenUsage, ToolSpec,
};
use uira_providers::{ModelClient, ModelResult, ProviderError, ResponseStream};

/// Mock model client for testing
pub struct MockModelClient {
    /// Queued responses to return
    responses: Arc<Mutex<VecDeque<MockResponse>>>,
    /// All messages that have been sent
    recorded_messages: Arc<Mutex<Vec<Vec<Message>>>>,
    /// Model name
    model: String,
    /// Provider name
    provider: String,
    /// Max tokens
    max_tokens: usize,
}

/// A queued response
#[derive(Clone)]
pub enum MockResponse {
    /// Text response
    Text(String),
    /// Tool call response
    ToolCall {
        id: String,
        name: String,
        input: serde_json::Value,
    },
    /// Error response
    Error(String),
    /// Multiple content blocks
    Blocks(Vec<ContentBlock>),
}

impl MockModelClient {
    /// Create a new mock client
    pub fn new() -> Self {
        Self {
            responses: Arc::new(Mutex::new(VecDeque::new())),
            recorded_messages: Arc::new(Mutex::new(Vec::new())),
            model: "mock-model".to_string(),
            provider: "mock".to_string(),
            max_tokens: 128_000,
        }
    }

    /// Queue a text response
    pub fn queue_text(&self, text: impl Into<String>) {
        let mut responses = self.responses.lock().unwrap();
        responses.push_back(MockResponse::Text(text.into()));
    }

    /// Queue a tool call response
    pub fn queue_tool_call(
        &self,
        id: impl Into<String>,
        name: impl Into<String>,
        input: serde_json::Value,
    ) {
        let mut responses = self.responses.lock().unwrap();
        responses.push_back(MockResponse::ToolCall {
            id: id.into(),
            name: name.into(),
            input,
        });
    }

    /// Queue an error response
    pub fn queue_error(&self, message: impl Into<String>) {
        let mut responses = self.responses.lock().unwrap();
        responses.push_back(MockResponse::Error(message.into()));
    }

    /// Queue a response with multiple content blocks
    pub fn queue_blocks(&self, blocks: Vec<ContentBlock>) {
        let mut responses = self.responses.lock().unwrap();
        responses.push_back(MockResponse::Blocks(blocks));
    }

    /// Get all recorded message calls
    pub fn recorded_messages(&self) -> Vec<Vec<Message>> {
        self.recorded_messages.lock().unwrap().clone()
    }

    /// Get the number of times chat was called
    pub fn call_count(&self) -> usize {
        self.recorded_messages.lock().unwrap().len()
    }

    /// Clear recorded messages
    #[allow(dead_code)]
    pub fn clear_recorded(&self) {
        self.recorded_messages.lock().unwrap().clear();
    }

    /// Check if there are queued responses
    #[allow(dead_code)]
    pub fn has_responses(&self) -> bool {
        !self.responses.lock().unwrap().is_empty()
    }

    fn next_response(&self) -> Option<MockResponse> {
        self.responses.lock().unwrap().pop_front()
    }

    fn make_response(&self, mock: MockResponse) -> ModelResult<ModelResponse> {
        match mock {
            MockResponse::Text(text) => Ok(ModelResponse {
                id: format!("msg_{}", uuid::Uuid::new_v4()),
                model: self.model.clone(),
                content: vec![ContentBlock::Text { text }],
                stop_reason: Some(StopReason::EndTurn),
                usage: TokenUsage {
                    input_tokens: 100,
                    output_tokens: 50,
                    ..Default::default()
                },
            }),
            MockResponse::ToolCall { id, name, input } => Ok(ModelResponse {
                id: format!("msg_{}", uuid::Uuid::new_v4()),
                model: self.model.clone(),
                content: vec![ContentBlock::ToolUse { id, name, input }],
                stop_reason: Some(StopReason::ToolUse),
                usage: TokenUsage {
                    input_tokens: 100,
                    output_tokens: 50,
                    ..Default::default()
                },
            }),
            MockResponse::Error(message) => Err(ProviderError::InvalidResponse(message)),
            MockResponse::Blocks(blocks) => {
                let stop_reason = if blocks
                    .iter()
                    .any(|b| matches!(b, ContentBlock::ToolUse { .. }))
                {
                    Some(StopReason::ToolUse)
                } else {
                    Some(StopReason::EndTurn)
                };

                Ok(ModelResponse {
                    id: format!("msg_{}", uuid::Uuid::new_v4()),
                    model: self.model.clone(),
                    content: blocks,
                    stop_reason,
                    usage: TokenUsage {
                        input_tokens: 100,
                        output_tokens: 50,
                        ..Default::default()
                    },
                })
            }
        }
    }
}

impl Default for MockModelClient {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ModelClient for MockModelClient {
    async fn chat(&self, messages: &[Message], _tools: &[ToolSpec]) -> ModelResult<ModelResponse> {
        // Record the messages
        self.recorded_messages
            .lock()
            .unwrap()
            .push(messages.to_vec());

        // Return next queued response
        match self.next_response() {
            Some(mock) => self.make_response(mock),
            None => {
                // No more responses, return a default
                Ok(ModelResponse {
                    id: format!("msg_{}", uuid::Uuid::new_v4()),
                    model: self.model.clone(),
                    content: vec![ContentBlock::Text {
                        text: "No more queued responses".to_string(),
                    }],
                    stop_reason: Some(StopReason::EndTurn),
                    usage: TokenUsage::default(),
                })
            }
        }
    }

    async fn chat_stream(
        &self,
        messages: &[Message],
        _tools: &[ToolSpec],
    ) -> ModelResult<ResponseStream> {
        // For now, just convert to blocking response
        // A more sophisticated implementation would stream chunks
        self.recorded_messages
            .lock()
            .unwrap()
            .push(messages.to_vec());

        let response = match self.next_response() {
            Some(mock) => self.make_response(mock)?,
            None => ModelResponse {
                id: format!("msg_{}", uuid::Uuid::new_v4()),
                model: self.model.clone(),
                content: vec![ContentBlock::Text {
                    text: "No more queued responses".to_string(),
                }],
                stop_reason: Some(StopReason::EndTurn),
                usage: TokenUsage::default(),
            },
        };

        // Create a stream that yields the response as chunks
        let chunks = response_to_chunks(response);
        let stream = futures::stream::iter(chunks.into_iter().map(Ok));

        Ok(Box::pin(stream))
    }

    fn supports_tools(&self) -> bool {
        true
    }

    fn max_tokens(&self) -> usize {
        self.max_tokens
    }

    fn model(&self) -> &str {
        &self.model
    }

    fn provider(&self) -> &str {
        &self.provider
    }
}

/// Convert a ModelResponse to stream chunks
fn response_to_chunks(response: ModelResponse) -> Vec<StreamChunk> {
    use uira_protocol::{ContentDelta, StreamMessageStart};

    let mut chunks = Vec::new();

    // Message start
    chunks.push(StreamChunk::MessageStart {
        message: StreamMessageStart {
            id: response.id.clone(),
            model: response.model.clone(),
            usage: response.usage.clone(),
        },
    });

    // Content blocks
    for (index, block) in response.content.iter().enumerate() {
        // Block start
        chunks.push(StreamChunk::ContentBlockStart {
            index,
            content_block: block.clone(),
        });

        // Block delta
        match block {
            ContentBlock::Text { text } => {
                // Split into lines for more realistic streaming
                for line in text.lines() {
                    chunks.push(StreamChunk::ContentBlockDelta {
                        index,
                        delta: ContentDelta::TextDelta {
                            text: format!("{}\n", line),
                        },
                    });
                }
            }
            ContentBlock::ToolUse { input, .. } => {
                chunks.push(StreamChunk::ContentBlockDelta {
                    index,
                    delta: ContentDelta::InputJsonDelta {
                        partial_json: input.to_string(),
                    },
                });
            }
            _ => {}
        }

        // Block stop
        chunks.push(StreamChunk::ContentBlockStop { index });
    }

    // Message delta with final usage
    chunks.push(StreamChunk::MessageDelta {
        delta: uira_protocol::MessageDelta {
            stop_reason: response.stop_reason.clone(),
        },
        usage: Some(response.usage),
    });

    // Message stop
    chunks.push(StreamChunk::MessageStop);

    chunks
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_mock_client_text_response() {
        let client = MockModelClient::new();
        client.queue_text("Hello, world!");

        let response = client.chat(&[Message::user("Hi")], &[]).await.unwrap();

        assert_eq!(response.text(), "Hello, world!");
        assert_eq!(client.call_count(), 1);
    }

    #[tokio::test]
    async fn test_mock_client_tool_call() {
        let client = MockModelClient::new();
        client.queue_tool_call(
            "tc_123",
            "read_file",
            serde_json::json!({"path": "/tmp/test.txt"}),
        );

        let response = client
            .chat(&[Message::user("Read a file")], &[])
            .await
            .unwrap();

        assert!(response.has_tool_calls());
        let tool_calls = response.tool_calls();
        assert_eq!(tool_calls.len(), 1);
        assert_eq!(tool_calls[0].name, "read_file");
    }

    #[tokio::test]
    async fn test_mock_client_error() {
        let client = MockModelClient::new();
        client.queue_error("API rate limit exceeded");

        let result = client.chat(&[Message::user("Hi")], &[]).await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_mock_client_multiple_responses() {
        let client = MockModelClient::new();
        client.queue_tool_call("tc_1", "bash", serde_json::json!({"command": "ls"}));
        client.queue_text("Here are the files...");

        // First call - tool call
        let response1 = client
            .chat(&[Message::user("List files")], &[])
            .await
            .unwrap();
        assert!(response1.has_tool_calls());

        // Second call - text response
        let response2 = client
            .chat(
                &[
                    Message::user("List files"),
                    Message::with_blocks(
                        uira_protocol::Role::Assistant,
                        vec![ContentBlock::ToolUse {
                            id: "tc_1".to_string(),
                            name: "bash".to_string(),
                            input: serde_json::json!({"command": "ls"}),
                        }],
                    ),
                    Message::with_blocks(
                        uira_protocol::Role::User,
                        vec![ContentBlock::tool_result("tc_1", "file1.txt\nfile2.txt")],
                    ),
                ],
                &[],
            )
            .await
            .unwrap();

        assert!(!response2.has_tool_calls());
        assert_eq!(response2.text(), "Here are the files...");
        assert_eq!(client.call_count(), 2);
    }

    #[tokio::test]
    async fn test_mock_client_recorded_messages() {
        let client = MockModelClient::new();
        client.queue_text("Response 1");
        client.queue_text("Response 2");

        let _ = client.chat(&[Message::user("First message")], &[]).await;
        let _ = client.chat(&[Message::user("Second message")], &[]).await;

        let recorded = client.recorded_messages();
        assert_eq!(recorded.len(), 2);
        assert_eq!(recorded[0].len(), 1);
        assert_eq!(recorded[1].len(), 1);
    }

    #[tokio::test]
    async fn test_mock_client_streaming() {
        use futures::StreamExt;

        let client = MockModelClient::new();
        client.queue_text("Hello\nWorld");

        let mut stream = client
            .chat_stream(&[Message::user("Hi")], &[])
            .await
            .unwrap();

        let mut chunks = Vec::new();
        while let Some(chunk) = stream.next().await {
            chunks.push(chunk.unwrap());
        }

        // Should have: MessageStart, BlockStart, 2x BlockDelta (lines), BlockStop, MessageDelta, MessageStop
        assert!(chunks.len() >= 5);
        assert!(matches!(chunks[0], StreamChunk::MessageStart { .. }));
        assert!(matches!(chunks.last(), Some(StreamChunk::MessageStop)));
    }
}
