use std::path::{Path, PathBuf};

use chrono::Utc;

use crate::astrape_state::{
    append_session_id, create_astrape_state, get_active_plan_path, get_plan_progress,
    get_plan_summaries, read_astrape_state, write_astrape_state, AstrapeState, PlanProgress,
    PlanSummary,
};
use crate::notepad_wisdom::init_plan_notepad;
use crate::state_manager::types::{SessionLifecycleEvent, SessionStateSnapshot};

pub mod types;

/// Manages session lifecycle and plan state persistence.
///
/// This is the glue layer that combines:
/// - `astrape_state` (plan progress + state persistence)
/// - `notepad_wisdom` (plan-scoped learning persistence)
#[derive(Debug, Clone)]
pub struct StateManager {
    directory: PathBuf,
}

impl StateManager {
    pub fn new(directory: impl Into<PathBuf>) -> Self {
        Self {
            directory: directory.into(),
        }
    }

    pub fn directory(&self) -> &Path {
        &self.directory
    }

    pub fn get_active_plan_path(&self) -> Option<String> {
        get_active_plan_path(&self.directory)
    }

    pub fn read_state(&self) -> Option<AstrapeState> {
        read_astrape_state(&self.directory)
    }

    /// Start a new plan session.
    ///
    /// Creates `.astrape/state.json` (via astrape_state) and initializes the plan notepad.
    pub fn start_plan(&self, plan_path: impl AsRef<Path>, session_id: &str) -> Option<AstrapeState> {
        let plan_path = plan_path.as_ref();
        let state = create_astrape_state(plan_path, session_id);
        if !write_astrape_state(&self.directory, &state) {
            return None;
        }

        // Best-effort notepad initialization.
        let _ = init_plan_notepad(&state.plan_name, &self.directory);

        Some(state)
    }

    /// Resume an existing plan session by appending the session id.
    pub fn resume_plan(&self, session_id: &str) -> Option<AstrapeState> {
        append_session_id(&self.directory, session_id)
    }

    pub fn get_plan_summaries(&self) -> Vec<PlanSummary> {
        get_plan_summaries(&self.directory)
    }

    pub fn get_plan_progress(&self, plan_path: impl AsRef<Path>) -> PlanProgress {
        get_plan_progress(plan_path)
    }

    pub fn snapshot(&self, event: SessionLifecycleEvent) -> SessionStateSnapshot {
        SessionStateSnapshot {
            event,
            timestamp: Utc::now(),
            directory: self.directory.clone(),
            active_plan: self.get_active_plan_path(),
            state: self.read_state(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn start_plan_creates_state_and_notepad() {
        let dir = TempDir::new().unwrap();
        let plans_dir = dir.path().join(".omc").join("plans");
        std::fs::create_dir_all(&plans_dir).unwrap();
        let plan_path = plans_dir.join("demo.md");
        std::fs::write(&plan_path, "- [ ] task\n").unwrap();

        let manager = StateManager::new(dir.path());
        let state = manager.start_plan(&plan_path, "ses_demo").expect("state");

        assert_eq!(state.plan_name, "demo");
        assert_eq!(manager.get_active_plan_path(), Some(state.active_plan.clone()));

        // Notepad is best-effort, but should exist for new plan.
        let notepad_dir = dir.path().join(crate::astrape_state::NOTEPAD_BASE_PATH).join("demo");
        assert!(notepad_dir.exists());
    }

    #[test]
    fn snapshot_captures_state() {
        let dir = TempDir::new().unwrap();
        let manager = StateManager::new(dir.path());
        let snap = manager.snapshot(SessionLifecycleEvent::SessionStart);
        assert_eq!(snap.event, SessionLifecycleEvent::SessionStart);
        assert_eq!(snap.directory, dir.path().to_path_buf());
    }
}
