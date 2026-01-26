//! Error types for Astrape SDK

use thiserror::Error;

/// SDK error types
#[derive(Error, Debug)]
pub enum SdkError {
    #[error("Configuration error: {0}")]
    Config(String),

    #[error("Session error: {0}")]
    Session(String),

    #[error("Agent error: {0}")]
    Agent(String),

    #[error("SDK integration error: {0}")]
    Integration(String),

    #[error("Serialization error: {0}")]
    Serialization(String),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("{0}")]
    Other(String),
}

/// Result type for SDK operations
pub type SdkResult<T> = Result<T, SdkError>;

impl From<String> for SdkError {
    fn from(s: String) -> Self {
        SdkError::Other(s)
    }
}

impl From<&str> for SdkError {
    fn from(s: &str) -> Self {
        SdkError::Other(s.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display() {
        let err = SdkError::Config("Invalid config".to_string());
        assert_eq!(err.to_string(), "Configuration error: Invalid config");
    }

    #[test]
    fn test_error_from_string() {
        let err: SdkError = "Something went wrong".into();
        assert!(matches!(err, SdkError::Other(_)));
    }
}
