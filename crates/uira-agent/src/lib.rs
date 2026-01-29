//! Uira Agent - Core agent loop
//!
//! This crate provides the main agent loop that orchestrates:
//! - Model communication
//! - Tool execution
//! - Approval flow
//! - Context management
//! - Event streaming
//! - Session persistence (JSONL rollout)

mod agent;
mod config;
mod control;
mod error;
pub mod events;
pub mod rollout;
mod session;
pub mod streaming;
mod turn;

pub use agent::Agent;
pub use config::AgentConfig;
pub use control::AgentControl;
pub use error::AgentLoopError;
pub use events::{EventSender, EventStream};
pub use rollout::{EventWrapper, RolloutItem, RolloutRecorder, SessionMetaLine};
pub use session::Session;
pub use streaming::StreamController;
pub use turn::{TurnContext, TurnState};
