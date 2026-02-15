//! Telegram channel implementation using teloxide.
//!
//! Provides a [`TelegramChannel`] that implements the [`Channel`] trait,
//! allowing Uira to receive and send messages via the Telegram Bot API.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use chrono::Utc;
use teloxide::prelude::*;
use teloxide::types::Me;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tracing::{debug, error, info, warn};
use uira_core::schema::TelegramChannelConfig;

use super::channel::Channel;
use super::error::ChannelError;
use super::types::{ChannelCapabilities, ChannelMessage, ChannelResponse, ChannelType};

/// Maximum message length for Telegram messages (in characters).
const TELEGRAM_MAX_MESSAGE_LENGTH: usize = 4096;

/// Telegram channel that communicates via the Telegram Bot API.
///
/// Uses teloxide for polling-based update handling. Inbound text messages
/// are converted to [`ChannelMessage`] and forwarded through an mpsc channel.
/// Outbound messages are sent via the Telegram Bot API, automatically chunked
/// if they exceed Telegram's 4096-character limit.
pub struct TelegramChannel {
    config: TelegramChannelConfig,
    message_tx: Option<mpsc::Sender<ChannelMessage>>,
    message_rx: Option<mpsc::Receiver<ChannelMessage>>,
    bot_handle: Option<JoinHandle<()>>,
    sent_messages: Arc<Mutex<Vec<ChannelResponse>>>,
}

impl TelegramChannel {
    /// Create a new Telegram channel from the given configuration.
    pub fn new(config: TelegramChannelConfig) -> Self {
        let (tx, rx) = mpsc::channel(256);
        Self {
            config,
            message_tx: Some(tx),
            message_rx: Some(rx),
            bot_handle: None,
            sent_messages: Arc::new(Mutex::new(Vec::new())),
        }
    }
}

/// Check whether a user is allowed based on the allowed_users list.
///
/// If `allowed_users` is empty, all users are allowed.
/// Otherwise, the username or user ID (as string) must be in the list.
pub fn is_user_allowed(allowed_users: &[String], username: Option<&str>, user_id: u64) -> bool {
    if allowed_users.is_empty() {
        return true;
    }
    let user_id_str = user_id.to_string();
    for allowed in allowed_users {
        if allowed == &user_id_str {
            return true;
        }
        if let Some(uname) = username {
            // Allow matching with or without leading '@'
            let allowed_trimmed = allowed.strip_prefix('@').unwrap_or(allowed);
            if uname == allowed_trimmed {
                return true;
            }
        }
    }
    false
}

/// Chunk a message into pieces that fit within Telegram's character limit.
///
/// Attempts to split at newline boundaries when possible, falling back
/// to hard splits at the maximum length.
pub fn chunk_message(text: &str, max_len: usize) -> Vec<String> {
    if text.len() <= max_len {
        return vec![text.to_string()];
    }

    let mut chunks = Vec::new();
    let mut remaining = text;

    while !remaining.is_empty() {
        if remaining.len() <= max_len {
            chunks.push(remaining.to_string());
            break;
        }

        let split_at = remaining[..max_len]
            .rfind('\n')
            .map(|pos| pos + 1)
            .unwrap_or(max_len);

        let (chunk, rest) = remaining.split_at(split_at);
        chunks.push(chunk.to_string());
        remaining = rest;
    }

    chunks
}

/// Convert a teloxide [`Message`] into a [`ChannelMessage`].
///
/// Returns `None` if the message has no text content or no sender.
pub fn telegram_message_to_channel_message(
    msg: &Message,
    bot_username: &str,
) -> Option<ChannelMessage> {
    let text = msg.text()?;
    let from = msg.from.as_ref()?;

    let sender = from.username.as_deref().unwrap_or("unknown").to_string();

    let mut metadata = HashMap::new();
    metadata.insert("chat_id".to_string(), msg.chat.id.to_string());
    metadata.insert("message_id".to_string(), msg.id.to_string());
    metadata.insert("user_id".to_string(), from.id.to_string());
    metadata.insert("bot_username".to_string(), bot_username.to_string());

    if let Some(ref uname) = from.username {
        metadata.insert("username".to_string(), uname.clone());
    }

    Some(ChannelMessage {
        sender,
        content: text.to_string(),
        channel_type: ChannelType::Telegram,
        channel_id: msg.chat.id.to_string(),
        timestamp: Utc::now(),
        metadata,
    })
}

