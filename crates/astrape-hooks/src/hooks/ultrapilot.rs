//! Ultrapilot Coordinator Hook
//!
//! Manages parallel worker spawning and coordination for ultrapilot mode.
//! Decomposes tasks, spawns workers (max 5), tracks progress, and integrates results
//! while managing file ownership to avoid conflicts.

use chrono::{DateTime, Utc};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

/// Configuration options for ultrapilot behavior
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UltrapilotConfig {
    /// Maximum number of parallel workers
    #[serde(default = "default_max_workers")]
    pub max_workers: u32,
    /// Maximum iterations before giving up
    #[serde(default = "default_max_iterations")]
    pub max_iterations: u32,
    /// Timeout per worker in milliseconds
    #[serde(default = "default_worker_timeout")]
    pub worker_timeout: u64,
    /// Model to use for workers (haiku/sonnet/opus)
    #[serde(default = "default_worker_model")]
    pub worker_model: String,
    /// List of shared files that only coordinator can modify
    #[serde(default = "default_shared_files")]
    pub shared_files: Vec<String>,
    /// Whether to enable verbose logging
    #[serde(default)]
    pub verbose: bool,
}

fn default_max_workers() -> u32 {
    5
}

fn default_max_iterations() -> u32 {
    3
}

fn default_worker_timeout() -> u64 {
    300000 // 5 minutes
}

fn default_worker_model() -> String {
    "sonnet".to_string()
}

fn default_shared_files() -> Vec<String> {
    vec![
        "package.json".to_string(),
        "package-lock.json".to_string(),
        "tsconfig.json".to_string(),
        "jest.config.js".to_string(),
        ".gitignore".to_string(),
        "README.md".to_string(),
        "Makefile".to_string(),
        "go.mod".to_string(),
        "go.sum".to_string(),
        "Cargo.toml".to_string(),
        "Cargo.lock".to_string(),
        "pyproject.toml".to_string(),
        "requirements.txt".to_string(),
        "setup.py".to_string(),
    ]
}

impl Default for UltrapilotConfig {
    fn default() -> Self {
        Self {
            max_workers: default_max_workers(),
            max_iterations: default_max_iterations(),
            worker_timeout: default_worker_timeout(),
            worker_model: default_worker_model(),
            shared_files: default_shared_files(),
            verbose: false,
        }
    }
}

/// State of an individual worker
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerState {
    /// Unique worker ID
    pub id: String,
    /// Worker index (0-4)
    pub index: usize,
    /// Task assigned to this worker
    pub task: String,
    /// Files this worker owns (can modify)
    pub owned_files: Vec<String>,
    /// Current status
    pub status: WorkerStatus,
    /// Task agent ID (from Task tool)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task_id: Option<String>,
    /// Start timestamp
    #[serde(skip_serializing_if = "Option::is_none")]
    pub started_at: Option<DateTime<Utc>>,
    /// Completion timestamp
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<DateTime<Utc>>,
    /// Error message if failed
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// Files created by this worker
    pub files_created: Vec<String>,
    /// Files modified by this worker
    pub files_modified: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WorkerStatus {
    Pending,
    Running,
    Complete,
    Failed,
}

/// File ownership mapping to prevent conflicts
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileOwnership {
    /// Files owned by the coordinator (shared files)
    pub coordinator: Vec<String>,
    /// Files owned by each worker (keyed by worker ID)
    pub workers: HashMap<String, Vec<String>>,
    /// Files that have conflicts (multiple workers attempted to modify)
    pub conflicts: Vec<String>,
}

impl Default for FileOwnership {
    fn default() -> Self {
        Self {
            coordinator: Vec::new(),
            workers: HashMap::new(),
            conflicts: Vec::new(),
        }
    }
}

/// Complete ultrapilot state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UltrapilotState {
    /// Whether ultrapilot is currently active
    pub active: bool,
    /// Current iteration number
    pub iteration: u32,
    /// Maximum iterations before giving up
    pub max_iterations: u32,
    /// Original task provided by user
    pub original_task: String,
    /// Decomposed subtasks
    pub subtasks: Vec<String>,
    /// State for each worker
    pub workers: Vec<WorkerState>,
    /// File ownership mapping
    pub ownership: FileOwnership,
    /// Metrics and timestamps
    pub started_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<DateTime<Utc>>,
    pub total_workers_spawned: u32,
    pub successful_workers: u32,
    pub failed_workers: u32,
    /// Session binding
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
}

