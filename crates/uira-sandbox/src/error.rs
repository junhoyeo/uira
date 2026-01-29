//! Sandbox error types

use thiserror::Error;

#[derive(Debug, Error)]
pub enum SandboxError {
    #[error("sandbox not available on this platform")]
    NotAvailable,

    #[error("sandbox setup failed: {0}")]
    SetupFailed(String),

    #[error("execution denied: {reason}")]
    ExecutionDenied { reason: String },

    #[error("policy violation: {0}")]
    PolicyViolation(String),

    #[error("command not allowed: {command}")]
    CommandNotAllowed { command: String },

    #[error("path access denied: {path}")]
    PathAccessDenied { path: String },

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}
