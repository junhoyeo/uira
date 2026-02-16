use std::collections::HashMap;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use chrono::{DateTime, Utc};
use futures_util::StreamExt;
use tokio::sync::{broadcast, mpsc, RwLock};
use tokio::task::JoinHandle;
use tokio::time::{timeout, Duration};

use uira_agent::{Agent, AgentConfig, EventStream};
use uira_core::schema::GatewaySettings;
use uira_providers::{ModelClient, ModelClientBuilder, ProviderConfig};
use uira_types::{Message, Provider, ThreadEvent};

use crate::config::SessionConfig;
use crate::error::GatewayError;

/// Status of a managed session
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionStatus {
    Active,
    Idle,
    ShuttingDown,
}

impl std::fmt::Display for SessionStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SessionStatus::Active => write!(f, "active"),
            SessionStatus::Idle => write!(f, "idle"),
            SessionStatus::ShuttingDown => write!(f, "shutting_down"),
        }
    }
}

/// Information about a managed session
#[derive(Debug, Clone)]
pub struct SessionInfo {
    pub id: String,
    pub status: SessionStatus,
    pub created_at: DateTime<Utc>,
    pub last_message_at: DateTime<Utc>,
    pub config: SessionConfig,
}

struct ManagedSession {
    info: SessionInfo,
    agent_input_tx: mpsc::Sender<Message>,
    event_broadcast_tx: broadcast::Sender<serde_json::Value>,
    _relay_handle: JoinHandle<()>,
    agent_handle: JoinHandle<()>,
    agent_control: Arc<AtomicBool>,
}

/// Manages multiple concurrent agent sessions for the gateway.
#[derive(Clone)]
pub struct SessionManager {
    sessions: Arc<RwLock<HashMap<String, ManagedSession>>>,
    max_sessions: usize,
    settings: Arc<std::sync::RwLock<GatewaySettings>>,
    next_id: Arc<AtomicU64>,
    reaper_started: Arc<AtomicBool>,
    reaper_interval: Duration,
    reaper_handle: Arc<std::sync::Mutex<Option<JoinHandle<()>>>>,
    #[cfg(test)]
    test_model_client: Option<Arc<dyn ModelClient>>,
}

impl SessionManager {
    pub fn new(max_sessions: usize) -> Self {
        Self::new_with_settings(max_sessions, GatewaySettings::default())
    }

    pub fn new_with_settings(max_sessions: usize, settings: GatewaySettings) -> Self {
        Self {
            sessions: Arc::new(RwLock::new(HashMap::new())),
            max_sessions,
            settings: Arc::new(std::sync::RwLock::new(settings)),
            next_id: Arc::new(AtomicU64::new(1)),
            reaper_started: Arc::new(AtomicBool::new(false)),
            reaper_interval: Duration::from_secs(60),
            reaper_handle: Arc::new(std::sync::Mutex::new(None)),
            #[cfg(test)]
            test_model_client: None,
        }
    }

    #[cfg(test)]
    pub(crate) fn new_with_test_client(
        max_sessions: usize,
        settings: GatewaySettings,
        test_model_client: Arc<dyn ModelClient>,
    ) -> Self {
        Self {
            sessions: Arc::new(RwLock::new(HashMap::new())),
            max_sessions,
            settings: Arc::new(std::sync::RwLock::new(settings)),
            next_id: Arc::new(AtomicU64::new(1)),
            reaper_started: Arc::new(AtomicBool::new(false)),
            reaper_interval: Duration::from_secs(60),
            reaper_handle: Arc::new(std::sync::Mutex::new(None)),
            test_model_client: Some(test_model_client),
        }
    }

    /// Update the default configuration used for new sessions.
    /// Existing sessions are not affected.
    pub fn update_default_config(&self, settings: GatewaySettings) {
        *self.settings.write().unwrap_or_else(|e| e.into_inner()) = settings;
    }

    /// Create a new session. Returns the session ID.
    pub async fn create_session(&self, config: SessionConfig) -> Result<String, GatewayError> {
        #[cfg(test)]
        if let Some(test_model_client) = &self.test_model_client {
            return self
                .create_session_with_client(config, test_model_client.clone())
                .await;
        }

        let client = self.build_model_client(&config)?;
        self.create_session_with_client(config, client).await
    }

