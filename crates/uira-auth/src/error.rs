use thiserror::Error;

#[derive(Error, Debug)]
pub enum AuthError {
    #[error("OAuth flow failed: {0}")]
    OAuthFailed(String),

    #[error("Token expired")]
    TokenExpired,

    #[error("Invalid credentials")]
    InvalidCredentials,

    #[error("Provider not found: {0}")]
    ProviderNotFound(String),

    #[error("Storage error: {0}")]
    StorageError(String),

    #[error("HTTP error: {0}")]
    HttpError(#[from] reqwest::Error),

    #[error("Serialization error: {0}")]
    SerializationError(#[from] serde_json::Error),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("{0}")]
    Other(String),

    #[error("Device flow timeout")]
    DeviceFlowTimeout,

    #[error("Network error: {0}")]
    NetworkError(String),

    #[error("OAuth error: {0}")]
    OAuthError(String),
}

pub type Result<T> = std::result::Result<T, AuthError>;
