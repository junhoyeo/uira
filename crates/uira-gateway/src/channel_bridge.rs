use std::collections::HashMap;
use std::sync::Arc;

use futures_util::StreamExt;
use tokio::sync::RwLock;
use tokio::task::JoinHandle;
use tracing::{debug, error, info};
use uira_agent::EventStream;
use uira_types::ThreadEvent;

use crate::channels::{Channel, ChannelResponse};

use crate::config::SessionConfig;
use crate::error::GatewayError;
use crate::session_manager::SessionManager;
use crate::skills::{get_context_injection, SkillError, SkillLoader};

/// Per-channel skill configuration with pre-resolved context injection strings.
#[derive(Debug, Clone, Default)]
pub struct ChannelSkillConfig {
    /// Map from channel type string (e.g., "telegram", "slack") to pre-resolved skill context
    configs: HashMap<String, ResolvedChannelSkills>,
}

#[derive(Debug, Clone)]
struct ResolvedChannelSkills {
    skill_names: Vec<String>,
    context_injection: String,
}

impl ChannelSkillConfig {
    pub fn new() -> Self {
        Self::default()
    }

    /// Build channel skill config from per-channel active skill names.
    ///
    /// If a loader is provided, skill names are resolved and context XML is precomputed.
    /// If no loader is provided, skills are stored but context injection remains empty.
    pub fn from_active_skills(
        skill_loader: Option<&SkillLoader>,
        channel_active_skills: HashMap<String, Vec<String>>,
    ) -> Result<Self, SkillError> {
        let mut config = Self::new();

        for (channel_type, skill_names) in channel_active_skills {
            let context_injection = if let Some(loader) = skill_loader {
                let loaded = loader.load_active_skills(&skill_names)?;
                get_context_injection(&loaded)
            } else {
                String::new()
            };

            config.add_channel_skills(&channel_type, skill_names, context_injection);
        }

        Ok(config)
    }

    /// Register skills for a channel type. Takes the skill names and the pre-resolved
    /// context injection string (from `get_context_injection()`).
    pub fn add_channel_skills(
        &mut self,
        channel_type: &str,
        skill_names: Vec<String>,
        context_injection: String,
    ) {
        self.configs.insert(
            channel_type.to_string(),
            ResolvedChannelSkills {
                skill_names,
                context_injection,
            },
        );
    }

    /// Get the SessionConfig for a given channel type, with skills pre-populated.
    fn session_config_for_channel(&self, channel_type: &str) -> SessionConfig {
        match self.configs.get(channel_type) {
            Some(resolved) => SessionConfig {
                skills: resolved.skill_names.clone(),
                skill_context: if resolved.context_injection.is_empty() {
                    None
                } else {
                    Some(resolved.context_injection.clone())
                },
                ..Default::default()
            },
            None => SessionConfig::default(),
        }
    }
}

/// Routes messages between Channel implementations and the SessionManager.
///
/// Maintains a `(channel_type, sender_id) -> session_id` mapping so that
/// each unique sender is automatically associated with a persistent session.
pub struct ChannelBridge {
    session_manager: Arc<SessionManager>,
    sender_sessions: Arc<RwLock<HashMap<(String, String), String>>>,
    session_routes: Arc<RwLock<HashMap<String, (String, String)>>>,
    channels: Arc<RwLock<HashMap<String, Arc<dyn Channel>>>>,
    channel_handles: Vec<JoinHandle<()>>,
    response_handles: Arc<RwLock<Vec<JoinHandle<()>>>>,
    skill_config: Arc<ChannelSkillConfig>,
}

impl ChannelBridge {
    /// Create a new ChannelBridge backed by the given SessionManager.
    pub fn new(session_manager: Arc<SessionManager>) -> Self {
        Self::with_skill_config(session_manager, ChannelSkillConfig::default())
    }

    pub fn with_skill_config(
        session_manager: Arc<SessionManager>,
        skill_config: ChannelSkillConfig,
    ) -> Self {
        Self {
            session_manager,
            sender_sessions: Arc::new(RwLock::new(HashMap::new())),
            session_routes: Arc::new(RwLock::new(HashMap::new())),
            channels: Arc::new(RwLock::new(HashMap::new())),
            channel_handles: Vec::new(),
            response_handles: Arc::new(RwLock::new(Vec::new())),
            skill_config: Arc::new(skill_config),
        }
    }

