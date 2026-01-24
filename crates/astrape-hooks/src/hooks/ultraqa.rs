//! UltraQA Loop Hook
//!
//! QA cycling workflow that runs test → architect verify → fix → repeat
//! until the QA goal is met or max cycles reached.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

use crate::hook::{Hook, HookContext, HookResult};
use crate::hooks::ralph::RalphHook;
use crate::types::{HookEvent, HookInput, HookOutput};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum UltraQAGoalType {
    Tests,
    Build,
    Lint,
    Typecheck,
    Custom,
}

impl UltraQAGoalType {
    pub fn get_command(&self) -> &'static str {
        match self {
            Self::Tests => "npm test",
            Self::Build => "npm run build",
            Self::Lint => "npm run lint",
            Self::Typecheck => "npm run typecheck || tsc --noEmit",
            Self::Custom => "# Custom command based on goal pattern",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UltraQAState {
    pub active: bool,
    pub goal_type: UltraQAGoalType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub goal_pattern: Option<String>,
    pub cycle: u32,
    pub max_cycles: u32,
    pub failures: Vec<String>,
    pub started_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct UltraQAOptions {
    pub max_cycles: Option<u32>,
    pub custom_pattern: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UltraQAExitReason {
    GoalMet,
    MaxCycles,
    SameFailure,
    EnvError,
    Cancelled,
}

#[derive(Debug, Clone)]
pub struct UltraQAResult {
    pub success: bool,
    pub cycles: u32,
    pub reason: UltraQAExitReason,
    pub diagnosis: Option<String>,
}

const DEFAULT_MAX_CYCLES: u32 = 5;
const SAME_FAILURE_THRESHOLD: usize = 3;

pub struct UltraQAHook;

impl UltraQAHook {
    pub fn new() -> Self {
        Self
    }

    fn get_state_file_path(directory: &str) -> PathBuf {
        Path::new(directory).join(".omc").join("ultraqa-state.json")
    }

    fn ensure_state_dir(directory: &str) -> std::io::Result<()> {
        let omc_dir = Path::new(directory).join(".omc");
        if !omc_dir.exists() {
            fs::create_dir_all(&omc_dir)?;
        }
        Ok(())
    }

    pub fn read_state(directory: &str) -> Option<UltraQAState> {
        let state_file = Self::get_state_file_path(directory);
        if !state_file.exists() {
            return None;
        }

        let content = fs::read_to_string(&state_file).ok()?;
        serde_json::from_str(&content).ok()
    }

    pub fn write_state(directory: &str, state: &UltraQAState) -> bool {
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

    pub fn clear_state(directory: &str) -> bool {
        let state_file = Self::get_state_file_path(directory);
        if !state_file.exists() {
            return true;
        }
        fs::remove_file(&state_file).is_ok()
    }

    pub fn is_ralph_loop_active(directory: &str) -> bool {
        RalphHook::read_state(Some(directory))
            .map(|s| s.active)
            .unwrap_or(false)
    }

    pub fn start(
        directory: &str,
        goal_type: UltraQAGoalType,
        session_id: &str,
        options: Option<UltraQAOptions>,
    ) -> Result<(), String> {
        if Self::is_ralph_loop_active(directory) {
            return Err(
                "Cannot start UltraQA while Ralph Loop is active. Cancel Ralph Loop first."
                    .to_string(),
            );
        }

        let opts = options.unwrap_or_default();
        let state = UltraQAState {
            active: true,
            goal_type,
            goal_pattern: opts.custom_pattern,
            cycle: 1,
            max_cycles: opts.max_cycles.unwrap_or(DEFAULT_MAX_CYCLES),
            failures: Vec::new(),
            started_at: Utc::now(),
            session_id: Some(session_id.to_string()),
        };

        if Self::write_state(directory, &state) {
            Ok(())
        } else {
            Err("Failed to write UltraQA state".to_string())
        }
    }

    pub fn record_failure(
        directory: &str,
        failure_description: &str,
    ) -> Option<(UltraQAState, Option<String>)> {
        let mut state = Self::read_state(directory)?;

        if !state.active {
            return None;
        }

        state.failures.push(failure_description.to_string());

        // Check for repeated failures
        let same_failure_msg = {
            let recent_failures: Vec<_> = state
                .failures
                .iter()
                .rev()
                .take(SAME_FAILURE_THRESHOLD)
                .collect();

            if recent_failures.len() >= SAME_FAILURE_THRESHOLD {
                let first_normalized = Self::normalize_failure(recent_failures[0]);
                let all_same = recent_failures
                    .iter()
                    .all(|f| Self::normalize_failure(f) == first_normalized);

                if all_same {
                    Some(format!(
                        "Same failure detected {} times: {}",
                        SAME_FAILURE_THRESHOLD,
                        recent_failures[0].clone()
                    ))
                } else {
                    None
                }
            } else {
                None
            }
        };

        if let Some(msg) = same_failure_msg {
            return Some((state, Some(msg)));
        }

        state.cycle += 1;

        let max_cycles = state.max_cycles;
        if state.cycle > max_cycles {
            return Some((state, Some(format!("Max cycles ({}) reached", max_cycles))));
        }

        Self::write_state(directory, &state);
        Some((state, None))
    }

    pub fn complete(directory: &str) -> Option<UltraQAResult> {
        let state = Self::read_state(directory)?;

        let result = UltraQAResult {
            success: true,
            cycles: state.cycle,
            reason: UltraQAExitReason::GoalMet,
            diagnosis: None,
        };

        Self::clear_state(directory);
        Some(result)
    }

    pub fn stop(
        directory: &str,
        reason: UltraQAExitReason,
        diagnosis: &str,
    ) -> Option<UltraQAResult> {
        let state = Self::read_state(directory)?;

        let result = UltraQAResult {
            success: false,
            cycles: state.cycle,
            reason,
            diagnosis: Some(diagnosis.to_string()),
        };

        Self::clear_state(directory);
        Some(result)
    }

    pub fn cancel(directory: &str) -> bool {
        Self::clear_state(directory)
    }

    fn normalize_failure(failure: &str) -> String {
        let iso_timestamp = Regex::new(r"\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}").unwrap();
        let line_col = Regex::new(r":\d+:\d+").unwrap();
        let timing = Regex::new(r"\d+ms").unwrap();
        let whitespace = Regex::new(r"\s+").unwrap();

        let mut result = failure.to_string();
        result = iso_timestamp.replace_all(&result, "").to_string();
        result = line_col.replace_all(&result, "").to_string();
        result = timing.replace_all(&result, "").to_string();
        result = whitespace.replace_all(&result, " ").to_string();
        result.trim().to_lowercase()
    }

    pub fn format_progress_message(cycle: u32, max_cycles: u32, status: &str) -> String {
        format!("[ULTRAQA Cycle {}/{}] {}", cycle, max_cycles, status)
    }
}

impl Default for UltraQAHook {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Hook for UltraQAHook {
    fn name(&self) -> &str {
        "ultraqa"
    }

    fn events(&self) -> &[HookEvent] {
        &[HookEvent::UserPromptSubmit, HookEvent::Stop]
    }

    async fn execute(
        &self,
        _event: HookEvent,
        _input: &HookInput,
        _context: &HookContext,
    ) -> HookResult {
        // UltraQA loop processing
        Ok(HookOutput::pass())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_goal_type_command() {
        assert_eq!(UltraQAGoalType::Tests.get_command(), "npm test");
        assert_eq!(UltraQAGoalType::Build.get_command(), "npm run build");
        assert_eq!(UltraQAGoalType::Lint.get_command(), "npm run lint");
    }

    #[test]
    fn test_normalize_failure() {
        let failure = "Error at 2024-01-15T10:30:00 in file:10:20 took 500ms";
        let normalized = UltraQAHook::normalize_failure(failure);
        assert!(!normalized.contains("2024"));
        assert!(!normalized.contains("500ms"));
        assert!(!normalized.contains(":10:20"));
    }

    #[test]
    fn test_format_progress_message() {
        let msg = UltraQAHook::format_progress_message(2, 5, "Running tests");
        assert_eq!(msg, "[ULTRAQA Cycle 2/5] Running tests");
    }
}
