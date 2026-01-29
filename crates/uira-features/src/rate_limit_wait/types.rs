use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimitStatus {
    pub is_limited: bool,
    pub last_checked_at: DateTime<Utc>,
    pub five_hour_resets_at: Option<DateTime<Utc>>,
    pub weekly_resets_at: Option<DateTime<Utc>>,
    pub next_reset_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockedPane {
    pub id: String,
    pub session: String,
    pub window_index: usize,
    pub first_detected_at: DateTime<Utc>,
    pub resume_attempted: bool,
    pub resume_successful: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DaemonState {
    pub is_running: bool,
    pub pid: Option<u32>,
    pub started_at: Option<DateTime<Utc>>,
    pub last_poll_at: Option<DateTime<Utc>>,
    pub rate_limit_status: Option<RateLimitStatus>,
    pub blocked_panes: Vec<BlockedPane>,
    pub resumed_pane_ids: Vec<String>,
    pub total_resume_attempts: u32,
    pub successful_resumes: u32,
    pub error_count: u32,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone)]
pub struct DaemonConfig {
    pub poll_interval_ms: u64,
    pub pane_lines_to_capture: usize,
    pub verbose: bool,
    pub state_file_path: PathBuf,
    pub pid_file_path: PathBuf,
    pub log_file_path: PathBuf,
}

impl Default for DaemonConfig {
    fn default() -> Self {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
        let state_dir = PathBuf::from(home).join(".uira/state");

        Self {
            poll_interval_ms: 30000, // 30 seconds
            pane_lines_to_capture: 100,
            verbose: false,
            state_file_path: state_dir.join("rate-limit-daemon.json"),
            pid_file_path: state_dir.join("rate-limit-daemon.pid"),
            log_file_path: state_dir.join("rate-limit-daemon.log"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TmuxPane {
    pub id: String,
    pub session: String,
    pub window_index: usize,
    pub active: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum DaemonResponse {
    Started { pid: u32, message: String },
    Stopped { message: String },
    Status { state: DaemonState },
    Error { message: String },
    BlockedPanesDetected { panes: Vec<BlockedPane> },
}
