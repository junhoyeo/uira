//! Ralph Hook - Self-Referential Work Loop
//!
//! Self-referential work loop that continues until a completion promise is detected.
//! Ralph ensures tasks are completed by requiring explicit completion markers.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

use crate::hook::{Hook, HookContext, HookResult};
use crate::hooks::ultrawork::UltraworkHook;
use crate::types::{HookEvent, HookInput, HookOutput};

/// Ralph loop state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RalphState {
    /// Whether ralph mode is currently active
    pub active: bool,
    /// Current iteration count
    pub iteration: u32,
    /// Maximum iterations before giving up
    pub max_iterations: u32,
    /// The completion promise to look for
    pub completion_promise: String,
    /// Session ID the mode is bound to
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    /// The original prompt
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt: Option<String>,
    /// When ralph was started
    pub started_at: DateTime<Utc>,
    /// Last checked timestamp
    pub last_checked_at: DateTime<Utc>,
}

impl Default for RalphState {
    fn default() -> Self {
        Self {
            active: false,
            iteration: 0,
            max_iterations: 10,
            completion_promise: "TASK COMPLETE".to_string(),
            session_id: None,
            prompt: None,
            started_at: Utc::now(),
            last_checked_at: Utc::now(),
        }
    }
}

/// Ralph hook options
#[derive(Debug, Clone)]
pub struct RalphOptions {
    pub max_iterations: u32,
    pub completion_promise: String,
}

impl Default for RalphOptions {
    fn default() -> Self {
        Self {
            max_iterations: 10,
            completion_promise: "TASK COMPLETE".to_string(),
        }
    }
}

/// Ralph Hook for self-referential work loops
pub struct RalphHook {
    default_options: RalphOptions,
}

impl RalphHook {
    pub fn new() -> Self {
        Self {
            default_options: RalphOptions::default(),
        }
    }

    pub fn with_options(options: RalphOptions) -> Self {
        Self {
            default_options: options,
        }
    }

    pub fn default_options(&self) -> &RalphOptions {
        &self.default_options
    }

    /// Get the state file path for Ralph
    fn get_state_file_path(directory: &str) -> PathBuf {
        Path::new(directory)
            .join(".astrape")
            .join("ralph-state.json")
    }

    /// Get global state file path
    fn get_global_state_file_path() -> Option<PathBuf> {
        dirs::home_dir().map(|h| h.join(".claude").join("ralph-state.json"))
    }

    /// Ensure the .astrape directory exists
    fn ensure_state_dir(directory: &str) -> std::io::Result<()> {
        let astrape_dir = Path::new(directory).join(".astrape");
        if !astrape_dir.exists() {
            fs::create_dir_all(&astrape_dir)?;
        }
        Ok(())
    }

