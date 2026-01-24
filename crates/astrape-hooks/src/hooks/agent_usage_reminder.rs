//! Agent Usage Reminder Hook
//!
//! Reminds users to use specialized agents when they make direct tool calls
//! for searching or fetching content instead of delegating to agents.
//!
//! Ported from: oh-my-claudecode/src/hooks/agent-usage-reminder

use async_trait::async_trait;
use lazy_static::lazy_static;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::PathBuf;
use std::sync::RwLock;

use crate::hook::{Hook, HookContext, HookResult};
use crate::types::{HookEvent, HookInput, HookOutput};

pub const HOOK_NAME: &str = "agent-usage-reminder";

/// Reminder message shown to users
pub const REMINDER_MESSAGE: &str = r#"
[Agent Usage Reminder]

You called a search/fetch tool directly without leveraging specialized agents.

RECOMMENDED: Use Task tool with explore/researcher agents for better results:

```
// Parallel exploration - fire multiple agents simultaneously
Task(agent=\"explore\", prompt=\"Find all files matching pattern X\")
Task(agent=\"explore\", prompt=\"Search for implementation of Y\")
Task(agent=\"researcher\", prompt=\"Lookup documentation for Z\")

// Then continue your work while they run in background
// System will notify you when each completes
```

WHY:
- Agents can perform deeper, more thorough searches
- Background tasks run in parallel, saving time
- Specialized agents have domain expertise
- Reduces context window usage in main session

ALWAYS prefer: Multiple parallel Task calls > Direct tool calls
"#;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AgentUsageState {
    pub session_id: String,
    pub agent_used: bool,
    pub reminder_count: u32,
    pub updated_at: u64,
}

impl AgentUsageState {
    fn new(session_id: &str) -> Self {
        Self {
            session_id: session_id.to_string(),
            agent_used: false,
            reminder_count: 0,
            updated_at: now_ms(),
        }
    }
}

fn now_ms() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn astrape_storage_dir() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("ASTRAPE_ASTRAPE_DIR") {
        if !dir.is_empty() {
            return Some(PathBuf::from(dir));
        }
    }

    dirs::home_dir().map(|h| h.join(".astrape"))
}

fn agent_usage_reminder_storage_dir() -> Option<PathBuf> {
    astrape_storage_dir().map(|d| d.join("agent-usage-reminder"))
}

fn get_storage_path(session_id: &str) -> Option<PathBuf> {
    agent_usage_reminder_storage_dir().map(|d| d.join(format!("{}.json", session_id)))
}

pub fn load_agent_usage_state(session_id: &str) -> Option<AgentUsageState> {
    let file_path = get_storage_path(session_id)?;
    if !file_path.exists() {
        return None;
    }

    let content = fs::read_to_string(file_path).ok()?;
    serde_json::from_str(&content).ok()
}

pub fn save_agent_usage_state(state: &AgentUsageState) {
    let Some(storage_dir) = agent_usage_reminder_storage_dir() else {
        return;
    };

    let _ = fs::create_dir_all(&storage_dir);

    let Some(file_path) = get_storage_path(&state.session_id) else {
        return;
    };

    if let Ok(content) = serde_json::to_string_pretty(state) {
        let _ = fs::write(file_path, content);
    }
}

pub fn clear_agent_usage_state(session_id: &str) {
    let Some(file_path) = get_storage_path(session_id) else {
        return;
    };
    if file_path.exists() {
        let _ = fs::remove_file(file_path);
    }
}

lazy_static! {
    /// All tool names normalized to lowercase for case-insensitive matching
    pub static ref TARGET_TOOLS: HashSet<&'static str> = HashSet::from([
        "grep",
        "safe_grep",
        "glob",
        "safe_glob",
        "webfetch",
        "context7_resolve-library-id",
        "context7_query-docs",
        "websearch_web_search_exa",
        "context7_get-library-docs",
    ]);

    /// Agent tools that indicate agent usage
    pub static ref AGENT_TOOLS: HashSet<&'static str> =
        HashSet::from(["task", "call_omo_agent", "astrape_task"]);

    static ref TOOL_NAME_PATTERN: Regex =
        Regex::new(r"(?i)^(?:functions\.)?([a-z0-9][a-z0-9_-]*)$").unwrap();

    static ref SESSION_STATES: RwLock<HashMap<String, AgentUsageState>> =
        RwLock::new(HashMap::new());
}

pub fn normalize_tool_name(tool: &str) -> String {
    let tool = tool.trim();
    if let Some(caps) = TOOL_NAME_PATTERN.captures(tool) {
        return caps
            .get(1)
            .map(|m| m.as_str().to_lowercase())
            .unwrap_or_else(|| tool.to_lowercase());
    }
    tool.to_lowercase()
}

fn get_or_create_state(session_id: &str) -> AgentUsageState {
    if let Ok(states) = SESSION_STATES.read() {
        if let Some(state) = states.get(session_id) {
            return state.clone();
        }
    }

    let persisted = load_agent_usage_state(session_id);
    let state = persisted.unwrap_or_else(|| AgentUsageState::new(session_id));

    if let Ok(mut states) = SESSION_STATES.write() {
        states.insert(session_id.to_string(), state.clone());
    }

    state
}

fn mark_agent_used(session_id: &str) {
    let mut state = get_or_create_state(session_id);
    state.agent_used = true;
    state.updated_at = now_ms();
    save_agent_usage_state(&state);

    if let Ok(mut states) = SESSION_STATES.write() {
        states.insert(session_id.to_string(), state);
    }
}

