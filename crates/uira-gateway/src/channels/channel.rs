use async_trait::async_trait;
use tokio::sync::mpsc;

use super::error::ChannelError;
use super::types::{ChannelCapabilities, ChannelMessage, ChannelResponse, ChannelType};

#[async_trait]
pub trait Channel: Send + Sync {
    fn channel_type(&self) -> ChannelType;

    fn capabilities(&self) -> ChannelCapabilities;

    async fn start(&mut self) -> Result<(), ChannelError>;

    async fn stop(&mut self) -> Result<(), ChannelError>;

    async fn send_message(&self, response: ChannelResponse) -> Result<(), ChannelError>;

    fn take_message_receiver(&mut self) -> Option<mpsc::Receiver<ChannelMessage>>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use std::sync::{Arc, Mutex};

    struct MockChannel {
        channel_type: ChannelType,
        started: bool,
        message_tx: Option<mpsc::Sender<ChannelMessage>>,
        message_rx: Option<mpsc::Receiver<ChannelMessage>>,
        sent_messages: Arc<Mutex<Vec<ChannelResponse>>>,
    }

    impl MockChannel {
        fn new(channel_type: ChannelType) -> Self {
            let (tx, rx) = mpsc::channel(32);
            Self {
                channel_type,
                started: false,
                message_tx: Some(tx),
                message_rx: Some(rx),
                sent_messages: Arc::new(Mutex::new(Vec::new())),
            }
        }

        async fn inject_message(&self, content: &str, sender: &str) {
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

    #[tokio::test]
    async fn test_mock_channel_lifecycle() {
        let mut channel = MockChannel::new(ChannelType::Telegram);

        channel.start().await.unwrap();

        let mut rx = channel.take_message_receiver().unwrap();
        channel.inject_message("hello", "user1").await;

        let msg = rx.recv().await.unwrap();
        assert_eq!(msg.content, "hello");
        assert_eq!(msg.sender, "user1");
        assert_eq!(msg.channel_type, ChannelType::Telegram);

        channel.stop().await.unwrap();
    }

    #[tokio::test]
    async fn test_channel_capabilities() {
        let channel = MockChannel::new(ChannelType::Slack);
        let caps = channel.capabilities();

        assert_eq!(caps.max_message_length, 4096);
        assert!(caps.supports_markdown);
    }

    #[tokio::test]
    async fn test_send_message() {
        let mut channel = MockChannel::new(ChannelType::Telegram);
        channel.start().await.unwrap();

        let response = ChannelResponse {
            content: "reply".to_string(),
            recipient: "user1".to_string(),
        };
        channel.send_message(response).await.unwrap();

        let sent = channel.sent_messages.lock().unwrap();
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0].content, "reply");
        assert_eq!(sent[0].recipient, "user1");
    }

    #[tokio::test]
    async fn test_channel_type_display() {
        assert_eq!(ChannelType::Telegram.to_string(), "telegram");
        assert_eq!(ChannelType::Slack.to_string(), "slack");
    }

    #[tokio::test]
    async fn test_channel_message_serialization() {
        let msg = ChannelMessage {
            sender: "user1".to_string(),
            content: "test message".to_string(),
            channel_type: ChannelType::Slack,
            channel_id: "ch-1".to_string(),
            timestamp: Utc::now(),
            metadata: Default::default(),
        };

        let json = serde_json::to_string(&msg).unwrap();
        let deserialized: ChannelMessage = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.sender, "user1");
        assert_eq!(deserialized.content, "test message");
        assert_eq!(deserialized.channel_type, ChannelType::Slack);
        assert_eq!(deserialized.channel_id, "ch-1");
    }
}