    fn spawn_response_delivery_task(
        session_id: String,
        mut event_stream: EventStream,
        channels: Arc<RwLock<HashMap<String, Arc<dyn Channel>>>>,
        session_routes: Arc<RwLock<HashMap<String, (String, String)>>>,
    ) -> JoinHandle<()> {
        tokio::spawn(async move {
            let mut pending_text = String::new();

            while let Some(event) = event_stream.next().await {
                match event {
                    ThreadEvent::ContentDelta { delta } => {
                        pending_text.push_str(&delta);
                    }
                    ThreadEvent::TurnCompleted { .. } | ThreadEvent::ThreadCompleted { .. } => {
                        ChannelBridge::flush_response(
                            &session_id,
                            &mut pending_text,
                            &channels,
                            &session_routes,
                        )
                        .await;
                    }
                    ThreadEvent::Error { message, .. } => {
                        ChannelBridge::flush_response(
                            &session_id,
                            &mut pending_text,
                            &channels,
                            &session_routes,
                        )
                        .await;

                        ChannelBridge::deliver_to_channel(
                            &session_id,
                            message,
                            &channels,
                            &session_routes,
                        )
                        .await;
                    }
                    _ => {}
                }
            }

            ChannelBridge::flush_response(
                &session_id,
                &mut pending_text,
                &channels,
                &session_routes,
            )
            .await;
            debug!(session_id = %session_id, "Response delivery task ended");
        })
    }

    async fn flush_response(
        session_id: &str,
        pending_text: &mut String,
        channels: &Arc<RwLock<HashMap<String, Arc<dyn Channel>>>>,
        session_routes: &Arc<RwLock<HashMap<String, (String, String)>>>,
    ) {
        if pending_text.is_empty() {
            return;
        }

        let response_text = std::mem::take(pending_text);
        ChannelBridge::deliver_to_channel(session_id, response_text, channels, session_routes)
            .await;
    }

