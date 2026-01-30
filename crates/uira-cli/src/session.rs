//! Session persistence for resuming agent conversations

use chrono::{DateTime, Utc};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::LazyLock;
use uira_protocol::{Message, TokenUsage};

/// Stored session metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionMeta {
    pub id: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub provider: String,
    pub model: String,
    pub turns: usize,
    pub usage: TokenUsage,
    /// Brief summary of the conversation (first user message)
    pub summary: String,
}

/// Full session data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredSession {
    pub meta: SessionMeta,
    pub messages: Vec<Message>,
    pub working_directory: PathBuf,
}

/// Session storage manager
pub struct SessionStorage {
    sessions_dir: PathBuf,
}

impl SessionStorage {
    /// Create a new session storage
    pub fn new() -> std::io::Result<Self> {
        let sessions_dir = Self::sessions_dir()?;
        std::fs::create_dir_all(&sessions_dir)?;
        Ok(Self { sessions_dir })
    }

    fn sessions_dir() -> std::io::Result<PathBuf> {
        let data_dir = dirs::data_dir()
            .or_else(dirs::home_dir)
            .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "No data dir"))?;

        Ok(data_dir.join("uira").join("sessions"))
    }

    fn validate_session_id(session_id: &str) -> std::io::Result<()> {
        static SESSION_ID_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
            Regex::new(r"^[A-Za-z0-9_-]{1,64}$").expect("Invalid session ID regex")
        });

        if !SESSION_ID_PATTERN.is_match(session_id) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "Invalid session ID: must be 1-64 alphanumeric characters, underscores, or hyphens",
            ));
        }
        Ok(())
    }

    /// Load a session by ID
    pub fn load(&self, session_id: &str) -> std::io::Result<StoredSession> {
        Self::validate_session_id(session_id)?;
        let session_path = self.sessions_dir.join(format!("{}.json", session_id));
        let content = std::fs::read_to_string(session_path)?;
        serde_json::from_str(&content)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
    }

    /// List all sessions (most recent first)
    pub fn list(&self) -> std::io::Result<Vec<SessionMeta>> {
        let mut sessions = Vec::new();

        if !self.sessions_dir.exists() {
            return Ok(sessions);
        }

        for entry in std::fs::read_dir(&self.sessions_dir)? {
            let entry = entry?;
            let path = entry.path();

            if path.extension().is_some_and(|e| e == "json") {
                if let Ok(content) = std::fs::read_to_string(&path) {
                    if let Ok(session) = serde_json::from_str::<StoredSession>(&content) {
                        sessions.push(session.meta);
                    }
                }
            }
        }

        // Sort by updated_at (most recent first)
        sessions.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));

        Ok(sessions)
    }

    /// List recent sessions (up to limit)
    pub fn list_recent(&self, limit: usize) -> std::io::Result<Vec<SessionMeta>> {
        let mut sessions = self.list()?;
        sessions.truncate(limit);
        Ok(sessions)
    }
}

impl Default for SessionStorage {
    fn default() -> Self {
        Self::new().unwrap_or_else(|_| Self {
            sessions_dir: PathBuf::from(".uira-sessions"),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_meta_serialization() {
        let meta = SessionMeta {
            id: "test-123".to_string(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            provider: "anthropic".to_string(),
            model: "claude-3".to_string(),
            turns: 5,
            usage: TokenUsage::default(),
            summary: "Test session".to_string(),
        };

        let json = serde_json::to_string(&meta).unwrap();
        let parsed: SessionMeta = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.id, "test-123");
    }

    #[test]
    fn test_validate_session_id_valid_ids() {
        // Valid IDs with alphanumeric characters
        assert!(SessionStorage::validate_session_id("abc123").is_ok());
        assert!(SessionStorage::validate_session_id("test-session").is_ok());
        assert!(SessionStorage::validate_session_id("test_session").is_ok());
        assert!(SessionStorage::validate_session_id("ABC123").is_ok());
        assert!(SessionStorage::validate_session_id("a").is_ok());
        assert!(SessionStorage::validate_session_id("Z").is_ok());
        assert!(SessionStorage::validate_session_id("0").is_ok());
        assert!(SessionStorage::validate_session_id("_").is_ok());
        assert!(SessionStorage::validate_session_id("-").is_ok());

        // Max length (64 characters)
        let max_id = "a".repeat(64);
        assert!(SessionStorage::validate_session_id(&max_id).is_ok());

        // Mixed valid characters
        assert!(SessionStorage::validate_session_id("test-123_ABC").is_ok());
        assert!(SessionStorage::validate_session_id("session_2024-01-30").is_ok());
    }

    #[test]
    fn test_validate_session_id_invalid_ids() {
        // Empty string
        assert!(SessionStorage::validate_session_id("").is_err());

        // Path separators
        assert!(SessionStorage::validate_session_id("test/session").is_err());
        assert!(SessionStorage::validate_session_id("test\\session").is_err());
        assert!(SessionStorage::validate_session_id("/test").is_err());
        assert!(SessionStorage::validate_session_id("test/").is_err());

        // Parent directory references
        assert!(SessionStorage::validate_session_id("..").is_err());
        assert!(SessionStorage::validate_session_id("test..session").is_err());
        assert!(SessionStorage::validate_session_id("../test").is_err());

        // Null character
        assert!(SessionStorage::validate_session_id("test\0session").is_err());

        // Special characters not allowed
        assert!(SessionStorage::validate_session_id("test@session").is_err());
        assert!(SessionStorage::validate_session_id("test.session").is_err());
        assert!(SessionStorage::validate_session_id("test session").is_err());
        assert!(SessionStorage::validate_session_id("test#session").is_err());
        assert!(SessionStorage::validate_session_id("test$session").is_err());

        // Exceeds max length (65 characters)
        let too_long = "a".repeat(65);
        assert!(SessionStorage::validate_session_id(&too_long).is_err());
    }

    #[test]
    fn test_validate_session_id_error_message() {
        let result = SessionStorage::validate_session_id("invalid/path");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::InvalidInput);
        assert!(err.to_string().contains("alphanumeric"));
    }
}
