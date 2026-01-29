//! Session persistence for resuming agent conversations

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
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

#[allow(dead_code)]
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

    /// Save a session to disk
    pub fn save(&self, session: &StoredSession) -> std::io::Result<()> {
        let session_path = self.sessions_dir.join(format!("{}.json", session.meta.id));
        let content = serde_json::to_string_pretty(session)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        std::fs::write(session_path, content)
    }

    /// Load a session by ID
    pub fn load(&self, session_id: &str) -> std::io::Result<StoredSession> {
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

    /// Delete a session
    pub fn delete(&self, session_id: &str) -> std::io::Result<()> {
        let session_path = self.sessions_dir.join(format!("{}.json", session_id));
        std::fs::remove_file(session_path)
    }

    /// Clean up old sessions (keep only N most recent)
    pub fn cleanup(&self, keep: usize) -> std::io::Result<usize> {
        let sessions = self.list()?;
        let mut deleted = 0;

        for session in sessions.into_iter().skip(keep) {
            if self.delete(&session.id).is_ok() {
                deleted += 1;
            }
        }

        Ok(deleted)
    }
}

impl Default for SessionStorage {
    fn default() -> Self {
        Self::new().unwrap_or_else(|_| Self {
            sessions_dir: PathBuf::from(".uira-sessions"),
        })
    }
}

/// Create a session from agent execution
#[allow(dead_code)]
pub fn create_session(
    session_id: &str,
    messages: Vec<Message>,
    provider: &str,
    model: &str,
    turns: usize,
    usage: TokenUsage,
    working_directory: PathBuf,
) -> StoredSession {
    let summary = messages
        .iter()
        .find(|m| matches!(m.role, uira_protocol::Role::User))
        .map(|m| match &m.content {
            uira_protocol::MessageContent::Text(t) => {
                let truncated: String = t.chars().take(100).collect();
                if t.len() > 100 {
                    format!("{}...", truncated)
                } else {
                    truncated
                }
            }
            _ => "...".to_string(),
        })
        .unwrap_or_else(|| "Empty session".to_string());

    let now = Utc::now();

    StoredSession {
        meta: SessionMeta {
            id: session_id.to_string(),
            created_at: now,
            updated_at: now,
            provider: provider.to_string(),
            model: model.to_string(),
            turns,
            usage,
            summary,
        },
        messages,
        working_directory,
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
}
