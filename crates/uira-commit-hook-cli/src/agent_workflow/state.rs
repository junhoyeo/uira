use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::Path;

use super::WorkflowTask;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowState {
    pub active: bool,
    pub task: WorkflowTask,
    pub iteration: u32,
    pub max_iterations: u32,
    pub session_id: String,
    pub started_at: DateTime<Utc>,
    pub last_activity_at: DateTime<Utc>,
    pub files_changed: Vec<String>,
    pub session_path: Option<String>,
    pub git_state_before: Option<Vec<String>>,
}

impl WorkflowState {
    pub fn new(task: WorkflowTask, session_id: String, max_iterations: u32) -> Self {
        let now = Utc::now();
        Self {
            active: true,
            task,
            iteration: 0,
            max_iterations,
            session_id,
            started_at: now,
            last_activity_at: now,
            files_changed: vec![],
            session_path: None,
            git_state_before: None,
        }
    }

    pub fn read(task: WorkflowTask) -> Option<Self> {
        let path = task.state_file();
        let content = std::fs::read_to_string(&path).ok()?;
        serde_json::from_str(&content).ok()
    }

    pub fn write(&self) -> anyhow::Result<()> {
        let path = self.task.state_file();

        if let Some(parent) = Path::new(&path).parent() {
            std::fs::create_dir_all(parent)?;
        }

        let content = serde_json::to_string_pretty(self)?;
        std::fs::write(&path, content)?;
        Ok(())
    }

    pub fn clear(task: WorkflowTask) -> anyhow::Result<()> {
        let path = task.state_file();
        match std::fs::remove_file(&path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e.into()),
        }
    }

    pub fn increment(&mut self) {
        self.iteration += 1;
        self.last_activity_at = Utc::now();
    }
}
