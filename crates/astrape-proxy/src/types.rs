//! Anthropic API wire types.
//!
//! Claude Code speaks (a subset of) Anthropic's `/v1/messages` API.
//! This file ports the Python/Pydantic models used by `claude-code-proxy`.
//!
//! Notes:
//! - Incoming requests can use shorthand strings for `system` and `message.content`.
//!   These are accepted via `#[serde(untagged)]` enums.
//! - Internally we prefer the structured `Vec<ContentBlock>` representation.

use serde::{Deserialize, Serialize};

/// A message role in the Anthropic Messages API.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    User,
    Assistant,
}

/// A message in the Anthropic Messages API.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: Role,

    /// Anthropic allows either a string or an array of content blocks.
    pub content: Content,
}

/// Either a string shorthand or a full content block list.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Content {
    String(String),
    Blocks(Vec<ContentBlock>),
}

impl Content {
    /// Convert this content into a `Vec<ContentBlock>`.
    pub fn into_blocks(self) -> Vec<ContentBlock> {
        match self {
            Content::String(s) => vec![ContentBlock::Text { text: s }],
            Content::Blocks(v) => v,
        }
    }

    /// Borrowed conversion to a block list.
    pub fn as_blocks(&self) -> Vec<ContentBlock> {
        match self {
            Content::String(s) => vec![ContentBlock::Text { text: s.clone() }],
            Content::Blocks(v) => v.clone(),
        }
    }

    /// Lossy plain-text representation.
    pub fn to_plaintext(&self) -> String {
        self.as_blocks()
            .into_iter()
            .map(|b| b.to_plaintext())
            .collect()
    }
}

/// System prompt input.
///
/// The Anthropic API accepts either a plain string, or an array of typed system
/// content objects.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum SystemContent {
    String(String),
    Blocks(Vec<SystemBlock>),
}

impl SystemContent {
    /// Convert to a plain string (joining blocks in order).
    pub fn to_plaintext(&self) -> String {
        match self {
            SystemContent::String(s) => s.clone(),
            SystemContent::Blocks(v) => v.iter().map(|b| b.text.clone()).collect(),
        }
    }
}

/// A system content block.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemBlock {
    #[serde(rename = "type")]
    pub kind: SystemBlockType,
    pub text: String,
}

/// Type of system content block.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SystemBlockType {
    Text,
}

/// A content block within `messages[].content`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentBlock {
    /// Plain text.
    Text { text: String },

    /// Image input.
    Image { source: ImageSource },

    /// A tool invocation requested by the model.
    ToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
    },

    /// A tool invocation result supplied by the client.
    ToolResult {
        tool_use_id: String,
        #[serde(default)]
        content: ToolResultContent,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        is_error: Option<bool>,
    },
}

impl ContentBlock {
    /// Lossy plain-text representation (images/tool calls are stringified).
    pub fn to_plaintext(&self) -> String {
        match self {
            ContentBlock::Text { text } => text.clone(),
            ContentBlock::Image { .. } => "[image]".to_string(),
            ContentBlock::ToolUse { name, .. } => format!("[tool_use:{}]", name),
            ContentBlock::ToolResult { content, .. } => content.to_plaintext(),
        }
    }
}

/// Image content source.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageSource {
    #[serde(rename = "type")]
    pub kind: ImageSourceType,
    pub media_type: String,
    pub data: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImageSourceType {
    Base64,
}

/// Tool result content can be a string shorthand or an array of content blocks.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ToolResultContent {
    String(String),
    Blocks(Vec<ContentBlock>),
}

impl Default for ToolResultContent {
    fn default() -> Self {
        ToolResultContent::String(String::new())
    }
}

impl ToolResultContent {
    /// Lossy plain-text representation.
    pub fn to_plaintext(&self) -> String {
        match self {
            ToolResultContent::String(s) => s.clone(),
            ToolResultContent::Blocks(v) => v.iter().map(|b| b.to_plaintext()).collect(),
        }
    }
}

/// Tool specification (Anthropic schema).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tool {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(rename = "input_schema")]
    pub input_schema: serde_json::Value,
}

/// How the model should choose tools.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ToolChoice {
    Auto,
    Any,
    Tool { name: String },
}

/// Anthropic "thinking" configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThinkingConfig {
    #[serde(rename = "type")]
    pub kind: ThinkingType,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub budget_tokens: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThinkingType {
    Enabled,
    Disabled,
}

/// Request body for `/v1/messages`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessagesRequest {
    pub model: String,
    pub messages: Vec<Message>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub system: Option<SystemContent>,
    pub max_tokens: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stream: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub top_k: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stop_sequences: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<Tool>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<ToolChoice>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thinking: Option<ThinkingConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
}

/// Response body for `/v1/messages`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessagesResponse {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub role: Role,
    pub content: Vec<ContentBlock>,
    pub model: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stop_reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stop_sequence: Option<String>,
    pub usage: Usage,
}

/// Token usage info.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Usage {
    pub input_tokens: u32,
    pub output_tokens: u32,
}

/// Request body for `/v1/messages/count_tokens`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenCountRequest {
    pub model: String,
    pub messages: Vec<Message>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub system: Option<SystemContent>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<Tool>>,
}

/// Response body for `/v1/messages/count_tokens`.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct TokenCountResponse {
    pub input_tokens: u32,
}
