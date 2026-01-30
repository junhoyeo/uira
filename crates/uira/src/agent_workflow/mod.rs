pub mod completion;
pub mod config;
pub mod git_tracker;
pub mod prompts;
pub mod state;
pub mod verification;
pub mod workflow;

pub use completion::CompletionDetector;
pub use config::{TaskOptions, WorkflowConfig};
pub use git_tracker::GitTracker;
pub use state::WorkflowState;
pub use verification::{VerificationResult, WorkflowVerifier};
pub use workflow::{AgentWorkflow, WorkflowResult};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum WorkflowTask {
    Typos,
    Diagnostics,
    Comments,
}

impl WorkflowTask {
    pub fn name(&self) -> &'static str {
        match self {
            Self::Typos => "typos",
            Self::Diagnostics => "diagnostics",
            Self::Comments => "comments",
        }
    }

    pub fn state_file(&self) -> String {
        format!(".uira/workflow/{}-session.json", self.name())
    }
}
