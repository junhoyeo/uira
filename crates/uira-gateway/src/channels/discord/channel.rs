use std::sync::Arc;

use async_trait::async_trait;
use serenity::all::{ChannelId, CreateMessage, EditMessage, Http};
use tokio::sync::{mpsc, RwLock};
use tokio::task::JoinHandle;
use tracing::{debug, error, info};
use uira_core::schema::DiscordChannelConfig;

use super::chunk::{chunk_discord_text, ChunkOpts};
use super::components::ComponentRegistry;
use super::config::{normalize_discord_token, resolve_discord_account};
use super::handler::{build_gateway_intents, DiscordHandler};
use crate::channels::channel::Channel;
use crate::channels::error::ChannelError;
use crate::channels::types::{ChannelCapabilities, ChannelMessage, ChannelResponse, ChannelType};

pub struct DiscordChannel {
    config: DiscordChannelConfig,
    message_tx: Option<mpsc::Sender<ChannelMessage>>,
    message_rx: Option<mpsc::Receiver<ChannelMessage>>,
    bot_handle: Option<JoinHandle<()>>,
    http: Option<Arc<Http>>,
    component_registry: Arc<ComponentRegistry>,
    bot_user_id: Arc<RwLock<Option<u64>>>,
}

impl DiscordChannel {
    pub fn new(config: DiscordChannelConfig) -> Self {
        let (tx, rx) = mpsc::channel(256);
        Self {
            config,
            message_tx: Some(tx),
            message_rx: Some(rx),
            bot_handle: None,
            http: None,
            component_registry: Arc::new(ComponentRegistry::new()),
            bot_user_id: Arc::new(RwLock::new(None)),
        }
    }

    pub fn component_registry(&self) -> &Arc<ComponentRegistry> {
        &self.component_registry
    }

    fn chunk_opts(&self) -> ChunkOpts {
        ChunkOpts {
            max_chars: self.config.text_chunk_limit,
            max_lines: self.config.max_lines_per_message,
        }
    }
}

#[async_trait]
impl Channel for DiscordChannel {
    fn channel_type(&self) -> ChannelType {
        ChannelType::Discord
    }

    fn capabilities(&self) -> ChannelCapabilities {
        let supports_streaming = self.config.stream_mode != "off";
        ChannelCapabilities {
            max_message_length: self.config.text_chunk_limit,
            supports_markdown: true,
            supports_streaming,
            stream_throttle_ms: Some(self.config.stream_throttle_ms),
        }
    }

    async fn start(&mut self) -> Result<(), ChannelError> {
        let account = resolve_discord_account(&self.config);
        if !account.enabled {
            return Err(ChannelError::Other(format!(
                "Discord account '{}' is disabled",
                account.account_id
            )));
        }

        let token = normalize_discord_token(&account.token);
        if token.is_empty() {
            return Err(ChannelError::AuthError(
                "Discord bot token is empty".to_string(),
            ));
        }

        let intents = build_gateway_intents(&self.config);

        let message_tx = self
            .message_tx
            .clone()
            .ok_or_else(|| ChannelError::Other("Message sender not available".to_string()))?;

        let handler = DiscordHandler {
            config: self.config.clone(),
            message_tx,
            component_registry: Arc::clone(&self.component_registry),
            bot_user_id: Arc::clone(&self.bot_user_id),
        };

        let http = Arc::new(Http::new(&format!("Bot {token}")));
        self.http = Some(Arc::clone(&http));

        let token_owned = token.clone();
        let handle = tokio::spawn(async move {
            let client = serenity::Client::builder(&token_owned, intents)
                .event_handler(handler)
                .await;

            match client {
                Ok(mut client) => {
                    info!("Starting Discord gateway connection");
                    if let Err(e) = client.start().await {
                        error!(error = %e, "Discord client error");
                    }
                }
                Err(e) => {
                    error!(error = %e, "Failed to build Discord client");
                }
            }
        });

        self.bot_handle = Some(handle);
        info!(
            account_id = %account.account_id,
            "Discord channel started"
        );
        Ok(())
    }

    async fn stop(&mut self) -> Result<(), ChannelError> {
        if let Some(handle) = self.bot_handle.take() {
            handle.abort();
            debug!("Discord bot task aborted");
        }
        self.component_registry.clear();
        info!("Discord channel stopped");
        Ok(())
    }

    async fn send_message(&self, response: ChannelResponse) -> Result<(), ChannelError> {
        let http = self
            .http
            .as_ref()
            .ok_or_else(|| ChannelError::SendFailed("Discord not connected".to_string()))?;

        let channel_id: u64 = response
            .recipient
            .parse()
            .map_err(|_| ChannelError::SendFailed(format!("Invalid channel ID: {}", response.recipient)))?;

        let chunks = chunk_discord_text(&response.content, &self.chunk_opts());

        for chunk in chunks {
            let msg = CreateMessage::new().content(&chunk);
            ChannelId::new(channel_id)
                .send_message(http.as_ref(), msg)
                .await
                .map_err(|e| ChannelError::SendFailed(format!("Discord send error: {e}")))?;
        }

        Ok(())
    }

    async fn send_message_returning_id(
        &self,
        response: ChannelResponse,
    ) -> Result<Option<String>, ChannelError> {
        let http = self
            .http
            .as_ref()
            .ok_or_else(|| ChannelError::SendFailed("Discord not connected".to_string()))?;

        let channel_id: u64 = response
            .recipient
            .parse()
            .map_err(|_| ChannelError::SendFailed(format!("Invalid channel ID: {}", response.recipient)))?;

        let chunks = chunk_discord_text(&response.content, &self.chunk_opts());
        let mut last_message_id = None;

        for chunk in chunks {
            let msg = CreateMessage::new().content(&chunk);
            let sent = ChannelId::new(channel_id)
                .send_message(http.as_ref(), msg)
                .await
                .map_err(|e| ChannelError::SendFailed(format!("Discord send error: {e}")))?;
            last_message_id = Some(sent.id.get().to_string());
        }

        Ok(last_message_id)
    }

    async fn edit_message(
        &self,
        recipient: &str,
        message_id: &str,
        new_content: &str,
    ) -> Result<(), ChannelError> {
        let http = self
            .http
            .as_ref()
            .ok_or_else(|| ChannelError::SendFailed("Discord not connected".to_string()))?;

        let channel_id: u64 = recipient
            .parse()
            .map_err(|_| ChannelError::SendFailed(format!("Invalid channel ID: {recipient}")))?;

        let msg_id: u64 = message_id
            .parse()
            .map_err(|_| ChannelError::SendFailed(format!("Invalid message ID: {message_id}")))?;

        let truncated = if new_content.len() > self.config.text_chunk_limit {
            let boundary = crate::channels::types::floor_char_boundary(
                new_content,
                self.config.text_chunk_limit,
            );
            &new_content[..boundary]
        } else {
            new_content
        };

        let edit = EditMessage::new().content(truncated);
        ChannelId::new(channel_id)
            .edit_message(http.as_ref(), serenity::all::MessageId::new(msg_id), edit)
            .await
            .map_err(|e| ChannelError::SendFailed(format!("Discord edit error: {e}")))?;

        Ok(())
    }

    fn take_message_receiver(&mut self) -> Option<mpsc::Receiver<ChannelMessage>> {
        self.message_rx.take()
    }
}
