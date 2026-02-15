//! Chat display widget

/// Message for display
#[derive(Debug, Clone)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
    pub tool_output: Option<ToolOutputState>,
}

impl ChatMessage {
    pub fn new(role: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            role: role.into(),
            content: content.into(),
            tool_output: None,
        }
    }

    pub fn tool(
        role: impl Into<String>,
        content: impl Into<String>,
        tool_name: impl Into<String>,
        summary: impl Into<String>,
        collapsed: bool,
    ) -> Self {
        Self {
            role: role.into(),
            content: content.into(),
            tool_output: Some(ToolOutputState {
                tool_name: tool_name.into(),
                summary: summary.into(),
                collapsed,
            }),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ToolOutputState {
    pub tool_name: String,
    pub summary: String,
    pub collapsed: bool,
}
