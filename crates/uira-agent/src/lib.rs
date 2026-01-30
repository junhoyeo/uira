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
pub mod approval;
mod config;
mod control;
mod error;
pub mod events;
mod executor;
pub mod goals;
pub mod ralph;
pub mod rollout;
mod session;
pub mod streaming;
mod turn;

use std::sync::Arc;
use uira_providers::ModelClient;

pub use agent::Agent;
pub use approval::{
    approval_channel, ApprovalError, ApprovalPending, ApprovalReceiver, ApprovalSender,
};
pub use config::AgentConfig;
pub use control::AgentControl;
pub use error::AgentLoopError;
pub use events::{EventSender, EventStream};
pub use executor::{ExecutorConfig, RecursiveAgentExecutor};
pub use goals::GoalVerifier;
pub use ralph::{RalphConfig, RalphController, RalphDecision};
pub use rollout::{EventWrapper, RolloutItem, RolloutRecorder, SessionMetaLine};
pub use session::Session;
pub use streaming::{StreamController, StreamOutput};
pub use turn::{TurnContext, TurnState};

pub enum AgentCommand {
    SwitchClient(Arc<dyn ModelClient>),
}

/// Sender for agent commands
pub type CommandSender = tokio::sync::mpsc::Sender<AgentCommand>;
/// Receiver for agent commands
pub type CommandReceiver = tokio::sync::mpsc::Receiver<AgentCommand>;
