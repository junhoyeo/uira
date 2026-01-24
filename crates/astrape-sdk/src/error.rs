//! Error types for Astrape SDK

use thiserror::Error;

/// SDK error types
#[derive(Error, Debug)]
pub enum SdkError {
    /// Configuration error
    #[error("Configuration error: {0}")]
    Config(String),

    /// Session error
    #[error("Session error: {0}")]
    Session(String),

    /// Agent error
    #[error("Agent error: {0}")]
    Agent(String),

    /// SDK integration error (napi-rs)
    #[error("SDK integration error: {0}")]
    Integration(String),

    /// Serialization error
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    /// IO error
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// Generic error
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
