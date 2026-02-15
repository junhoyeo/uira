use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use futures_util::StreamExt;
use tokio::sync::{mpsc, Mutex, RwLock};
use tokio::task::JoinHandle;
use tracing::{debug, error, info, warn};
use uira_agent::EventStream;
use uira_types::ThreadEvent;

use crate::channels::{Channel, ChannelCapabilities, ChannelMessage, ChannelResponse, ChannelType};

use crate::config::SessionConfig;
use crate::error::GatewayError;
use crate::session_manager::SessionManager;
use crate::skills::{get_context_injection, SkillError, SkillLoader};

// Type aliases for complex types
type SenderSessionMap = Arc<RwLock<HashMap<(String, String, String), String>>>;
type SessionRouteMap = Arc<RwLock<HashMap<String, (String, String, String)>>>;
type SharedChannelInner = Arc<Mutex<Box<dyn Channel>>>;
type ChannelMap = Arc<RwLock<HashMap<(String, String), SharedChannelInner>>>;
type OutboundChannelMap = Arc<RwLock<HashMap<String, Arc<dyn Channel>>>>;

struct SharedChannelProxy {
    inner: SharedChannelInner,
    channel_type: ChannelType,
    capabilities: ChannelCapabilities,
}

impl SharedChannelProxy {
    fn new(
        inner: SharedChannelInner,
        channel_type: ChannelType,
        capabilities: ChannelCapabilities,
    ) -> Self {
        Self {
            inner,
            channel_type,
            capabilities,
        }
    }
}

#[async_trait::async_trait]
impl Channel for SharedChannelProxy {
    fn channel_type(&self) -> ChannelType {
        self.channel_type.clone()
    }

    fn capabilities(&self) -> ChannelCapabilities {
        self.capabilities.clone()
    }

    async fn start(&mut self) -> Result<(), crate::channels::ChannelError> {
        let mut guard = self.inner.lock().await;
        guard.start().await
    }

    async fn stop(&mut self) -> Result<(), crate::channels::ChannelError> {
        let mut guard = self.inner.lock().await;
        guard.stop().await
    }

    async fn send_message(&self, response: ChannelResponse) -> Result<(), crate::channels::ChannelError> {
        let guard = self.inner.lock().await;
        guard.send_message(response).await
    }

    fn take_message_receiver(&mut self) -> Option<mpsc::Receiver<ChannelMessage>> {
        None
    }
}

struct RateLimiter {
    max_messages: usize,
    window: Duration,
    timestamps: HashMap<(String, String, String), Vec<Instant>>,
}

impl RateLimiter {
    fn new(max_messages: usize, window: Duration) -> Self {
        Self {
            max_messages,
            window,
            timestamps: HashMap::new(),
        }
    }

    fn check_and_record(&mut self, key: &(String, String, String)) -> bool {
        let now = Instant::now();
        let entries = self.timestamps.entry(key.clone()).or_default();

        entries.retain(|&ts| now.duration_since(ts) < self.window);

        if entries.len() >= self.max_messages {
            return false;
        }

        entries.push(now);
        true
    }

    fn cleanup_stale(&mut self) {
        let now = Instant::now();
        self.timestamps.retain(|_, entries| {
            entries.retain(|&ts| now.duration_since(ts) < self.window);
            !entries.is_empty()
        });
    }
}

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
/// Maintains sender affinity via `(channel_type, account_id, sender_id) -> session_id` so that
/// each unique sender on each account is associated with a persistent session, and separately
/// routes responses via `session_id -> (channel_type, account_id, channel_id)`.
/// Multiple accounts of the same channel type (e.g., two Telegram bots) are supported
/// by keying channels on `(channel_type, account_id)`.
pub struct ChannelBridge {
    session_manager: Arc<SessionManager>,
    sender_sessions: SenderSessionMap,
    session_routes: SessionRouteMap,
    channels: ChannelMap,
    outbound_channels: Option<OutboundChannelMap>,
    channel_handles: Vec<JoinHandle<()>>,
    response_handles: Arc<RwLock<Vec<JoinHandle<()>>>>,
    skill_config: Arc<ChannelSkillConfig>,
}