#[async_trait]
impl Channel for TelegramChannel {
    fn channel_type(&self) -> ChannelType {
        ChannelType::Telegram
    }

    fn capabilities(&self) -> ChannelCapabilities {
        ChannelCapabilities {
            max_message_length: TELEGRAM_MAX_MESSAGE_LENGTH,
            supports_markdown: true,
        }
    }

    async fn start(&mut self) -> Result<(), ChannelError> {
        if self.bot_handle.is_some() {
            return Err(ChannelError::Other(
                "Telegram channel already started".to_string(),
            ));
        }

        let bot = Bot::new(&self.config.bot_token);

        let me: Me = bot
            .get_me()
            .await
            .map_err(|e| ChannelError::ConnectionFailed(format!("Failed to get bot info: {e}")))?;
        let bot_username = me.username().to_string();

        info!(
            "Telegram bot @{} started, listening for messages",
            bot_username
        );

        let message_tx = self
            .message_tx
            .clone()
            .ok_or_else(|| ChannelError::ChannelClosed)?;
        let allowed_users = self.config.allowed_users.clone();

        let handle = tokio::spawn(async move {
            let handler = Update::filter_message().endpoint(move |_bot: Bot, msg: Message| {
                let tx = message_tx.clone();
                let allowed = allowed_users.clone();
                let bot_uname = bot_username.clone();

                async move {
                    if let Some(from) = &msg.from {
                        if !is_user_allowed(&allowed, from.username.as_deref(), from.id.0) {
                            debug!(
                                "Ignoring message from non-allowed user: {:?} (id: {})",
                                from.username, from.id
                            );
                            return Ok(());
                        }
                    } else {
                        warn!("Ignoring message with no sender");
                        return Ok(());
                    }

                    if let Some(channel_msg) = telegram_message_to_channel_message(&msg, &bot_uname)
                    {
                        if let Err(e) = tx.send(channel_msg).await {
                            error!("Failed to forward Telegram message: {}", e);
                        }
                    }

                    respond(())
                }
            });

            Dispatcher::builder(bot, handler).build().dispatch().await;
        });

        self.bot_handle = Some(handle);
        Ok(())
    }

    async fn stop(&mut self) -> Result<(), ChannelError> {
        if let Some(handle) = self.bot_handle.take() {
            handle.abort();
            info!("Telegram polling task aborted");
        }

        self.message_tx.take();

        Ok(())
    }

