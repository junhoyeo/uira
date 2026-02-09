//! Session persistence for resuming agent conversations

use chrono::{DateTime, Utc};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::LazyLock;
use uira_agent::RolloutRecorder;
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
        // Prefer ~/.uira for consistency with other CLI tools, fall back to XDG data dir
        // for environments where HOME is unset (systemd services, containers)
        let base_dir = dirs::home_dir()
            .map(|h| h.join(".uira"))
            .or_else(|| dirs::data_dir().map(|d| d.join("uira")))
            .ok_or_else(|| {
                std::io::Error::new(std::io::ErrorKind::NotFound, "No home or data dir")
            })?;

        Ok(base_dir.join("sessions"))
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

pub struct SessionEntry {
    pub thread_id: String,
    pub timestamp: DateTime<Utc>,
    pub model: String,
    pub provider: String,
    pub turns: usize,
    pub parent_id: Option<String>,
    pub fork_count: u32,
    pub path: PathBuf,
}

pub fn list_rollout_sessions(limit: usize) -> std::io::Result<Vec<SessionEntry>> {
    let rollouts = RolloutRecorder::list_recent(limit)?;
    let entries = rollouts
        .into_iter()
        .map(|(path, meta)| SessionEntry {
            thread_id: meta.thread_id,
            timestamp: meta.timestamp,
            model: meta.model,
            provider: meta.provider,
            turns: meta.turns,
            parent_id: meta.parent_id.map(|id| id.to_string()),
            fork_count: meta.fork_count,
            path,
        })
        .collect();
    Ok(entries)
}

pub fn display_sessions_list(entries: &[SessionEntry]) {
    if entries.is_empty() {
        println!("No sessions found.");
        return;
    }

    println!(
        "{:<24} {:<20} {:<24} {:>5} {:>5}",
        "SESSION ID", "TIMESTAMP", "MODEL", "TURNS", "FORKS"
    );
    println!("{}", "-".repeat(82));

    for entry in entries {
        let timestamp = entry.timestamp.format("%Y-%m-%d %H:%M:%S");
        let model_display = if entry.model.len() > 20 {
            format!("{}...", &entry.model[..17])
        } else {
            entry.model.clone()
        };

        let id_prefix = if entry.parent_id.is_some() {
            "└─"
        } else {
            ""
        };

        println!(
            "{}{:<24} {:<20} {:<24} {:>5} {:>5}",
            id_prefix,
            truncate(&entry.thread_id, 24 - id_prefix.len()),
            timestamp,
            model_display,
            entry.turns,
            entry.fork_count
        );
    }
}

pub fn display_sessions_tree(entries: &[SessionEntry]) {
    if entries.is_empty() {
        println!("No sessions found.");
        return;
    }

    let mut by_parent: HashMap<Option<String>, Vec<&SessionEntry>> = HashMap::new();
    for entry in entries {
        by_parent
            .entry(entry.parent_id.clone())
            .or_default()
            .push(entry);
    }

    let roots: Vec<_> = entries.iter().filter(|e| e.parent_id.is_none()).collect();

    println!("Session Fork Tree:");
    println!("{}", "=".repeat(60));

    for root in roots {
        print_tree_node(root, &by_parent, "", true);
    }
}

fn print_tree_node(
    entry: &SessionEntry,
    by_parent: &HashMap<Option<String>, Vec<&SessionEntry>>,
    prefix: &str,
    is_last: bool,
) {
    let connector = if prefix.is_empty() {
        ""
    } else if is_last {
        "└── "
    } else {
        "├── "
    };

    let timestamp = entry.timestamp.format("%m-%d %H:%M");
    println!(
        "{}{}{} ({}, {} turns)",
        prefix, connector, entry.thread_id, timestamp, entry.turns
    );

    if let Some(children) = by_parent.get(&Some(entry.thread_id.clone())) {
        let child_prefix = if prefix.is_empty() {
            "    ".to_string()
        } else if is_last {
            format!("{}    ", prefix)
        } else {
            format!("{}│   ", prefix)
        };

        for (i, child) in children.iter().enumerate() {
            let is_last_child = i == children.len() - 1;
            print_tree_node(child, by_parent, &child_prefix, is_last_child);
        }
    }
}

fn truncate(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len.saturating_sub(3)])
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
