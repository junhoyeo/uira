//! Background Notification Hook
//!
//! Handles notifications for background tasks completing.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use lazy_static::lazy_static;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::RwLock;

use crate::hook::{Hook, HookContext, HookResult};
use crate::types::{HookEvent, HookInput, HookOutput};

pub const HOOK_NAME: &str = "background-notification";

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackgroundNotificationHookConfig {
    /// Whether to automatically clear notifications after they're shown.
    ///
    /// Default: true
    #[serde(default = "default_true")]
    pub auto_clear: bool,

    /// Whether to show notifications only for the current session.
    ///
    /// Default: true
    #[serde(default = "default_true")]
    pub current_session_only: bool,
}

impl Default for BackgroundNotificationHookConfig {
    fn default() -> Self {
        Self {
            auto_clear: true,
            current_session_only: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BackgroundTaskStatus {
    Queued,
    Pending,
    Running,
    Completed,
    Error,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskProgress {
    pub tool_calls: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_tool: Option<String>,
    pub last_update: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_message_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackgroundTask {
    pub id: String,
    pub session_id: String,
    pub parent_session_id: String,
    pub description: String,
    pub prompt: String,
    pub agent: String,
    pub status: BackgroundTaskStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub queued_at: Option<DateTime<Utc>>,
    pub started_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub progress: Option<TaskProgress>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub concurrency_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_model: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationCheckResult {
    pub has_notifications: bool,
    pub tasks: Vec<BackgroundTask>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackgroundNotificationHookInput {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub directory: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub event: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackgroundNotificationHookOutput {
    #[serde(rename = "continue")]
    pub should_continue: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notification_count: Option<usize>,
}

impl BackgroundNotificationHookOutput {
    pub fn pass() -> Self {
        Self {
            should_continue: true,
            message: None,
            notification_count: None,
        }
    }
}

fn emoji_for_status(status: &BackgroundTaskStatus) -> &'static str {
    match status {
        BackgroundTaskStatus::Completed => "✓",
        BackgroundTaskStatus::Error => "✗",
        _ => "○",
    }
}

fn status_to_upper(status: &BackgroundTaskStatus) -> &'static str {
    match status {
        BackgroundTaskStatus::Queued => "QUEUED",
        BackgroundTaskStatus::Pending => "PENDING",
        BackgroundTaskStatus::Running => "RUNNING",
        BackgroundTaskStatus::Completed => "COMPLETED",
        BackgroundTaskStatus::Error => "ERROR",
        BackgroundTaskStatus::Cancelled => "CANCELLED",
    }
}

fn truncate_chars(s: &str, max_chars: usize) -> (&str, bool) {
    if s.chars().count() <= max_chars {
        return (s, false);
    }

    let mut end = 0usize;
    for (count, (idx, ch)) in s.char_indices().enumerate() {
        if count == max_chars {
            break;
        }
        end = idx + ch.len_utf8();
    }
    (&s[..end], true)
}

fn format_duration(start: DateTime<Utc>, end: Option<DateTime<Utc>>) -> String {
    let end = end.unwrap_or_else(Utc::now);
    let duration = end - start;
    let seconds_total = duration.num_seconds().max(0);
    let minutes_total = seconds_total / 60;
    let hours = minutes_total / 60;

    if hours > 0 {
        format!("{}h {}m {}s", hours, minutes_total % 60, seconds_total % 60)
    } else if minutes_total > 0 {
        format!("{}m {}s", minutes_total, seconds_total % 60)
    } else {
        format!("{}s", seconds_total)
    }
}

fn format_task_notification(task: &BackgroundTask) -> String {
    let status = status_to_upper(&task.status);
    let duration = format_duration(task.started_at, task.completed_at);
    let emoji = emoji_for_status(&task.status);

    let mut lines = vec![
        format!("{} [{}] {}", emoji, status, task.description),
        format!("  Agent: {}", task.agent),
        format!("  Duration: {}", duration),
    ];

    if let Some(progress) = &task.progress {
        // Match TS truthiness: only show if > 0
        if progress.tool_calls > 0 {
            lines.push(format!("  Tool calls: {}", progress.tool_calls));
        }
    }

    if let Some(result) = &task.result {
        let (preview, truncated) = truncate_chars(result, 200);
        let suffix = if truncated { "..." } else { "" };
        lines.push(format!("  Result: {}{}", preview, suffix));
    }

    if let Some(error) = &task.error {
        lines.push(format!("  Error: {}", error));
    }

    lines.join("\n")
}

fn default_format_notification(tasks: &[BackgroundTask]) -> String {
    if tasks.is_empty() {
        return String::new();
    }

    let header = if tasks.len() == 1 {
        "\n[BACKGROUND TASK COMPLETED]\n".to_string()
    } else {
        format!("\n[{} BACKGROUND TASKS COMPLETED]\n", tasks.len())
    };

    let task_descriptions = tasks
        .iter()
        .map(format_task_notification)
        .collect::<Vec<_>>()
        .join("\n\n");

    format!("{}\n{}\n", header, task_descriptions)
}

#[derive(Debug, Default)]
pub struct BackgroundNotificationManager {
    notifications: HashMap<String, Vec<BackgroundTask>>,
}

impl BackgroundNotificationManager {
    pub fn get_pending_notifications(&self, session_id: &str) -> Vec<BackgroundTask> {
        self.notifications
            .get(session_id)
            .cloned()
            .unwrap_or_default()
    }

    pub fn clear_notifications(&mut self, session_id: &str) {
        self.notifications.remove(session_id);
    }

    pub fn mark_for_notification(&mut self, task: BackgroundTask) {
        self.notifications
            .entry(task.parent_session_id.clone())
            .or_default()
            .push(task);
    }

    pub fn load_task_from_disk(&self, task_id: &str) -> Option<BackgroundTask> {
        let tasks_dir = background_tasks_dir()?;
        let task_path = tasks_dir.join(format!("{}.json", task_id));
        let content = fs::read_to_string(task_path).ok()?;
        serde_json::from_str(&content).ok()
    }
}

lazy_static! {
    /// Global manager for background task notifications.
    pub static ref MANAGER: RwLock<BackgroundNotificationManager> =
        RwLock::new(BackgroundNotificationManager::default());
    static ref TASK_EVENT_PATTERN: Regex = Regex::new(r"^task\.(completed|failed)$").unwrap();
}

/// Get the directory where background task state files are stored.
pub fn background_tasks_dir() -> Option<PathBuf> {
    if let Ok(dir) = std::env::var("UIRA_BACKGROUND_TASKS_DIR") {
        if !dir.is_empty() {
            return Some(PathBuf::from(dir));
        }
    }

    dirs::home_dir().map(|h| h.join(".claude").join(".uira").join("background-tasks"))
}

fn handle_background_event(event: &serde_json::Value) {
    #[derive(Debug, Deserialize)]
    struct Event {
        #[serde(rename = "type")]
        event_type: String,
        #[serde(default)]
        properties: HashMap<String, serde_json::Value>,
    }

    let parsed: Event = match serde_json::from_value(event.clone()) {
        Ok(v) => v,
        Err(_) => return,
    };

    if !TASK_EVENT_PATTERN.is_match(&parsed.event_type) {
        return;
    }

    let task_id = match parsed.properties.get("taskId").and_then(|v| v.as_str()) {
        Some(s) if !s.is_empty() => s,
        _ => return,
    };

    let task = {
        let mgr = match MANAGER.read() {
            Ok(m) => m,
            Err(_) => return,
        };
        mgr.load_task_from_disk(task_id)
    };

    if let Some(task) = task {
        if let Ok(mut mgr) = MANAGER.write() {
            mgr.mark_for_notification(task);
        }
    }
}

/// Public entry point for triggering background event processing from NAPI.
pub fn handle_background_event_public(event: &serde_json::Value) {
    handle_background_event(event);
}

pub fn check_background_notifications(
    session_id: &str,
    config: Option<&BackgroundNotificationHookConfig>,
) -> NotificationCheckResult {
    let tasks = MANAGER
        .read()
        .ok()
        .map(|mgr| mgr.get_pending_notifications(session_id))
        .unwrap_or_default();

    if tasks.is_empty() {
        return NotificationCheckResult {
            has_notifications: false,
            tasks: Vec::new(),
            message: None,
        };
    }

    // TS supports a custom formatter; Rust port uses the default formatter.
    let _cfg = config.cloned().unwrap_or_default();
    let message = default_format_notification(&tasks);

    NotificationCheckResult {
        has_notifications: true,
        tasks,
        message: Some(message),
    }
}

pub fn process_background_notification(
    input: &BackgroundNotificationHookInput,
    config: Option<&BackgroundNotificationHookConfig>,
) -> BackgroundNotificationHookOutput {
    let Some(session_id) = input.session_id.as_deref() else {
        return BackgroundNotificationHookOutput::pass();
    };

    let result = check_background_notifications(session_id, config);
    if !result.has_notifications {
        return BackgroundNotificationHookOutput::pass();
    }

    let auto_clear = config.map(|c| c.auto_clear).unwrap_or(true);
    if auto_clear {
        if let Ok(mut mgr) = MANAGER.write() {
            mgr.clear_notifications(session_id);
        }
    }

    BackgroundNotificationHookOutput {
        should_continue: true,
        message: result.message,
        notification_count: Some(result.tasks.len()),
    }
}

pub struct BackgroundNotificationHook {
    config: BackgroundNotificationHookConfig,
}

impl BackgroundNotificationHook {
    pub fn new() -> Self {
        Self {
            config: BackgroundNotificationHookConfig::default(),
        }
    }

    pub fn with_config(config: BackgroundNotificationHookConfig) -> Self {
        Self { config }
    }
}

impl Default for BackgroundNotificationHook {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Hook for BackgroundNotificationHook {
    fn name(&self) -> &str {
        HOOK_NAME
    }

    fn events(&self) -> &[HookEvent] {
        const EVENTS: &[HookEvent] = &[HookEvent::PostToolUse, HookEvent::SessionIdle];
        EVENTS
    }

    async fn execute(
        &self,
        _event: HookEvent,
        input: &HookInput,
        context: &HookContext,
    ) -> HookResult {
        // If an event payload is provided (via extra), process it first.
        if let Some(ev) = input.extra.get("event") {
            handle_background_event(ev);
        }

        let session_id = input
            .session_id
            .as_deref()
            .or(context.session_id.as_deref());
        let Some(session_id) = session_id else {
            return Ok(HookOutput::pass());
        };

        let result = check_background_notifications(session_id, Some(&self.config));
        if !result.has_notifications {
            return Ok(HookOutput::pass());
        }

        if self.config.auto_clear {
            if let Ok(mut mgr) = MANAGER.write() {
                mgr.clear_notifications(session_id);
            }
        }

        Ok(HookOutput::continue_with_message(
            result.message.unwrap_or_default(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use std::sync::Mutex;
    use tempfile::tempdir;

    lazy_static! {
        static ref ENV_LOCK: Mutex<()> = Mutex::new(());
    }

    fn clear_manager() {
        if let Ok(mut mgr) = MANAGER.write() {
            mgr.notifications.clear();
        }
    }

    #[test]
    fn test_format_duration() {
        let start = DateTime::parse_from_rfc3339("2026-01-24T00:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let end = DateTime::parse_from_rfc3339("2026-01-24T00:00:45Z")
            .unwrap()
            .with_timezone(&Utc);
        assert_eq!(format_duration(start, Some(end)), "45s");

        let end = DateTime::parse_from_rfc3339("2026-01-24T00:03:05Z")
            .unwrap()
            .with_timezone(&Utc);
        assert_eq!(format_duration(start, Some(end)), "3m 5s");

        let end = DateTime::parse_from_rfc3339("2026-01-24T02:03:05Z")
            .unwrap()
            .with_timezone(&Utc);
        assert_eq!(format_duration(start, Some(end)), "2h 3m 5s");
    }

    #[test]
    fn test_format_task_notification() {
        let start = DateTime::parse_from_rfc3339("2026-01-24T00:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let end = DateTime::parse_from_rfc3339("2026-01-24T00:00:10Z")
            .unwrap()
            .with_timezone(&Utc);

        let task = BackgroundTask {
            id: "bg_1".to_string(),
            session_id: "ses_child".to_string(),
            parent_session_id: "ses_parent".to_string(),
            description: "Do thing".to_string(),
            prompt: "Do thing".to_string(),
            agent: "explore".to_string(),
            status: BackgroundTaskStatus::Completed,
            queued_at: None,
            started_at: start,
            completed_at: Some(end),
            result: Some("x".repeat(250)),
            error: None,
            progress: Some(TaskProgress {
                tool_calls: 2,
                last_tool: None,
                last_update: end,
                last_message: None,
                last_message_at: None,
            }),
            concurrency_key: None,
            parent_model: None,
        };

        let rendered = format_task_notification(&task);
        assert!(rendered.contains("✓ [COMPLETED] Do thing"));
        assert!(rendered.contains("  Agent: explore"));
        assert!(rendered.contains("  Duration: 10s"));
        assert!(rendered.contains("  Tool calls: 2"));
        assert!(rendered.contains("  Result: "));
        assert!(rendered.contains("..."));
    }

    #[test]
    fn test_default_format_notification_header() {
        let start = DateTime::parse_from_rfc3339("2026-01-24T00:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let task = BackgroundTask {
            id: "bg_1".to_string(),
            session_id: "ses_child".to_string(),
            parent_session_id: "ses_parent".to_string(),
            description: "One".to_string(),
            prompt: "One".to_string(),
            agent: "explore".to_string(),
            status: BackgroundTaskStatus::Completed,
            queued_at: None,
            started_at: start,
            completed_at: Some(start),
            result: None,
            error: None,
            progress: None,
            concurrency_key: None,
            parent_model: None,
        };

        let msg = default_format_notification(&[task]);
        assert!(msg.starts_with("\n[BACKGROUND TASK COMPLETED]\n\n"));
        assert!(msg.ends_with("\n"));
    }

    #[test]
    fn test_handle_background_event_marks_notification_from_disk() {
        clear_manager();

        let _guard = ENV_LOCK.lock().unwrap();

        let dir = tempdir().unwrap();
        std::env::set_var("UIRA_BACKGROUND_TASKS_DIR", dir.path());

        let start = DateTime::parse_from_rfc3339("2026-01-24T00:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let task = BackgroundTask {
            id: "bg_test".to_string(),
            session_id: "ses_child".to_string(),
            parent_session_id: "ses_parent".to_string(),
            description: "Work".to_string(),
            prompt: "Work".to_string(),
            agent: "explore".to_string(),
            status: BackgroundTaskStatus::Completed,
            queued_at: None,
            started_at: start,
            completed_at: Some(start),
            result: None,
            error: None,
            progress: None,
            concurrency_key: None,
            parent_model: None,
        };

        let path = Path::new(dir.path()).join("bg_test.json");
        fs::write(path, serde_json::to_string_pretty(&task).unwrap()).unwrap();

        let event = serde_json::json!({
            "type": "task.completed",
            "properties": { "taskId": "bg_test" }
        });
        handle_background_event(&event);

        let pending = MANAGER
            .read()
            .unwrap()
            .get_pending_notifications("ses_parent");
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].id, "bg_test");

        clear_manager();
        std::env::remove_var("UIRA_BACKGROUND_TASKS_DIR");
    }
}