impl ChannelBridge {
    const MAX_PENDING_TEXT_BYTES: usize = 64 * 1024;
    const RATE_LIMIT_MESSAGES: usize = 10;
    const RATE_LIMIT_WINDOW_SECS: u64 = 60;

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
            outbound_channels: None,
            channel_handles: Vec::new(),
            response_handles: Arc::new(RwLock::new(Vec::new())),
            skill_config: Arc::new(skill_config),
        }
    }

    pub fn with_outbound_channels(mut self, outbound_channels: OutboundChannelMap) -> Self {
        self.outbound_channels = Some(outbound_channels);
        self
    }

    fn spawn_response_delivery_task(
        session_id: String,
        mut event_stream: EventStream,
        channels: ChannelMap,
        session_routes: SessionRouteMap,
    ) -> JoinHandle<()> {
        tokio::spawn(async move {
            let mut pending_text = String::new();

            while let Some(event) = event_stream.next().await {
                match event {
                    ThreadEvent::ContentDelta { delta } => {
                        pending_text.push_str(&delta);
                        if pending_text.len() >= Self::MAX_PENDING_TEXT_BYTES {
                            ChannelBridge::flush_response(
                                &session_id,
                                &mut pending_text,
                                &channels,
                                &session_routes,
                            )
                            .await;
                        }
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
        channels: &ChannelMap,
        session_routes: &SessionRouteMap,
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
        channels: &ChannelMap,
        session_routes: &SessionRouteMap,
    ) {
        if content.is_empty() {
            return;
        }

        let route = {
            let routes = session_routes.read().await;
            routes.get(session_id).cloned()
        };

        let Some((channel_type, account_id, channel_id)) = route else {
            error!(session_id = %session_id, "Missing channel route for session response");
            return;
        };

        let channel = {
            let channels_guard = channels.read().await;
            channels_guard
                .get(&(channel_type.clone(), account_id.clone()))
                .cloned()
        };

        let Some(channel) = channel else {
            error!(
                session_id = %session_id,
                channel_type = %channel_type,
                account_id = %account_id,
                "Missing channel for session response"
            );
            return;
        };

        let capabilities = channel.capabilities();
        let content = adapt_content_for_capabilities(content, &capabilities);
        if content.is_empty() {
            debug!(
                session_id = %session_id,
                channel_type = %channel_type,
                account_id = %account_id,
                "Skipping empty adapted response"
            );
            return;
        }
        if content.len() > capabilities.max_message_length {
            debug!(
                session_id = %session_id,
                channel_type = %channel_type,
                account_id = %account_id,
                content_len = content.len(),
                max_message_length = capabilities.max_message_length,
                "Response exceeds channel max_message_length; relying on channel chunking"
            );
        }

        let response = ChannelResponse {
            content,
            recipient: channel_id,
        };

        let send_result = {
            let guard = channel.lock().await;
            guard.send_message(response).await
        };

        if let Err(e) = send_result {
            error!(
                session_id = %session_id,
                channel_type = %channel_type,
                account_id = %account_id,
                error = %e,
                "Failed to send session response to channel"
            );
        }
    }

    /// Register a channel with an account identifier and start listening for inbound messages.
    pub async fn register_channel(
        &mut self,
        mut channel: Box<dyn Channel>,
        account_id: String,
    ) -> Result<(), GatewayError> {
        let channel_type = channel.channel_type();
        let channel_type_key = channel_type.to_string();
        let channel_capabilities = channel.capabilities();
        channel
            .start()
            .await
            .map_err(|e| GatewayError::ServerError(format!("Failed to start channel: {e}")))?;

        info!(
            channel_type = %channel_type_key,
            account_id = %account_id,
            max_message_length = channel_capabilities.max_message_length,
            supports_markdown = channel_capabilities.supports_markdown,
            "Registered channel"
        );

        let rx = channel
            .take_message_receiver()
            .ok_or_else(|| GatewayError::ServerError("Channel has no message receiver".into()))?;

        let shared_channel: SharedChannelInner = Arc::new(Mutex::new(channel));
        let outbound_channel: Arc<dyn Channel> = Arc::new(SharedChannelProxy::new(
            shared_channel.clone(),
            channel_type,
            channel_capabilities,
        ));
        {
            let mut channels = self.channels.write().await;
            channels.insert(
                (channel_type_key.clone(), account_id.clone()),
                shared_channel,
            );
        }

        if let Some(outbound_channels) = &self.outbound_channels {
            let mut outbound = outbound_channels.write().await;
            outbound
                .entry(channel_type_key.clone())
                .or_insert(outbound_channel);
        }

        let session_manager = self.session_manager.clone();
        let sender_sessions = self.sender_sessions.clone();
        let session_routes = self.session_routes.clone();
        let channels = self.channels.clone();
        let response_handles = self.response_handles.clone();
        let skill_config = self.skill_config.clone();

        let handle = tokio::spawn(async move {
            let mut rx = rx;
            let mut rate_limiter = RateLimiter::new(
                ChannelBridge::RATE_LIMIT_MESSAGES,
                Duration::from_secs(ChannelBridge::RATE_LIMIT_WINDOW_SECS),
            );
            let mut cleanup_counter = 0u32;

            while let Some(msg) = rx.recv().await {
                let channel_type_str = msg.channel_type.to_string();
                let key = (
                    channel_type_str.clone(),
                    account_id.clone(),
                    msg.sender.clone(),
                );

                if !rate_limiter.check_and_record(&key) {
                    warn!(
                        channel_type = %key.0,
                        account_id = %key.1,
                        sender = %key.2,
                        "Rate limited: dropping inbound message from sender"
                    );
                    continue;
                }

                cleanup_counter = cleanup_counter.wrapping_add(1);
                if cleanup_counter.is_multiple_of(100) {
                    rate_limiter.cleanup_stale();
                }

                let (session_id, is_new_session) = {
                    let mut write_guard = sender_sessions.write().await;
                    if let Some(existing_id) = write_guard.get(&key) {
                        (existing_id.clone(), false)
                    } else {
                        let session_config =
                            skill_config.session_config_for_channel(&channel_type_str);
                        match session_manager.create_session(session_config).await {
                            Ok(id) => {
                                info!(
                                    channel_type = %key.0,
                                    account_id = %key.1,
                                    sender = %key.2,
                                    session_id = %id,
                                    "Created new session for sender"
                                );
                                write_guard.insert(key.clone(), id.clone());
                                (id, true)
                            }
                            Err(e) => {
                                error!(
                                    channel_type = %key.0,
                                    account_id = %key.1,
                                    sender = %key.2,
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
                    routes.insert(
                        session_id.clone(),
                        (
                            channel_type_str.clone(),
                            account_id.clone(),
                            msg.channel_id.clone(),
                        ),
                    );
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
                            handles.retain(|h| !h.is_finished());
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
                    let is_stale_session = matches!(
                        &e,
                        GatewayError::SessionNotFound(_) | GatewayError::SendFailed(_)
                    );

                    if is_stale_session {
                        {
                            let mut write_guard = sender_sessions.write().await;
                            write_guard.remove(&key);
                        }

                        {
                            let mut routes = session_routes.write().await;
                            routes.remove(&session_id);
                        }

                        info!(
                            session_id = %session_id,
                            channel_type = %key.0,
                            account_id = %key.1,
                            sender = %key.2,
                            error = %e,
                            "Evicted stale session mapping after routing failure"
                        );
                    } else {
                        error!(
                            session_id = %session_id,
                            error = %e,
                            "Failed to route message to session"
                        );
                    }
                }
            }

            debug!("Channel listener task ended (receiver closed)");
        });

        self.channel_handles.push(handle);
        Ok(())
    }

    pub async fn get_session_for_sender(
        &self,
        channel_type: &str,
        account_id: &str,
        sender: &str,
    ) -> Option<String> {
        let guard = self.sender_sessions.read().await;
        guard
            .get(&(
                channel_type.to_string(),
                account_id.to_string(),
                sender.to_string(),
            ))
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

        let channels_to_stop = {
            let mut guard = self.channels.write().await;
            guard.drain().collect::<Vec<_>>()
        };

        let mut stopped_channels = 0usize;
        for ((channel_type, account_id), channel) in channels_to_stop {
            let mut guard = channel.lock().await;
            if let Err(e) = guard.stop().await {
                warn!(
                    channel_type = %channel_type,
                    account_id = %account_id,
                    error = %e,
                    "Failed to stop channel cleanly"
                );
            }
            stopped_channels += 1;
        }

        if let Some(outbound_channels) = &self.outbound_channels {
            let mut guard = outbound_channels.write().await;
            guard.clear();
        }

        info!(
            stopped_channels,
            "ChannelBridge stopped, listeners aborted and state cleared"
        );
    }
}

fn adapt_content_for_capabilities(content: String, capabilities: &ChannelCapabilities) -> String {
    if capabilities.supports_markdown {
        return content;
    }

    strip_markdown_formatting(&content)
}

fn strip_markdown_formatting(content: &str) -> String {
    let mut stripped = content
        .replace("```", "")
        .replace("`", "")
        .replace("**", "")
        .replace("__", "")
        .replace("~~", "");

    stripped = stripped
        .lines()
        .map(|line| {
            let line = line
                .strip_prefix("###### ")
                .or_else(|| line.strip_prefix("##### "))
                .or_else(|| line.strip_prefix("#### "))
                .or_else(|| line.strip_prefix("### "))
                .or_else(|| line.strip_prefix("## "))
                .or_else(|| line.strip_prefix("# "))
                .unwrap_or(line);
            let line = line.strip_prefix("> ").unwrap_or(line);
            line.strip_prefix("- ").unwrap_or(line).trim_end()
        })
        .collect::<Vec<_>>()
        .join("\n");

    stripped.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::channels::{ChannelError, ChannelMessage, ChannelResponse, ChannelType};
    use crate::testing::{MockChannel, MockModelClient};
    use async_trait::async_trait;
    use chrono::Utc;
    use std::sync::{Arc, Mutex};
    use tokio::sync::mpsc;
    use tokio::time::{sleep, Duration, Instant};
    use uira_core::schema::GatewaySettings;
    use uira_providers::ProviderError;

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

        bridge.register_channel(Box::new(channel), "default".to_string()).await.unwrap();

        tx.send(make_channel_message(
            "user1",
            "hello",
            ChannelType::Telegram,
        ))
        .await
        .unwrap();

        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        let session_id = bridge
            .get_session_for_sender("telegram", "default", "user1")
            .await
            .unwrap();
        assert!(session_id.starts_with("gw_ses_"));
        assert_eq!(sm.session_count().await, 1);

        bridge.stop().await;
    }

    #[test]
    fn test_rate_limiter() {
        let mut limiter = RateLimiter::new(3, Duration::from_secs(60));
        let key = (
            "telegram".to_string(),
            "bot1".to_string(),
            "user1".to_string(),
        );

        assert!(limiter.check_and_record(&key));
        assert!(limiter.check_and_record(&key));
        assert!(limiter.check_and_record(&key));
        assert!(!limiter.check_and_record(&key));

        let key2 = (
            "telegram".to_string(),
            "bot1".to_string(),
            "user2".to_string(),
        );
        assert!(limiter.check_and_record(&key2));
    }

    #[test]
    fn test_rate_limiter_cleanup_stale() {
        let mut limiter = RateLimiter::new(2, Duration::from_millis(1));
        let key1 = (
            "telegram".to_string(),
            "bot1".to_string(),
            "user1".to_string(),
        );
        let key2 = (
            "telegram".to_string(),
            "bot1".to_string(),
            "user2".to_string(),
        );

        limiter.check_and_record(&key1);
        limiter.check_and_record(&key2);
        assert_eq!(limiter.timestamps.len(), 2);

        std::thread::sleep(Duration::from_millis(5));

        limiter.cleanup_stale();
        assert!(
            limiter.timestamps.is_empty(),
            "cleanup_stale should remove all expired entries"
        );
    }

    #[tokio::test]
    async fn test_same_sender_reuses_session() {
        let sm = test_session_manager(100);
        let mut bridge = ChannelBridge::new(sm.clone());

        let channel = MockChannel::new(ChannelType::Slack);
        let tx = channel.sender();

        bridge.register_channel(Box::new(channel), "default".to_string()).await.unwrap();

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
            .get_session_for_sender("slack", "default", "user_a")
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

        bridge.register_channel(Box::new(channel), "default".to_string()).await.unwrap();

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
            .get_session_for_sender("telegram", "default", "alice")
            .await
            .unwrap();
        let sid_bob = bridge
            .get_session_for_sender("telegram", "default", "bob")
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

        bridge.register_channel(Box::new(channel), "default".to_string()).await.unwrap();

        tx.send(make_channel_message(
            "user1",
            "hello",
            ChannelType::Telegram,
        ))
        .await
        .unwrap();
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        let session_id = bridge
            .get_session_for_sender("telegram", "default", "user1")
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
        bridge.register_channel(Box::new(tg_channel), "default".to_string()).await.unwrap();

        let slack_channel = MockChannel::new(ChannelType::Slack);
        let slack_tx = slack_channel.sender();
        bridge
            .register_channel(Box::new(slack_channel), "default".to_string())
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
            .get_session_for_sender("telegram", "default", "alice")
            .await
            .unwrap();
        let slack_session = bridge
            .get_session_for_sender("slack", "default", "alice")
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
            .register_channel(Box::new(slack_channel), "default".to_string())
            .await
            .unwrap();

        slack_tx
            .send(make_channel_message("user-a", "hello", ChannelType::Slack))
            .await
            .unwrap();
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        let session_id = bridge
            .get_session_for_sender("slack", "default", "user-a")
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

        bridge.register_channel(Box::new(channel), "default".to_string()).await.unwrap();

        tx.send(make_channel_message(
            "user1",
            "hello",
            ChannelType::Telegram,
        ))
        .await
        .unwrap();
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        assert!(bridge
            .get_session_for_sender("telegram", "default", "user1")
            .await
            .is_some());
        assert_eq!(bridge.channel_handles.len(), 1);

        bridge.stop().await;

        assert!(bridge
            .get_session_for_sender("telegram", "default", "user1")
            .await
            .is_none());
        assert!(bridge.channel_handles.is_empty());
    }

    #[tokio::test]
    async fn test_get_session_for_unknown_sender_returns_none() {
        let sm = test_session_manager(100);
        let bridge = ChannelBridge::new(sm);

        assert!(bridge
            .get_session_for_sender("telegram", "default", "nobody")
            .await
            .is_none());
    }

    #[tokio::test]
    async fn test_routes_agent_response_back_to_originating_channel() {
        let sm = test_session_manager_with_mock_client(MockModelClient::new("hello from mock"));
        let mut bridge = ChannelBridge::new(sm);

        let channel = MockChannel::new(ChannelType::Telegram);
        let tx = channel.sender();
        let sent_messages = channel.sent_messages_shared();

        bridge.register_channel(Box::new(channel), "default".to_string()).await.unwrap();

        tx.send(make_channel_message(
            "user1",
            "hello",
            ChannelType::Telegram,
        ))
        .await
        .unwrap();

        let sent = wait_for_sent_message_count(&sent_messages, 1).await;
        assert_eq!(sent[0].recipient, "test-channel");
        assert_eq!(sent[0].content.trim_end(), "hello from mock");

        bridge.stop().await;
    }

    #[tokio::test]
    async fn test_routes_agent_error_back_to_originating_channel() {
        let sm = test_session_manager_with_mock_client(MockModelClient::new("unused").with_error(
            ProviderError::InvalidResponse("mock upstream failure".to_string()),
        ));
        let mut bridge = ChannelBridge::new(sm);

        let channel = MockChannel::new(ChannelType::Slack);
        let tx = channel.sender();
        let sent_messages = channel.sent_messages_shared();

        bridge.register_channel(Box::new(channel), "default".to_string()).await.unwrap();

        tx.send(make_channel_message(
            "user_err",
            "trigger",
            ChannelType::Slack,
        ))
        .await
        .unwrap();

        let sent = wait_for_sent_message_count(&sent_messages, 1).await;
        assert_eq!(sent[0].recipient, "test-channel");
        assert!(sent[0].content.contains("mock upstream failure"));

        bridge.stop().await;
    }

    #[tokio::test]
    async fn test_multiple_sessions_on_same_channel_route_responses_to_channel() {
        let sm = test_session_manager_with_mock_client(MockModelClient::new("shared response"));
        let mut bridge = ChannelBridge::new(sm.clone());

        let channel = MockChannel::new(ChannelType::Telegram);
        let tx = channel.sender();
        let sent_messages = channel.sent_messages_shared();

        bridge.register_channel(Box::new(channel), "default".to_string()).await.unwrap();

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
        let channel_count = sent
            .iter()
            .filter(|msg| msg.recipient == "test-channel")
            .count();
        assert_eq!(channel_count, 2);

        let alice_session = bridge
            .get_session_for_sender("telegram", "default", "alice")
            .await
            .unwrap();
        let bob_session = bridge
            .get_session_for_sender("telegram", "default", "bob")
            .await
            .unwrap();
        assert_ne!(alice_session, bob_session);
        assert_eq!(sm.session_count().await, 2);

        bridge.stop().await;
    }

    #[tokio::test]
    async fn test_two_mock_channels_same_type_registered() {
        let sm = test_session_manager(100);
        let mut bridge = ChannelBridge::new(sm.clone());

        let chan_a = MockChannel::new(ChannelType::Telegram);
        let tx_a = chan_a.sender();
        bridge
            .register_channel(Box::new(chan_a), "bot-a".to_string())
            .await
            .unwrap();

        let chan_b = MockChannel::new(ChannelType::Telegram);
        let _tx_b = chan_b.sender();
        bridge
            .register_channel(Box::new(chan_b), "bot-b".to_string())
            .await
            .unwrap();

        tx_a.send(make_channel_message("alice", "hi", ChannelType::Telegram))
            .await
            .unwrap();
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        let session = bridge
            .get_session_for_sender("telegram", "bot-a", "alice")
            .await;
        assert!(session.is_some());
        assert!(session.unwrap().starts_with("gw_ses_"));
        assert_eq!(sm.session_count().await, 1);

        bridge.stop().await;
    }

    #[tokio::test]
    async fn test_multi_account_messages_routed_correctly() {
        let sm = test_session_manager(100);
        let mut bridge = ChannelBridge::new(sm.clone());

        let chan_a = MockChannel::new(ChannelType::Telegram);
        let tx_a = chan_a.sender();
        bridge
            .register_channel(Box::new(chan_a), "bot-a".to_string())
            .await
            .unwrap();

        let chan_b = MockChannel::new(ChannelType::Telegram);
        let tx_b = chan_b.sender();
        bridge
            .register_channel(Box::new(chan_b), "bot-b".to_string())
            .await
            .unwrap();

        tx_a.send(make_channel_message("alice", "msg-a", ChannelType::Telegram))
            .await
            .unwrap();
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        tx_b.send(make_channel_message("alice", "msg-b", ChannelType::Telegram))
            .await
            .unwrap();
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        let session_a = bridge
            .get_session_for_sender("telegram", "bot-a", "alice")
            .await
            .unwrap();
        let session_b = bridge
            .get_session_for_sender("telegram", "bot-b", "alice")
            .await
            .unwrap();

        assert_ne!(session_a, session_b);
        assert_eq!(sm.session_count().await, 2);

        bridge.stop().await;
    }

    #[tokio::test]
    async fn test_multi_account_responses_routed_to_correct_channel() {
        let sm =
            test_session_manager_with_mock_client(MockModelClient::new("multi-account reply"));
        let mut bridge = ChannelBridge::new(sm.clone());

        let chan_a = MockChannel::new(ChannelType::Telegram);
        let tx_a = chan_a.sender();
        let sent_a = chan_a.sent_messages_shared();
        bridge
            .register_channel(Box::new(chan_a), "bot-a".to_string())
            .await
            .unwrap();

        let chan_b = MockChannel::new(ChannelType::Telegram);
        let _tx_b = chan_b.sender();
        let sent_b = chan_b.sent_messages_shared();
        bridge
            .register_channel(Box::new(chan_b), "bot-b".to_string())
            .await
            .unwrap();

        tx_a.send(make_channel_message("alice", "hello", ChannelType::Telegram))
            .await
            .unwrap();

        let messages_a = wait_for_sent_message_count(&sent_a, 1).await;
        assert_eq!(messages_a[0].recipient, "test-channel");
        assert_eq!(messages_a[0].content.trim_end(), "multi-account reply");

        let messages_b = sent_b.lock().unwrap().clone();
        assert!(messages_b.is_empty());

        bridge.stop().await;
    }

    struct CapabilityMockChannel {
        channel_type: ChannelType,
        capabilities: ChannelCapabilities,
        message_tx: Option<mpsc::Sender<ChannelMessage>>,
        message_rx: Option<mpsc::Receiver<ChannelMessage>>,
        sent_messages: Arc<Mutex<Vec<ChannelResponse>>>,
        started: bool,
    }

    impl CapabilityMockChannel {
        fn new(channel_type: ChannelType, capabilities: ChannelCapabilities) -> Self {
            let (tx, rx) = mpsc::channel(32);
            Self {
                channel_type,
                capabilities,
                message_tx: Some(tx),
                message_rx: Some(rx),
                sent_messages: Arc::new(Mutex::new(Vec::new())),
                started: false,
            }
        }

        fn sender(&self) -> mpsc::Sender<ChannelMessage> {
            self.message_tx.clone().expect("sender already taken")
        }

        fn sent_messages_shared(&self) -> Arc<Mutex<Vec<ChannelResponse>>> {
            self.sent_messages.clone()
        }
    }

    #[async_trait]
    impl Channel for CapabilityMockChannel {
        fn channel_type(&self) -> ChannelType {
            self.channel_type.clone()
        }

        fn capabilities(&self) -> ChannelCapabilities {
            self.capabilities.clone()
        }

        async fn start(&mut self) -> Result<(), ChannelError> {
            if self.started {
                return Err(ChannelError::Other("Already started".to_string()));
            }
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

    #[tokio::test]
    async fn test_strips_markdown_for_channels_without_markdown_support() {
        let sm = test_session_manager_with_mock_client(MockModelClient::new(
            "# Hello **world**\n\n- `code` _value_",
        ));
        let mut bridge = ChannelBridge::new(sm);

        let channel = CapabilityMockChannel::new(
            ChannelType::Slack,
            ChannelCapabilities {
                max_message_length: 4096,
                supports_markdown: false,
            },
        );
        let tx = channel.sender();
        let sent_messages = channel.sent_messages_shared();

        bridge
            .register_channel(Box::new(channel), "default".to_string())
            .await
            .unwrap();

        tx.send(make_channel_message(
            "user1",
            "hello",
            ChannelType::Slack,
        ))
        .await
        .unwrap();

        let sent = wait_for_sent_message_count(&sent_messages, 1).await;
        assert_eq!(sent[0].content.trim_end(), "Hello world\n\ncode value");

        bridge.stop().await;
    }
}
