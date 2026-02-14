use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use chrono::Utc;
use futures_util::stream;
use tokio::sync::mpsc;

use uira_providers::{ModelClient, ModelResult, ProviderError, ResponseStream};
use uira_types::{ContentBlock, ContentDelta, ModelResponse, StreamChunk, TokenUsage};

use crate::channels::channel::Channel;
use crate::channels::error::ChannelError;
use crate::channels::types::{
    ChannelCapabilities, ChannelMessage, ChannelResponse, ChannelType,
};

pub struct MockModelClient {
    response_text: String,
    delay: Option<Duration>,
    error: Mutex<Option<ProviderError>>,
    call_count: AtomicUsize,
}

impl MockModelClient {
    pub fn new(response: &str) -> Self {
        Self {
            response_text: response.to_string(),
            delay: None,
            error: Mutex::new(None),
            call_count: AtomicUsize::new(0),
        }
    }

    pub fn with_delay(mut self, delay: Duration) -> Self {
        self.delay = Some(delay);
        self
    }

    pub fn with_error(self, error: ProviderError) -> Self {
        *self.error.lock().unwrap() = Some(error);
        self
    }

    pub fn call_count(&self) -> usize {
        self.call_count.load(Ordering::SeqCst)
    }
}

#[async_trait]
impl ModelClient for MockModelClient {
    async fn chat(
        &self,
        _messages: &[uira_types::Message],
        _tools: &[uira_types::ToolSpec],
    ) -> ModelResult<ModelResponse> {
        self.call_count.fetch_add(1, Ordering::SeqCst);

        if let Some(delay) = self.delay {
            tokio::time::sleep(delay).await;
        }

        {
            let mut err_guard = self.error.lock().unwrap();
            if let Some(err) = err_guard.take() {
                // ProviderError is not Clone, so error mode is single-use
                return Err(err);
            }
        }

        Ok(ModelResponse {
            id: "mock-response-id".to_string(),
            model: "mock-model".to_string(),
            content: vec![ContentBlock::Text {
                text: self.response_text.clone(),
            }],
            stop_reason: Some(uira_types::StopReason::EndTurn),
            usage: TokenUsage::default(),
        })
    }

    async fn chat_stream(
        &self,
        _messages: &[uira_types::Message],
        _tools: &[uira_types::ToolSpec],
    ) -> ModelResult<ResponseStream> {
        let text = self.response_text.clone();
        let chunks: Vec<Result<StreamChunk, ProviderError>> = vec![
            Ok(StreamChunk::ContentBlockDelta {
                index: 0,
                delta: ContentDelta::TextDelta { text },
            }),
            Ok(StreamChunk::MessageStop),
        ];
        Ok(Box::pin(stream::iter(chunks)))
    }

    fn supports_tools(&self) -> bool {
        true
    }

    fn max_tokens(&self) -> usize {
        128_000
    }

    fn model(&self) -> &str {
        "mock-model"
    }

    fn provider(&self) -> &str {
        "mock"
    }
}

pub struct MockChannel {
    channel_type: ChannelType,
    started: bool,
    message_tx: Option<mpsc::Sender<ChannelMessage>>,
    message_rx: Option<mpsc::Receiver<ChannelMessage>>,
    sent_messages: Arc<Mutex<Vec<ChannelResponse>>>,
}

