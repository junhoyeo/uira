//! Session persistence using append-only JSONL rollout (Codex pattern)
//!
//! This module implements the RolloutRecorder which persists all session
//! events to a JSONL file for debugging, replay, and resume capabilities.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use uira_protocol::{Message, MessageId, SessionId, ThreadEvent, TokenUsage};

/// Items that can be recorded to the rollout
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RolloutItem {
    /// Session metadata (always first line)
    SessionMeta(SessionMetaLine),

    /// A conversation message (user, assistant, tool)
    Message(RolloutMessage),

    /// A thread event from execution (wrapped to avoid type field conflict)
    Event {
        #[serde(flatten)]
        event: EventWrapper,
    },

    /// Tool call being executed
    ToolCall {
        id: String,
        name: String,
        input: serde_json::Value,
    },

    /// Tool result after execution
    ToolResult {
        id: String,
        output: String,
        is_error: bool,
    },

    /// Turn context at end of turn
    TurnContext { turn: usize, usage: TokenUsage },

    /// Session fork event
    SessionForked {
        child_session_id: SessionId,
        forked_from_message: Option<MessageId>,
        message_count: usize,
        timestamp: DateTime<Utc>,
    },
}

/// Wrapper for ThreadEvent to handle serialization properly
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventWrapper {
    /// The event kind identifier
    pub event_type: String,
    /// Event data as JSON
    pub data: serde_json::Value,
}

impl From<ThreadEvent> for EventWrapper {
    fn from(event: ThreadEvent) -> Self {
        let data = serde_json::to_value(&event).unwrap_or(serde_json::Value::Null);
        let event_type = data
            .get("type")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string();
        Self { event_type, data }
    }
}

/// Wrapper for Message that includes timestamp
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RolloutMessage {
    pub message: Message,
    pub timestamp: DateTime<Utc>,
}

impl RolloutMessage {
    pub fn new(message: Message) -> Self {
        Self {
            message,
            timestamp: Utc::now(),
        }
    }
}

/// Session metadata stored as first line of rollout file
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionMetaLine {
    /// Unique session/thread identifier
    pub thread_id: String,

    /// When the session started
    pub timestamp: DateTime<Utc>,

    /// Model identifier being used
    pub model: String,

    /// Provider name (anthropic, openai, etc.)
    pub provider: String,

    /// Working directory
    pub cwd: PathBuf,

    /// Sandbox policy in effect
    pub sandbox_policy: String,

    /// Git commit hash if available
    #[serde(skip_serializing_if = "Option::is_none")]
    pub git_commit: Option<String>,

    /// Git branch if available
    #[serde(skip_serializing_if = "Option::is_none")]
    pub git_branch: Option<String>,

    /// Total turns when metadata was last updated
    #[serde(default)]
    pub turns: usize,

    /// Total token usage when metadata was last updated
    #[serde(default)]
    pub total_usage: TokenUsage,

    // --- Fork metadata (Phase 2) ---
    /// Parent session ID if this is a forked session
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<SessionId>,

    /// Message ID where the fork occurred
    #[serde(skip_serializing_if = "Option::is_none")]
    pub forked_from_message: Option<MessageId>,

    /// Number of child forks from this session
    #[serde(default)]
    pub fork_count: u32,
}

impl SessionMetaLine {
    pub fn new(
        thread_id: impl Into<String>,
        model: impl Into<String>,
        provider: impl Into<String>,
        cwd: PathBuf,
        sandbox_policy: impl Into<String>,
    ) -> Self {
        Self {
            thread_id: thread_id.into(),
            timestamp: Utc::now(),
            model: model.into(),
            provider: provider.into(),
            cwd,
            sandbox_policy: sandbox_policy.into(),
            git_commit: Self::get_git_commit(),
            git_branch: Self::get_git_branch(),
            turns: 0,
            total_usage: TokenUsage::default(),
            parent_id: None,
            forked_from_message: None,
            fork_count: 0,
        }
    }

    pub fn new_forked(
        thread_id: impl Into<String>,
        model: impl Into<String>,
        provider: impl Into<String>,
        cwd: PathBuf,
        sandbox_policy: impl Into<String>,
        parent_id: SessionId,
        forked_from_message: Option<MessageId>,
    ) -> Self {
        Self {
            thread_id: thread_id.into(),
            timestamp: Utc::now(),
            model: model.into(),
            provider: provider.into(),
            cwd,
            sandbox_policy: sandbox_policy.into(),
            git_commit: Self::get_git_commit(),
            git_branch: Self::get_git_branch(),
            turns: 0,
            total_usage: TokenUsage::default(),
            parent_id: Some(parent_id),
            forked_from_message,
            fork_count: 0,
        }
    }

