//! Message types for model communication

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Role of a message participant
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    System,
    User,
    Assistant,
    Tool,
}

/// A message in the conversation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: Role,
    pub content: MessageContent,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
}

impl Message {
    pub fn system(content: impl Into<String>) -> Self {
        Self {
            role: Role::System,
            content: MessageContent::Text(content.into()),
            name: None,
            tool_call_id: None,
        }
    }

    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: Role::User,
            content: MessageContent::Text(content.into()),
            name: None,
            tool_call_id: None,
        }
    }

    pub fn user_prompt(prompt: impl AsRef<str>) -> Self {
        Self {
            role: Role::User,
            content: MessageContent::from_prompt(prompt.as_ref()),
            name: None,
            tool_call_id: None,
        }
    }

    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            role: Role::Assistant,
            content: MessageContent::Text(content.into()),
            name: None,
            tool_call_id: None,
        }
    }

    pub fn assistant_with_tool_calls(tool_calls: Vec<ToolCall>) -> Self {
        Self {
            role: Role::Assistant,
            content: MessageContent::ToolCalls(tool_calls),
            name: None,
            tool_call_id: None,
        }
    }

    pub fn tool_result(tool_call_id: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            role: Role::Tool,
            content: MessageContent::Text(content.into()),
            name: None,
            tool_call_id: Some(tool_call_id.into()),
        }
    }

    pub fn with_blocks(role: Role, blocks: Vec<ContentBlock>) -> Self {
        Self {
            role,
            content: MessageContent::Blocks(blocks),
            name: None,
            tool_call_id: None,
        }
    }

    /// Estimate token count for this message (~4 chars per token)
    pub fn estimate_tokens(&self) -> usize {
        let content_len = match &self.content {
            MessageContent::Text(s) => s.len(),
            MessageContent::Blocks(blocks) => blocks.iter().map(|b| b.estimate_chars()).sum(),
            MessageContent::ToolCalls(calls) => calls.iter().map(|c| c.estimate_chars()).sum(),
        };
        content_len.div_ceil(4)
    }
}

/// Content of a message
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum MessageContent {
    Text(String),
    Blocks(Vec<ContentBlock>),
    ToolCalls(Vec<ToolCall>),
}

impl MessageContent {
    pub fn as_text(&self) -> Option<&str> {
        match self {
            Self::Text(s) => Some(s),
            _ => None,
        }
    }