fn reset_state(session_id: &str) {
    if let Ok(mut states) = SESSION_STATES.write() {
        states.remove(session_id);
    }
    clear_agent_usage_state(session_id);
}

fn should_remind(tool_name: &str, session_id: &str) -> bool {
    let state = get_or_create_state(session_id);
    TARGET_TOOLS.contains(tool_name) && !state.agent_used
}

fn increment_reminder_count(session_id: &str) {
    let mut state = get_or_create_state(session_id);
    state.reminder_count += 1;
    state.updated_at = now_ms();
    save_agent_usage_state(&state);

    if let Ok(mut states) = SESSION_STATES.write() {
        states.insert(session_id.to_string(), state);
    }
}

pub struct AgentUsageReminderHook;

impl AgentUsageReminderHook {
    pub fn new() -> Self {
        Self
    }
}

impl Default for AgentUsageReminderHook {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Hook for AgentUsageReminderHook {
    fn name(&self) -> &str {
        HOOK_NAME
    }

    fn events(&self) -> &[HookEvent] {
        const EVENTS: &[HookEvent] = &[HookEvent::PostToolUse, HookEvent::Stop];
        EVENTS
    }

    async fn execute(
        &self,
        event: HookEvent,
        input: &HookInput,
        context: &HookContext,
    ) -> HookResult {
        let session_id = input
            .session_id
            .as_deref()
            .or(context.session_id.as_deref());
        let Some(session_id) = session_id else {
            return Ok(HookOutput::pass());
        };

        // Clean up persisted state when a session ends.
        if matches!(event, HookEvent::Stop) {
            reset_state(session_id);
            return Ok(HookOutput::pass());
        }

        let Some(tool_name) = input.tool_name.as_deref() else {
            return Ok(HookOutput::pass());
        };

        let tool_name = normalize_tool_name(tool_name);

        // Mark agent as used if agent tool was called.
        if AGENT_TOOLS.contains(tool_name.as_str()) {
            mark_agent_used(session_id);
            return Ok(HookOutput::pass());
        }

        // Only track target tools (search/fetch tools).
        if !TARGET_TOOLS.contains(tool_name.as_str()) {
            return Ok(HookOutput::pass());
        }

        if should_remind(tool_name.as_str(), session_id) {
            increment_reminder_count(session_id);
            return Ok(HookOutput::continue_with_message(REMINDER_MESSAGE));
        }

        Ok(HookOutput::pass())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    lazy_static! {
        static ref ENV_LOCK: Mutex<()> = Mutex::new(());
    }

    fn clear_in_memory_state() {
        if let Ok(mut states) = SESSION_STATES.write() {
            states.clear();
        }
    }

    #[test]
    fn test_normalize_tool_name() {
        assert_eq!(normalize_tool_name("Grep"), "grep");
        assert_eq!(normalize_tool_name("functions.grep"), "grep");
        assert_eq!(
            normalize_tool_name("context7_query-docs"),
            "context7_query-docs"
        );
    }

    #[test]
    fn test_storage_roundtrip() {
        clear_in_memory_state();

        let _guard = ENV_LOCK.lock().unwrap();

        let dir = tempfile::tempdir().unwrap();
        std::env::set_var("ASTRAPE_ASTRAPE_DIR", dir.path());

        let state = AgentUsageState {
            session_id: "ses_1".to_string(),
            agent_used: false,
            reminder_count: 2,
            updated_at: 123,
        };

        save_agent_usage_state(&state);

        let file_path = get_storage_path("ses_1").unwrap();
        assert!(file_path.exists());

        let loaded = load_agent_usage_state("ses_1").unwrap();
        assert_eq!(loaded, state);

        clear_agent_usage_state("ses_1");
        assert!(load_agent_usage_state("ses_1").is_none());

        std::env::remove_var("ASTRAPE_ASTRAPE_DIR");
    }

    #[tokio::test]
    async fn test_hook_reminds_until_agent_used() {
        clear_in_memory_state();

        let _guard = ENV_LOCK.lock().unwrap();
        let dir = tempfile::tempdir().unwrap();
        std::env::set_var("ASTRAPE_ASTRAPE_DIR", dir.path());

        let hook = AgentUsageReminderHook::new();
        let context = HookContext::new(Some("ses_x".to_string()), "/tmp".to_string());

        let input = HookInput {
            session_id: Some("ses_x".to_string()),
            prompt: None,
            message: None,
            parts: None,
            tool_name: Some("Grep".to_string()),
            tool_input: None,
            tool_output: None,
            directory: None,
            stop_reason: None,
            user_requested: None,
            extra: HashMap::new(),
        };

        let out = hook
            .execute(HookEvent::PostToolUse, &input, &context)
            .await
            .unwrap();
        assert!(out.should_continue);
        assert_eq!(out.message, Some(REMINDER_MESSAGE.to_string()));

        // Mark agent tool used.
        let input_agent = HookInput {
            tool_name: Some("call_omo_agent".to_string()),
            ..input.clone()
        };
        let out = hook
            .execute(HookEvent::PostToolUse, &input_agent, &context)
            .await
            .unwrap();
        assert!(out.message.is_none());

        // After agent used, no more reminders.
        let out = hook
            .execute(HookEvent::PostToolUse, &input, &context)
            .await
            .unwrap();
        assert!(out.message.is_none());

        // Stop clears state
        let out = hook
            .execute(HookEvent::Stop, &input, &context)
            .await
            .unwrap();
        assert!(out.message.is_none());
        assert!(load_agent_usage_state("ses_x").is_none());

        std::env::remove_var("ASTRAPE_ASTRAPE_DIR");
    }
}
