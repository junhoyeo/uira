use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::RwLock;
use tokio::task::JoinHandle;
use tracing::{debug, error, info};

use crate::channels::Channel;

use crate::config::SessionConfig;
use crate::error::GatewayError;
use crate::session_manager::SessionManager;

/// Routes messages between Channel implementations and the SessionManager.
///
/// Maintains a `(channel_type, sender_id) -> session_id` mapping so that
/// each unique sender is automatically associated with a persistent session.
pub struct ChannelBridge {
    session_manager: Arc<SessionManager>,
    sender_sessions: Arc<RwLock<HashMap<(String, String), String>>>,
    channel_handles: Vec<JoinHandle<()>>,
}

impl ChannelBridge {
    /// Create a new ChannelBridge backed by the given SessionManager.
    pub fn new(session_manager: Arc<SessionManager>) -> Self {
        Self {
            session_manager,
            sender_sessions: Arc::new(RwLock::new(HashMap::new())),
            channel_handles: Vec::new(),
        }
    }

    /// Register a channel and start listening for inbound messages.
    ///
    /// 1. Starts the channel
    /// 2. Takes the message receiver
    /// 3. Spawns a tokio task that routes each inbound message to the
    ///    appropriate session, auto-creating sessions for new senders
    pub async fn register_channel(
        &mut self,
        mut channel: Box<dyn Channel>,
    ) -> Result<(), GatewayError> {
        channel
            .start()
            .await
            .map_err(|e| GatewayError::ServerError(format!("Failed to start channel: {e}")))?;

        let rx = channel
            .take_message_receiver()
            .ok_or_else(|| GatewayError::ServerError("Channel has no message receiver".into()))?;

        let session_manager = self.session_manager.clone();
        let sender_sessions = self.sender_sessions.clone();

        let handle = tokio::spawn(async move {
            let mut rx = rx;
            while let Some(msg) = rx.recv().await {
                let key = (msg.channel_type.to_string(), msg.sender.clone());

                let session_id = {
                    let read_guard = sender_sessions.read().await;
                    read_guard.get(&key).cloned()
                };

                let session_id = match session_id {
                    Some(id) => id,
                    None => {
                        match session_manager
                            .create_session(SessionConfig::default())
                            .await
                        {
                            Ok(id) => {
                                info!(
                                    channel_type = %key.0,
                                    sender = %key.1,
                                    session_id = %id,
                                    "Created new session for sender"
                                );
                                let mut write_guard = sender_sessions.write().await;
                                write_guard.insert(key.clone(), id.clone());
                                id
                            }
                            Err(e) => {
                                error!(
                                    channel_type = %key.0,
                                    sender = %key.1,
                                    error = %e,
                                    "Failed to create session for sender"
                                );
                                continue;
                            }
                        }
                    }
                };

                debug!(
                    session_id = %session_id,
                    sender = %msg.sender,
                    "Routing message to session"
                );

                if let Err(e) = session_manager
                    .send_message(&session_id, msg.content)
                    .await
                {
                    error!(
                        session_id = %session_id,
                        error = %e,
                        "Failed to route message to session"
                    );
                }
            }

            debug!("Channel listener task ended (receiver closed)");
        });

        self.channel_handles.push(handle);
        Ok(())
    }

    /// Returns the session_id for a given sender, if one exists.
    pub async fn get_session_for_sender(
        &self,
        channel_type: &str,
        sender: &str,
    ) -> Option<String> {
        let guard = self.sender_sessions.read().await;
        guard
            .get(&(channel_type.to_string(), sender.to_string()))
            .cloned()
    }

