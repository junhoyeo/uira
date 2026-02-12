use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use chrono::{DateTime, Utc};
use tokio::sync::{mpsc, RwLock};

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
    message_tx: mpsc::Sender<String>,
    _message_rx: mpsc::Receiver<String>,
}

/// Manages multiple concurrent agent sessions for the gateway.
pub struct SessionManager {
    sessions: Arc<RwLock<HashMap<String, ManagedSession>>>,
    max_sessions: usize,
    next_id: Arc<AtomicU64>,
}

impl SessionManager {
    pub fn new(max_sessions: usize) -> Self {
        Self {
            sessions: Arc::new(RwLock::new(HashMap::new())),
            max_sessions,
            next_id: Arc::new(AtomicU64::new(1)),
        }
    }

    /// Create a new session. Returns the session ID.
    pub async fn create_session(&self, config: SessionConfig) -> Result<String, GatewayError> {
        let mut sessions = self.sessions.write().await;

        if sessions.len() >= self.max_sessions {
            return Err(GatewayError::MaxSessionsReached(self.max_sessions));
        }

        let id = format!("gw_ses_{}", self.next_id.fetch_add(1, Ordering::Relaxed));
        let (message_tx, message_rx) = mpsc::channel(128);

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
                message_tx,
                _message_rx: message_rx,
            },
        );

        Ok(id)
    }

    /// Destroy a session by ID.
    pub async fn destroy_session(&self, session_id: &str) -> Result<(), GatewayError> {
        let mut sessions = self.sessions.write().await;
        if sessions.remove(session_id).is_none() {
            return Err(GatewayError::SessionNotFound(session_id.to_string()));
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
            session.message_tx.clone()
        };

        sender
            .send(message)
            .await
            .map_err(|e| GatewayError::SendFailed(e.to_string()))
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_create_session() {
        let manager = SessionManager::new(10);
        let id = manager
            .create_session(SessionConfig::default())
            .await
            .unwrap();
        assert!(id.starts_with("gw_ses_"));
        assert_eq!(manager.session_count().await, 1);
    }

    #[tokio::test]
    async fn test_destroy_session() {
        let manager = SessionManager::new(10);
        let id = manager
            .create_session(SessionConfig::default())
            .await
            .unwrap();
        manager.destroy_session(&id).await.unwrap();
        assert_eq!(manager.session_count().await, 0);
    }

    #[tokio::test]
    async fn test_destroy_nonexistent_session() {
        let manager = SessionManager::new(10);
        let result = manager.destroy_session("nonexistent").await;
        assert!(matches!(result, Err(GatewayError::SessionNotFound(_))));
    }

    #[tokio::test]
    async fn test_list_sessions() {
        let manager = SessionManager::new(10);
        let id1 = manager
            .create_session(SessionConfig::default())
            .await
            .unwrap();
        let id2 = manager
            .create_session(SessionConfig::default())
            .await
            .unwrap();
        let sessions = manager.list_sessions().await;
        assert_eq!(sessions.len(), 2);
        let ids: Vec<&str> = sessions.iter().map(|s| s.id.as_str()).collect();
        assert!(ids.contains(&id1.as_str()));
        assert!(ids.contains(&id2.as_str()));
    }

    #[tokio::test]
    async fn test_max_sessions_limit() {
        let manager = SessionManager::new(2);
        manager
            .create_session(SessionConfig::default())
            .await
            .unwrap();
        manager
            .create_session(SessionConfig::default())
            .await
            .unwrap();
        let result = manager.create_session(SessionConfig::default()).await;
        assert!(matches!(result, Err(GatewayError::MaxSessionsReached(_))));
    }

    #[tokio::test]
    async fn test_send_message() {
        let manager = SessionManager::new(10);
        let id = manager
            .create_session(SessionConfig::default())
            .await
            .unwrap();
        let result = manager.send_message(&id, "hello".to_string()).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_send_message_nonexistent_session() {
        let manager = SessionManager::new(10);
        let result = manager.send_message("nonexistent", "hello".to_string()).await;
        assert!(matches!(result, Err(GatewayError::SessionNotFound(_))));
    }

    #[tokio::test]
    async fn test_has_session() {
        let manager = SessionManager::new(10);
        let id = manager
            .create_session(SessionConfig::default())
            .await
            .unwrap();
        assert!(manager.has_session(&id).await);
        assert!(!manager.has_session("nonexistent").await);
    }

    #[tokio::test]
    async fn test_concurrent_session_access() {
        let manager = Arc::new(SessionManager::new(100));
        let mut handles = vec![];
        for _ in 0..10 {
            let m = manager.clone();
            handles.push(tokio::spawn(async move {
                m.create_session(SessionConfig::default()).await.unwrap()
            }));
        }
        for h in handles {
            h.await.unwrap();
        }
        assert_eq!(manager.session_count().await, 10);
    }
}
