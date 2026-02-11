use thiserror::Error;

#[derive(Debug, Error)]
pub enum GatewayError {
    #[error("Session not found: {0}")]
    SessionNotFound(String),

    #[error("Maximum sessions reached (limit: {0})")]
    MaxSessionsReached(usize),

    #[error("Session creation failed: {0}")]
    SessionCreationFailed(String),

    #[error("Session shutdown failed: {0}")]
    SessionShutdownFailed(String),

    #[error("Failed to send message: {0}")]
    SendFailed(String),

    #[error("Gateway server error: {0}")]
    ServerError(String),
}
