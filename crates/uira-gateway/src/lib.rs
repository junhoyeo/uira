//! WebSocket gateway for multi-session Uira agent management.

pub mod channel_bridge;
pub mod channels;
pub mod config;
pub mod error;
pub mod protocol;
pub mod server;
pub mod session_manager;
pub mod skills;

pub use channel_bridge::ChannelBridge;
pub use channels::*;
pub use config::SessionConfig;
pub use error::GatewayError;
pub use protocol::{GatewayMessage, GatewayResponse};
pub use server::GatewayServer;
pub use session_manager::{SessionInfo, SessionManager, SessionStatus};
pub use skills::*;