    /// Stop all channel listener tasks and clear state.
    pub async fn stop(&mut self) {
        for handle in self.channel_handles.drain(..) {
            handle.abort();
        }
        let mut guard = self.sender_sessions.write().await;
        guard.clear();
        info!("ChannelBridge stopped, all listeners aborted and state cleared");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use chrono::Utc;
    use std::sync::Mutex;
    use tokio::sync::mpsc;
    use crate::channels::{
        ChannelCapabilities, ChannelError, ChannelMessage, ChannelResponse, ChannelType,
    };

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

        fn sender(&self) -> mpsc::Sender<ChannelMessage> {
            self.message_tx.clone().expect("sender already taken")
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
            self.started = true;
            Ok(())
        }

        async fn stop(&mut self) -> Result<(), ChannelError> {
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

    fn make_channel_message(sender: &str, content: &str, channel_type: ChannelType) -> ChannelMessage {
        ChannelMessage {
            sender: sender.to_string(),
            content: content.to_string(),
            channel_type,
            channel_id: "test-channel".to_string(),
            timestamp: Utc::now(),
            metadata: Default::default(),
        }
    }

    #[tokio::test]
    async fn test_register_channel_and_route_message() {
        let sm = Arc::new(SessionManager::new(100));
        let mut bridge = ChannelBridge::new(sm.clone());

        let channel = MockChannel::new(ChannelType::Telegram);
        let tx = channel.sender();

        bridge.register_channel(Box::new(channel)).await.unwrap();

        tx.send(make_channel_message("user1", "hello", ChannelType::Telegram))
            .await
            .unwrap();

        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        let session_id = bridge
            .get_session_for_sender("telegram", "user1")
            .await
            .unwrap();
        assert!(session_id.starts_with("gw_ses_"));
        assert_eq!(sm.session_count().await, 1);

        bridge.stop().await;
    }

    #[tokio::test]
    async fn test_same_sender_reuses_session() {
        let sm = Arc::new(SessionManager::new(100));
        let mut bridge = ChannelBridge::new(sm.clone());

        let channel = MockChannel::new(ChannelType::Slack);
        let tx = channel.sender();

        bridge.register_channel(Box::new(channel)).await.unwrap();

        tx.send(make_channel_message("user_a", "msg1", ChannelType::Slack))
            .await
            .unwrap();
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        tx.send(make_channel_message("user_a", "msg2", ChannelType::Slack))
            .await
            .unwrap();
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        assert_eq!(sm.session_count().await, 1);

        let sid = bridge
            .get_session_for_sender("slack", "user_a")
            .await
            .unwrap();
        assert!(sid.starts_with("gw_ses_"));

        bridge.stop().await;
    }

    #[tokio::test]
    async fn test_different_senders_get_different_sessions() {
        let sm = Arc::new(SessionManager::new(100));
        let mut bridge = ChannelBridge::new(sm.clone());

        let channel = MockChannel::new(ChannelType::Telegram);
        let tx = channel.sender();

        bridge.register_channel(Box::new(channel)).await.unwrap();

        tx.send(make_channel_message("alice", "hi", ChannelType::Telegram))
            .await
            .unwrap();
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        tx.send(make_channel_message("bob", "hey", ChannelType::Telegram))
            .await
            .unwrap();
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        assert_eq!(sm.session_count().await, 2);

        let sid_alice = bridge
            .get_session_for_sender("telegram", "alice")
            .await
            .unwrap();
        let sid_bob = bridge
            .get_session_for_sender("telegram", "bob")
            .await
            .unwrap();
        assert_ne!(sid_alice, sid_bob);

        bridge.stop().await;
    }

    #[tokio::test]
    async fn test_stop_clears_all_state() {
        let sm = Arc::new(SessionManager::new(100));
        let mut bridge = ChannelBridge::new(sm.clone());

        let channel = MockChannel::new(ChannelType::Telegram);
        let tx = channel.sender();

        bridge.register_channel(Box::new(channel)).await.unwrap();

        tx.send(make_channel_message("user1", "hello", ChannelType::Telegram))
            .await
            .unwrap();
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        assert!(bridge
            .get_session_for_sender("telegram", "user1")
            .await
            .is_some());
        assert_eq!(bridge.channel_handles.len(), 1);

        bridge.stop().await;

        assert!(bridge
            .get_session_for_sender("telegram", "user1")
            .await
            .is_none());
        assert!(bridge.channel_handles.is_empty());
    }

    #[tokio::test]
    async fn test_get_session_for_unknown_sender_returns_none() {
        let sm = Arc::new(SessionManager::new(100));
        let bridge = ChannelBridge::new(sm);

        assert!(bridge
            .get_session_for_sender("telegram", "nobody")
            .await
            .is_none());
    }
}