    /// Read Ralph state from disk
    pub fn read_state(directory: Option<&str>) -> Option<RalphState> {
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

    /// Write Ralph state to disk
    pub fn write_state(state: &RalphState, directory: Option<&str>) -> bool {
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

        // Write to global
        if let Some(global_state_file) = Self::get_global_state_file_path() {
            if let Some(parent) = global_state_file.parent() {
                let _ = fs::create_dir_all(parent);
            }
            if let Ok(content) = serde_json::to_string_pretty(state) {
                if fs::write(&global_state_file, content).is_ok() {
                    success = true;
                }
            }
        }

        success
    }

    /// Clear Ralph state
    pub fn clear_state(directory: Option<&str>) -> bool {
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

    /// Activate ralph mode
    pub fn activate(
        prompt: &str,
        session_id: Option<&str>,
        directory: Option<&str>,
        options: Option<RalphOptions>,
    ) -> bool {
        let opts = options.unwrap_or_default();
        let state = RalphState {
            active: true,
            iteration: 0,
            max_iterations: opts.max_iterations,
            completion_promise: opts.completion_promise,
            session_id: session_id.map(|s| s.to_string()),
            prompt: Some(prompt.to_string()),
            started_at: Utc::now(),
            last_checked_at: Utc::now(),
        };

        Self::write_state(&state, directory)
    }

    /// Increment Ralph iteration
    pub fn increment_iteration(directory: Option<&str>) -> Option<RalphState> {
        let mut state = Self::read_state(directory)?;

        if !state.active {
            return None;
        }

        state.iteration += 1;
        state.last_checked_at = Utc::now();

        if Self::write_state(&state, directory) {
            Some(state)
        } else {
            None
        }
    }

    /// Detect completion promise in text
    pub fn detect_completion_promise(text: &str, promise: &str) -> bool {
        // Check for <promise>...</promise> tags
        let promise_pattern = format!(r"<promise>\s*{}\s*</promise>", regex::escape(promise));
        if let Ok(re) = Regex::new(&promise_pattern) {
            if re.is_match(text) {
                return true;
            }
        }

        // Check for plain text match
        text.contains(promise)
    }

    /// Get continuation prompt
    fn get_continuation_prompt(state: &RalphState) -> String {
        format!(
            r#"<ralph-continuation>

[RALPH - ITERATION {}/{}]

Your previous attempt did not output the completion promise. The work is NOT done yet.

CRITICAL INSTRUCTIONS:
1. Review your progress and the original task
2. Check your todo list - are ALL items marked complete?
3. Continue from where you left off
4. When FULLY complete, output: <promise>{}</promise>
5. Do NOT stop until the task is truly done

{}

</ralph-continuation>

---

"#,
            state.iteration,
            state.max_iterations,
            state.completion_promise,
            state
                .prompt
                .as_ref()
                .map(|p| format!("Original task: {}", p))
                .unwrap_or_default()
        )
    }
}

impl Default for RalphHook {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Hook for RalphHook {
    fn name(&self) -> &str {
        "ralph"
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

        // Check if this is the right session
        if let (Some(state_sid), Some(sid)) = (&state.session_id, &input.session_id) {
            if state_sid != sid {
                return Ok(HookOutput::pass());
            }
        }

        // TODO: Check transcript for completion promise
        // For now, we check if max iterations reached
        if state.iteration >= state.max_iterations {
            // Clear both ralph and linked ultrawork
            Self::clear_state(Some(&context.directory));
            UltraworkHook::deactivate(Some(&context.directory));
            return Ok(HookOutput::continue_with_message(format!(
                "[RALPH LOOP STOPPED] Max iterations ({}) reached without completion promise. Consider reviewing the task requirements.",
                state.max_iterations
            )));
        }

        // Increment and continue
        let new_state = match Self::increment_iteration(Some(&context.directory)) {
            Some(s) => s,
            None => return Ok(HookOutput::pass()),
        };

        let message = Self::get_continuation_prompt(&new_state);
        Ok(HookOutput::block_with_reason(message))
    }

    fn priority(&self) -> i32 {
        100 // Highest priority - ralph takes precedence
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ralph_state_default() {
        let state = RalphState::default();
        assert!(!state.active);
        assert_eq!(state.iteration, 0);
        assert_eq!(state.max_iterations, 10);
        assert_eq!(state.completion_promise, "TASK COMPLETE");
    }

    #[test]
    fn test_detect_completion_promise() {
        // Test with tags
        assert!(RalphHook::detect_completion_promise(
            "Some text <promise>TASK COMPLETE</promise> more text",
            "TASK COMPLETE"
        ));

        // Test with whitespace in tags
        assert!(RalphHook::detect_completion_promise(
            "<promise> TASK COMPLETE </promise>",
            "TASK COMPLETE"
        ));

        // Test plain text
        assert!(RalphHook::detect_completion_promise(
            "The work is done: TASK COMPLETE",
            "TASK COMPLETE"
        ));

        // Test no match
        assert!(!RalphHook::detect_completion_promise(
            "Still working on it",
            "TASK COMPLETE"
        ));
    }

    #[test]
    fn test_get_continuation_prompt() {
        let state = RalphState {
            active: true,
            iteration: 3,
            max_iterations: 10,
            completion_promise: "DONE".to_string(),
            session_id: None,
            prompt: Some("Build the feature".to_string()),
            started_at: Utc::now(),
            last_checked_at: Utc::now(),
        };

        let prompt = RalphHook::get_continuation_prompt(&state);
        assert!(prompt.contains("ITERATION 3/10"));
        assert!(prompt.contains("<promise>DONE</promise>"));
        assert!(prompt.contains("Build the feature"));
    }
}
