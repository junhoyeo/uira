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

    /// Send a message and return its message ID for later editing.
    /// Returns None if the channel doesn't support streaming.
    async fn send_message_returning_id(
        &self,
        response: ChannelResponse,
    ) -> Result<Option<String>, ChannelError> {
        self.send_message(response).await?;
        Ok(None)
    }

    /// Edit an existing message by its ID.
    async fn edit_message(
        &self,
        recipient: &str,
        message_id: &str,
        new_content: &str,
    ) -> Result<(), ChannelError> {
        let _ = (recipient, message_id, new_content);
        Ok(())
    }

    fn take_message_receiver(&mut self) -> Option<mpsc::Receiver<ChannelMessage>>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::MockChannel;
    use chrono::Utc;

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
        assert!(!caps.supports_streaming);
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

        let sent = channel.sent_messages();
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
