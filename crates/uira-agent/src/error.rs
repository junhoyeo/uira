//! Agent loop error types

use thiserror::Error;
use uira_protocol::AgentError;

#[derive(Debug, Error)]
pub enum AgentLoopError {
    #[error("agent error: {0}")]
    Agent(#[from] AgentError),

    #[error("provider error: {0}")]
    Provider(#[from] uira_providers::ProviderError),

    #[error("context error: {0}")]
    Context(#[from] uira_context::ContextError),

    #[error("sandbox error: {0}")]
    Sandbox(#[from] uira_sandbox::SandboxError),

    #[error("tool error: {tool} - {message}")]
    Tool { tool: String, message: String },

    #[error("configuration error: {0}")]
    Configuration(String),

    #[error("IO error: {0}")]
    Io(String),

    #[error("cancelled")]
    Cancelled,
}

impl AgentLoopError {
    pub fn is_recoverable(&self) -> bool {
        match self {
            Self::Agent(e) => e.is_recoverable(),
            Self::Tool { .. } => true,
            Self::Cancelled => false,
            _ => false,
        }
    }
}
