//! Session state management and lifecycle tracking
//!
//! Provides thread-safe state management for Astrape sessions, including:
//! - Session lifecycle tracking (idle, active, background, complete)
//! - State persistence and retrieval
//! - Concurrent access via RwLock

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use thiserror::Error;

/// Session state lifecycle stages
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum SessionState {
    /// Session is idle, no active work
    #[default]
    Idle,
    /// Session is actively executing
    Active,
    /// Session is running in background
    Background,
    /// Session is paused
    Paused,
    /// Session completed successfully
    Complete,
    /// Session failed
    Failed,
    /// Session was cancelled
    Cancelled,
}

impl SessionState {
    /// Check if the session is in a terminal state (cannot transition further)
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            SessionState::Complete | SessionState::Failed | SessionState::Cancelled
        )
    }

    /// Check if the session is currently running (active or background)
    pub fn is_running(&self) -> bool {
        matches!(self, SessionState::Active | SessionState::Background)
    }
}

/// Errors that can occur during state management
#[derive(Error, Debug)]
pub enum StateManagerError {
    #[error("Session not found: {0}")]
    SessionNotFound(String),

    #[error("Invalid state transition from {from:?} to {to:?}")]
    InvalidTransition {
        from: SessionState,
        to: SessionState,
    },

    #[error("Session is in terminal state {0:?}")]
    TerminalState(SessionState),

    #[error("Serialization error: {0}")]
    SerializationError(#[from] serde_json::Error),
}

/// Session metadata and state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    /// Unique session identifier
    pub id: String,
    /// Current state
    pub state: SessionState,
    /// Session-specific metadata
    pub metadata: HashMap<String, serde_json::Value>,
    /// Timestamp when session was created (Unix timestamp)
    pub created_at: u64,
    /// Timestamp when session was last updated (Unix timestamp)
    pub updated_at: u64,
}

impl Session {
    /// Create a new session
    pub fn new(id: String) -> Self {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        Self {
            id,
            state: SessionState::default(),
            metadata: HashMap::new(),
            created_at: now,
            updated_at: now,
        }
    }

    /// Update the session state
    pub fn set_state(&mut self, state: SessionState) {
        self.state = state;
        self.updated_at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
    }

    /// Get metadata value by key
    pub fn get_metadata(&self, key: &str) -> Option<&serde_json::Value> {
        self.metadata.get(key)
    }

    /// Set metadata value
    pub fn set_metadata(&mut self, key: String, value: serde_json::Value) {
        self.metadata.insert(key, value);
        self.updated_at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
    }
}

/// Thread-safe state manager for session lifecycle management
#[derive(Debug, Clone)]
pub struct StateManager {
    sessions: Arc<RwLock<HashMap<String, Session>>>,
}

impl Default for StateManager {
    fn default() -> Self {
        Self::new()
    }
}

