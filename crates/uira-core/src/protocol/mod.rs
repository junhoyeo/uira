//! Uira Protocol - Shared types, events, and protocol definitions
//!
//! This crate defines the fundamental types used across the Uira AI harness:
//! - Message types for model communication
//! - Event types for streaming and JSONL output
//! - Tool call/response types
//! - Common error types

mod events;
mod messages;
mod primitives;
mod tools;
mod types;

pub use events::*;
pub use messages::*;
pub use primitives::*;
pub use tools::*;
pub use types::*;
