//! Turn context and state

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::oneshot;
use uira_types::{ReviewDecision, SandboxPreference};
use uira_providers::ModelClient;
use uira_sandbox::SandboxPolicy;

/// Per-turn immutable context
pub struct TurnContext {
    /// Model client for this turn
    pub client: Arc<dyn ModelClient>,

    /// Sandbox policy for this turn
    pub sandbox_policy: SandboxPolicy,

    /// Sandbox preference for tools
    pub sandbox_preference: SandboxPreference,

    /// Working directory
    pub cwd: PathBuf,

    /// Turn number
    pub turn_number: usize,
}

impl TurnContext {
    pub fn new(
        client: Arc<dyn ModelClient>,
        sandbox_policy: SandboxPolicy,
        cwd: PathBuf,
        turn_number: usize,
    ) -> Self {
        Self {
            client,
            sandbox_policy,
            sandbox_preference: SandboxPreference::default(),
            cwd,
            turn_number,
        }
    }
}

/// Mutable state for a turn
pub struct TurnState {
    /// Pending approval requests
    pub pending_approvals: HashMap<String, oneshot::Sender<ReviewDecision>>,

    /// Pending user input requests
    pub pending_user_input: HashMap<String, oneshot::Sender<String>>,

    /// Whether the turn has been cancelled
    pub cancelled: bool,
}

impl TurnState {
    pub fn new() -> Self {
        Self {
            pending_approvals: HashMap::new(),
            pending_user_input: HashMap::new(),
            cancelled: false,
        }
    }

    /// Request approval for an action
    pub fn request_approval(&mut self, id: String) -> oneshot::Receiver<ReviewDecision> {
        let (tx, rx) = oneshot::channel();
        self.pending_approvals.insert(id, tx);
        rx
    }

    /// Resolve a pending approval
    pub fn resolve_approval(&mut self, id: &str, decision: ReviewDecision) -> bool {
        if let Some(tx) = self.pending_approvals.remove(id) {
            tx.send(decision).is_ok()
        } else {
            false
        }
    }

    /// Cancel the turn
    pub fn cancel(&mut self) {
        self.cancelled = true;
        for (_, tx) in self.pending_approvals.drain() {
            let _ = tx.send(ReviewDecision::Deny {
                reason: Some("cancelled".to_string()),
            });
        }
        self.pending_user_input.clear();
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled
    }
}

impl Default for TurnState {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_turn_state_approval() {
        let mut state = TurnState::new();
        let rx = state.request_approval("req_1".to_string());

        assert!(state.resolve_approval("req_1", ReviewDecision::Approve));
        assert_eq!(rx.blocking_recv().unwrap().is_approved(), true);
    }

    #[test]
    fn test_turn_state_cancel() {
        let mut state = TurnState::new();
        let rx = state.request_approval("req_1".to_string());

        state.cancel();
        assert!(state.is_cancelled());

        let decision = rx.blocking_recv().unwrap();
        assert!(decision.is_denied());
    }
}