    pub fn from_prompt(prompt: &str) -> Self {
        let references = parse_prompt_image_references(prompt);

        if references.is_empty() {
            return Self::Text(prompt.to_string());
        }

        let mut blocks = Vec::new();
        let mut cursor = 0;

        for reference in references {
            if reference.start > cursor {
                let text = &prompt[cursor..reference.start];
                if !text.is_empty() {
                    blocks.push(ContentBlock::Text {
                        text: text.to_string(),
                    });
                }
            }

            blocks.push(ContentBlock::Image {
                source: ImageSource::FilePath {
                    path: reference.path,
                },
            });
            cursor = reference.end;
        }

        if cursor < prompt.len() {
            let text = &prompt[cursor..];
            if !text.is_empty() {
                blocks.push(ContentBlock::Text {
                    text: text.to_string(),
                });
            }
        }

        if blocks.is_empty() {
            Self::Text(prompt.to_string())
        } else {
            Self::Blocks(blocks)
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PromptImageReference {
    start: usize,
    end: usize,
    path: String,
}

fn parse_prompt_image_references(prompt: &str) -> Vec<PromptImageReference> {
    let mut refs = Vec::new();
    let mut cursor = 0;

    while let Some(reference) = find_next_prompt_image_reference(prompt, cursor) {
        cursor = reference.end;
        refs.push(reference);
    }

    refs
}

fn find_next_prompt_image_reference(prompt: &str, from: usize) -> Option<PromptImageReference> {
    let markdown = find_markdown_image_reference(prompt, from);
    let bracket = find_bracket_image_reference(prompt, from);

    match (markdown, bracket) {
        (Some(m), Some(b)) => {
            if m.start <= b.start {
                Some(m)
            } else {
                Some(b)
            }
        }
        (Some(m), None) => Some(m),
        (None, Some(b)) => Some(b),
        (None, None) => None,
    }
}

fn find_markdown_image_reference(prompt: &str, from: usize) -> Option<PromptImageReference> {
    let mut cursor = from;

    while let Some(relative_start) = prompt[cursor..].find("![") {
        let start = cursor + relative_start;
        let after_marker = start + 2;

        let Some(relative_mid) = prompt[after_marker..].find("](") else {
            cursor = after_marker;
            continue;
        };

        let path_start = after_marker + relative_mid + 2;
        let Some(relative_end) = prompt[path_start..].find(')') else {
            cursor = path_start;
            continue;
        };

        let path_end = path_start + relative_end;
        let end = path_end + 1;
        let raw_path = &prompt[path_start..path_end];

        if let Some(path) = normalize_image_path(raw_path) {
            return Some(PromptImageReference { start, end, path });
        }

        cursor = end;
    }

    None
}

fn find_bracket_image_reference(prompt: &str, from: usize) -> Option<PromptImageReference> {
    let marker = "[image:";
    let mut cursor = from;

    while let Some(relative_start) = prompt[cursor..].find(marker) {
        let start = cursor + relative_start;
        let path_start = start + marker.len();

        let Some(relative_end) = prompt[path_start..].find(']') else {
            cursor = path_start;
            continue;
        };

        let path_end = path_start + relative_end;
        let end = path_end + 1;
        let raw_path = &prompt[path_start..path_end];

        if let Some(path) = normalize_image_path(raw_path) {
            return Some(PromptImageReference { start, end, path });
        }

        cursor = end;
    }

    None
}

fn normalize_image_path(path: &str) -> Option<String> {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return None;
    }

    let unquoted = trimmed
        .strip_prefix('"')
        .and_then(|v| v.strip_suffix('"'))
        .or_else(|| {
            trimmed
                .strip_prefix('\'')
                .and_then(|v| v.strip_suffix('\''))
        })
        .unwrap_or(trimmed)
        .trim();

    if unquoted.is_empty() {
        None
    } else {
        Some(unquoted.to_string())
    }
}

/// A content block within a message
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentBlock {
    Text {
        text: String,
    },
    Image {
        source: ImageSource,
    },
    ToolUse {
        id: String,
        name: String,
        input: Value,
    },
    ToolResult {
        tool_use_id: String,
        content: String,
        #[serde(default)]
        is_error: bool,
    },
    Thinking {
        thinking: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        signature: Option<String>,
    },
}

impl ContentBlock {
    pub fn text(s: impl Into<String>) -> Self {
        Self::Text { text: s.into() }
    }

    pub fn tool_use(id: impl Into<String>, name: impl Into<String>, input: Value) -> Self {
        Self::ToolUse {
            id: id.into(),
            name: name.into(),
            input,
        }
    }

    pub fn tool_result(tool_use_id: impl Into<String>, content: impl Into<String>) -> Self {
        Self::ToolResult {
            tool_use_id: tool_use_id.into(),
            content: content.into(),
            is_error: false,
        }
    }

    pub fn tool_error(tool_use_id: impl Into<String>, error: impl Into<String>) -> Self {
        Self::ToolResult {
            tool_use_id: tool_use_id.into(),
            content: error.into(),
            is_error: true,
        }
    }

    fn estimate_chars(&self) -> usize {
        match self {
            Self::Text { text } => text.len(),
            Self::Image { .. } => 4000,
            Self::ToolUse { name, input, .. } => name.len() + input.to_string().len(),
            Self::ToolResult { content, .. } => content.len(),
            Self::Thinking { thinking, .. } => thinking.len(),
        }
    }
}

/// Image source for multimodal messages
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ImageSource {
    Base64 { media_type: String, data: String },
    Url { url: String },
    FilePath { path: String },
}

/// A tool call from the model
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub input: Value,
}

impl ToolCall {
    pub fn new(id: impl Into<String>, name: impl Into<String>, input: Value) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            input,
        }
    }

    fn estimate_chars(&self) -> usize {
        self.name.len() + self.input.to_string().len()
    }
}

/// Response from the model
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelResponse {
    pub id: String,
    pub model: String,
    pub content: Vec<ContentBlock>,
    pub stop_reason: Option<crate::StopReason>,
    pub usage: crate::TokenUsage,
}

impl ModelResponse {
    /// Extract text content from the response
    pub fn text(&self) -> String {
        self.content
            .iter()
            .filter_map(|block| {
                if let ContentBlock::Text { text } = block {
                    Some(text.as_str())
                } else {
                    None
                }
            })
            .collect::<Vec<_>>()
            .join("")
    }