impl MockChannel {
    pub fn new(channel_type: ChannelType) -> Self {
        let (tx, rx) = mpsc::channel(32);
        Self {
            channel_type,
            started: false,
            message_tx: Some(tx),
            message_rx: Some(rx),
            sent_messages: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub async fn inject_message(&self, content: &str, sender: &str) {
        if let Some(tx) = &self.message_tx {
            let msg = ChannelMessage {
                sender: sender.to_string(),
                content: content.to_string(),
                channel_type: self.channel_type.clone(),
                channel_id: "mock-channel".to_string(),
                timestamp: Utc::now(),
                metadata: Default::default(),
            };
            tx.send(msg).await.unwrap();
        }
    }

    pub fn sent_messages(&self) -> Vec<ChannelResponse> {
        self.sent_messages.lock().unwrap().clone()
    }

    pub fn sent_message_count(&self) -> usize {
        self.sent_messages.lock().unwrap().len()
    }
}

#[async_trait]
impl Channel for MockChannel {
    fn channel_type(&self) -> ChannelType {
        self.channel_type.clone()
    }

    fn capabilities(&self) -> ChannelCapabilities {
        ChannelCapabilities {
            max_message_length: 4096,
            supports_markdown: true,
        }
    }

    async fn start(&mut self) -> Result<(), ChannelError> {
        if self.started {
            return Err(ChannelError::Other("Already started".to_string()));
        }
        self.started = true;
        Ok(())
    }

    async fn stop(&mut self) -> Result<(), ChannelError> {
        if !self.started {
            return Err(ChannelError::ChannelClosed);
        }
        self.started = false;
        self.message_tx.take();
        Ok(())
    }

    async fn send_message(&self, response: ChannelResponse) -> Result<(), ChannelError> {
        self.sent_messages.lock().unwrap().push(response);
        Ok(())
    }

    fn take_message_receiver(&mut self) -> Option<mpsc::Receiver<ChannelMessage>> {
        self.message_rx.take()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_util::StreamExt;

    #[tokio::test]
    async fn test_mock_model_client_returns_configured_response() {
        let client = MockModelClient::new("Hello from mock");
        let response = client.chat(&[], &[]).await.unwrap();

        assert_eq!(response.text(), "Hello from mock");
        assert_eq!(response.model, "mock-model");
        assert_eq!(client.call_count(), 1);

        let _ = client.chat(&[], &[]).await.unwrap();
        assert_eq!(client.call_count(), 2);
    }

    #[tokio::test]
    async fn test_mock_model_client_error_mode() {
        let client = MockModelClient::new("ignored").with_error(
            ProviderError::InvalidResponse("test error".to_string()),
        );

        let result = client.chat(&[], &[]).await;
        assert!(result.is_err());

        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("test error"));

        let result = client.chat(&[], &[]).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_mock_model_client_stream() {
        let client = MockModelClient::new("streamed text");
        let mut stream = client.chat_stream(&[], &[]).await.unwrap();

        let first = stream.next().await.unwrap().unwrap();
        match first {
            StreamChunk::ContentBlockDelta { delta, .. } => match delta {
                ContentDelta::TextDelta { text } => assert_eq!(text, "streamed text"),
                other => panic!("unexpected delta: {:?}", other),
            },
            other => panic!("unexpected chunk: {:?}", other),
        }

        let second = stream.next().await.unwrap().unwrap();
        assert!(matches!(second, StreamChunk::MessageStop));

        assert!(stream.next().await.is_none());
    }

    #[tokio::test]
    async fn test_mock_channel_records_sent_messages() {
        let mut channel = MockChannel::new(ChannelType::Telegram);
        channel.start().await.unwrap();

        let response = ChannelResponse {
            content: "reply text".to_string(),
            recipient: "user1".to_string(),
        };
        channel.send_message(response).await.unwrap();

        assert_eq!(channel.sent_message_count(), 1);
        let sent = channel.sent_messages();
        assert_eq!(sent[0].content, "reply text");
        assert_eq!(sent[0].recipient, "user1");
    }

    #[tokio::test]
    async fn test_mock_channel_inject_message() {
        let mut channel = MockChannel::new(ChannelType::Slack);
        channel.start().await.unwrap();

        let mut rx = channel.take_message_receiver().unwrap();
        channel.inject_message("hello world", "tester").await;

        let msg = rx.recv().await.unwrap();
        assert_eq!(msg.content, "hello world");
        assert_eq!(msg.sender, "tester");
        assert_eq!(msg.channel_type, ChannelType::Slack);
        assert_eq!(msg.channel_id, "mock-channel");
    }
}
