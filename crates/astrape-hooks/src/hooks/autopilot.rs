//! Autopilot Hook - Autonomous Phase Orchestrator
//!
//! A persistent state machine that enforces autonomous execution across phases.
//! State is persisted to `.omc/autopilot-state.json` for crash recovery.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

use crate::hook::{Hook, HookContext, HookResult};
use crate::types::{HookEvent, HookInput, HookOutput};

pub const AUTOPILOT_STATE_FILE: &str = "autopilot-state.json";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AutopilotPhase {
    Idle,
    Planning,
    Executing,
    Verifying,
    Complete,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutopilotConfig {
    #[serde(default = "default_max_iterations")]
    pub max_iterations: u32,
}

fn default_max_iterations() -> u32 {
    10
}

impl Default for AutopilotConfig {
    fn default() -> Self {
        Self {
            max_iterations: default_max_iterations(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutopilotState {
    pub active: bool,
    pub phase: AutopilotPhase,
    pub iteration: u32,
    pub max_iterations: u32,
    pub original_task: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub plan_path: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,

    pub started_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<DateTime<Utc>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_error: Option<String>,
}

impl AutopilotState {
    pub fn new(task: String, session_id: Option<String>, config: AutopilotConfig) -> Self {
        let now = Utc::now();
        Self {
            active: true,
            phase: AutopilotPhase::Planning,
            iteration: 1,
            max_iterations: config.max_iterations,
            original_task: task,
            plan_path: None,
            session_id,
            started_at: now,
            updated_at: now,
            completed_at: None,
            last_error: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AutopilotSignal {
    PlanningComplete,
    ExecutionComplete,
    VerifyingComplete,
    AutopilotComplete,
    AutopilotCancelled,
}

impl AutopilotSignal {
    fn as_str(&self) -> &'static str {
        match self {
            Self::PlanningComplete => "PLANNING_COMPLETE",
            Self::ExecutionComplete => "EXECUTION_COMPLETE",
            Self::VerifyingComplete => "VERIFYING_COMPLETE",
            Self::AutopilotComplete => "AUTOPILOT_COMPLETE",
            Self::AutopilotCancelled => "AUTOPILOT_CANCELLED",
        }
    }
}

pub fn expected_signal_for_phase(phase: AutopilotPhase) -> Option<AutopilotSignal> {
    match phase {
        AutopilotPhase::Planning => Some(AutopilotSignal::PlanningComplete),
        AutopilotPhase::Executing => Some(AutopilotSignal::ExecutionComplete),
        AutopilotPhase::Verifying => Some(AutopilotSignal::AutopilotComplete),
        _ => None,
    }
}

pub fn validate_transition(from: AutopilotPhase, to: AutopilotPhase) -> bool {
    matches!(
        (from, to),
        (AutopilotPhase::Idle, AutopilotPhase::Planning)
            | (AutopilotPhase::Planning, AutopilotPhase::Executing)
            | (AutopilotPhase::Executing, AutopilotPhase::Verifying)
            | (AutopilotPhase::Verifying, AutopilotPhase::Complete)
            | (_, AutopilotPhase::Failed)
            | (_, AutopilotPhase::Cancelled)
    )
}

pub fn validate_state(state: &AutopilotState) -> Result<(), String> {
    if state.max_iterations == 0 {
        return Err("max_iterations must be > 0".to_string());
    }
    if state.iteration == 0 {
        return Err("iteration must be >= 1".to_string());
    }
    if state.original_task.trim().is_empty() {
        return Err("original_task must be non-empty".to_string());
    }
    Ok(())
}

pub fn detect_signal(text: &str, signal: AutopilotSignal) -> bool {
    // Simple case-insensitive contains; avoids regex lookarounds.
    let needle = signal.as_str();
    text.to_ascii_uppercase().contains(needle)
}

pub fn detect_any_signal(text: &str) -> Option<AutopilotSignal> {
    // Order matters: completion/cancel should win.
    let signals = [
        AutopilotSignal::AutopilotCancelled,
        AutopilotSignal::AutopilotComplete,
        AutopilotSignal::VerifyingComplete,
        AutopilotSignal::ExecutionComplete,
        AutopilotSignal::PlanningComplete,
    ];

    signals.into_iter().find(|&s| detect_signal(text, s))
}

pub struct AutopilotHook {
    _default_config: AutopilotConfig,
}

impl AutopilotHook {
    pub fn new() -> Self {
        Self {
            _default_config: AutopilotConfig::default(),
        }
    }

    pub fn with_config(config: AutopilotConfig) -> Self {
        Self {
            _default_config: config,
        }
    }

    fn get_state_file_path(directory: &str) -> PathBuf {
        Path::new(directory).join(".omc").join(AUTOPILOT_STATE_FILE)
    }

    fn ensure_state_dir(directory: &str) -> std::io::Result<()> {
        let omc_dir = Path::new(directory).join(".omc");
        if !omc_dir.exists() {
            fs::create_dir_all(&omc_dir)?;
        }
        Ok(())
    }

    pub fn read_state(directory: &str) -> Option<AutopilotState> {
        let state_file = Self::get_state_file_path(directory);
        if !state_file.exists() {
            return None;
        }

        let content = fs::read_to_string(&state_file).ok()?;
        serde_json::from_str(&content).ok()
    }

    pub fn write_state(directory: &str, state: &AutopilotState) -> bool {
        if Self::ensure_state_dir(directory).is_err() {
            return false;
        }
        if validate_state(state).is_err() {
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

    pub fn is_active(directory: &str) -> bool {
        Self::read_state(directory)
            .map(|s| s.active)
            .unwrap_or(false)
    }

    pub fn start(
        directory: &str,
        task: &str,
        session_id: Option<String>,
        config: Option<AutopilotConfig>,
    ) -> Result<AutopilotState, String> {
        let merged_config = config.unwrap_or_default();
        let state = AutopilotState::new(task.to_string(), session_id, merged_config);
        if !Self::write_state(directory, &state) {
            return Err(format!(
                "Failed to persist autopilot state to {}",
                directory
            ));
        }
        Ok(state)
    }

    pub fn transition(directory: &str, to: AutopilotPhase) -> Result<AutopilotState, String> {
        let mut state = Self::read_state(directory).ok_or_else(|| "no state".to_string())?;
        if !state.active {
            return Err("autopilot not active".to_string());
        }
        if !validate_transition(state.phase, to) {
            return Err(format!("invalid transition: {:?} -> {:?}", state.phase, to));
        }

        state.phase = to;
        state.updated_at = Utc::now();
        if matches!(
            to,
            AutopilotPhase::Complete | AutopilotPhase::Failed | AutopilotPhase::Cancelled
        ) {
            state.active = false;
            state.completed_at = Some(state.updated_at);
        }

        if Self::write_state(directory, &state) {
            Ok(state)
        } else {
            Err("failed to persist state".to_string())
        }
    }

    pub fn fail(directory: &str, error: impl Into<String>) -> Option<AutopilotState> {
        let mut state = Self::read_state(directory)?;
        if !state.active {
            return None;
        }
        state.last_error = Some(error.into());
        state.phase = AutopilotPhase::Failed;
        state.active = false;
        state.updated_at = Utc::now();
        state.completed_at = Some(state.updated_at);
        Self::write_state(directory, &state).then_some(state)
    }

    pub fn cancel(directory: &str, reason: Option<&str>) -> Option<AutopilotState> {
        let mut state = Self::read_state(directory)?;
        if !state.active {
            return None;
        }
        if let Some(r) = reason {
            state.last_error = Some(r.to_string());
        }
        state.phase = AutopilotPhase::Cancelled;
        state.active = false;
        state.updated_at = Utc::now();
        state.completed_at = Some(state.updated_at);
        Self::write_state(directory, &state).then_some(state)
    }

    fn increment_iteration(directory: &str) -> Option<AutopilotState> {
        let mut state = Self::read_state(directory)?;
        if !state.active {
            return None;
        }
        state.iteration += 1;
        state.updated_at = Utc::now();
        Self::write_state(directory, &state).then_some(state)
    }

    fn get_phase_prompt(state: &AutopilotState) -> String {
        match state.phase {
            AutopilotPhase::Planning => format!(
                "## AUTOPILOT PHASE: PLANNING\n\nOriginal task:\n{}\n\nWhen the plan is finished, output: PLANNING_COMPLETE\n",
                state.original_task
            ),
            AutopilotPhase::Executing => {
                "## AUTOPILOT PHASE: EXECUTING\n\nExecute the plan and implement the task.\n\nWhen implementation is finished, output: EXECUTION_COMPLETE\n".to_string()
            }
            AutopilotPhase::Verifying => {
                "## AUTOPILOT PHASE: VERIFYING\n\nRun verification (tests/build/lint as applicable).\n\nWhen fully verified, output: AUTOPILOT_COMPLETE\n".to_string()
            }
            _ => String::new(),
        }
    }

    fn continuation_prompt(state: &AutopilotState) -> String {
        format!(
            r#"<autopilot-continuation>

[AUTOPILOT - PHASE: {:?} | ITERATION {}/{}]

Your previous response did not signal phase completion. Continue working.

{}

</autopilot-continuation>

---

"#,
            state.phase,
            state.iteration,
            state.max_iterations,
            Self::get_phase_prompt(state)
        )
    }
}

impl Default for AutopilotHook {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Hook for AutopilotHook {
    fn name(&self) -> &str {
        "autopilot"
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
        let mut state = match Self::read_state(&context.directory) {
            Some(s) => s,
            None => return Ok(HookOutput::pass()),
        };

        if !state.active {
            return Ok(HookOutput::pass());
        }

        // Check session binding
        if let (Some(bound_sid), Some(sid)) = (&state.session_id, &input.session_id) {
            if bound_sid != sid {
                return Ok(HookOutput::pass());
            }
        }

        // Cancellation via explicit signal in the assistant output.
        let prompt_text = input.get_prompt_text();
        if detect_signal(&prompt_text, AutopilotSignal::AutopilotCancelled) {
            Self::cancel(&context.directory, Some("cancelled by signal"));
            return Ok(HookOutput::continue_with_message(
                "[AUTOPILOT CANCELLED] Session cancelled; progress preserved in .omc/autopilot-state.json",
            ));
        }

        // Phase advancement based on signal in the assistant output.
        if let Some(expected) = expected_signal_for_phase(state.phase) {
            if detect_signal(&prompt_text, expected)
                || (state.phase == AutopilotPhase::Verifying
                    && detect_signal(&prompt_text, AutopilotSignal::VerifyingComplete))
            {
                let next = match state.phase {
                    AutopilotPhase::Planning => AutopilotPhase::Executing,
                    AutopilotPhase::Executing => AutopilotPhase::Verifying,
                    AutopilotPhase::Verifying => AutopilotPhase::Complete,
                    _ => state.phase,
                };

                let _ = Self::transition(&context.directory, next);
                state = match Self::read_state(&context.directory) {
                    Some(s) => s,
                    None => return Ok(HookOutput::pass()),
                };

                if state.phase == AutopilotPhase::Complete {
                    return Ok(HookOutput::continue_with_message(
                        "[AUTOPILOT COMPLETE] All phases finished successfully.",
                    ));
                }
            }
        }

        // Safety limit.
        if state.iteration >= state.max_iterations {
            Self::fail(
                &context.directory,
                format!("max iterations ({}) reached", state.max_iterations),
            );
            return Ok(HookOutput::continue_with_message(format!(
                "[AUTOPILOT STOPPED] Max iterations ({}) reached. State preserved in .omc/autopilot-state.json",
                state.max_iterations
            )));
        }

        // Continue current phase.
        let new_state = match Self::increment_iteration(&context.directory) {
            Some(s) => s,
            None => return Ok(HookOutput::pass()),
        };

        Ok(HookOutput::block_with_reason(Self::continuation_prompt(
            &new_state,
        )))
    }

    fn priority(&self) -> i32 {
        90
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let cfg = AutopilotConfig::default();
        assert_eq!(cfg.max_iterations, 10);
    }

    #[test]
    fn test_validate_transition() {
        assert!(validate_transition(
            AutopilotPhase::Planning,
            AutopilotPhase::Executing
        ));
        assert!(!validate_transition(
            AutopilotPhase::Planning,
            AutopilotPhase::Verifying
        ));
        assert!(validate_transition(
            AutopilotPhase::Executing,
            AutopilotPhase::Failed
        ));
    }

    #[test]
    fn test_detect_signal_case_insensitive() {
        assert!(detect_signal(
            "planning_complete",
            AutopilotSignal::PlanningComplete
        ));
        assert!(detect_signal(
            "... EXECUTION_COMPLETE ...",
            AutopilotSignal::ExecutionComplete
        ));
        assert!(!detect_signal(
            "no signals here",
            AutopilotSignal::AutopilotComplete
        ));
    }

    #[test]
    fn test_state_persistence_and_transitions() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_str().unwrap();

        assert!(!AutopilotHook::is_active(root));

        let state = AutopilotHook::start(root, "build feature", None, None).unwrap();
        assert!(state.active);
        assert_eq!(state.phase, AutopilotPhase::Planning);
        assert!(AutopilotHook::is_active(root));

        let loaded = AutopilotHook::read_state(root).unwrap();
        assert_eq!(loaded.original_task, "build feature");

        let state = AutopilotHook::transition(root, AutopilotPhase::Executing).unwrap();
        assert_eq!(state.phase, AutopilotPhase::Executing);

        let state = AutopilotHook::transition(root, AutopilotPhase::Verifying).unwrap();
        assert_eq!(state.phase, AutopilotPhase::Verifying);

        let state = AutopilotHook::transition(root, AutopilotPhase::Complete).unwrap();
        assert_eq!(state.phase, AutopilotPhase::Complete);
        assert!(!state.active);
        assert!(state.completed_at.is_some());
        assert!(!AutopilotHook::is_active(root));
    }

    #[test]
    fn test_cancel() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_str().unwrap();

        AutopilotHook::start(root, "task", None, None).unwrap();
        let state = AutopilotHook::cancel(root, Some("user request")).unwrap();
        assert_eq!(state.phase, AutopilotPhase::Cancelled);
        assert!(!state.active);
        assert_eq!(state.last_error.as_deref(), Some("user request"));
    }
}