    async fn create_session_with_client(
        &self,
        config: SessionConfig,
        client: Arc<dyn ModelClient>,
    ) -> Result<String, GatewayError> {
        self.start_reaper();

        // Phase 1: Check capacity and reserve ID under read lock
        let id = {
            let sessions = self.sessions.read().await;
            if sessions.len() >= self.max_sessions {
                return Err(GatewayError::MaxSessionsReached(self.max_sessions));
            }
            format!("gw_ses_{}", self.next_id.fetch_add(1, Ordering::Relaxed))
        };

        // Phase 2: Build agent OUTSIDE the lock
        let agent_config = self.build_agent_config(&config)?;
        let agent = Agent::new(agent_config, client);
        let agent = agent
            .with_session_recording()
            .map_err(|e| GatewayError::SessionCreationFailed(e.to_string()))?;
        let (agent, event_stream) = agent.with_event_stream();
        let (event_broadcast_tx, _) = broadcast::channel::<serde_json::Value>(256);
        let relay_broadcast_tx = event_broadcast_tx.clone();
        let relay_handle = tokio::spawn(async move {
            let mut event_stream = event_stream;
            while let Some(event) = event_stream.next().await {
                match serde_json::to_value(&event) {
                    Ok(event_json) => {
                        let _ = relay_broadcast_tx.send(event_json);
                    }
                    Err(error) => {
                        tracing::warn!(%error, "Failed to serialize agent event; skipping");
                    }
                }
            }
        });

        let agent_control = agent.control().cancel_signal();
        let (mut agent, agent_input_tx, _approval_rx, _command_tx) = agent.with_interactive();
        let agent_handle = tokio::spawn(async move {
            if let Err(error) = agent.run_interactive().await {
                tracing::debug!("Gateway session agent exited with error: {}", error);
            }
        });

        let info = SessionInfo {
            id: id.clone(),
            status: SessionStatus::Active,
            created_at: Utc::now(),
            last_message_at: Utc::now(),
            config,
        };

        // Phase 3: Insert under write lock (fast — just a HashMap insert)
        let mut sessions = self.sessions.write().await;
        // Re-check capacity (another session may have been created between phase 1 and 3)
        if sessions.len() >= self.max_sessions {
            // Clean up the agent we just created
            relay_handle.abort();
            agent_handle.abort();
            return Err(GatewayError::MaxSessionsReached(self.max_sessions));
        }
        sessions.insert(
            id.clone(),
            ManagedSession {
                info,
                agent_input_tx,
                event_broadcast_tx,
                _relay_handle: relay_handle,
                agent_handle,
                agent_control,
            },
        );

        Ok(id)
    }

    pub fn start_reaper(&self) {
        let Some(idle_timeout_secs) = self
            .settings
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .idle_timeout_secs
        else {
            return;
        };

        if self.reaper_started.swap(true, Ordering::SeqCst) {
            return;
        }

        let manager = self.clone();
        let idle_timeout = chrono::Duration::seconds(
            i64::try_from(idle_timeout_secs).unwrap_or(i64::MAX),
        );

        let handle = tokio::spawn(async move {
            let mut interval = tokio::time::interval(manager.reaper_interval);
            loop {
                interval.tick().await;

                let now = Utc::now();
                let idle_session_ids = {
                    let sessions = manager.sessions.read().await;
                    sessions
                        .iter()
                        .filter_map(|(session_id, session)| {
                            let idle_duration = now.signed_duration_since(session.info.last_message_at);
                            if idle_duration > idle_timeout {
                                Some(session_id.clone())
                            } else {
                                None
                            }
                        })
                        .collect::<Vec<_>>()
                };

                for session_id in idle_session_ids {
                    // Re-check under read lock before destroying to avoid race condition
                    let still_idle = {
                        let sessions = manager.sessions.read().await;
                        sessions.get(&session_id).is_some_and(|s| {
                            let idle = now.signed_duration_since(s.info.last_message_at);
                            idle > idle_timeout
                        })
                    };

                    if still_idle {
                        if let Err(error) = manager.destroy_session(&session_id).await {
                            if !matches!(error, GatewayError::SessionNotFound(_)) {
                                tracing::debug!(
                                    session_id,
                                    error = %error,
                                    "Failed to destroy idle gateway session"
                                );
                            }
                        }
                    }
                }
            }
        });

        // Store the reaper handle for shutdown
        *self
            .reaper_handle
            .lock()
            .unwrap_or_else(|e| e.into_inner()) = Some(handle);
    }