/// Result from integrating worker outputs
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntegrationResult {
    /// Whether integration was successful
    pub success: bool,
    /// All files created across workers
    pub files_created: Vec<String>,
    /// All files modified across workers
    pub files_modified: Vec<String>,
    /// List of conflicts that need manual resolution
    pub conflicts: Vec<String>,
    /// List of errors encountered
    pub errors: Vec<String>,
    /// Summary of work completed
    pub summary: String,
}

pub struct UltrapilotHook;

impl UltrapilotHook {
    pub fn new() -> Self {
        Self
    }

    /// Get the state file path
    fn get_state_file_path(directory: &str) -> PathBuf {
        Path::new(directory)
            .join(".omc")
            .join("state")
            .join("ultrapilot-state.json")
    }

    /// Ensure the state directory exists
    fn ensure_state_dir(directory: &str) -> std::io::Result<()> {
        let state_dir = Path::new(directory).join(".omc").join("state");
        if !state_dir.exists() {
            fs::create_dir_all(&state_dir)?;
        }
        Ok(())
    }

    /// Read ultrapilot state from disk
    pub fn read_state(directory: &str) -> Option<UltrapilotState> {
        let state_file = Self::get_state_file_path(directory);
        if !state_file.exists() {
            return None;
        }

        let content = fs::read_to_string(&state_file).ok()?;
        serde_json::from_str(&content).ok()
    }

    /// Write ultrapilot state to disk
    pub fn write_state(directory: &str, state: &UltrapilotState) -> bool {
        if Self::ensure_state_dir(directory).is_err() {
            return false;
        }

        let state_file = Self::get_state_file_path(directory);
        let content = match serde_json::to_string_pretty(state) {
            Ok(c) => c,
            Err(_) => return false,
        };

        fs::write(&state_file, content).is_ok()
    }

    /// Clear ultrapilot state
    pub fn clear_state(directory: &str) -> bool {
        let state_file = Self::get_state_file_path(directory);
        if !state_file.exists() {
            return true;
        }
        fs::remove_file(&state_file).is_ok()
    }

    /// Check if ultrapilot is active
    pub fn is_active(directory: &str) -> bool {
        Self::read_state(directory)
            .map(|s| s.active)
            .unwrap_or(false)
    }

    /// Initialize a new ultrapilot session
    pub fn init(
        directory: &str,
        task: &str,
        subtasks: Vec<String>,
        session_id: Option<String>,
        config: Option<UltrapilotConfig>,
    ) -> UltrapilotState {
        let merged_config = config.unwrap_or_default();
        let now = Utc::now();

        let state = UltrapilotState {
            active: true,
            iteration: 1,
            max_iterations: merged_config.max_iterations,
            original_task: task.to_string(),
            subtasks,
            workers: Vec::new(),
            ownership: FileOwnership {
                coordinator: merged_config.shared_files,
                workers: HashMap::new(),
                conflicts: Vec::new(),
            },
            started_at: now,
            completed_at: None,
            total_workers_spawned: 0,
            successful_workers: 0,
            failed_workers: 0,
            session_id,
        };

        Self::write_state(directory, &state);
        state
    }

