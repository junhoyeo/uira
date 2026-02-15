//! Chat display widget

/// Message for display
#[derive(Debug, Clone)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
    pub tool_output: Option<ToolOutputState>,
    #[allow(dead_code)]
    pub agent_name: Option<String>,
    #[allow(dead_code)]
    pub session_id: Option<String>,
    #[allow(dead_code)]
    pub message_id: Option<String>,
    #[allow(dead_code)]
    pub turn_number: Option<usize>,
    #[allow(dead_code)]
    pub timestamp: Option<u64>,
}

impl ChatMessage {
    pub fn new(role: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            role: role.into(),
            content: content.into(),
            tool_output: None,
            agent_name: None,
            session_id: None,
            message_id: None,
            turn_number: None,
            timestamp: None,
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
            agent_name: None,
            session_id: None,
            message_id: None,
            turn_number: None,
            timestamp: None,
        }
    }

    pub fn with_agent(mut self, agent_name: Option<String>) -> Self {
        self.agent_name = agent_name;
        self
    }
}

#[derive(Debug, Clone)]
pub struct ToolOutputState {
    pub tool_name: String,
    pub summary: String,
    pub collapsed: bool,
}
