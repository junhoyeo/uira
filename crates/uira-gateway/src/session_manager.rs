use std::collections::HashMap;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use chrono::{DateTime, Utc};
use tokio::task::JoinHandle;
use tokio::sync::{mpsc, RwLock};
use tokio::time::{timeout, Duration};

use uira_agent::{Agent, AgentConfig, EventStream};
use uira_core::schema::GatewaySettings;
use uira_providers::{ModelClient, ModelClientBuilder, ProviderConfig};
use uira_types::{Message, Provider};

use crate::config::SessionConfig;
use crate::error::GatewayError;

/// Status of a managed session
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionStatus {
    Active,
    Idle,
    ShuttingDown,
}

/// Information about a managed session
#[derive(Debug, Clone)]
pub struct SessionInfo {
    pub id: String,
    pub status: SessionStatus,
    pub created_at: DateTime<Utc>,
    pub config: SessionConfig,
    pub skill_context: Option<String>,
}

struct ManagedSession {
    info: SessionInfo,
    agent_input_tx: mpsc::Sender<Message>,
    event_stream: Option<EventStream>,
    agent_handle: JoinHandle<()>,
    agent_control: Arc<AtomicBool>,
}

/// Manages multiple concurrent agent sessions for the gateway.
pub struct SessionManager {
    sessions: Arc<RwLock<HashMap<String, ManagedSession>>>,
    max_sessions: usize,
    settings: GatewaySettings,
    next_id: Arc<AtomicU64>,
}

impl SessionManager {
    pub fn new(max_sessions: usize) -> Self {
        Self::new_with_settings(max_sessions, GatewaySettings::default())
    }

    pub fn new_with_settings(max_sessions: usize, settings: GatewaySettings) -> Self {
        Self {
            sessions: Arc::new(RwLock::new(HashMap::new())),
            max_sessions,
            settings,
            next_id: Arc::new(AtomicU64::new(1)),
        }
    }

    /// Create a new session. Returns the session ID.
    pub async fn create_session(&self, config: SessionConfig) -> Result<String, GatewayError> {
        let client = self.build_model_client(&config)?;
        self.create_session_with_client(config, client).await
    }

    async fn create_session_with_client(
        &self,
        config: SessionConfig,
        client: Arc<dyn ModelClient>,
    ) -> Result<String, GatewayError> {
        let mut sessions = self.sessions.write().await;

        if sessions.len() >= self.max_sessions {
            return Err(GatewayError::MaxSessionsReached(self.max_sessions));
        }

        let id = format!("gw_ses_{}", self.next_id.fetch_add(1, Ordering::Relaxed));
        let agent_config = self.build_agent_config(&config);

        let agent = Agent::new(agent_config, client);
        let (agent, event_stream) = agent.with_event_stream();
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
            skill_context: config.skill_context.clone(),
            config,
        };

        sessions.insert(
            id.clone(),
            ManagedSession {
                info,
                agent_input_tx,
                event_stream: Some(event_stream),
                agent_handle,
                agent_control,
            },
        );