    /// Destroy a session by ID.
    pub async fn destroy_session(&self, session_id: &str) -> Result<(), GatewayError> {
        let mut sessions = self.sessions.write().await;

        if let Some(session) = sessions.get_mut(session_id) {
            session.info.status = SessionStatus::ShuttingDown;
        }

        let session = sessions
            .remove(session_id)
            .ok_or_else(|| GatewayError::SessionNotFound(session_id.to_string()))?;

        drop(sessions);

        let ManagedSession {
            agent_input_tx,
            event_broadcast_tx: _,
            _relay_handle,
            agent_handle,
            agent_control,
            info: _,
        } = session;

        agent_control.store(true, Ordering::SeqCst);
        drop(agent_input_tx);
        _relay_handle.abort();

        let abort_handle = agent_handle.abort_handle();
        match timeout(Duration::from_secs(5), agent_handle).await {
            Ok(Ok(())) => {}
            Ok(Err(error)) => {
                return Err(GatewayError::SessionShutdownFailed(format!(
                    "Agent task join error: {}",
                    error
                )));
            }
            Err(_) => {
                abort_handle.abort();
                return Err(GatewayError::SessionShutdownFailed(
                    "Timed out waiting for agent shutdown".to_string(),
                ));
            }
        }

        Ok(())
    }

    /// List all active sessions.
    pub async fn list_sessions(&self) -> Vec<SessionInfo> {
        let sessions = self.sessions.read().await;
        let mut infos: Vec<SessionInfo> = sessions
            .values()
            .map(|session| session.info.clone())
            .collect();
        infos.sort_by(|a, b| a.created_at.cmp(&b.created_at));
        infos
    }

    /// Send a message to a specific session.
    pub async fn send_message(
        &self,
        session_id: &str,
        message: String,
    ) -> Result<(), GatewayError> {
        let sender = {
            let mut sessions = self.sessions.write().await;
            let session = sessions
                .get_mut(session_id)
                .ok_or_else(|| GatewayError::SessionNotFound(session_id.to_string()))?;
            if session.info.status == SessionStatus::ShuttingDown {
                return Err(GatewayError::SendFailed(format!(
                    "Session '{}' is shutting down",
                    session_id
                )));
            }
            session.info.status = SessionStatus::Active;
            session.info.last_message_at = Utc::now();
            session.agent_input_tx.clone()
        };

        let prompt = Message::user_prompt(&message);

        sender
            .send(prompt)
            .await
            .map_err(|e| GatewayError::SendFailed(e.to_string()))
    }

    pub async fn shutdown(&self) -> Result<(), GatewayError> {
        // Abort the reaper task
        if let Some(handle) = self
            .reaper_handle
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take()
        {
            handle.abort();
        }

        let sessions = {
            let mut sessions = self.sessions.write().await;
            sessions.drain().map(|(_, session)| session).collect::<Vec<_>>()
        };

        let mut shutdown_errors = Vec::new();

        for session in sessions {
            let ManagedSession {
                agent_input_tx,
                event_broadcast_tx: _,
                _relay_handle,
                agent_handle,
                agent_control,
                info: _,
            } = session;

            agent_control.store(true, Ordering::SeqCst);
            drop(agent_input_tx);
            _relay_handle.abort();

            let abort_handle = agent_handle.abort_handle();
            match timeout(Duration::from_secs(10), agent_handle).await {
                Ok(Ok(())) => {}
                Ok(Err(error)) => shutdown_errors.push(format!("Agent task join error: {}", error)),
                Err(_) => {
                    abort_handle.abort();
                    shutdown_errors.push("Timed out waiting for agent shutdown".to_string());
                }
            }
        }

        if shutdown_errors.is_empty() {
            Ok(())
        } else {
            Err(GatewayError::SessionShutdownFailed(shutdown_errors.join(", ")))
        }
    }

    pub async fn subscribe_events(
        &self,
        session_id: &str,
    ) -> Option<broadcast::Receiver<serde_json::Value>> {
        let sessions = self.sessions.read().await;
        sessions
            .get(session_id)
            .map(|session| session.event_broadcast_tx.subscribe())
    }