    async fn deliver_to_channel(
        session_id: &str,
        content: String,
        channels: &Arc<RwLock<HashMap<String, Arc<dyn Channel>>>>,
        session_routes: &Arc<RwLock<HashMap<String, (String, String)>>>,
    ) {
        if content.is_empty() {
            return;
        }

        let route = {
            let routes = session_routes.read().await;
            routes.get(session_id).cloned()
        };

        let Some((channel_type, sender_id)) = route else {
            error!(session_id = %session_id, "Missing channel route for session response");
            return;
        };

        let channel = {
            let channels_guard = channels.read().await;
            channels_guard.get(&channel_type).cloned()
        };

        let Some(channel) = channel else {
            error!(
                session_id = %session_id,
                channel_type = %channel_type,
                "Missing channel for session response"
            );
            return;
        };

        let response = ChannelResponse {
            content,
            recipient: sender_id,
        };

        if let Err(e) = channel.send_message(response).await {
            error!(
                session_id = %session_id,
                channel_type = %channel_type,
                error = %e,
                "Failed to send session response to channel"
            );
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
        let channel_type = channel.channel_type().to_string();
        channel
            .start()
            .await
            .map_err(|e| GatewayError::ServerError(format!("Failed to start channel: {e}")))?;

        let rx = channel
            .take_message_receiver()
            .ok_or_else(|| GatewayError::ServerError("Channel has no message receiver".into()))?;

        let channel: Arc<dyn Channel> = Arc::from(channel);
        {
            let mut channels = self.channels.write().await;
            channels.insert(channel_type, channel);
        }

        let session_manager = self.session_manager.clone();
        let sender_sessions = self.sender_sessions.clone();
        let session_routes = self.session_routes.clone();
        let channels = self.channels.clone();
        let response_handles = self.response_handles.clone();
        let skill_config = self.skill_config.clone();

        let handle = tokio::spawn(async move {
            let mut rx = rx;
            while let Some(msg) = rx.recv().await {
                let key = (msg.channel_type.to_string(), msg.sender.clone());

                let session_id = {
                    let read_guard = sender_sessions.read().await;
                    read_guard.get(&key).cloned()
                };

                let (session_id, is_new_session) = match session_id {
                    Some(id) => (id, false),
                    None => {
                        let session_config = skill_config.session_config_for_channel(&key.0);
                        match session_manager.create_session(session_config).await {
                            Ok(id) => {
                                info!(
                                    channel_type = %key.0,
                                    sender = %key.1,
                                    session_id = %id,
                                    "Created new session for sender"
                                );
                                let mut write_guard = sender_sessions.write().await;
                                write_guard.insert(key.clone(), id.clone());
                                (id, true)
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

                {
                    let mut routes = session_routes.write().await;
                    routes.insert(session_id.clone(), key.clone());
                }

                if is_new_session {
                    match session_manager.take_event_stream(&session_id).await {
                        Some(event_stream) => {
                            let delivery_handle = ChannelBridge::spawn_response_delivery_task(
                                session_id.clone(),
                                event_stream,
                                channels.clone(),
                                session_routes.clone(),
                            );
                            let mut handles = response_handles.write().await;
                            handles.push(delivery_handle);
                        }
                        None => {
                            error!(
                                session_id = %session_id,
                                "Missing event stream for new session"
                            );
                        }
                    }
                }

                debug!(
                    session_id = %session_id,
                    sender = %msg.sender,
                    "Routing message to session"
                );

                if let Err(e) = session_manager.send_message(&session_id, msg.content).await {
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
    pub async fn get_session_for_sender(&self, channel_type: &str, sender: &str) -> Option<String> {
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

        {
            let mut handles = self.response_handles.write().await;
            for handle in handles.drain(..) {
                handle.abort();
            }
        }

        {
            let mut guard = self.sender_sessions.write().await;
            guard.clear();
        }

        {
            let mut guard = self.session_routes.write().await;
            guard.clear();
        }

        {
            let mut guard = self.channels.write().await;
            guard.clear();
        }

        info!("ChannelBridge stopped, all listeners aborted and state cleared");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::channels::{
        ChannelCapabilities, ChannelError, ChannelMessage, ChannelResponse, ChannelType,
    };
    use crate::testing::MockModelClient;
    use async_trait::async_trait;
    use chrono::Utc;
    use std::sync::Mutex;
    use tokio::sync::mpsc;
    use tokio::time::{sleep, Duration, Instant};
    use uira_core::schema::GatewaySettings;
    use uira_providers::ProviderError;

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

        fn sent_messages(&self) -> Arc<Mutex<Vec<ChannelResponse>>> {
            self.sent_messages.clone()
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

    fn make_channel_message(
        sender: &str,
        content: &str,
        channel_type: ChannelType,
    ) -> ChannelMessage {
        ChannelMessage {
            sender: sender.to_string(),
            content: content.to_string(),
            channel_type,
            channel_id: "test-channel".to_string(),
            timestamp: Utc::now(),
            metadata: Default::default(),
        }
    }

    fn test_session_manager(max_sessions: usize) -> Arc<SessionManager> {
        Arc::new(SessionManager::new_with_settings(
            max_sessions,
            GatewaySettings {
                provider: "ollama".to_string(),
                model: "llama3.1".to_string(),
                ..GatewaySettings::default()
            },
        ))
    }

    fn test_session_manager_with_mock_client(mock_client: MockModelClient) -> Arc<SessionManager> {
        Arc::new(SessionManager::new_with_test_client(
            100,
            GatewaySettings {
                provider: "ollama".to_string(),
                model: "llama3.1".to_string(),
                ..GatewaySettings::default()
            },
            Arc::new(mock_client),
        ))
    }

    async fn wait_for_sent_message_count(
        sent_messages: &Arc<Mutex<Vec<ChannelResponse>>>,
        expected_count: usize,
    ) -> Vec<ChannelResponse> {
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            let messages = sent_messages.lock().unwrap().clone();
            if messages.len() >= expected_count {
                return messages;
            }

            if Instant::now() >= deadline {
                panic!(
                    "timed out waiting for {expected_count} sent message(s), got {}",
                    messages.len()
                );
            }

            sleep(Duration::from_millis(10)).await;
        }
    }

    #[tokio::test]
    async fn test_register_channel_and_route_message() {
        let sm = test_session_manager(100);
        let mut bridge = ChannelBridge::new(sm.clone());

        let channel = MockChannel::new(ChannelType::Telegram);
        let tx = channel.sender();

        bridge.register_channel(Box::new(channel)).await.unwrap();

        tx.send(make_channel_message(
            "user1",
            "hello",
            ChannelType::Telegram,
        ))
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
        let sm = test_session_manager(100);
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
        let sm = test_session_manager(100);
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
    async fn test_channel_bridge_with_skill_config() {
        let sm = test_session_manager(100);
        let mut skill_config = ChannelSkillConfig::new();
        skill_config.add_channel_skills(
            "telegram",
            vec!["slack".to_string(), "github".to_string()],
            "<skill name=\"slack\">x</skill>".to_string(),
        );
        let mut bridge = ChannelBridge::with_skill_config(sm.clone(), skill_config);

        let channel = MockChannel::new(ChannelType::Telegram);
        let tx = channel.sender();

        bridge.register_channel(Box::new(channel)).await.unwrap();

        tx.send(make_channel_message(
            "user1",
            "hello",
            ChannelType::Telegram,
        ))
        .await
        .unwrap();
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        let session_id = bridge
            .get_session_for_sender("telegram", "user1")
            .await
            .unwrap();
        let config = sm.get_session_config(&session_id).await.unwrap();
        assert_eq!(
            config.skills,
            vec!["slack".to_string(), "github".to_string()]
        );
        assert_eq!(
            config.skill_context,
            Some("<skill name=\"slack\">x</skill>".to_string())
        );

        bridge.stop().await;
    }

    #[tokio::test]
    async fn test_different_channels_get_different_skills() {
        let sm = test_session_manager(100);
        let mut skill_config = ChannelSkillConfig::new();
        skill_config.add_channel_skills(
            "telegram",
            vec!["telegram-helper".to_string()],
            "<skill name=\"telegram-helper\">a</skill>".to_string(),
        );
        skill_config.add_channel_skills(
            "slack",
            vec!["slack".to_string(), "triage".to_string()],
            "<skill name=\"slack\">b</skill>".to_string(),
        );
        let mut bridge = ChannelBridge::with_skill_config(sm.clone(), skill_config);

        let tg_channel = MockChannel::new(ChannelType::Telegram);
        let tg_tx = tg_channel.sender();
        bridge.register_channel(Box::new(tg_channel)).await.unwrap();

        let slack_channel = MockChannel::new(ChannelType::Slack);
        let slack_tx = slack_channel.sender();
        bridge
            .register_channel(Box::new(slack_channel))
            .await
            .unwrap();

        tg_tx
            .send(make_channel_message(
                "alice",
                "hello",
                ChannelType::Telegram,
            ))
            .await
            .unwrap();
        slack_tx
            .send(make_channel_message("alice", "hello", ChannelType::Slack))
            .await
            .unwrap();
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        let tg_session = bridge
            .get_session_for_sender("telegram", "alice")
            .await
            .unwrap();
        let slack_session = bridge
            .get_session_for_sender("slack", "alice")
            .await
            .unwrap();

        let tg_config = sm.get_session_config(&tg_session).await.unwrap();
        assert_eq!(tg_config.skills, vec!["telegram-helper".to_string()]);
        assert_eq!(
            tg_config.skill_context,
            Some("<skill name=\"telegram-helper\">a</skill>".to_string())
        );

        let slack_config = sm.get_session_config(&slack_session).await.unwrap();
        assert_eq!(
            slack_config.skills,
            vec!["slack".to_string(), "triage".to_string()]
        );
        assert_eq!(
            slack_config.skill_context,
            Some("<skill name=\"slack\">b</skill>".to_string())
        );

        bridge.stop().await;
    }

    #[tokio::test]
    async fn test_channel_without_skills_gets_default_config() {
        let sm = test_session_manager(100);
        let mut skill_config = ChannelSkillConfig::new();
        skill_config.add_channel_skills(
            "telegram",
            vec!["telegram-only".to_string()],
            "<skill name=\"telegram-only\">x</skill>".to_string(),
        );
        let mut bridge = ChannelBridge::with_skill_config(sm.clone(), skill_config);

        let slack_channel = MockChannel::new(ChannelType::Slack);
        let slack_tx = slack_channel.sender();
        bridge
            .register_channel(Box::new(slack_channel))
            .await
            .unwrap();

        slack_tx
            .send(make_channel_message("user-a", "hello", ChannelType::Slack))
            .await
            .unwrap();
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        let session_id = bridge
            .get_session_for_sender("slack", "user-a")
            .await
            .unwrap();
        let config = sm.get_session_config(&session_id).await.unwrap();
        assert!(config.skills.is_empty());
        assert!(config.skill_context.is_none());

        bridge.stop().await;
    }

    #[tokio::test]
    async fn test_stop_clears_all_state() {
        let sm = test_session_manager(100);
        let mut bridge = ChannelBridge::new(sm.clone());

        let channel = MockChannel::new(ChannelType::Telegram);
        let tx = channel.sender();

        bridge.register_channel(Box::new(channel)).await.unwrap();

        tx.send(make_channel_message(
            "user1",
            "hello",
            ChannelType::Telegram,
        ))
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
        let sm = test_session_manager(100);
        let bridge = ChannelBridge::new(sm);

        assert!(bridge
            .get_session_for_sender("telegram", "nobody")
            .await
            .is_none());
    }

    #[tokio::test]
    async fn test_routes_agent_response_back_to_originating_sender() {
        let sm = test_session_manager_with_mock_client(MockModelClient::new("hello from mock"));
        let mut bridge = ChannelBridge::new(sm);

        let channel = MockChannel::new(ChannelType::Telegram);
        let tx = channel.sender();
        let sent_messages = channel.sent_messages();

        bridge.register_channel(Box::new(channel)).await.unwrap();

        tx.send(make_channel_message(
            "user1",
            "hello",
            ChannelType::Telegram,
        ))
        .await
        .unwrap();

        let sent = wait_for_sent_message_count(&sent_messages, 1).await;
        assert_eq!(sent[0].recipient, "user1");
        assert_eq!(sent[0].content.trim_end(), "hello from mock");

        bridge.stop().await;
    }

    #[tokio::test]
    async fn test_routes_agent_error_back_to_originating_sender() {
        let sm = test_session_manager_with_mock_client(MockModelClient::new("unused").with_error(
            ProviderError::InvalidResponse("mock upstream failure".to_string()),
        ));
        let mut bridge = ChannelBridge::new(sm);

        let channel = MockChannel::new(ChannelType::Slack);
        let tx = channel.sender();
        let sent_messages = channel.sent_messages();

        bridge.register_channel(Box::new(channel)).await.unwrap();

        tx.send(make_channel_message(
            "user_err",
            "trigger",
            ChannelType::Slack,
        ))
        .await
        .unwrap();

        let sent = wait_for_sent_message_count(&sent_messages, 1).await;
        assert_eq!(sent[0].recipient, "user_err");
        assert!(sent[0].content.contains("mock upstream failure"));

        bridge.stop().await;
    }

    #[tokio::test]
    async fn test_multiple_sessions_on_same_channel_route_responses_to_correct_senders() {
        let sm = test_session_manager_with_mock_client(MockModelClient::new("shared response"));
        let mut bridge = ChannelBridge::new(sm.clone());

        let channel = MockChannel::new(ChannelType::Telegram);
        let tx = channel.sender();
        let sent_messages = channel.sent_messages();

        bridge.register_channel(Box::new(channel)).await.unwrap();

        tx.send(make_channel_message(
            "alice",
            "first",
            ChannelType::Telegram,
        ))
        .await
        .unwrap();
        tx.send(make_channel_message("bob", "second", ChannelType::Telegram))
            .await
            .unwrap();

        let sent = wait_for_sent_message_count(&sent_messages, 2).await;
        let alice_count = sent.iter().filter(|msg| msg.recipient == "alice").count();
        let bob_count = sent.iter().filter(|msg| msg.recipient == "bob").count();
        assert_eq!(alice_count, 1);
        assert_eq!(bob_count, 1);

        let alice_session = bridge
            .get_session_for_sender("telegram", "alice")
            .await
            .unwrap();
        let bob_session = bridge
            .get_session_for_sender("telegram", "bob")
            .await
            .unwrap();
        assert_ne!(alice_session, bob_session);
        assert_eq!(sm.session_count().await, 2);

        bridge.stop().await;
    }
}