        Ok(id)
    }

    /// Destroy a session by ID.
    pub async fn destroy_session(&self, session_id: &str) -> Result<(), GatewayError> {
        let mut sessions = self.sessions.write().await;

        let session = sessions
            .remove(session_id)
            .ok_or_else(|| GatewayError::SessionNotFound(session_id.to_string()))?;

        drop(sessions);

        let ManagedSession {
            agent_input_tx,
            event_stream: _,
            agent_handle,
            agent_control,
            info: _,
        } = session;

        agent_control.store(true, Ordering::SeqCst);
        drop(agent_input_tx);

        match timeout(Duration::from_secs(5), agent_handle).await {
            Ok(Ok(())) => {}
            Ok(Err(error)) => {
                return Err(GatewayError::SessionShutdownFailed(format!(
                    "Agent task join error: {}",
                    error
                )));
            }
            Err(_) => {
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
        let mut infos: Vec<SessionInfo> = sessions.values().map(|session| session.info.clone()).collect();
        infos.sort_by(|a, b| a.id.cmp(&b.id));
        infos
    }

    /// Send a message to a specific session.
    pub async fn send_message(&self, session_id: &str, message: String) -> Result<(), GatewayError> {
        let sender = {
            let sessions = self.sessions.read().await;
            let session = sessions
                .get(session_id)
                .ok_or_else(|| GatewayError::SessionNotFound(session_id.to_string()))?;
            session.agent_input_tx.clone()
        };

        let prompt = Message::user_prompt(&message);

        sender
            .send(prompt)
            .await
            .map_err(|e| GatewayError::SendFailed(e.to_string()))
    }

    pub async fn take_event_stream(&self, session_id: &str) -> Option<EventStream> {
        let mut sessions = self.sessions.write().await;
        sessions
            .get_mut(session_id)
            .and_then(|session| session.event_stream.take())
    }

    /// Get the number of active sessions.
    pub async fn session_count(&self) -> usize {
        self.sessions.read().await.len()
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

    fn build_model_client(&self, config: &SessionConfig) -> Result<Arc<dyn ModelClient>, GatewayError> {
        let provider_config = self.build_provider_config(config)?;
        ModelClientBuilder::new()
            .with_config(provider_config)
            .build()
            .map_err(|error| GatewayError::SessionCreationFailed(error.to_string()))
    }

    fn build_provider_config(&self, config: &SessionConfig) -> Result<ProviderConfig, GatewayError> {
        let provider_name = config
            .provider
            .as_deref()
            .unwrap_or(self.settings.provider.as_str());
        let provider = parse_provider(provider_name)?;
        let model = config
            .model
            .clone()
            .unwrap_or_else(|| self.settings.model.clone());

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

    fn build_agent_config(&self, config: &SessionConfig) -> AgentConfig {
        let mut agent_config = AgentConfig::new().full_auto();

        let model = config
            .model
            .clone()
            .unwrap_or_else(|| self.settings.model.clone());
        agent_config = agent_config.with_model(model);

        let working_directory = config
            .working_directory
            .as_ref()
            .or(self.settings.working_directory.as_ref());
        if let Some(path) = working_directory {
            agent_config = agent_config.with_working_directory(path);
        }

        if let Some(skill_context) = &config.skill_context {
            agent_config = agent_config.with_additional_context(vec![skill_context.clone()]);
        }

        agent_config
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

    use futures_util::StreamExt;
    use uira_providers::ModelClient;
    use uira_types::ThreadEvent;

    use crate::testing::MockModelClient;

    fn test_settings() -> GatewaySettings {
        GatewaySettings {
            provider: "ollama".to_string(),
            model: "llama3.1".to_string(),
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
        assert!(manager.take_event_stream(&id).await.is_some());
        assert!(manager.take_event_stream(&id).await.is_none());
    }

    #[tokio::test]
    async fn test_send_message_to_agent() {
        let manager = SessionManager::new_with_settings(10, test_settings());
        let client = Arc::new(MockModelClient::new("ok"));
        let id = manager
            .create_session_with_client(SessionConfig::default(), client.clone() as Arc<dyn ModelClient>)
            .await
            .unwrap();
        let mut event_stream = manager.take_event_stream(&id).await.unwrap();

        manager
            .send_message(&id, "hello".to_string())
            .await
            .unwrap();

        let mut observed_agent_activity = false;
        for _ in 0..20 {
            if let Ok(Some(event)) = timeout(Duration::from_millis(100), event_stream.next()).await {
                match event {
                    ThreadEvent::TurnStarted { .. }
                    | ThreadEvent::ContentDelta { .. }
                    | ThreadEvent::ThreadCompleted { .. }
                    | ThreadEvent::Error { .. } => {
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
        let result = manager.send_message("nonexistent", "hello".to_string()).await;
        assert!(matches!(result, Err(GatewayError::SessionNotFound(_))));
    }

    #[tokio::test]
    async fn test_destroy_nonexistent_session() {
        let manager = SessionManager::new_with_settings(10, test_settings());
        let result = manager.destroy_session("nonexistent").await;
        assert!(matches!(result, Err(GatewayError::SessionNotFound(_))));
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
    async fn test_concurrent_session_access() {
        let manager = Arc::new(SessionManager::new_with_settings(100, test_settings()));
        let mut handles = vec![];
        for _ in 0..10 {
            let m = manager.clone();
            handles.push(tokio::spawn(async move {
                let client = Arc::new(MockModelClient::new("ok"));
                m.create_session_with_client(SessionConfig::default(), client as Arc<dyn ModelClient>)
                    .await
                    .unwrap()
            }));
        }
        for h in handles {
            h.await.unwrap();
        }
        assert_eq!(manager.session_count().await, 10);
    }
}