    fn get_git_commit() -> Option<String> {
        std::process::Command::new("git")
            .args(["rev-parse", "HEAD"])
            .output()
            .ok()
            .and_then(|output| {
                if output.status.success() {
                    String::from_utf8(output.stdout)
                        .ok()
                        .map(|s| s.trim().to_string())
                } else {
                    None
                }
            })
    }

    fn get_git_branch() -> Option<String> {
        std::process::Command::new("git")
            .args(["rev-parse", "--abbrev-ref", "HEAD"])
            .output()
            .ok()
            .and_then(|output| {
                if output.status.success() {
                    String::from_utf8(output.stdout)
                        .ok()
                        .map(|s| s.trim().to_string())
                } else {
                    None
                }
            })
    }
}

/// Recorder for session rollout with append-only JSONL persistence
pub struct RolloutRecorder {
    /// Open file handle for appending
    file: File,

    /// Path to the rollout file
    path: PathBuf,

    /// Session metadata (cached)
    meta: SessionMetaLine,
}

impl RolloutRecorder {
    /// Create a new rollout recorder for a session
    pub fn new(meta: SessionMetaLine) -> std::io::Result<Self> {
        let dir = Self::sessions_dir()?;
        std::fs::create_dir_all(&dir)?;

        let timestamp = meta.timestamp.format("%Y%m%d-%H%M%S");
        let path = dir.join(format!("rollout-{}-{}.jsonl", timestamp, meta.thread_id));

        let file = OpenOptions::new().create(true).append(true).open(&path)?;

        let mut recorder = Self { file, path, meta };

        // Write metadata as first line
        recorder.record(&RolloutItem::SessionMeta(recorder.meta.clone()))?;

        Ok(recorder)
    }

