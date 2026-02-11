//! WebSocket gateway for multi-session Uira agent management.

pub mod config;
pub mod error;
pub mod session_manager;

pub use config::SessionConfig;
pub use error::GatewayError;
pub use session_manager::{SessionInfo, SessionManager, SessionStatus};
