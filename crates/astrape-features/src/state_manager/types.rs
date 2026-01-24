use std::path::PathBuf;

use chrono::{DateTime, Utc};

use crate::astrape_state::AstrapeState;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionLifecycleEvent {
    SessionStart,
    SessionEnd,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SessionStateSnapshot {
    pub event: SessionLifecycleEvent,
    pub timestamp: DateTime<Utc>,
    pub directory: PathBuf,
    pub active_plan: Option<String>,
    pub state: Option<AstrapeState>,
}
