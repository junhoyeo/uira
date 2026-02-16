use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Type of messaging channel
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChannelType {
    Telegram,
    Slack,
}

impl std::fmt::Display for ChannelType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ChannelType::Telegram => write!(f, "telegram"),
            ChannelType::Slack => write!(f, "slack"),
        }
    }
}

/// An inbound message from a channel
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelMessage {
    pub sender: String,
    pub content: String,
    pub channel_type: ChannelType,
    pub channel_id: String,
    pub timestamp: DateTime<Utc>,
    #[serde(default)]
    pub metadata: HashMap<String, String>,
}

/// An outbound response to send through a channel
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelResponse {
    pub content: String,
    pub recipient: String,
}

/// Capabilities of a channel
#[derive(Debug, Clone)]
pub struct ChannelCapabilities {
    pub max_message_length: usize,
    pub supports_markdown: bool,
    pub supports_streaming: bool,
}

/// Floors a string to a valid UTF-8 character boundary.
///
/// If `max_len` is within the string, returns the largest valid boundary ≤ `max_len`.
/// Otherwise, returns the full string length.
pub fn floor_char_boundary(text: &str, max_len: usize) -> usize {
    if max_len >= text.len() {
        return text.len();
    }

    let mut i = max_len;
    while i > 0 && !text.is_char_boundary(i) {
        i -= 1;
    }
    i
}