    pub async fn take_event_stream(&self, session_id: &str) -> Option<EventStream> {
        let mut event_rx = self.subscribe_events(session_id).await?;
        let (event_tx, event_stream) = EventStream::channel(256);
        let stream_session_id = session_id.to_string();
        tokio::spawn(async move {
            loop {
                let event_json = match event_rx.recv().await {
                    Ok(event_json) => event_json,
                    Err(broadcast::error::RecvError::Lagged(skipped)) => {
                        tracing::warn!(
                            session_id = %stream_session_id,
                            skipped,
                            "Event stream bridge lagged behind; skipping missed events"
                        );
                        continue;
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                };

                if let Ok(event) = serde_json::from_value::<ThreadEvent>(event_json) {
                    if event_tx.send(event).await.is_err() {
                        break;
                    }
                }
            }
        });
        Some(event_stream)
    }

    /// Get the number of active sessions.
    pub async fn session_count(&self) -> usize {
        self.sessions.read().await.len()
    }

    /// Get the maximum number of sessions allowed.
    pub fn max_sessions(&self) -> usize {
        self.max_sessions
    }

    /// Get the config for a specific session (for testing/inspection).
    pub async fn get_session_config(&self, session_id: &str) -> Option<SessionConfig> {
        let sessions = self.sessions.read().await;
        sessions.get(session_id).map(|s| s.info.config.clone())
    }

    /// Check if a session exists.
    pub async fn has_session(&self, session_id: &str) -> bool {
        self.sessions.read().await.contains_key(session_id)
    }

    fn build_model_client(
        &self,
        config: &SessionConfig,
    ) -> Result<Arc<dyn ModelClient>, GatewayError> {
        let provider_config = self.build_provider_config(config)?;
        ModelClientBuilder::new()
            .with_config(provider_config)
            .build()
            .map_err(|error| GatewayError::SessionCreationFailed(error.to_string()))
    }

    fn build_provider_config(
        &self,
        config: &SessionConfig,
    ) -> Result<ProviderConfig, GatewayError> {
        let settings = self.settings.read().map_err(|error| {
            GatewayError::SessionCreationFailed(format!("Settings lock poisoned: {error}"))
        })?;
        let provider_name = config
            .provider
            .as_deref()
            .unwrap_or(settings.provider.as_str());
        let provider = parse_provider(provider_name)?;
        let model = config
            .model
            .clone()
            .unwrap_or_else(|| settings.model.clone());

        let mut provider_config = ProviderConfig {
            provider,
            model,
            ..Default::default()
        };

        if provider == Provider::Ollama {
            provider_config.base_url = Some(
                std::env::var("OLLAMA_HOST")
                    .unwrap_or_else(|_| "http://localhost:11434".to_string()),
            );
        }

        Ok(provider_config)
    }

    fn build_agent_config(&self, config: &SessionConfig) -> Result<AgentConfig, GatewayError> {
        let settings = self.settings.read().map_err(|error| {
            GatewayError::SessionCreationFailed(format!("Settings lock poisoned: {error}"))
        })?;
        let mut agent_config = AgentConfig::new().full_auto();

        let model = config
            .model
            .clone()
            .unwrap_or_else(|| settings.model.clone());
        agent_config = agent_config.with_model(model);

        let working_directory = config
            .working_directory
            .as_ref()
            .or(settings.working_directory.as_ref());
        if let Some(path) = working_directory {
            agent_config = agent_config.with_working_directory(path);
        }

        if let Some(skill_context) = &config.skill_context {
            agent_config = agent_config.with_additional_context(vec![skill_context.clone()]);
        }

        Ok(agent_config)
    }
}

fn parse_provider(provider_name: &str) -> Result<Provider, GatewayError> {
    match provider_name.to_ascii_lowercase().as_str() {
        "anthropic" => Ok(Provider::Anthropic),
        "openai" => Ok(Provider::OpenAI),
        "google" | "gemini" => Ok(Provider::Google),
        "ollama" => Ok(Provider::Ollama),
        "opencode" => Ok(Provider::OpenCode),
        "openrouter" => Ok(Provider::OpenRouter),
        "custom" => Ok(Provider::Custom),
        _ => Err(GatewayError::SessionCreationFailed(format!(
            "Unknown provider: {}",
            provider_name
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::time::Duration as StdDuration;

    use uira_providers::ModelClient;

    use crate::testing::MockModelClient;

    fn test_settings() -> GatewaySettings {
        GatewaySettings {
            provider: "ollama".to_string(),
            model: "llama3.1".to_string(),
            ..GatewaySettings::default()
        }
    }

    fn test_settings_with_idle_timeout(idle_timeout_secs: Option<u64>) -> GatewaySettings {
        GatewaySettings {
            provider: "ollama".to_string(),
            model: "llama3.1".to_string(),
            idle_timeout_secs,
            ..GatewaySettings::default()
        }
    }

    #[tokio::test]
    async fn test_create_session_with_agent() {
        let manager = SessionManager::new_with_settings(10, test_settings());
        let client = Arc::new(MockModelClient::new("ok"));
        let id = manager
            .create_session_with_client(SessionConfig::default(), client as Arc<dyn ModelClient>)
            .await
            .unwrap();

        assert!(id.starts_with("gw_ses_"));
        assert_eq!(manager.session_count().await, 1);
        assert!(manager.subscribe_events(&id).await.is_some());
        assert!(manager.subscribe_events(&id).await.is_some());
    }

    #[tokio::test]
    async fn test_send_message_to_agent() {
        let manager = SessionManager::new_with_settings(10, test_settings());
        let client = Arc::new(MockModelClient::new("ok"));
        let id = manager
            .create_session_with_client(
                SessionConfig::default(),
                client.clone() as Arc<dyn ModelClient>,
            )
            .await
            .unwrap();
        let mut event_stream = manager.subscribe_events(&id).await.unwrap();

        manager
            .send_message(&id, "hello".to_string())
            .await
            .unwrap();

        let mut observed_agent_activity = false;
        for _ in 0..20 {
            if let Ok(Ok(event)) = timeout(Duration::from_millis(100), event_stream.recv()).await {
                match event.get("type").and_then(serde_json::Value::as_str) {
                    Some("turn_started")
                    | Some("content_delta")
                    | Some("thread_completed")
                    | Some("error") => {
                        observed_agent_activity = true;
                        break;
                    }
                    _ => {}
                }
            }
        }

        assert!(observed_agent_activity);
    }

    #[tokio::test]
    async fn test_destroy_session_cancels_agent() {
        let manager = SessionManager::new_with_settings(10, test_settings());
        let client = Arc::new(MockModelClient::new("ok").with_delay(StdDuration::from_millis(100)));
        let id = manager
            .create_session_with_client(SessionConfig::default(), client as Arc<dyn ModelClient>)
            .await
            .unwrap();

        manager.destroy_session(&id).await.unwrap();
        assert_eq!(manager.session_count().await, 0);
        assert!(!manager.has_session(&id).await);
    }

    #[tokio::test]
    async fn test_send_message_updates_last_message_at() {
        let manager = SessionManager::new_with_settings(10, test_settings());
        let client = Arc::new(MockModelClient::new("ok"));
        let id = manager
            .create_session_with_client(SessionConfig::default(), client as Arc<dyn ModelClient>)
            .await
            .unwrap();

        let before = manager
            .list_sessions()
            .await
            .into_iter()
            .find(|session| session.id == id)
            .unwrap()
            .last_message_at;

        tokio::time::sleep(Duration::from_millis(20)).await;

        manager
            .send_message(&id, "hello".to_string())
            .await
            .unwrap();

        let after = manager
            .list_sessions()
            .await
            .into_iter()
            .find(|session| session.id == id)
            .unwrap()
            .last_message_at;

        assert!(after > before);
    }

    #[tokio::test]
    async fn test_reaper_destroys_idle_session() {
        let mut manager = SessionManager::new_with_settings(
            10,
            test_settings_with_idle_timeout(Some(1)),
        );
        manager.reaper_interval = Duration::from_millis(20);
        manager.start_reaper();

        let client = Arc::new(MockModelClient::new("ok"));
        let id = manager
            .create_session_with_client(SessionConfig::default(), client as Arc<dyn ModelClient>)
            .await
            .unwrap();

        {
            let mut sessions = manager.sessions.write().await;
            let session = sessions.get_mut(&id).unwrap();
            session.info.last_message_at = Utc::now() - chrono::Duration::seconds(2);
        }

        for _ in 0..25 {
            if !manager.has_session(&id).await {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }

        assert!(!manager.has_session(&id).await);
    }

    #[tokio::test]
    async fn test_reaper_keeps_active_session() {
        let mut manager = SessionManager::new_with_settings(
            10,
            test_settings_with_idle_timeout(Some(1)),
        );
        manager.reaper_interval = Duration::from_millis(20);
        manager.start_reaper();

        let client = Arc::new(MockModelClient::new("ok"));
        let id = manager
            .create_session_with_client(SessionConfig::default(), client as Arc<dyn ModelClient>)
            .await
            .unwrap();

        for _ in 0..4 {
            manager
                .send_message(&id, "keepalive".to_string())
                .await
                .unwrap();
            tokio::time::sleep(Duration::from_millis(100)).await;
        }

        assert!(manager.has_session(&id).await);
    }

    #[tokio::test]
    async fn test_shutdown_cancels_all_sessions() {
        let manager = SessionManager::new_with_settings(10, test_settings());
        let mut session_ids = Vec::new();

        for _ in 0..3 {
            let client = Arc::new(MockModelClient::new("ok").with_delay(StdDuration::from_millis(50)));
            let id = manager
                .create_session_with_client(SessionConfig::default(), client as Arc<dyn ModelClient>)
                .await
                .unwrap();
            session_ids.push(id);
        }

        manager.shutdown().await.unwrap();

        assert_eq!(manager.session_count().await, 0);
        for session_id in session_ids {
            assert!(!manager.has_session(&session_id).await);
        }
    }

    #[tokio::test]
    async fn test_max_sessions_enforced() {
        let manager = SessionManager::new_with_settings(2, test_settings());
        let client1 = Arc::new(MockModelClient::new("first"));
        let client2 = Arc::new(MockModelClient::new("second"));
        let client3 = Arc::new(MockModelClient::new("third"));

        manager
            .create_session_with_client(SessionConfig::default(), client1 as Arc<dyn ModelClient>)
            .await
            .unwrap();
        manager
            .create_session_with_client(SessionConfig::default(), client2 as Arc<dyn ModelClient>)
            .await
            .unwrap();

        let result = manager
            .create_session_with_client(SessionConfig::default(), client3 as Arc<dyn ModelClient>)
            .await;
        assert!(matches!(result, Err(GatewayError::MaxSessionsReached(_))));
    }

    #[tokio::test]
    async fn test_send_message_nonexistent_session() {
        let manager = SessionManager::new_with_settings(10, test_settings());
        let result = manager
            .send_message("nonexistent", "hello".to_string())
            .await;
        assert!(matches!(result, Err(GatewayError::SessionNotFound(_))));
    }

    #[tokio::test]
    async fn test_destroy_nonexistent_session() {
        let manager = SessionManager::new_with_settings(10, test_settings());
        let result = manager.destroy_session("nonexistent").await;
        assert!(matches!(result, Err(GatewayError::SessionNotFound(_))));
    }

    #[tokio::test]
    async fn test_send_message_to_shutting_down_session() {
        let manager = SessionManager::new_with_settings(10, test_settings());
        let client = Arc::new(MockModelClient::new("ok"));
        let id = manager
            .create_session_with_client(SessionConfig::default(), client as Arc<dyn ModelClient>)
            .await
            .unwrap();

        {
            let mut sessions = manager.sessions.write().await;
            sessions.get_mut(&id).unwrap().info.status = SessionStatus::ShuttingDown;
        }

        let result = manager.send_message(&id, "hello".to_string()).await;
        assert!(matches!(result, Err(GatewayError::SendFailed(_))));
        let err = result.unwrap_err().to_string();
        assert!(err.contains("shutting down"));
    }

    #[tokio::test]
    async fn test_list_sessions() {
        let manager = SessionManager::new_with_settings(10, test_settings());
        let client1 = Arc::new(MockModelClient::new("one"));
        let client2 = Arc::new(MockModelClient::new("two"));

        let id1 = manager
            .create_session_with_client(SessionConfig::default(), client1 as Arc<dyn ModelClient>)
            .await
            .unwrap();
        let id = manager
            .create_session_with_client(SessionConfig::default(), client2 as Arc<dyn ModelClient>)
            .await
            .unwrap();

        let sessions = manager.list_sessions().await;
        assert_eq!(sessions.len(), 2);
        let ids: Vec<&str> = sessions.iter().map(|s| s.id.as_str()).collect();
        assert!(ids.contains(&id1.as_str()));
        assert!(ids.contains(&id.as_str()));
    }

    #[tokio::test]
    async fn test_has_session() {
        let manager = SessionManager::new_with_settings(10, test_settings());
        let client = Arc::new(MockModelClient::new("ok"));
        let id = manager
            .create_session_with_client(SessionConfig::default(), client as Arc<dyn ModelClient>)
            .await
            .unwrap();

        assert!(manager.has_session(&id).await);
        assert!(!manager.has_session("nonexistent").await);
    }

    #[tokio::test]
    async fn test_session_creates_session_file() {
        use std::io::{BufRead, BufReader};
        use uira_agent::SessionRecorder;

        // Record existing session files before session creation
        let existing_files: std::collections::HashSet<std::path::PathBuf> =
            SessionRecorder::list_sessions()
                .unwrap_or_default()
                .into_iter()
                .map(|(path, _)| path)
                .collect();

        let manager = SessionManager::new_with_settings(10, test_settings());
        let client = Arc::new(MockModelClient::new("ok"));
        let _id = manager
            .create_session_with_client(SessionConfig::default(), client as Arc<dyn ModelClient>)
            .await
            .unwrap();

        // Find the new session file
        let all_files = SessionRecorder::list_sessions().unwrap();
        let new_files: Vec<_> = all_files
            .into_iter()
            .filter(|(path, _)| !existing_files.contains(path))
            .collect();

        assert!(
            !new_files.is_empty(),
            "Expected a new session file to be created"
        );

        let (session_path, _meta) = &new_files[0];
        assert!(session_path.exists());

        // Read the first line and verify it's valid JSON with type "session_meta"
        let file = std::fs::File::open(session_path).unwrap();
        let mut reader = BufReader::new(file);
        let mut first_line = String::new();
        reader.read_line(&mut first_line).unwrap();

        let parsed: serde_json::Value = serde_json::from_str(&first_line).unwrap();
        assert_eq!(parsed["type"], "session_meta");
        assert!(parsed["thread_id"].is_string());
        assert!(parsed["model"].is_string());
        assert!(parsed["provider"].is_string());

        // Cleanup: remove the test session file
        let _ = std::fs::remove_file(session_path);
    }

    #[tokio::test]
    async fn test_concurrent_session_access() {
        let manager = Arc::new(SessionManager::new_with_settings(100, test_settings()));
        let mut handles = vec![];
        for _ in 0..10 {
            let m = manager.clone();
            handles.push(tokio::spawn(async move {
                let client = Arc::new(MockModelClient::new("ok"));
                m.create_session_with_client(
                    SessionConfig::default(),
                    client as Arc<dyn ModelClient>,
                )
                .await
                .unwrap()
            }));
        }
        for h in handles {
            h.await.unwrap();
        }
        assert_eq!(manager.session_count().await, 10);
    }

    #[tokio::test]
    async fn test_update_default_config_affects_new_sessions() {
        let manager = SessionManager::new_with_test_client(
            10,
            test_settings(),
            Arc::new(MockModelClient::new("ok")),
        );

        let mut new_settings = test_settings();
        new_settings.model = "gpt-4".to_string();
        manager.update_default_config(new_settings);

        assert_eq!(manager.settings.read().unwrap().model, "gpt-4");

        let id = manager
            .create_session(SessionConfig::default())
            .await
            .unwrap();
        assert!(manager.has_session(&id).await);
    }

    #[tokio::test]
    async fn test_update_default_config_does_not_affect_existing_sessions() {
        let manager = SessionManager::new_with_test_client(
            10,
            test_settings(),
            Arc::new(MockModelClient::new("ok")),
        );

        let config = SessionConfig {
            model: Some("llama3.1".to_string()),
            ..SessionConfig::default()
        };
        let id = manager.create_session(config).await.unwrap();

        let mut new_settings = test_settings();
        new_settings.model = "gpt-4".to_string();
        manager.update_default_config(new_settings);

        let session_config = manager.get_session_config(&id).await.unwrap();
        assert_eq!(session_config.model, Some("llama3.1".to_string()));
    }
}
