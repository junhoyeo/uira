//! Astrape SDK - Rust bindings for Claude Agent SDK
//!
//! This crate provides Rust types and bindings to interact with
//! @anthropic-ai/claude-agent-sdk from Rust code.
//!
//! # Architecture
//!
//! The SDK uses a TypeScript bridge subprocess to call the SDK from Rust.
//! This allows Astrape to leverage the existing SDK without reimplementing it.
//!
//! # Example
//!
//! ```ignore
//! use astrape_sdk::{SdkBridge, QueryParams};
//!
//! let mut bridge = SdkBridge::new()?;
//! assert!(bridge.ping()?);
//!
//! let params = QueryParams {
//!     prompt: "Hello, Claude!".to_string(),
//!     options: None,
//! };
//! let mut rx = bridge.query(params)?;
//! while let Some(msg) = rx.recv().await {
//!     println!("{:?}", msg);
//! }
//! ```

mod agent;
mod bridge;
mod config;
mod error;
mod mcp;
mod session;
mod types;

pub use agent::*;
pub use bridge::*;
pub use config::*;
pub use error::*;
pub use mcp::*;
pub use session::*;
pub use types::*;
