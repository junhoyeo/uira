//! Ultrawork State Management Hook
//!
//! Manages persistent ultrawork mode state across sessions.
//! When ultrawork is activated and todos remain incomplete,
//! this module ensures the mode persists until all work is done.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

use crate::hook::{Hook, HookContext, HookResult};
use crate::hooks::ralph::RalphHook;
use crate::hooks::todo_continuation::TodoContinuationHook;
use crate::types::{HookEvent, HookInput, HookOutput};

/// Ultrawork mode state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UltraworkState {
    /// Whether ultrawork mode is currently active
    pub active: bool,
    /// When ultrawork was activated
    pub started_at: DateTime<Utc>,
    /// The original prompt that triggered ultrawork
    pub original_prompt: String,
    /// Session ID the mode is bound to
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    /// Number of times the mode has been reinforced
    pub reinforcement_count: u32,
    /// Last time the mode was checked/reinforced
    pub last_checked_at: DateTime<Utc>,
    /// Whether this ultrawork session is linked to a ralph-loop session
    #[serde(skip_serializing_if = "Option::is_none")]
    pub linked_to_ralph: Option<bool>,
}

impl Default for UltraworkState {
    fn default() -> Self {
        Self {
            active: false,
            started_at: Utc::now(),
            original_prompt: String::new(),
            session_id: None,
            reinforcement_count: 0,
            last_checked_at: Utc::now(),
            linked_to_ralph: None,
        }
    }
}

/// Ultrawork hook for managing persistent work mode
pub struct UltraworkHook;

impl UltraworkHook {
    pub fn new() -> Self {
        Self
    }

    /// Get the state file path for Ultrawork (local)
    fn get_state_file_path(directory: &str) -> PathBuf {
        Path::new(directory)
            .join(".astrape")
            .join("ultrawork-state.json")
    }

    /// Get global state file path (for cross-session persistence)
    fn get_global_state_file_path() -> Option<PathBuf> {
        dirs::home_dir().map(|h| h.join(".claude").join("ultrawork-state.json"))
    }

    /// Ensure the .astrape directory exists
    fn ensure_state_dir(directory: &str) -> std::io::Result<()> {
        let astrape_dir = Path::new(directory).join(".astrape");
        if !astrape_dir.exists() {
            fs::create_dir_all(&astrape_dir)?;
        }
        Ok(())
    }

    /// Ensure the ~/.claude directory exists
    fn ensure_global_state_dir() -> std::io::Result<()> {
        if let Some(claude_dir) = dirs::home_dir().map(|h| h.join(".claude")) {
            if !claude_dir.exists() {
                fs::create_dir_all(&claude_dir)?;
            }
        }
        Ok(())
    }

    /// Read Ultrawork state from disk (checks both local and global)
    pub fn read_state(directory: Option<&str>) -> Option<UltraworkState> {
        // Check local state first
        if let Some(dir) = directory {
            let local_state_file = Self::get_state_file_path(dir);
            if local_state_file.exists() {
                if let Ok(content) = fs::read_to_string(&local_state_file) {
                    if let Ok(state) = serde_json::from_str(&content) {
                        return Some(state);
                    }
                }
            }
        }

        // Check global state
        if let Some(global_state_file) = Self::get_global_state_file_path() {
            if global_state_file.exists() {
                if let Ok(content) = fs::read_to_string(&global_state_file) {
                    if let Ok(state) = serde_json::from_str(&content) {
                        return Some(state);
                    }
                }
            }
        }

        None
    }

    /// Write Ultrawork state to disk (both local and global for redundancy)
    pub fn write_state(state: &UltraworkState, directory: Option<&str>) -> bool {
        let mut success = false;

        // Write to local .astrape
        if let Some(dir) = directory {
            if Self::ensure_state_dir(dir).is_ok() {
                let local_state_file = Self::get_state_file_path(dir);
                if let Ok(content) = serde_json::to_string_pretty(state) {
                    if fs::write(&local_state_file, content).is_ok() {
                        success = true;
                    }
                }
            }
        }

        // Write to global ~/.claude for cross-session persistence
        if Self::ensure_global_state_dir().is_ok() {
            if let Some(global_state_file) = Self::get_global_state_file_path() {
                if let Ok(content) = serde_json::to_string_pretty(state) {
                    if fs::write(&global_state_file, content).is_ok() {
                        success = true;
                    }
                }
            }
        }

        success
    }

    /// Activate ultrawork mode
    pub fn activate(
        prompt: &str,
        session_id: Option<&str>,
        directory: Option<&str>,
        linked_to_ralph: Option<bool>,
    ) -> bool {
        let state = UltraworkState {
            active: true,
            started_at: Utc::now(),
            original_prompt: prompt.to_string(),
            session_id: session_id.map(|s| s.to_string()),
            reinforcement_count: 0,
            last_checked_at: Utc::now(),
            linked_to_ralph,
        };

        Self::write_state(&state, directory)
    }

