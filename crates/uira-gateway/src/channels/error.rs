use thiserror::Error;

#[derive(Debug, Error)]
pub enum ChannelError {
    #[error("Connection failed: {0}")]
    ConnectionFailed(String),

    #[error("Failed to send message: {0}")]
    SendFailed(String),

    #[error("Authentication error: {0}")]
    AuthError(String),

    #[error("Rate limited: {0}")]
    RateLimited(String),

    #[error("Channel closed")]
    ChannelClosed,

    #[error("Channel error: {0}")]
    Other(String),
}
