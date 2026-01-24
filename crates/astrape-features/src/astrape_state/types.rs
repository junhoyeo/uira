use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AstrapeState {
    /// Absolute path to the active plan file
    pub active_plan: String,
    /// ISO timestamp when work started
    pub started_at: String,
    /// Session IDs that have worked on this plan
    pub session_ids: Vec<String>,
    /// Plan name derived from filename
    pub plan_name: String,
    /// Optional metadata
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlanProgress {
    pub total: usize,
    pub completed: usize,
    pub is_complete: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlanSummary {
    pub path: std::path::PathBuf,
    pub name: String,
    pub progress: PlanProgress,
    pub last_modified: DateTime<Utc>,
}