    /// Open an existing rollout file for resuming
    pub fn open(path: PathBuf) -> std::io::Result<Self> {
        // Read metadata from first line
        let meta = Self::extract_metadata(&path)?.ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Missing session metadata in rollout file",
            )
        })?;

        // Open for appending
        let file = OpenOptions::new().append(true).open(&path)?;

        Ok(Self { file, path, meta })
    }

    /// Get the sessions directory
    fn sessions_dir() -> std::io::Result<PathBuf> {
        let data_dir = dirs::data_dir().or_else(dirs::home_dir).ok_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::NotFound, "No data directory found")
        })?;

        Ok(data_dir.join("uira").join("sessions"))
    }

    /// Append an item to the rollout (immediate flush)
    pub fn record(&mut self, item: &RolloutItem) -> std::io::Result<()> {
        let line = serde_json::to_string(item)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        writeln!(self.file, "{}", line)?;
        self.file.flush()?;
        Ok(())
    }

    /// Record a message
    pub fn record_message(&mut self, message: Message) -> std::io::Result<()> {
        self.record(&RolloutItem::Message(RolloutMessage::new(message)))
    }

    /// Record a tool call
    pub fn record_tool_call(
        &mut self,
        id: impl Into<String>,
        name: impl Into<String>,
        input: serde_json::Value,
    ) -> std::io::Result<()> {
        self.record(&RolloutItem::ToolCall {
            id: id.into(),
            name: name.into(),
            input,
        })
    }

    /// Record a tool result
    pub fn record_tool_result(
        &mut self,
        id: impl Into<String>,
        output: impl Into<String>,
        is_error: bool,
    ) -> std::io::Result<()> {
        self.record(&RolloutItem::ToolResult {
            id: id.into(),
            output: output.into(),
            is_error,
        })
    }

    /// Record turn context
    pub fn record_turn(&mut self, turn: usize, usage: TokenUsage) -> std::io::Result<()> {
        // Update cached metadata
        self.meta.turns = turn;
        self.meta.total_usage = self.meta.total_usage.clone() + usage.clone();

        self.record(&RolloutItem::TurnContext { turn, usage })
    }

    /// Record a thread event
    pub fn record_event(&mut self, event: ThreadEvent) -> std::io::Result<()> {
        self.record(&RolloutItem::Event {
            event: EventWrapper::from(event),
        })
    }

    pub fn record_fork(
        &mut self,
        child_session_id: SessionId,
        forked_from_message: Option<MessageId>,
        message_count: usize,
    ) -> std::io::Result<()> {
        self.meta.fork_count += 1;
        self.record(&RolloutItem::SessionForked {
            child_session_id,
            forked_from_message,
            message_count,
            timestamp: Utc::now(),
        })
    }

    /// Load all items from a rollout file
    pub fn load(path: &PathBuf) -> std::io::Result<Vec<RolloutItem>> {
        let file = File::open(path)?;
        let reader = BufReader::new(file);
        let mut items = Vec::new();

        for line in reader.lines() {
            let line = line?;
            if line.trim().is_empty() {
                continue;
            }
            let item: RolloutItem = serde_json::from_str(&line)
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
            items.push(item);
        }

        Ok(items)
    }

    /// Extract only the metadata (first line) from a rollout file
    pub fn extract_metadata(path: &PathBuf) -> std::io::Result<Option<SessionMetaLine>> {
        let file = File::open(path)?;
        let mut reader = BufReader::new(file);
        let mut first_line = String::new();
        reader.read_line(&mut first_line)?;

        if first_line.trim().is_empty() {
            return Ok(None);
        }

        match serde_json::from_str::<RolloutItem>(&first_line) {
            Ok(RolloutItem::SessionMeta(meta)) => Ok(Some(meta)),
            Ok(_) => Ok(None),
            Err(_) => Ok(None),
        }
    }

    /// List all rollout files (most recent first)
    pub fn list_rollouts() -> std::io::Result<Vec<(PathBuf, SessionMetaLine)>> {
        let dir = Self::sessions_dir()?;
        if !dir.exists() {
            return Ok(Vec::new());
        }

        let mut rollouts = Vec::new();

        for entry in std::fs::read_dir(&dir)? {
            let entry = entry?;
            let path = entry.path();

            if path.extension().is_some_and(|e| e == "jsonl") {
                if let Ok(Some(meta)) = Self::extract_metadata(&path) {
                    rollouts.push((path, meta));
                }
            }
        }

        // Sort by timestamp (most recent first)
        rollouts.sort_by(|a, b| b.1.timestamp.cmp(&a.1.timestamp));

        Ok(rollouts)
    }

    /// List recent rollouts (up to limit)
    pub fn list_recent(limit: usize) -> std::io::Result<Vec<(PathBuf, SessionMetaLine)>> {
        let mut rollouts = Self::list_rollouts()?;
        rollouts.truncate(limit);
        Ok(rollouts)
    }

    /// Get the path of this rollout file
    pub fn path(&self) -> &PathBuf {
        &self.path
    }

    /// Get the session metadata
    pub fn meta(&self) -> &SessionMetaLine {
        &self.meta
    }
}

/// Extract messages from rollout items for context reconstruction
pub fn extract_messages(items: &[RolloutItem]) -> Vec<Message> {
    items
        .iter()
        .filter_map(|item| {
            if let RolloutItem::Message(rm) = item {
                Some(rm.message.clone())
            } else {
                None
            }
        })
        .collect()
}

/// Get the last turn number from rollout items
pub fn get_last_turn(items: &[RolloutItem]) -> usize {
    items
        .iter()
        .filter_map(|item| {
            if let RolloutItem::TurnContext { turn, .. } = item {
                Some(*turn)
            } else {
                None
            }
        })
        .max()
        .unwrap_or(0)
}

/// Get total usage from rollout items
pub fn get_total_usage(items: &[RolloutItem]) -> TokenUsage {
    items
        .iter()
        .filter_map(|item| {
            if let RolloutItem::TurnContext { usage, .. } = item {
                Some(usage.clone())
            } else {
                None
            }
        })
        .fold(TokenUsage::default(), |acc, u| acc + u)
}

#[cfg(test)]
mod tests {
    use super::*;
    use uira_protocol::Role;

    #[test]
    fn test_rollout_item_serialization() {
        let item = RolloutItem::ToolCall {
            id: "tc_123".to_string(),
            name: "read_file".to_string(),
            input: serde_json::json!({"path": "/tmp/test.txt"}),
        };

        let json = serde_json::to_string(&item).unwrap();
        assert!(json.contains("\"type\":\"tool_call\""));

        let parsed: RolloutItem = serde_json::from_str(&json).unwrap();
        if let RolloutItem::ToolCall { id, name, .. } = parsed {
            assert_eq!(id, "tc_123");
            assert_eq!(name, "read_file");
        } else {
            panic!("Wrong variant");
        }
    }

