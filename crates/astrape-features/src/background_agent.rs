use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Condvar, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Type alias for stale session callback to reduce type complexity.
pub type StaleSessionCallback = Arc<dyn Fn(&BackgroundTask) + Send + Sync>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BackgroundTaskStatus {
    Queued,
    #[serde(alias = "pending")]
    Pending,
    Running,
    Completed,
    Error,
    Cancelled,
}

impl BackgroundTaskStatus {
    fn is_terminal(self) -> bool {
        matches!(
            self,
            BackgroundTaskStatus::Completed
                | BackgroundTaskStatus::Error
                | BackgroundTaskStatus::Cancelled
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskProgress {
    pub tool_calls: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_tool: Option<String>,
    pub last_update: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_message: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_message_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BackgroundTask {
    pub id: String,
    pub session_id: String,
    pub parent_session_id: String,
    pub description: String,
    pub prompt: String,
    pub agent: String,
    pub status: BackgroundTaskStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub queued_at: Option<DateTime<Utc>>,
    pub started_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub progress: Option<TaskProgress>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub concurrency_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_model: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaunchInput {
    pub description: String,
    pub prompt: String,
    pub agent: String,
    pub parent_session_id: String,
    pub model: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResumeInput {
    pub session_id: String,
    pub prompt: String,
    pub parent_session_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResumeContext {
    pub session_id: String,
    pub previous_prompt: String,
    pub tool_call_count: u64,
    pub last_tool_used: Option<String>,
    pub last_output_summary: Option<String>,
    pub started_at: DateTime<Utc>,
    pub last_activity_at: DateTime<Utc>,
}

#[derive(Clone, Default)]
pub struct BackgroundTaskConfig {
    pub default_concurrency: Option<usize>,
    pub model_concurrency: Option<HashMap<String, usize>>,
    pub provider_concurrency: Option<HashMap<String, usize>>,
    pub max_total_tasks: Option<usize>,
    pub task_timeout_ms: Option<u64>,
    pub max_queue_size: Option<usize>,
    pub stale_threshold_ms: Option<u64>,
    pub on_stale_session: Option<StaleSessionCallback>,
    pub storage_dir: Option<PathBuf>,
}

impl std::fmt::Debug for BackgroundTaskConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BackgroundTaskConfig")
            .field("default_concurrency", &self.default_concurrency)
            .field("model_concurrency", &self.model_concurrency)
            .field("provider_concurrency", &self.provider_concurrency)
            .field("max_total_tasks", &self.max_total_tasks)
            .field("task_timeout_ms", &self.task_timeout_ms)
            .field("max_queue_size", &self.max_queue_size)
            .field("stale_threshold_ms", &self.stale_threshold_ms)
            .field(
                "on_stale_session",
                &self.on_stale_session.as_ref().map(|_| "<callback>"),
            )
            .field("storage_dir", &self.storage_dir)
            .finish()
    }
}

#[derive(Debug, Default)]
struct ConcurrencyState {
    counts: HashMap<String, usize>,
    queued: HashMap<String, usize>,
}

#[derive(Debug)]
pub struct ConcurrencyManager {
    config: BackgroundTaskConfig,
    state: Mutex<ConcurrencyState>,
    cvar: Condvar,
}

impl ConcurrencyManager {
    pub fn new(config: BackgroundTaskConfig) -> Self {
        Self {
            config,
            state: Mutex::new(ConcurrencyState::default()),
            cvar: Condvar::new(),
        }
    }

    pub fn get_concurrency_limit(&self, key: &str) -> usize {
        if let Some(map) = &self.config.model_concurrency {
            if let Some(limit) = map.get(key).copied() {
                return if limit == 0 { usize::MAX } else { limit };
            }
        }

        let provider = key.split('/').next().unwrap_or(key);
        if let Some(map) = &self.config.provider_concurrency {
            if let Some(limit) = map.get(provider).copied() {
                return if limit == 0 { usize::MAX } else { limit };
            }
        }

        if let Some(limit) = self.config.default_concurrency {
            return if limit == 0 { usize::MAX } else { limit };
        }

        5
    }

    pub fn acquire(&self, key: &str) {
        let limit = self.get_concurrency_limit(key);
        if limit == usize::MAX {
            return;
        }

        let mut state = self.state.lock().expect("lock");
        let current = *state.counts.get(key).unwrap_or(&0);
        if current < limit {
            state.counts.insert(key.to_string(), current + 1);
            return;
        }

        *state.queued.entry(key.to_string()).or_insert(0) += 1;
        loop {
            state = self.cvar.wait(state).expect("wait");
            let current = *state.counts.get(key).unwrap_or(&0);
            if current < limit {
                state.counts.insert(key.to_string(), current + 1);
                if let Some(q) = state.queued.get_mut(key) {
                    *q = q.saturating_sub(1);
                }
                return;
            }
        }
    }

    pub fn release(&self, key: &str) {
        let limit = self.get_concurrency_limit(key);
        if limit == usize::MAX {
            return;
        }

        let mut state = self.state.lock().expect("lock");
        let current = *state.counts.get(key).unwrap_or(&0);
        if current > 0 {
            state.counts.insert(key.to_string(), current - 1);
        }
        self.cvar.notify_all();
    }

    pub fn get_count(&self, key: &str) -> usize {
        let state = self.state.lock().expect("lock");
        *state.counts.get(key).unwrap_or(&0)
    }

    pub fn get_queue_length(&self, key: &str) -> usize {
        let state = self.state.lock().expect("lock");
        *state.queued.get(key).unwrap_or(&0)
    }

    pub fn is_at_capacity(&self, key: &str) -> bool {
        let limit = self.get_concurrency_limit(key);
        if limit == usize::MAX {
            return false;
        }
        self.get_count(key) >= limit
    }

    pub fn clear(&self) {
        let mut state = self.state.lock().expect("lock");
        state.counts.clear();
        state.queued.clear();
        self.cvar.notify_all();
    }
}

fn default_storage_dir() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home)
        .join(".claude")
        .join(".astrape")
        .join("background-tasks")
}

#[derive(Debug)]
pub struct BackgroundManager {
    tasks: Mutex<HashMap<String, BackgroundTask>>,
    notifications: Mutex<HashMap<String, Vec<BackgroundTask>>>,
    concurrency: ConcurrencyManager,
    config: BackgroundTaskConfig,
    storage_dir: PathBuf,
}

impl BackgroundManager {
    pub fn new(config: BackgroundTaskConfig) -> Self {
        let storage_dir = config
            .storage_dir
            .clone()
            .unwrap_or_else(default_storage_dir);
        let manager = Self {
            tasks: Mutex::new(HashMap::new()),
            notifications: Mutex::new(HashMap::new()),
            concurrency: ConcurrencyManager::new(config.clone()),
            config,
            storage_dir,
        };

        manager.ensure_storage_dir();
        manager.load_persisted_tasks();
        manager
    }

    fn ensure_storage_dir(&self) {
        let _ = fs::create_dir_all(&self.storage_dir);
    }

    fn base36(mut v: u128) -> String {
        const DIGITS: &[u8] = b"0123456789abcdefghijklmnopqrstuvwxyz";
        if v == 0 {
            return "0".to_string();
        }
        let mut buf = Vec::new();
        while v > 0 {
            let idx = (v % 36) as usize;
            buf.push(DIGITS[idx]);
            v /= 36;
        }
        buf.reverse();
        String::from_utf8(buf).unwrap_or_else(|_| "0".to_string())
    }

    fn generate_task_id(&self) -> String {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default();
        let ts = Self::base36(now.as_millis());
        let nanos = now.subsec_nanos() as u128;
        let pid = std::process::id() as u128;
        let mixed = (nanos << 32) ^ pid;
        let randish = Self::base36(mixed);
        let randish = randish.chars().rev().take(6).collect::<String>();
        format!("bg_{ts}{randish}")
    }

    fn task_path(&self, task_id: &str) -> PathBuf {
        self.storage_dir.join(format!("{task_id}.json"))
    }

    fn persist_task(&self, task: &BackgroundTask) {
        let path = self.task_path(&task.id);
        let Ok(payload) = serde_json::to_string_pretty(task) else {
            return;
        };
        let _ = fs::write(path, payload);
    }

    fn load_persisted_tasks(&self) {
        let Ok(entries) = fs::read_dir(&self.storage_dir) else {
            return;
        };

        let mut tasks = self.tasks.lock().expect("lock");
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) != Some("json") {
                continue;
            }
            let Ok(content) = fs::read_to_string(&path) else {
                continue;
            };
            let Ok(task) = serde_json::from_str::<BackgroundTask>(&content) else {
                continue;
            };
            tasks.insert(task.id.clone(), task);
        }
    }

    pub fn launch(&self, input: LaunchInput) -> Result<BackgroundTask, String> {
        let (running_count, queued_count) = {
            let tasks = self.tasks.lock().expect("lock");
            let running_count = tasks
                .values()
                .filter(|t| t.status == BackgroundTaskStatus::Running)
                .count();
            let queued_count = tasks
                .values()
                .filter(|t| t.status == BackgroundTaskStatus::Queued)
                .count();
            (running_count, queued_count)
        };

        let max_total = self.config.max_total_tasks.unwrap_or(10);
        let tasks_in_flight = running_count + queued_count;
        if tasks_in_flight >= max_total {
            return Err(format!(
                "Maximum tasks in flight ({max_total}) reached. Currently: {running_count} running, {queued_count} queued. Wait for some tasks to complete."
            ));
        }

        if let Some(max_queue) = self.config.max_queue_size {
            if queued_count >= max_queue {
                return Err(format!(
                    "Maximum queue size ({max_queue}) reached. Currently: {running_count} running, {queued_count} queued. Wait for some tasks to start or complete."
                ));
            }
        }

        let task_id = self.generate_task_id();
        let session_id = format!("ses_{}", self.generate_task_id());
        let concurrency_key = input.agent.clone();
        let now = Utc::now();

        let task = BackgroundTask {
            id: task_id,
            session_id,
            parent_session_id: input.parent_session_id,
            description: input.description,
            prompt: input.prompt,
            agent: input.agent,
            status: BackgroundTaskStatus::Queued,
            queued_at: Some(now),
            started_at: now,
            completed_at: None,
            result: None,
            error: None,
            progress: Some(TaskProgress {
                tool_calls: 0,
                last_tool: None,
                last_update: now,
                last_message: None,
                last_message_at: None,
            }),
            concurrency_key: Some(concurrency_key.clone()),
            parent_model: input.model,
        };

        {
            self.tasks
                .lock()
                .expect("lock")
                .insert(task.id.clone(), task.clone());
        }
        self.persist_task(&task);

        self.concurrency.acquire(&concurrency_key);

        let mut tasks = self.tasks.lock().expect("lock");
        let Some(mut updated) = tasks.get(&task.id).cloned() else {
            // Release the concurrency slot we just acquired before returning error
            self.concurrency.release(&concurrency_key);
            return Err("Task disappeared".to_string());
        };

        updated.status = BackgroundTaskStatus::Running;
        updated.started_at = Utc::now();
        tasks.insert(updated.id.clone(), updated.clone());
        drop(tasks);
        self.persist_task(&updated);

        Ok(updated)
    }

    fn clear_storage(&self) {
        let Ok(entries) = fs::read_dir(&self.storage_dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("json") {
                let _ = fs::remove_file(path);
            }
        }
    }

    pub fn cleanup(&self) {
        self.concurrency.clear();
        self.tasks.lock().expect("lock").clear();
        self.notifications.lock().expect("lock").clear();
        self.clear_storage();
    }

    pub fn get_task(&self, task_id: &str) -> Option<BackgroundTask> {
        self.tasks.lock().expect("lock").get(task_id).cloned()
    }

    pub fn get_all_tasks(&self) -> Vec<BackgroundTask> {
        self.tasks.lock().expect("lock").values().cloned().collect()
    }

    pub fn get_tasks_for_session(&self, parent_session_id: &str) -> Vec<BackgroundTask> {
        self.tasks
            .lock()
            .expect("lock")
            .values()
            .filter(|t| t.parent_session_id == parent_session_id)
            .cloned()
            .collect()
    }

    pub fn update_task_status(
        &self,
        task_id: &str,
        status: BackgroundTaskStatus,
    ) -> Option<BackgroundTask> {
        let mut tasks = self.tasks.lock().expect("lock");
        if let Some(task) = tasks.get_mut(task_id) {
            task.status = status;
            if status.is_terminal() {
                task.completed_at = Some(Utc::now());
                // Release concurrency slot on terminal status to prevent resource leak
                // Use take() to atomically clear the key, preventing double-release
                if let Some(key) = task.concurrency_key.take() {
                    self.concurrency.release(&key);
                }
            }
            let updated = task.clone();
            drop(tasks);
            self.persist_task(&updated);
            Some(updated)
        } else {
            None
        }
    }

    pub fn complete_task(&self, task_id: &str, result: String) -> Option<BackgroundTask> {
        let mut tasks = self.tasks.lock().expect("lock");
        if let Some(task) = tasks.get_mut(task_id) {
            task.status = BackgroundTaskStatus::Completed;
            task.completed_at = Some(Utc::now());
            task.result = Some(result);
            // Use take() to atomically clear the key, preventing double-release
            if let Some(key) = task.concurrency_key.take() {
                self.concurrency.release(&key);
            }
            let updated = task.clone();
            drop(tasks);
            self.persist_task(&updated);
            Some(updated)
        } else {
            None
        }
    }

    pub fn fail_task(&self, task_id: &str, error: String) -> Option<BackgroundTask> {
        let mut tasks = self.tasks.lock().expect("lock");
        if let Some(task) = tasks.get_mut(task_id) {
            task.status = BackgroundTaskStatus::Error;
            task.completed_at = Some(Utc::now());
            task.error = Some(error);
            // Use take() to atomically clear the key, preventing double-release
            if let Some(key) = task.concurrency_key.take() {
                self.concurrency.release(&key);
            }
            let updated = task.clone();
            drop(tasks);
            self.persist_task(&updated);
            Some(updated)
        } else {
            None
        }
    }

    pub fn cancel_task(&self, task_id: &str) -> Option<BackgroundTask> {
        let mut tasks = self.tasks.lock().expect("lock");
        if let Some(task) = tasks.get_mut(task_id) {
            if task.status.is_terminal() {
                return Some(task.clone());
            }
            task.status = BackgroundTaskStatus::Cancelled;
            task.completed_at = Some(Utc::now());
            // Use take() to atomically clear the key, preventing double-release
            if let Some(key) = task.concurrency_key.take() {
                self.concurrency.release(&key);
            }
            let updated = task.clone();
            drop(tasks);
            self.persist_task(&updated);
            Some(updated)
        } else {
            None
        }
    }
}

static BACKGROUND_MANAGER_INSTANCE: Mutex<Option<Arc<BackgroundManager>>> = Mutex::new(None);

pub fn get_background_manager(config: BackgroundTaskConfig) -> Arc<BackgroundManager> {
    let mut instance = BACKGROUND_MANAGER_INSTANCE.lock().expect("lock");
    if let Some(existing) = instance.as_ref() {
        return existing.clone();
    }
    let manager = Arc::new(BackgroundManager::new(config));
    *instance = Some(manager.clone());
    manager
}

pub fn reset_background_manager() {
    let mut instance = BACKGROUND_MANAGER_INSTANCE.lock().expect("lock");
    if let Some(manager) = instance.take() {
        manager.cleanup();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use tempfile::TempDir;

    #[test]
    fn concurrency_manager_blocks_until_release() {
        let config = BackgroundTaskConfig {
            default_concurrency: Some(1),
            ..BackgroundTaskConfig::default()
        };
        let manager = Arc::new(ConcurrencyManager::new(config));
        manager.acquire("key");
        assert_eq!(manager.get_count("key"), 1);

        let (tx, rx) = std::sync::mpsc::channel();
        let manager_ref = manager.clone();
        let handle = std::thread::spawn(move || {
            manager_ref.acquire("key");
            tx.send(()).unwrap();
            manager_ref.release("key");
        });

        assert!(rx.recv_timeout(Duration::from_millis(50)).is_err());
        manager.release("key");
        assert!(rx.recv_timeout(Duration::from_secs(1)).is_ok());
        handle.join().unwrap();
    }

    #[test]
    fn background_manager_persists_and_loads_tasks() {
        let dir = TempDir::new().unwrap();
        let config = BackgroundTaskConfig {
            default_concurrency: Some(0),
            storage_dir: Some(dir.path().to_path_buf()),
            ..BackgroundTaskConfig::default()
        };
        let manager = BackgroundManager::new(config.clone());

        let task = manager
            .launch(LaunchInput {
                description: "do".to_string(),
                prompt: "p".to_string(),
                agent: "a".to_string(),
                parent_session_id: "parent".to_string(),
                model: None,
            })
            .unwrap();

        assert_eq!(manager.get_all_tasks().len(), 1);
        assert!(dir.path().join(format!("{}.json", task.id)).exists());

        let manager2 = BackgroundManager::new(config);
        assert_eq!(manager2.get_all_tasks().len(), 1);
        assert!(manager2.get_task(&task.id).is_some());
    }

    #[test]
    fn cleanup_clears_storage() {
        let dir = TempDir::new().unwrap();
        let config = BackgroundTaskConfig {
            default_concurrency: Some(0),
            storage_dir: Some(dir.path().to_path_buf()),
            ..BackgroundTaskConfig::default()
        };
        let manager = BackgroundManager::new(config.clone());

        let task = manager
            .launch(LaunchInput {
                description: "test".to_string(),
                prompt: "prompt".to_string(),
                agent: "agent".to_string(),
                parent_session_id: "parent".to_string(),
                model: None,
            })
            .unwrap();

        // Verify task was persisted
        let task_file = dir.path().join(format!("{}.json", task.id));
        assert!(task_file.exists());
        assert_eq!(manager.get_all_tasks().len(), 1);

        // Cleanup should remove storage files
        manager.cleanup();
        assert!(!task_file.exists());
        assert_eq!(manager.get_all_tasks().len(), 0);

        // New manager should not load any tasks
        let manager2 = BackgroundManager::new(config);
        assert_eq!(manager2.get_all_tasks().len(), 0);
    }
}
