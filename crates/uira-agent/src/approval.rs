//! Approval system for tool execution
//!
//! Provides bi-directional approval communication between Agent and TUI.

use tokio::sync::{mpsc, oneshot};
use uira_protocol::ReviewDecision;

/// A pending approval request that the agent is waiting on
#[derive(Debug)]
pub struct ApprovalPending {
    /// Unique request ID
    pub id: String,
    /// Name of the tool requesting approval
    pub tool_name: String,
    /// Tool input parameters
    pub input: serde_json::Value,
    /// Reason for requiring approval
    pub reason: String,
    /// Channel to send the decision back to agent
    pub response_tx: oneshot::Sender<ReviewDecision>,
}

impl ApprovalPending {
    /// Create a new pending approval with a response channel
    pub fn new(
        id: impl Into<String>,
        tool_name: impl Into<String>,
        input: serde_json::Value,
        reason: impl Into<String>,
    ) -> (Self, oneshot::Receiver<ReviewDecision>) {
        let (tx, rx) = oneshot::channel();
        (
            Self {
                id: id.into(),
                tool_name: tool_name.into(),
                input,
                reason: reason.into(),
                response_tx: tx,
            },
            rx,
        )
    }
}

/// Channel pair for approval communication
pub struct ApprovalChannel {
    /// Sender for the agent to send approval requests
    pub sender: mpsc::Sender<ApprovalPending>,
    /// Receiver for the TUI to receive approval requests
    pub receiver: mpsc::Receiver<ApprovalPending>,
}

impl ApprovalChannel {
    /// Create a new approval channel pair
    pub fn new(buffer: usize) -> Self {
        let (sender, receiver) = mpsc::channel(buffer);
        Self { sender, receiver }
    }
}

/// Sender half of the approval channel (held by Agent)
#[derive(Clone)]
pub struct ApprovalSender {
    sender: mpsc::Sender<ApprovalPending>,
}

impl ApprovalSender {
    pub fn new(sender: mpsc::Sender<ApprovalPending>) -> Self {
        Self { sender }
    }

    /// Send an approval request and wait for decision
    pub async fn request_approval(
        &self,
        id: impl Into<String>,
        tool_name: impl Into<String>,
        input: serde_json::Value,
        reason: impl Into<String>,
    ) -> Result<ReviewDecision, ApprovalError> {
        let (pending, rx) = ApprovalPending::new(id, tool_name, input, reason);

        // Send the request
        self.sender
            .send(pending)
            .await
            .map_err(|_| ApprovalError::ChannelClosed)?;

        // Wait for decision
        rx.await.map_err(|_| ApprovalError::ResponseDropped)
    }
}

/// Receiver half of the approval channel (held by TUI)
pub struct ApprovalReceiver {
    receiver: mpsc::Receiver<ApprovalPending>,
}

impl ApprovalReceiver {
    pub fn new(receiver: mpsc::Receiver<ApprovalPending>) -> Self {
        Self { receiver }
    }

    /// Receive the next pending approval
    pub async fn recv(&mut self) -> Option<ApprovalPending> {
        self.receiver.recv().await
    }
}

/// Create an approval channel pair
pub fn approval_channel(buffer: usize) -> (ApprovalSender, ApprovalReceiver) {
    let (tx, rx) = mpsc::channel(buffer);
    (ApprovalSender::new(tx), ApprovalReceiver::new(rx))
}

/// Errors that can occur during approval
#[derive(Debug, thiserror::Error)]
pub enum ApprovalError {
    #[error("approval channel closed")]
    ChannelClosed,

    #[error("approval response dropped")]
    ResponseDropped,

    #[error("approval denied{}", reason.as_ref().map(|r| format!(": {}", r)).unwrap_or_default())]
    Denied { reason: Option<String> },

    #[error("approval timeout")]
    Timeout,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_approval_channel() {
        let (sender, mut receiver) = approval_channel(10);

        // Spawn a task to handle approvals
        let handle = tokio::spawn(async move {
            if let Some(pending) = receiver.recv().await {
                assert_eq!(pending.tool_name, "bash");
                let _ = pending.response_tx.send(ReviewDecision::Approve);
            }
        });

        // Request approval
        let decision = sender
            .request_approval(
                "req_1",
                "bash",
                serde_json::json!({"command": "ls"}),
                "Executes shell command",
            )
            .await
            .unwrap();

        assert!(matches!(decision, ReviewDecision::Approve));
        handle.await.unwrap();
    }

    #[tokio::test]
    async fn test_approval_denied() {
        let (sender, mut receiver) = approval_channel(10);

        let handle = tokio::spawn(async move {
            if let Some(pending) = receiver.recv().await {
                let _ = pending.response_tx.send(ReviewDecision::Deny {
                    reason: Some("Dangerous".to_string()),
                });
            }
        });

        let decision = sender
            .request_approval("req_1", "rm", serde_json::json!({}), "Deletes files")
            .await
            .unwrap();

        assert!(matches!(decision, ReviewDecision::Deny { .. }));
        handle.await.unwrap();
    }
}