    #[test]
    fn test_session_meta_line() {
        let meta = SessionMetaLine::new(
            "thread_123",
            "claude-3",
            "anthropic",
            PathBuf::from("/home/user/project"),
            "workspace-write",
        );

        assert_eq!(meta.thread_id, "thread_123");
        assert_eq!(meta.model, "claude-3");
        assert_eq!(meta.provider, "anthropic");

        let json = serde_json::to_string(&meta).unwrap();
        let parsed: SessionMetaLine = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.thread_id, meta.thread_id);
    }

    #[test]
    fn test_rollout_message() {
        let msg = Message::user("Hello, world!");
        let rollout_msg = RolloutMessage::new(msg.clone());

        assert_eq!(rollout_msg.message.role, Role::User);

        let item = RolloutItem::Message(rollout_msg);
        let json = serde_json::to_string(&item).unwrap();
        assert!(json.contains("\"type\":\"message\""));
    }

    #[test]
    fn test_extract_messages() {
        let items = vec![
            RolloutItem::SessionMeta(SessionMetaLine::new(
                "thread_1",
                "model",
                "provider",
                PathBuf::from("."),
                "policy",
            )),
            RolloutItem::Message(RolloutMessage::new(Message::user("Hello"))),
            RolloutItem::ToolCall {
                id: "tc_1".to_string(),
                name: "test".to_string(),
                input: serde_json::Value::Null,
            },
            RolloutItem::Message(RolloutMessage::new(Message::assistant("Hi there!"))),
        ];

        let messages = extract_messages(&items);
        assert_eq!(messages.len(), 2);
    }

    #[test]
    fn test_get_last_turn() {
        let items = vec![
            RolloutItem::TurnContext {
                turn: 1,
                usage: TokenUsage::default(),
            },
            RolloutItem::TurnContext {
                turn: 2,
                usage: TokenUsage::default(),
            },
            RolloutItem::TurnContext {
                turn: 3,
                usage: TokenUsage::default(),
            },
        ];

        assert_eq!(get_last_turn(&items), 3);
    }

    #[test]
    fn test_get_total_usage() {
        let items = vec![
            RolloutItem::TurnContext {
                turn: 1,
                usage: TokenUsage {
                    input_tokens: 100,
                    output_tokens: 50,
                    ..Default::default()
                },
            },
            RolloutItem::TurnContext {
                turn: 2,
                usage: TokenUsage {
                    input_tokens: 200,
                    output_tokens: 100,
                    ..Default::default()
                },
            },
        ];

        let usage = get_total_usage(&items);
        assert_eq!(usage.input_tokens, 300);
        assert_eq!(usage.output_tokens, 150);
    }

    #[test]
    fn test_session_meta_fork_fields() {
        let meta = SessionMetaLine::new(
            "thread_123",
            "claude-3",
            "anthropic",
            PathBuf::from("/home/user/project"),
            "workspace-write",
        );

        assert!(meta.parent_id.is_none());
        assert!(meta.forked_from_message.is_none());
        assert_eq!(meta.fork_count, 0);
    }

    #[test]
    fn test_session_meta_forked() {
        let parent_id = SessionId::new();
        let message_id = MessageId::new();

        let meta = SessionMetaLine::new_forked(
            "thread_456",
            "claude-3",
            "anthropic",
            PathBuf::from("/home/user/project"),
            "workspace-write",
            parent_id.clone(),
            Some(message_id.clone()),
        );

        assert_eq!(meta.parent_id, Some(parent_id));
        assert_eq!(meta.forked_from_message, Some(message_id));
        assert_eq!(meta.fork_count, 0);
    }

    #[test]
    fn test_session_forked_item_serialization() {
        let item = RolloutItem::SessionForked {
            child_session_id: SessionId::new(),
            forked_from_message: Some(MessageId::new()),
            message_count: 5,
            timestamp: Utc::now(),
        };

        let json = serde_json::to_string(&item).unwrap();
        assert!(json.contains("\"type\":\"session_forked\""));

        let parsed: RolloutItem = serde_json::from_str(&json).unwrap();
        if let RolloutItem::SessionForked { message_count, .. } = parsed {
            assert_eq!(message_count, 5);
        } else {
            panic!("Wrong variant");
        }
    }
}
