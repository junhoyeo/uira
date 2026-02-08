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
pub mod event_system;
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
    approval_channel, ApprovalCache, ApprovalError, ApprovalKey, ApprovalPending, ApprovalReceiver,
    ApprovalSender, CacheDecision, CachedApproval,
};
pub use config::AgentConfig;
pub use control::AgentControl;
pub use error::AgentLoopError;
pub use event_system::{create_event_system, EventSystem};
pub use events::{EventSender, EventStream};
pub use executor::{ExecutorConfig, RecursiveAgentExecutor};
pub use goals::GoalVerifier;
pub use ralph::{RalphConfig, RalphController, RalphDecision};
pub use rollout::{EventWrapper, RolloutItem, RolloutRecorder, SessionMetaLine};
pub use session::Session;
pub use streaming::{StreamController, StreamOutput};
pub use turn::{TurnContext, TurnState};

#[derive(Debug, Clone)]
pub struct BranchInfo {
    pub name: String,
    pub parent: Option<String>,
    pub session_id: String,
    pub is_current: bool,
}

#[derive(Debug, Clone)]
pub struct ForkResult {
    pub branch_name: String,
    pub session_id: String,
    pub parent_branch: String,
}

pub enum AgentCommand {
    SwitchClient(Arc<dyn ModelClient>),
    Fork {
        branch_name: Option<String>,
        message_count: Option<usize>,
        response_tx: tokio::sync::oneshot::Sender<Result<ForkResult, String>>,
    },
    SwitchBranch {
        branch_name: String,
        response_tx: tokio::sync::oneshot::Sender<Result<String, String>>,
    },
    ListBranches {
        response_tx: tokio::sync::oneshot::Sender<Result<Vec<BranchInfo>, String>>,
    },
    BranchTree {
        response_tx: tokio::sync::oneshot::Sender<Result<String, String>>,
    },
}

/// Sender for agent commands
pub type CommandSender = tokio::sync::mpsc::Sender<AgentCommand>;
/// Receiver for agent commands
pub type CommandReceiver = tokio::sync::mpsc::Receiver<AgentCommand>;
