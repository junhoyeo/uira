use tracing::{span, Level, Span};

pub struct SessionSpan {
    span: Span,
}

impl SessionSpan {
    pub fn new(session_id: &str, model: &str) -> Self {
        let span = span!(
            Level::INFO,
            "session",
            session_id = %session_id,
            model = %model,
        );
        Self { span }
    }

    pub fn enter(&self) -> tracing::span::Entered<'_> {
        self.span.enter()
    }

    pub fn span(&self) -> &Span {
        &self.span
    }
}

pub struct TurnSpan {
    span: Span,
}

impl TurnSpan {
    pub fn new(turn_number: usize) -> Self {
        let span = span!(
            Level::INFO,
            "turn",
            turn = %turn_number,
        );
        Self { span }
    }

    pub fn enter(&self) -> tracing::span::Entered<'_> {
        self.span.enter()
    }

    pub fn record_tokens(&self, input: u64, output: u64) {
        self.span.record("input_tokens", input);
        self.span.record("output_tokens", output);
    }
}

pub struct ToolSpan {
    span: Span,
}

impl ToolSpan {
    pub fn new(tool_name: &str, tool_call_id: &str) -> Self {
        let span = span!(
            Level::DEBUG,
            "tool",
            tool = %tool_name,
            call_id = %tool_call_id,
        );
        Self { span }
    }

    pub fn enter(&self) -> tracing::span::Entered<'_> {
        self.span.enter()
    }

    pub fn record_duration(&self, duration_ms: u64) {
        self.span.record("duration_ms", duration_ms);
    }

    pub fn record_error(&self, error: &str) {
        self.span.record("error", error);
    }
}

pub struct AgentSpan {
    span: Span,
}

impl AgentSpan {
    pub fn new(agent_type: &str, task_id: &str) -> Self {
        let span = span!(
            Level::INFO,
            "agent",
            agent_type = %agent_type,
            task_id = %task_id,
        );
        Self { span }
    }

    pub fn enter(&self) -> tracing::span::Entered<'_> {
        self.span.enter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_span() {
        let span = SessionSpan::new("ses_123", "claude-sonnet");
        let _guard = span.enter();
    }

    #[test]
    fn test_tool_span() {
        let span = ToolSpan::new("bash", "tc_456");
        let _guard = span.enter();
        span.record_duration(150);
    }
}