    /// Decompose a task into parallelizable subtasks
    ///
    /// Uses heuristics to identify independent work units that can be executed in parallel.
    pub fn decompose_task(task: &str, config: &UltrapilotConfig) -> Vec<String> {
        let mut subtasks = Vec::new();

        // Look for explicit lists (numbered or bulleted)
        // Pattern: start of line, optional whitespace, number+dot or bullet, whitespace, content
        let list_item_pattern = Regex::new(r"(?m)^[\s]*(?:\d+\.|[-*+])\s+(.+)$").unwrap();

        for cap in list_item_pattern.captures_iter(task) {
            if let Some(item) = cap.get(1) {
                subtasks.push(item.as_str().trim().to_string());
            }
        }

        // If no explicit list found, look for sentences separated by periods or newlines
        if subtasks.is_empty() {
            let sentences: Vec<String> = task
                .split(&['.', ';', '\n'][..])
                .map(|s| s.trim().to_string())
                .filter(|s| s.len() > 10) // Filter out very short fragments
                .collect();

            subtasks.extend(sentences);
        }

        // If still no subtasks, treat entire task as single unit
        if subtasks.is_empty() {
            subtasks.push(task.to_string());
        }

        // Limit to maxWorkers
        subtasks
            .into_iter()
            .take(config.max_workers as usize)
            .collect()
    }

    /// Add a new worker
    pub fn add_worker(directory: &str, worker: WorkerState) -> bool {
        let mut state = match Self::read_state(directory) {
            Some(s) => s,
            None => return false,
        };

        state.workers.push(worker.clone());
        state.total_workers_spawned += 1;

        // Update ownership
        state
            .ownership
            .workers
            .insert(worker.id.clone(), worker.owned_files.clone());

        Self::write_state(directory, &state)
    }

    /// Update worker state
    pub fn update_worker_state(
        directory: &str,
        worker_id: &str,
        status: Option<WorkerStatus>,
        task_id: Option<String>,
        error: Option<String>,
    ) -> bool {
        let mut state = match Self::read_state(directory) {
            Some(s) => s,
            None => return false,
        };

        let worker = match state.workers.iter_mut().find(|w| w.id == worker_id) {
            Some(w) => w,
            None => return false,
        };

        if let Some(s) = status {
            worker.status = s;
        }
        if let Some(tid) = task_id {
            worker.task_id = Some(tid);
        }
        if let Some(e) = error {
            worker.error = Some(e);
        }

        Self::write_state(directory, &state)
    }

    /// Mark worker as complete
    pub fn complete_worker(
        directory: &str,
        worker_id: &str,
        files_created: Vec<String>,
        files_modified: Vec<String>,
    ) -> bool {
        let mut state = match Self::read_state(directory) {
            Some(s) => s,
            None => return false,
        };

        let worker = match state.workers.iter_mut().find(|w| w.id == worker_id) {
            Some(w) => w,
            None => return false,
        };

        worker.status = WorkerStatus::Complete;
        worker.completed_at = Some(Utc::now());
        worker.files_created = files_created;
        worker.files_modified = files_modified;
        state.successful_workers += 1;

        Self::write_state(directory, &state)
    }

    /// Mark worker as failed
    pub fn fail_worker(directory: &str, worker_id: &str, error: &str) -> bool {
        let mut state = match Self::read_state(directory) {
            Some(s) => s,
            None => return false,
        };

        let worker = match state.workers.iter_mut().find(|w| w.id == worker_id) {
            Some(w) => w,
            None => return false,
        };

        worker.status = WorkerStatus::Failed;
        worker.completed_at = Some(Utc::now());
        worker.error = Some(error.to_string());
        state.failed_workers += 1;

        Self::write_state(directory, &state)
    }

    /// Complete ultrapilot session
    pub fn complete(directory: &str) -> bool {
        let mut state = match Self::read_state(directory) {
            Some(s) => s,
            None => return false,
        };

        state.active = false;
        state.completed_at = Some(Utc::now());

        Self::write_state(directory, &state)
    }