    async fn send_message(&self, response: ChannelResponse) -> Result<(), ChannelError> {
        let bot = Bot::new(&self.config.bot_token);

        let chat_id: i64 = response.recipient.parse().map_err(|e| {
            ChannelError::SendFailed(format!("Invalid chat_id '{}': {e}", response.recipient))
        })?;

        let chunks = chunk_message(&response.content, TELEGRAM_MAX_MESSAGE_LENGTH);

        for chunk in &chunks {
            bot.send_message(ChatId(chat_id), chunk)
                .await
                .map_err(|e| ChannelError::SendFailed(format!("Telegram send error: {e}")))?;
        }

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

    fn test_config() -> TelegramChannelConfig {
        TelegramChannelConfig {
            bot_token: "test:fake-token".to_string(),
            allowed_users: Vec::new(),
        }
    }

    #[test]
    fn test_chunk_message_short() {
        let text = "Hello, world!";
        let chunks = chunk_message(text, 4096);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0], "Hello, world!");
    }

    #[test]
    fn test_chunk_message_exact_limit() {
        let text = "a".repeat(4096);
        let chunks = chunk_message(&text, 4096);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].len(), 4096);
    }

    #[test]
    fn test_chunk_message_exceeds_limit() {
        let text = "a".repeat(8192);
        let chunks = chunk_message(&text, 4096);
        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[0].len(), 4096);
        assert_eq!(chunks[1].len(), 4096);
    }

    #[test]
    fn test_chunk_message_splits_at_newline() {
        let mut text = "a".repeat(4000);
        text.push('\n');
        text.push_str(&"b".repeat(4000));

        let chunks = chunk_message(&text, 4096);
        assert_eq!(chunks.len(), 2);
        assert!(chunks[0].ends_with('\n'));
        assert!(chunks[0].len() <= 4096);
        assert!(chunks[1].starts_with('b'));
    }

    #[test]
    fn test_chunk_message_multiple_chunks() {
        let text = "a".repeat(12288);
        let chunks = chunk_message(&text, 4096);
        assert_eq!(chunks.len(), 3);
        for chunk in &chunks {
            assert_eq!(chunk.len(), 4096);
        }
    }

    #[test]
    fn test_chunk_message_empty() {
        let chunks = chunk_message("", 4096);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0], "");
    }

    #[test]
    fn test_chunk_message_uneven_split() {
        let text = "a".repeat(5000);
        let chunks = chunk_message(&text, 4096);
        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[0].len(), 4096);
        assert_eq!(chunks[1].len(), 904);
    }

    #[test]
    fn test_is_user_allowed_empty_list() {
        assert!(is_user_allowed(&[], Some("anyone"), 12345));
        assert!(is_user_allowed(&[], None, 12345));
    }

    #[test]
    fn test_is_user_allowed_by_id() {
        let allowed = vec!["12345".to_string()];
        assert!(is_user_allowed(&allowed, Some("john"), 12345));
        assert!(!is_user_allowed(&allowed, Some("john"), 99999));
    }

    #[test]
    fn test_is_user_allowed_by_username() {
        let allowed = vec!["john".to_string()];
        assert!(is_user_allowed(&allowed, Some("john"), 99999));
        assert!(!is_user_allowed(&allowed, Some("jane"), 99999));
    }

    #[test]
    fn test_is_user_allowed_by_username_with_at() {
        let allowed = vec!["@john".to_string()];
        assert!(is_user_allowed(&allowed, Some("john"), 99999));
    }

    #[test]
    fn test_is_user_allowed_no_username() {
        let allowed = vec!["john".to_string()];
        assert!(!is_user_allowed(&allowed, None, 99999));
    }

    #[test]
    fn test_is_user_allowed_multiple_entries() {
        let allowed = vec!["alice".to_string(), "12345".to_string(), "@bob".to_string()];
        assert!(is_user_allowed(&allowed, Some("alice"), 99));
        assert!(is_user_allowed(&allowed, Some("unknown"), 12345));
        assert!(is_user_allowed(&allowed, Some("bob"), 99));
        assert!(!is_user_allowed(&allowed, Some("eve"), 0));
    }

    #[test]
    fn test_telegram_channel_creation() {
        let config = test_config();
        let channel = TelegramChannel::new(config.clone());

        assert_eq!(channel.channel_type(), ChannelType::Telegram);
        assert!(channel.message_tx.is_some());
        assert!(channel.message_rx.is_some());
        assert!(channel.bot_handle.is_none());
        assert!(channel.sent_messages.lock().unwrap().is_empty());
    }

    #[test]
    fn test_telegram_channel_capabilities() {
        let channel = TelegramChannel::new(test_config());
        let caps = channel.capabilities();

        assert_eq!(caps.max_message_length, 4096);
        assert!(caps.supports_markdown);
    }

    #[tokio::test]
    async fn test_telegram_channel_take_receiver() {
        let mut channel = TelegramChannel::new(test_config());

        let rx = channel.take_message_receiver();
        assert!(rx.is_some());

        let rx2 = channel.take_message_receiver();
        assert!(rx2.is_none());
    }

    #[tokio::test]
    async fn test_telegram_channel_stop_without_start() {
        let mut channel = TelegramChannel::new(test_config());
        let result = channel.stop().await;
        assert!(result.is_ok());
    }

    #[test]
    fn test_max_message_length_constant() {
        assert_eq!(TELEGRAM_MAX_MESSAGE_LENGTH, 4096);
    }

    #[test]
    fn test_channel_type_is_telegram() {
        let channel = TelegramChannel::new(test_config());
        assert_eq!(channel.channel_type(), ChannelType::Telegram);
        assert_eq!(channel.channel_type().to_string(), "telegram");
    }
}
