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
}