    /// Extract tool calls from the response
    pub fn tool_calls(&self) -> Vec<ToolCall> {
        self.content
            .iter()
            .filter_map(|block| {
                if let ContentBlock::ToolUse { id, name, input } = block {
                    Some(ToolCall {
                        id: id.clone(),
                        name: name.clone(),
                        input: input.clone(),
                    })
                } else {
                    None
                }
            })
            .collect()
    }

    /// Check if the response contains tool calls
    pub fn has_tool_calls(&self) -> bool {
        self.content
            .iter()
            .any(|block| matches!(block, ContentBlock::ToolUse { .. }))
    }
}

/// Streaming chunk from the model
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum StreamChunk {
    MessageStart {
        message: StreamMessageStart,
    },
    ContentBlockStart {
        index: usize,
        content_block: ContentBlock,
    },
    ContentBlockDelta {
        index: usize,
        delta: ContentDelta,
    },
    ContentBlockStop {
        index: usize,
    },
    MessageDelta {
        delta: MessageDelta,
        usage: Option<crate::TokenUsage>,
    },
    MessageStop,
    Ping,
    Error {
        error: StreamError,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamMessageStart {
    pub id: String,
    pub model: String,
    #[serde(default)]
    pub usage: crate::TokenUsage,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentDelta {
    TextDelta { text: String },
    InputJsonDelta { partial_json: String },
    ThinkingDelta { thinking: String },
    SignatureDelta { signature: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageDelta {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_reason: Option<crate::StopReason>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamError {
    pub r#type: String,
    pub message: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_message_constructors() {
        let system = Message::system("You are a helpful assistant");
        assert_eq!(system.role, Role::System);

        let user = Message::user("Hello");
        assert_eq!(user.role, Role::User);

        let assistant = Message::assistant("Hi there!");
        assert_eq!(assistant.role, Role::Assistant);
    }

    #[test]
    fn test_content_block_serialization() {
        let block = ContentBlock::text("Hello");
        let json = serde_json::to_string(&block).unwrap();
        assert!(json.contains("\"type\":\"text\""));
    }

    #[test]
    fn test_tool_call() {
        let call = ToolCall::new(
            "tc_123",
            "read_file",
            serde_json::json!({"path": "/tmp/test"}),
        );
        assert_eq!(call.name, "read_file");
    }

    #[test]
    fn test_model_response_text() {
        let response = ModelResponse {
            id: "msg_123".to_string(),
            model: "claude-3-opus".to_string(),
            content: vec![ContentBlock::text("Hello, "), ContentBlock::text("world!")],
            stop_reason: Some(crate::StopReason::EndTurn),
            usage: Default::default(),
        };
        assert_eq!(response.text(), "Hello, world!");
    }

    #[test]
    fn test_estimate_tokens() {
        let msg = Message::user("Hello world"); // 11 chars
        let tokens = msg.estimate_tokens();
        assert!(tokens >= 2 && tokens <= 4); // ~11/4 = 2-3
    }

    #[test]
    fn test_user_prompt_without_images() {
        let msg = Message::user_prompt("Describe this bug");
        assert!(matches!(msg.content, MessageContent::Text(_)));
    }

    #[test]
    fn test_user_prompt_with_markdown_image() {
        let msg = Message::user_prompt("Please review ![mockup](./mockup.png) now");

        match msg.content {
            MessageContent::Blocks(blocks) => {
                assert_eq!(blocks.len(), 3);
                assert!(matches!(blocks[0], ContentBlock::Text { .. }));
                assert!(matches!(
                    blocks[1],
                    ContentBlock::Image {
                        source: ImageSource::FilePath { .. }
                    }
                ));
                assert!(matches!(blocks[2], ContentBlock::Text { .. }));
            }
            _ => panic!("expected blocks"),
        }
    }

    #[test]
    fn test_user_prompt_with_bracket_image() {
        let msg = Message::user_prompt("[image: ./screenshots/error.png]");

        match msg.content {
            MessageContent::Blocks(blocks) => {
                assert_eq!(blocks.len(), 1);
                match &blocks[0] {
                    ContentBlock::Image {
                        source: ImageSource::FilePath { path },
                    } => assert_eq!(path, "./screenshots/error.png"),
                    _ => panic!("expected file path image"),
                }
            }
            _ => panic!("expected blocks"),
        }
    }
}