    /// Get completed workers
    pub fn get_completed_workers(directory: &str) -> Vec<WorkerState> {
        Self::read_state(directory)
            .map(|s| {
                s.workers
                    .into_iter()
                    .filter(|w| w.status == WorkerStatus::Complete)
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Get running workers
    pub fn get_running_workers(directory: &str) -> Vec<WorkerState> {
        Self::read_state(directory)
            .map(|s| {
                s.workers
                    .into_iter()
                    .filter(|w| w.status == WorkerStatus::Running)
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Get failed workers
    pub fn get_failed_workers(directory: &str) -> Vec<WorkerState> {
        Self::read_state(directory)
            .map(|s| {
                s.workers
                    .into_iter()
                    .filter(|w| w.status == WorkerStatus::Failed)
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Record a file conflict
    pub fn record_conflict(directory: &str, file_path: &str) -> bool {
        let mut state = match Self::read_state(directory) {
            Some(s) => s,
            None => return false,
        };

        if !state.ownership.conflicts.contains(&file_path.to_string()) {
            state.ownership.conflicts.push(file_path.to_string());
        }

        Self::write_state(directory, &state)
    }

    /// Detect conflicts where multiple workers modified the same file
    fn detect_file_conflicts(state: &UltrapilotState) -> Vec<String> {
        let mut file_to_workers: HashMap<String, Vec<String>> = HashMap::new();

        // Build map of files to workers that modified them
        for worker in &state.workers {
            if worker.status != WorkerStatus::Complete {
                continue;
            }

            for file in &worker.files_modified {
                file_to_workers
                    .entry(file.clone())
                    .or_insert_with(Vec::new)
                    .push(worker.id.clone());
            }
        }

        // Find files with multiple workers
        file_to_workers
            .into_iter()
            .filter(|(_, workers)| workers.len() > 1)
            .map(|(file, _)| file)
            .collect()
    }

    /// Generate integration summary
    fn generate_integration_summary(
        state: &UltrapilotState,
        completed: &[WorkerState],
        failed: &[WorkerState],
        conflicts: &[String],
    ) -> String {
        let mut lines = Vec::new();

        lines.push("Ultrapilot Integration Summary".to_string());
        lines.push("==============================".to_string());
        lines.push(format!("Original Task: {}", state.original_task));
        lines.push(String::new());
        lines.push(format!("Workers: {} total", state.workers.len()));
        lines.push(format!("  - Completed: {}", completed.len()));
        lines.push(format!("  - Failed: {}", failed.len()));
        lines.push(String::new());

        if !completed.is_empty() {
            lines.push("Completed Workers:".to_string());
            for worker in completed {
                lines.push(format!("  - {}: {}", worker.id, worker.task));
                if !worker.files_created.is_empty() {
                    lines.push(format!("    Created: {}", worker.files_created.join(", ")));
                }
                if !worker.files_modified.is_empty() {
                    lines.push(format!(
                        "    Modified: {}",
                        worker.files_modified.join(", ")
                    ));
                }
            }
            lines.push(String::new());
        }

        if !failed.is_empty() {
            lines.push("Failed Workers:".to_string());
            for worker in failed {
                lines.push(format!("  - {}: {}", worker.id, worker.task));
                lines.push(format!(
                    "    Error: {}",
                    worker.error.as_deref().unwrap_or("Unknown error")
                ));
            }
            lines.push(String::new());
        }

        if !conflicts.is_empty() {
            lines.push("Conflicts Detected:".to_string());
            for file in conflicts {
                lines.push(format!("  - {}", file));
            }
            lines.push(String::new());
            lines.push("Manual resolution required for conflicting files.".to_string());
        }

        lines.join("\n")
    }

    /// Integrate results from completed workers
    pub fn integrate_results(directory: &str) -> IntegrationResult {
        let state = match Self::read_state(directory) {
            Some(s) => s,
            None => {
                return IntegrationResult {
                    success: false,
                    files_created: Vec::new(),
                    files_modified: Vec::new(),
                    conflicts: Vec::new(),
                    errors: vec!["Ultrapilot not initialized".to_string()],
                    summary: "Integration failed: no state found".to_string(),
                }
            }
        };

        let completed = Self::get_completed_workers(directory);
        let failed = Self::get_failed_workers(directory);

        let mut files_created = std::collections::HashSet::new();
        let mut files_modified = std::collections::HashSet::new();
        let mut errors = Vec::new();

        // Collect files from completed workers
        for worker in &completed {
            for f in &worker.files_created {
                files_created.insert(f.clone());
            }
            for f in &worker.files_modified {
                files_modified.insert(f.clone());
            }
        }

        // Collect errors from failed workers
        for worker in &failed {
            if let Some(error) = &worker.error {
                errors.push(format!("Worker {}: {}", worker.id, error));
            }
        }

        // Check for conflicts
        let conflicts = Self::detect_file_conflicts(&state);

        let success = errors.is_empty() && conflicts.is_empty();
        let summary = Self::generate_integration_summary(&state, &completed, &failed, &conflicts);

        IntegrationResult {
            success,
            files_created: files_created.into_iter().collect(),
            files_modified: files_modified.into_iter().collect(),
            conflicts,
            errors,
            summary,
        }
    }

    /// Check if a file is owned by a specific worker
    pub fn is_file_owned_by_worker(directory: &str, worker_id: &str, file_path: &str) -> bool {
        let state = match Self::read_state(directory) {
            Some(s) => s,
            None => return false,
        };

        state
            .ownership
            .workers
            .get(worker_id)
            .map(|files| files.contains(&file_path.to_string()))
            .unwrap_or(false)
    }

    /// Check if a file is shared (owned by coordinator)
    pub fn is_shared_file(directory: &str, file_path: &str) -> bool {
        let state = match Self::read_state(directory) {
            Some(s) => s,
            None => return false,
        };

        state.ownership.coordinator.contains(&file_path.to_string())
    }

    /// Assign file ownership to a worker
    pub fn assign_file_to_worker(directory: &str, worker_id: &str, file_path: &str) -> bool {
        let mut state = match Self::read_state(directory) {
            Some(s) => s,
            None => return false,
        };

        // Check if file is already owned by coordinator
        if state.ownership.coordinator.contains(&file_path.to_string()) {
            return false; // Cannot reassign shared files
        }

        // Check if file is already owned by another worker
        for (id, files) in &state.ownership.workers {
            if id != worker_id && files.contains(&file_path.to_string()) {
                Self::record_conflict(directory, file_path);
                return false; // Already owned by another worker
            }
        }

        // Assign to worker
        state
            .ownership
            .workers
            .entry(worker_id.to_string())
            .or_insert_with(Vec::new)
            .push(file_path.to_string());

        Self::write_state(directory, &state)
    }

    /// Handle shared files that multiple workers might need to access
    pub fn handle_shared_files(directory: &str, files: Vec<String>) -> bool {
        let mut state = match Self::read_state(directory) {
            Some(s) => s,
            None => return false,
        };

        // Add files to coordinator ownership
        for file in files {
            if !state.ownership.coordinator.contains(&file) {
                state.ownership.coordinator.push(file);
            }
        }

        Self::write_state(directory, &state)
    }
}

impl Default for UltrapilotHook {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ultrapilot_config_default() {
        let config = UltrapilotConfig::default();
        assert_eq!(config.max_workers, 5);
        assert_eq!(config.max_iterations, 3);
        assert_eq!(config.worker_timeout, 300000);
        assert_eq!(config.worker_model, "sonnet");
        assert!(!config.verbose);
        assert!(config.shared_files.contains(&"package.json".to_string()));
    }

    #[test]
    fn test_decompose_task_numbered_list() {
        let config = UltrapilotConfig::default();
        let task = "1. First task\n2. Second task\n3. Third task";
        let subtasks = UltrapilotHook::decompose_task(task, &config);
        assert_eq!(subtasks.len(), 3);
        assert_eq!(subtasks[0], "First task");
        assert_eq!(subtasks[1], "Second task");
        assert_eq!(subtasks[2], "Third task");
    }

    #[test]
    fn test_decompose_task_bulleted_list() {
        let config = UltrapilotConfig::default();
        let task = "- First task\n- Second task\n* Third task";
        let subtasks = UltrapilotHook::decompose_task(task, &config);
        assert_eq!(subtasks.len(), 3);
        assert_eq!(subtasks[0], "First task");
        assert_eq!(subtasks[1], "Second task");
        assert_eq!(subtasks[2], "Third task");
    }

    #[test]
    fn test_decompose_task_sentences() {
        let config = UltrapilotConfig::default();
        let task = "This is the first sentence. This is the second sentence; This is the third.";
        let subtasks = UltrapilotHook::decompose_task(task, &config);
        assert!(subtasks.len() >= 1);
        assert!(subtasks[0].contains("first sentence"));
    }

    #[test]
    fn test_decompose_task_single() {
        let config = UltrapilotConfig::default();
        let task = "Single task";
        let subtasks = UltrapilotHook::decompose_task(task, &config);
        assert_eq!(subtasks.len(), 1);
        assert_eq!(subtasks[0], "Single task");
    }

    #[test]
    fn test_decompose_task_max_workers_limit() {
        let mut config = UltrapilotConfig::default();
        config.max_workers = 2;
        let task = "1. First\n2. Second\n3. Third\n4. Fourth";
        let subtasks = UltrapilotHook::decompose_task(task, &config);
        assert_eq!(subtasks.len(), 2);
    }

    #[test]
    fn test_worker_state_serialization() {
        let worker = WorkerState {
            id: "worker-1".to_string(),
            index: 0,
            task: "Test task".to_string(),
            owned_files: vec!["file1.rs".to_string()],
            status: WorkerStatus::Pending,
            task_id: None,
            started_at: None,
            completed_at: None,
            error: None,
            files_created: Vec::new(),
            files_modified: Vec::new(),
        };

        let json = serde_json::to_string(&worker).unwrap();
        let deserialized: WorkerState = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.id, "worker-1");
        assert_eq!(deserialized.status, WorkerStatus::Pending);
    }

    #[test]
    fn test_integration_summary_generation() {
        let state = UltrapilotState {
            active: true,
            iteration: 1,
            max_iterations: 3,
            original_task: "Build feature".to_string(),
            subtasks: vec!["Task 1".to_string()],
            workers: Vec::new(),
            ownership: FileOwnership::default(),
            started_at: Utc::now(),
            completed_at: None,
            total_workers_spawned: 2,
            successful_workers: 1,
            failed_workers: 1,
            session_id: None,
        };

        let completed = vec![WorkerState {
            id: "worker-1".to_string(),
            index: 0,
            task: "Task 1".to_string(),
            owned_files: Vec::new(),
            status: WorkerStatus::Complete,
            task_id: None,
            started_at: None,
            completed_at: Some(Utc::now()),
            error: None,
            files_created: vec!["file1.rs".to_string()],
            files_modified: Vec::new(),
        }];

        let failed = vec![WorkerState {
            id: "worker-2".to_string(),
            index: 1,
            task: "Task 2".to_string(),
            owned_files: Vec::new(),
            status: WorkerStatus::Failed,
            task_id: None,
            started_at: None,
            completed_at: Some(Utc::now()),
            error: Some("Test error".to_string()),
            files_created: Vec::new(),
            files_modified: Vec::new(),
        }];

        let conflicts = vec!["conflict.rs".to_string()];

        let summary =
            UltrapilotHook::generate_integration_summary(&state, &completed, &failed, &conflicts);

        assert!(summary.contains("Ultrapilot Integration Summary"));
        assert!(summary.contains("Build feature"));
        assert!(summary.contains("Completed: 1"));
        assert!(summary.contains("Failed: 1"));
        assert!(summary.contains("worker-1"));
        assert!(summary.contains("worker-2"));
        assert!(summary.contains("conflict.rs"));
    }
}