impl StateManager {
    /// Create a new state manager
    pub fn new() -> Self {
        Self {
            sessions: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Create a new session
    pub fn create_session(&self, id: String) -> Session {
        let session = Session::new(id.clone());
        let mut sessions = self.sessions.write();
        sessions.insert(id, session.clone());
        session
    }

    /// Get a session by ID
    pub fn get_session(&self, id: &str) -> Option<Session> {
        let sessions = self.sessions.read();
        sessions.get(id).cloned()
    }

    /// Get current state for a session
    pub fn get_state(&self, id: &str) -> Result<SessionState, StateManagerError> {
        let sessions = self.sessions.read();
        sessions
            .get(id)
            .map(|s| s.state)
            .ok_or_else(|| StateManagerError::SessionNotFound(id.to_string()))
    }

    /// Set state for a session
    pub fn set_state(&self, id: &str, state: SessionState) -> Result<(), StateManagerError> {
        let mut sessions = self.sessions.write();
        let session = sessions
            .get_mut(id)
            .ok_or_else(|| StateManagerError::SessionNotFound(id.to_string()))?;

        // Prevent transitions from terminal states
        if session.state.is_terminal() {
            return Err(StateManagerError::TerminalState(session.state));
        }

        session.set_state(state);
        Ok(())
    }

    /// Check if a session is active
    pub fn is_active(&self, id: &str) -> bool {
        self.get_state(id)
            .map(|s| s == SessionState::Active)
            .unwrap_or(false)
    }

    /// Check if a session is running (active or background)
    pub fn is_running(&self, id: &str) -> bool {
        self.get_state(id).map(|s| s.is_running()).unwrap_or(false)
    }

    /// Get or create a session (thread-safe, uses single write lock)
    pub fn get_or_create_session(&self, id: &str) -> Session {
        // Use a single write lock to prevent race conditions between check and create
        let mut sessions = self.sessions.write();
        if let Some(session) = sessions.get(id) {
            session.clone()
        } else {
            let session = Session::new(id.to_string());
            let result = session.clone();
            sessions.insert(id.to_string(), session);
            result
        }
    }

    /// Update session metadata
    pub fn set_metadata(
        &self,
        id: &str,
        key: String,
        value: serde_json::Value,
    ) -> Result<(), StateManagerError> {
        let mut sessions = self.sessions.write();
        let session = sessions
            .get_mut(id)
            .ok_or_else(|| StateManagerError::SessionNotFound(id.to_string()))?;

        session.set_metadata(key, value);
        Ok(())
    }

    /// Get session metadata
    pub fn get_metadata(
        &self,
        id: &str,
        key: &str,
    ) -> Result<Option<serde_json::Value>, StateManagerError> {
        let sessions = self.sessions.read();
        let session = sessions
            .get(id)
            .ok_or_else(|| StateManagerError::SessionNotFound(id.to_string()))?;

        Ok(session.get_metadata(key).cloned())
    }

    /// Delete a session
    pub fn delete_session(&self, id: &str) -> Option<Session> {
        let mut sessions = self.sessions.write();
        sessions.remove(id)
    }

    /// List all session IDs
    pub fn list_sessions(&self) -> Vec<String> {
        let sessions = self.sessions.read();
        sessions.keys().cloned().collect()
    }

    /// Get count of active sessions
    pub fn active_count(&self) -> usize {
        let sessions = self.sessions.read();
        sessions
            .values()
            .filter(|s| s.state == SessionState::Active)
            .count()
    }

    /// Clear all sessions
    pub fn clear(&self) {
        let mut sessions = self.sessions.write();
        sessions.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_state_transitions() {
        assert!(!SessionState::Active.is_terminal());
        assert!(SessionState::Complete.is_terminal());
        assert!(SessionState::Active.is_running());
        assert!(!SessionState::Idle.is_running());
    }

    #[test]
    fn test_session_creation() {
        let session = Session::new("test-1".to_string());
        assert_eq!(session.id, "test-1");
        assert_eq!(session.state, SessionState::Idle);
        assert!(session.metadata.is_empty());
    }

    #[test]
    fn test_state_manager_basic() {
        let manager = StateManager::new();
        let session = manager.create_session("test-1".to_string());

        assert_eq!(session.state, SessionState::Idle);
        assert!(manager.is_active("test-1") == false);
    }

    #[test]
    fn test_state_transitions() {
        let manager = StateManager::new();
        manager.create_session("test-1".to_string());

        manager.set_state("test-1", SessionState::Active).unwrap();
        assert!(manager.is_active("test-1"));

        manager
            .set_state("test-1", SessionState::Background)
            .unwrap();
        assert!(manager.is_running("test-1"));
    }

    #[test]
    fn test_terminal_state_prevention() {
        let manager = StateManager::new();
        manager.create_session("test-1".to_string());

        manager.set_state("test-1", SessionState::Complete).unwrap();

        let result = manager.set_state("test-1", SessionState::Active);
        assert!(matches!(
            result,
            Err(StateManagerError::TerminalState(SessionState::Complete))
        ));
    }

    #[test]
    fn test_metadata() {
        let manager = StateManager::new();
        manager.create_session("test-1".to_string());

        manager
            .set_metadata("test-1", "key1".to_string(), serde_json::json!("value1"))
            .unwrap();

        let value = manager.get_metadata("test-1", "key1").unwrap();
        assert_eq!(value, Some(serde_json::json!("value1")));
    }

    #[test]
    fn test_session_not_found() {
        let manager = StateManager::new();
        let result = manager.get_state("nonexistent");
        assert!(matches!(result, Err(StateManagerError::SessionNotFound(_))));
    }

    #[test]
    fn test_list_and_count() {
        let manager = StateManager::new();
        manager.create_session("test-1".to_string());
        manager.create_session("test-2".to_string());
        manager.create_session("test-3".to_string());

        manager.set_state("test-1", SessionState::Active).unwrap();
        manager.set_state("test-2", SessionState::Active).unwrap();

        assert_eq!(manager.list_sessions().len(), 3);
        assert_eq!(manager.active_count(), 2);
    }

    #[test]
    fn test_clear() {
        let manager = StateManager::new();
        manager.create_session("test-1".to_string());
        manager.create_session("test-2".to_string());

        manager.clear();
        assert_eq!(manager.list_sessions().len(), 0);
    }
}