    /// Deactivate ultrawork mode
    pub fn deactivate(directory: Option<&str>) -> bool {
        // Remove local state
        if let Some(dir) = directory {
            let local_state_file = Self::get_state_file_path(dir);
            if local_state_file.exists() {
                let _ = fs::remove_file(&local_state_file);
            }
        }

        // Remove global state
        if let Some(global_state_file) = Self::get_global_state_file_path() {
            if global_state_file.exists() {
                return fs::remove_file(&global_state_file).is_ok();
            }
        }

        true
    }

    /// Increment reinforcement count
    pub fn increment_reinforcement(directory: Option<&str>) -> Option<UltraworkState> {
        let mut state = Self::read_state(directory)?;

        if !state.active {
            return None;
        }

        state.reinforcement_count += 1;
        state.last_checked_at = Utc::now();

        if Self::write_state(&state, directory) {
            Some(state)
        } else {
            None
        }
    }

    /// Check if ultrawork should defer to ralph
    pub fn should_defer_to_ralph(directory: Option<&str>) -> bool {
        if let Some(state) = Self::read_state(directory) {
            if state.linked_to_ralph == Some(true) {
                // Check if ralph is still active
                if RalphHook::read_state(directory)
                    .map(|s| s.active)
                    .unwrap_or(false)
                {
                    return true;
                }
            }
        }
        false
    }

    /// Check if ultrawork should be reinforced
    pub fn should_reinforce(session_id: Option<&str>, directory: Option<&str>) -> bool {
        let state = match Self::read_state(directory) {
            Some(s) => s,
            None => return false,
        };

        if !state.active {
            return false;
        }

        // If bound to a session, only reinforce for that session
        if let (Some(state_sid), Some(sid)) = (&state.session_id, session_id) {
            if state_sid != sid {
                return false;
            }
        }

        true
    }

    /// Get ultrawork persistence message for injection
    pub fn get_persistence_message(state: &UltraworkState) -> String {
        format!(
            r#"<ultrawork-persistence>

[ULTRAWORK MODE STILL ACTIVE - Reinforcement #{}]

Your ultrawork session is NOT complete. Incomplete todos remain.

REMEMBER THE ULTRAWORK RULES:
- **PARALLEL**: Fire independent delegate_task calls simultaneously - NEVER wait sequentially
- **BACKGROUND FIRST**: Use delegate_task for exploration (10+ concurrent)
- **TODO**: Track EVERY step. Mark complete IMMEDIATELY after each
- **VERIFY**: Check ALL requirements met before done
- **NO Premature Stopping**: ALL TODOs must be complete

Continue working on the next pending task. DO NOT STOP until all tasks are marked complete.

Original task: {}

</ultrawork-persistence>

---

"#,
            state.reinforcement_count + 1,
            state.original_prompt
        )
    }
}

impl Default for UltraworkHook {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Hook for UltraworkHook {
    fn name(&self) -> &str {
        "ultrawork"
    }

    fn events(&self) -> &[HookEvent] {
        &[HookEvent::Stop]
    }

    async fn execute(
        &self,
        _event: HookEvent,
        input: &HookInput,
        context: &HookContext,
    ) -> HookResult {
        let state = match Self::read_state(Some(&context.directory)) {
            Some(s) => s,
            None => return Ok(HookOutput::pass()),
        };

        if !state.active {
            return Ok(HookOutput::pass());
        }

        // If bound to a session, only reinforce for that session
        if let (Some(state_sid), Some(sid)) = (&state.session_id, &input.session_id) {
            if state_sid != sid {
                return Ok(HookOutput::pass());
            }
        }

        // Check if there are incomplete todos
        let todo_result = TodoContinuationHook::check_incomplete_todos(
            input.session_id.as_deref(),
            &context.directory,
            None,
        );

        if todo_result.count == 0 {
            // No incomplete todos, ultrawork can complete
            Self::deactivate(Some(&context.directory));
            return Ok(HookOutput::continue_with_message(
                "[ULTRAWORK COMPLETE] All tasks finished. Ultrawork mode deactivated. Well done!",
            ));
        }

        // Reinforce ultrawork mode
        let new_state = match Self::increment_reinforcement(Some(&context.directory)) {
            Some(s) => s,
            None => return Ok(HookOutput::pass()),
        };

        let message = Self::get_persistence_message(&new_state);
        Ok(HookOutput::block_with_reason(message))
    }

    fn priority(&self) -> i32 {
        75 // Higher than todo-continuation (50), lower than ralph (100)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ultrawork_state_default() {
        let state = UltraworkState::default();
        assert!(!state.active);
        assert!(state.original_prompt.is_empty());
        assert_eq!(state.reinforcement_count, 0);
    }

    #[test]
    fn test_get_persistence_message() {
        let state = UltraworkState {
            active: true,
            started_at: Utc::now(),
            original_prompt: "Build the feature".to_string(),
            session_id: None,
            reinforcement_count: 2,
            last_checked_at: Utc::now(),
            linked_to_ralph: None,
        };

        let message = UltraworkHook::get_persistence_message(&state);
        assert!(message.contains("ULTRAWORK MODE STILL ACTIVE"));
        assert!(message.contains("Reinforcement #3"));
        assert!(message.contains("Build the feature"));
    }
}
