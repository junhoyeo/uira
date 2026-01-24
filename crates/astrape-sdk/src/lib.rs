//! Astrape SDK - Rust bindings for Claude Agent SDK
//!
//! This crate provides Rust types and bindings to interact with
//! @anthropic-ai/claude-agent-sdk from Rust code.
//!
//! # Architecture
//!
//! The SDK uses napi-rs to call the TypeScript SDK from Rust.
//! This allows Astrape to leverage the existing SDK without reimplementing it.
//!
//! # Example
//!
//! ```ignore
//! use astrape_sdk::{AstrapeSession, SessionOptions, AgentConfig};
//!
//! let session = AstrapeSession::new(SessionOptions::default()).await?;
//! let result = session.invoke_agent("explore", "find auth logic").await?;
//! ```

mod types;
mod agent;
mod session;
mod config;
mod mcp;
mod error;

pub use types::*;
pub use agent::*;
pub use session::*;
pub use config::*;
pub use mcp::*;
pub use error::*;
