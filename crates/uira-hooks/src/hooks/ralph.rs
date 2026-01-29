//! Ralph Hook - Self-Referential Work Loop
//!
//! A sophisticated work loop system that continues until verified completion.
//! Ralph ensures tasks are completed by requiring explicit completion markers
//! and multi-layer verification.
//!
//! # Features
//!
//! - **Dual-Condition Exit**: Requires both subjective intent (promise/EXIT_SIGNAL)
//!   and objective completion (tests passing, todos complete, etc.)
//! - **Circuit Breaker**: Prevents infinite loops via stagnation and error detection
//! - **Progress Tracking**: Accumulates learnings and metrics across iterations
//! - **Session Management**: 24-hour expiration, branch change detection
//!
//! # Usage
//!
//! ```rust,ignore
//! use uira_hooks::hooks::ralph::{RalphHook, RalphOptions};
//!
//! // Activate ralph with default options
//! RalphHook::activate("Complete the feature", Some("session-123"), Some("/project"), None);
//!
//! // Or with custom options
//! let options = RalphOptions {
//!     max_iterations: 15,
//!     min_confidence: 60,
//!     ..Default::default()
//! };
//! RalphHook::activate("Refactor auth", None, Some("/project"), Some(options));
//! ```
//!
//! # Completion Protocol
//!
//! Ralph uses a dual-condition exit gate:
//!
//! 1. **Subjective Intent**: The agent must explicitly signal completion via:
//!    - `<promise>TASK COMPLETE</promise>` token
//!    - `EXIT_SIGNAL: true` in RALPH_STATUS block
//!
//! 2. **Objective Signals** (need 2+):
//!    - Tests passing
//!    - Build successful
//!    - All todos complete
//!    - Completion keywords in output
//!
//! # Circuit Breaker
//!
//! The circuit breaker protects against infinite loops:
//!
//! - **No Progress**: 3 consecutive iterations with no file changes
//! - **Same Error**: 5 occurrences of the same error
//! - **Output Decline**: 70% reduction in output size
//!
//! # State Files
//!
//! - `.uira/ralph-state.json`: Current session state
//! - `.uira/ralph-progress.json`: Iteration history and metrics
//! - `.uira/ralph-archives/`: Archived sessions (on branch change)

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

use crate::hook::{Hook, HookContext, HookResult};
use crate::hooks::circuit_breaker::{CircuitBreakerConfig, CircuitBreakerState};
use crate::hooks::todo_continuation::{IncompleteTodosResult, TodoContinuationHook};
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
    /// Minimum confidence score to allow exit
    #[serde(default = "default_min_confidence")]
    pub min_confidence: u32,
    /// Whether to require dual-condition for exit
    #[serde(default = "default_require_dual_condition")]
    pub require_dual_condition: bool,
    /// Session expiration duration in hours
    #[serde(default = "default_session_hours")]
    pub session_hours: u32,
    /// Git branch when ralph started
    #[serde(skip_serializing_if = "Option::is_none")]
    pub git_branch: Option<String>,
    /// Circuit breaker for stagnation detection
    #[serde(default)]
    pub circuit_breaker: CircuitBreakerState,
    /// Circuit breaker configuration
    #[serde(default)]
    pub circuit_config: CircuitBreakerConfig,
}

fn default_min_confidence() -> u32 {
    50
}
fn default_require_dual_condition() -> bool {
    true
}
fn default_session_hours() -> u32 {
    24
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
            min_confidence: 50,
            require_dual_condition: true,
            session_hours: 24,
            git_branch: None,
            circuit_breaker: CircuitBreakerState::default(),
            circuit_config: CircuitBreakerConfig::default(),
        }
    }
}

/// Ralph hook options
#[derive(Debug, Clone)]
pub struct RalphOptions {
    pub max_iterations: u32,
    pub completion_promise: String,
    /// Minimum confidence score to allow exit (0-100)
    pub min_confidence: u32,
    /// Whether to require dual-condition (default: true)
    pub require_dual_condition: bool,
}

impl Default for RalphOptions {
    fn default() -> Self {
        Self {
            max_iterations: 10,
            completion_promise: "TASK COMPLETE".to_string(),
            min_confidence: 50,
            require_dual_condition: true,
        }
    }
}

/// Progress accumulation across iterations
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RalphProgress {
    /// Iteration-by-iteration history
    pub iterations: Vec<IterationRecord>,
    /// Cumulative file modifications
    pub files_modified: Vec<String>,
    /// Accumulated learnings
    pub learnings: Vec<String>,
    /// Encountered blockers
    pub blockers: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IterationRecord {
    pub iteration: u32,
    pub timestamp: DateTime<Utc>,
    pub output_size: usize,
    pub confidence: u32,
}

impl RalphProgress {
    pub fn add_iteration(&mut self, record: IterationRecord) {
        self.iterations.push(record);
    }

    pub fn add_file(&mut self, file: &str) {
        if !self.files_modified.contains(&file.to_string()) {
            self.files_modified.push(file.to_string());
        }
    }

    pub fn add_learning(&mut self, learning: &str) {
        self.learnings.push(learning.to_string());
    }

    pub fn get_summary(&self) -> String {
        format!(
            "Iterations: {}, Files: {}, Learnings: {}",
            self.iterations.len(),
            self.files_modified.len(),
            self.learnings.len()
        )
    }
}

/// Confidence score and signals for completion detection
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CompletionSignals {
    /// Confidence score (0-100)
    pub confidence: u32,
    /// Individual signal detections
    pub signals: Vec<CompletionSignal>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletionSignal {
    pub signal_type: SignalType,
    pub weight: u32,
    pub detected: bool,
    pub evidence: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SignalType {
    /// Explicit promise token: <promise>COMPLETE</promise>
    PromiseToken,
    /// EXIT_SIGNAL: true in RALPH_STATUS block
    ExitSignal,
    /// "all tasks complete" or similar keywords
    CompletionKeywords,
    /// Tests passing indicator
    TestsPassing,
    /// Build successful indicator
    BuildSuccess,
    /// All todos marked complete
    TodosComplete,
    /// All configured goals passing their target scores
    GoalsPassing,
}

impl CompletionSignals {
    /// Aggregate confidence from detected signals
    pub fn calculate_confidence(&mut self) {
        let total_weight: u32 = self
            .signals
            .iter()
            .filter(|s| s.detected)
            .map(|s| s.weight)
            .sum();
        self.confidence = total_weight.min(100);
    }

    /// Check if dual-condition exit is met
    /// Requires BOTH:
    /// 1. Subjective intent (EXIT_SIGNAL or PromiseToken)
    /// 2. Objective completion (2+ other signals)
    pub fn is_exit_allowed(&self) -> bool {
        let has_subjective = self.signals.iter().any(|s| {
            s.detected
                && matches!(
                    s.signal_type,
                    SignalType::PromiseToken | SignalType::ExitSignal
                )
        });

        let objective_count = self
            .signals
            .iter()
            .filter(|s| {
                s.detected
                    && !matches!(
                        s.signal_type,
                        SignalType::PromiseToken | SignalType::ExitSignal
                    )
            })
            .count();

        has_subjective && objective_count >= 2
    }
}

/// RALPH_STATUS block structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RalphStatusBlock {
    pub status: RalphStatusValue,
    pub tasks_completed_this_loop: u32,
    pub files_modified: u32,
    pub tests_status: TestsStatus,
    pub work_type: WorkType,
    pub exit_signal: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RalphStatusValue {
    InProgress,
    Complete,
    Blocked,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum TestsStatus {
    Passing,
    Failing,
    NotRun,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum WorkType {
    Implementation,
    Testing,
    Documentation,
    Refactoring,
    Debugging,
}

impl RalphStatusBlock {
    /// Format as string for output
    pub fn format(&self) -> String {
        format!(
            r#"---RALPH_STATUS---
STATUS: {:?}
TASKS_COMPLETED_THIS_LOOP: {}
FILES_MODIFIED: {}
TESTS_STATUS: {:?}
WORK_TYPE: {:?}
EXIT_SIGNAL: {}
---END_RALPH_STATUS---"#,
            self.status,
            self.tasks_completed_this_loop,
            self.files_modified,
            self.tests_status,
            self.work_type,
            self.exit_signal
        )
    }

    /// Parse from text
    pub fn parse(text: &str) -> Option<Self> {
        let start = text.find("---RALPH_STATUS---")?;
        let end_offset = text[start..].find("---END_RALPH_STATUS---")?;
        let block = &text[start..start + end_offset];

        let status = Self::extract_field(block, "STATUS")?;
        let tasks = Self::extract_field(block, "TASKS_COMPLETED_THIS_LOOP")?;
        let files = Self::extract_field(block, "FILES_MODIFIED")?;
        let tests = Self::extract_field(block, "TESTS_STATUS")?;
        let work = Self::extract_field(block, "WORK_TYPE")?;
        let exit = Self::extract_field(block, "EXIT_SIGNAL")?;

        Some(Self {
            status: Self::parse_status(&status)?,
            tasks_completed_this_loop: tasks.parse().ok()?,
            files_modified: files.parse().ok()?,
            tests_status: Self::parse_tests_status(&tests)?,
            work_type: Self::parse_work_type(&work)?,
            exit_signal: exit.to_lowercase() == "true",
        })
    }

    fn extract_field(block: &str, field: &str) -> Option<String> {
        for line in block.lines() {
            if line.starts_with(field) {
                if let Some(value) = line.split(':').nth(1) {
                    return Some(value.trim().to_string());
                }
            }
        }
        None
    }

    fn parse_status(s: &str) -> Option<RalphStatusValue> {
        match s.to_uppercase().as_str() {
            "IN_PROGRESS" | "INPROGRESS" => Some(RalphStatusValue::InProgress),
            "COMPLETE" => Some(RalphStatusValue::Complete),
            "BLOCKED" => Some(RalphStatusValue::Blocked),
            _ => None,
        }
    }

    fn parse_tests_status(s: &str) -> Option<TestsStatus> {
        match s.to_uppercase().as_str() {
            "PASSING" => Some(TestsStatus::Passing),
            "FAILING" => Some(TestsStatus::Failing),
            "NOT_RUN" | "NOTRUN" => Some(TestsStatus::NotRun),
            _ => None,
        }
    }

    fn parse_work_type(s: &str) -> Option<WorkType> {
        match s.to_uppercase().as_str() {
            "IMPLEMENTATION" => Some(WorkType::Implementation),
            "TESTING" => Some(WorkType::Testing),
            "DOCUMENTATION" => Some(WorkType::Documentation),
            "REFACTORING" => Some(WorkType::Refactoring),
            "DEBUGGING" => Some(WorkType::Debugging),
            _ => None,
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
        Path::new(directory).join(".uira").join("ralph-state.json")
    }

    /// Get the progress file path
    fn get_progress_file_path(directory: &str) -> PathBuf {
        Path::new(directory)
            .join(".uira")
            .join("ralph-progress.json")
    }

    /// Get global state file path
    fn get_global_state_file_path() -> Option<PathBuf> {
        dirs::home_dir().map(|h| h.join(".claude").join("ralph-state.json"))
    }

    /// Ensure the .uira directory exists
    fn ensure_state_dir(directory: &str) -> std::io::Result<()> {
        let uira_dir = Path::new(directory).join(".uira");
        if !uira_dir.exists() {
            fs::create_dir_all(&uira_dir)?;
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

        // Write to local .uira
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

    /// Read progress from disk
    pub fn read_progress(directory: &str) -> Option<RalphProgress> {
        let progress_file = Self::get_progress_file_path(directory);
        if !progress_file.exists() {
            return None;
        }
        let content = fs::read_to_string(&progress_file).ok()?;
        serde_json::from_str(&content).ok()
    }

    /// Write progress to disk
    pub fn write_progress(directory: &str, progress: &RalphProgress) -> bool {
        if Self::ensure_state_dir(directory).is_err() {
            return false;
        }
        let progress_file = Self::get_progress_file_path(directory);
        let content = match serde_json::to_string_pretty(progress) {
            Ok(c) => c,
            Err(_) => return false,
        };
        fs::write(&progress_file, content).is_ok()
    }

    /// Record iteration progress
    pub fn record_iteration_progress(
        directory: &str,
        iteration: u32,
        output_size: usize,
        confidence: u32,
    ) {
        let mut progress = Self::read_progress(directory).unwrap_or_default();
        progress.add_iteration(IterationRecord {
            iteration,
            timestamp: Utc::now(),
            output_size,
            confidence,
        });
        let _ = Self::write_progress(directory, &progress);
    }

    /// Clear progress file
    pub fn clear_progress(directory: &str) {
        let progress_file = Self::get_progress_file_path(directory);
        if progress_file.exists() {
            let _ = fs::remove_file(&progress_file);
        }
    }

    /// Check if session has expired
    pub fn is_session_expired(state: &RalphState) -> bool {
        let expiration = state.started_at + chrono::Duration::hours(state.session_hours as i64);
        Utc::now() > expiration
    }

    /// Get current git branch
    fn get_current_branch(directory: &str) -> Option<String> {
        let output = std::process::Command::new("git")
            .args(["rev-parse", "--abbrev-ref", "HEAD"])
            .current_dir(directory)
            .output()
            .ok()?;

        if output.status.success() {
            Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
        } else {
            None
        }
    }

    /// Check if branch has changed since ralph started
    pub fn has_branch_changed(state: &RalphState, directory: &str) -> bool {
        if let (Some(original), Some(current)) =
            (&state.git_branch, Self::get_current_branch(directory))
        {
            original != &current
        } else {
            false
        }
    }

    /// Activate ralph mode
    pub fn activate(
        prompt: &str,
        session_id: Option<&str>,
        directory: Option<&str>,
        options: Option<RalphOptions>,
    ) -> bool {
        let opts = options.unwrap_or_default();
        let git_branch = directory.and_then(Self::get_current_branch);

        let state = RalphState {
            active: true,
            iteration: 0,
            max_iterations: opts.max_iterations,
            completion_promise: opts.completion_promise,
            session_id: session_id.map(|s| s.to_string()),
            prompt: Some(prompt.to_string()),
            started_at: Utc::now(),
            last_checked_at: Utc::now(),
            min_confidence: opts.min_confidence,
            require_dual_condition: opts.require_dual_condition,
            session_hours: 24,
            git_branch,
            circuit_breaker: CircuitBreakerState::default(),
            circuit_config: CircuitBreakerConfig::default(),
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

        // Update circuit breaker (simple metrics - no detailed tracking yet)
        // Pass 1 for files_changed to indicate "unknown but not stalled" until proper tracking
        state.circuit_breaker.record_iteration(
            1,     // files_changed - assume progress until proper tracking implemented
            false, // tests_changed - not tracked yet
            0,     // output_size - not tracked yet
            None,  // error - not tracked yet
            &state.circuit_config,
        );

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

    pub fn detect_completion_signals(
        text: &str,
        promise: &str,
        todo_result: Option<&IncompleteTodosResult>,
    ) -> CompletionSignals {
        Self::detect_completion_signals_with_goals(text, promise, todo_result, None)
    }

    pub fn detect_completion_signals_with_goals(
        text: &str,
        promise: &str,
        todo_result: Option<&IncompleteTodosResult>,
        goals_result: Option<&uira_goals::VerificationResult>,
    ) -> CompletionSignals {
        let mut signals = CompletionSignals::default();

        signals.signals.push(CompletionSignal {
            signal_type: SignalType::PromiseToken,
            weight: 40,
            detected: Self::detect_completion_promise(text, promise),
            evidence: None,
        });

        let exit_signal = Self::detect_exit_signal(text);
        signals.signals.push(CompletionSignal {
            signal_type: SignalType::ExitSignal,
            weight: 30,
            detected: exit_signal,
            evidence: None,
        });

        let keywords = [
            "all tasks complete",
            "work is done",
            "successfully completed",
            "finished all",
        ];
        let keyword_match = keywords.iter().any(|k| text.to_lowercase().contains(k));
        signals.signals.push(CompletionSignal {
            signal_type: SignalType::CompletionKeywords,
            weight: 10,
            detected: keyword_match,
            evidence: None,
        });

        let tests_passing = text.contains("tests passed") || text.contains("All tests pass");
        signals.signals.push(CompletionSignal {
            signal_type: SignalType::TestsPassing,
            weight: 15,
            detected: tests_passing,
            evidence: None,
        });

        let build_success = text.contains("Build successful") || text.contains("build completed");
        signals.signals.push(CompletionSignal {
            signal_type: SignalType::BuildSuccess,
            weight: 15,
            detected: build_success,
            evidence: None,
        });

        let (todo_detected, todo_evidence) = if let Some(result) = todo_result {
            (
                result.count == 0 && result.total > 0,
                Some(format!(
                    "{}/{} complete",
                    result.total - result.count,
                    result.total
                )),
            )
        } else {
            (false, None)
        };
        signals.signals.push(CompletionSignal {
            signal_type: SignalType::TodosComplete,
            weight: 20,
            detected: todo_detected,
            evidence: todo_evidence,
        });

        let (goals_detected, goals_evidence) = if let Some(result) = goals_result {
            let passed = result.results.iter().filter(|r| r.passed).count();
            let total = result.results.len();
            (
                result.all_passed && total > 0,
                Some(format!("{}/{} goals passed", passed, total)),
            )
        } else {
            (false, None)
        };
        signals.signals.push(CompletionSignal {
            signal_type: SignalType::GoalsPassing,
            weight: 25,
            detected: goals_detected,
            evidence: goals_evidence,
        });

        signals.calculate_confidence();
        signals
    }

    fn detect_exit_signal(text: &str) -> bool {
        if let Some(start) = text.find("---RALPH_STATUS---") {
            if let Some(end) = text[start..].find("---END_RALPH_STATUS---") {
                let block = &text[start..start + end];
                return block.contains("EXIT_SIGNAL: true") || block.contains("EXIT_SIGNAL:true");
            }
        }
        false
    }

    pub async fn check_goals_from_config(
        directory: &str,
    ) -> Option<uira_goals::VerificationResult> {
        let config_path = std::path::Path::new(directory).join("uira.yml");
        if !config_path.exists() {
            return None;
        }

        let config = uira_config::load_config(Some(&config_path)).ok()?;

        if !config.goals.auto_verify {
            return None;
        }

        let goals = &config.goals.goals;

        if goals.is_empty() {
            return None;
        }

        let runner = uira_goals::GoalRunner::new(directory);
        Some(runner.check_all(goals).await)
    }

    pub fn build_verification_feedback(
        signals: &CompletionSignals,
        state: &RalphState,
        goals_result: &Option<uira_goals::VerificationResult>,
    ) -> String {
        let mut feedback = Vec::new();

        if signals.confidence < state.min_confidence {
            feedback.push(format!(
                "Confidence {} is below minimum threshold {}",
                signals.confidence, state.min_confidence
            ));
        }

        if state.require_dual_condition && !signals.is_exit_allowed() {
            let has_subjective = signals.signals.iter().any(|s| {
                s.detected
                    && matches!(
                        s.signal_type,
                        SignalType::PromiseToken | SignalType::ExitSignal
                    )
            });
            let objective_count = signals
                .signals
                .iter()
                .filter(|s| {
                    s.detected
                        && !matches!(
                            s.signal_type,
                            SignalType::PromiseToken | SignalType::ExitSignal
                        )
                })
                .count();

            if !has_subjective {
                feedback.push("Missing subjective intent (promise or exit signal)".to_string());
            }
            if objective_count < 2 {
                feedback.push(format!(
                    "Need 2+ objective signals, only have {}",
                    objective_count
                ));
            }
        }

        if let Some(goals) = goals_result {
            for result in &goals.results {
                if !result.passed {
                    feedback.push(format!(
                        "Goal '{}': {:.1}% (target: {:.1}%)",
                        result.name, result.score, result.target
                    ));
                }
            }
        }

        for signal in &signals.signals {
            if !signal.detected {
                match signal.signal_type {
                    SignalType::TodosComplete => {
                        if let Some(evidence) = &signal.evidence {
                            feedback.push(format!("Todos incomplete: {}", evidence));
                        }
                    }
                    SignalType::TestsPassing => {
                        feedback.push("Tests not passing".to_string());
                    }
                    _ => {}
                }
            }
        }

        if feedback.is_empty() {
            "Unknown verification failure".to_string()
        } else {
            feedback.join("\n- ")
        }
    }

    fn get_verification_failure_prompt(state: &RalphState, feedback: &str) -> String {
        format!(
            r#"<ralph-verification-failed>

[RALPH - ITERATION {}/{} - VERIFICATION FAILED]

You signaled completion, but the system verification checks did not pass.

VERIFICATION FAILURES:
- {}

WHAT YOU MUST DO:
1. Review the failures above
2. Fix each issue before signaling completion again
3. For goals: ensure your commands output scores meeting the target
4. For todos: mark ALL items complete
5. For tests: ensure all tests pass

When ALL verifications pass, output: <promise>{}</promise>

{}

</ralph-verification-failed>

---

"#,
            state.iteration,
            state.max_iterations,
            feedback,
            state.completion_promise,
            state
                .prompt
                .as_ref()
                .map(|p| format!("Original task: {}", p))
                .unwrap_or_default()
        )
    }

    pub fn get_continuation_prompt(state: &RalphState) -> String {
        format!(
            r#"<ralph-continuation>

[RALPH - ITERATION {}/{}]

Your previous attempt did not output the completion promise. The work is NOT done yet.

CRITICAL INSTRUCTIONS:
1. Review your progress and the original task
2. Check your todo list - are ALL items marked complete?
3. Continue from where you left off
4. Output a RALPH_STATUS block showing your progress:

---RALPH_STATUS---
STATUS: IN_PROGRESS | COMPLETE | BLOCKED
TASKS_COMPLETED_THIS_LOOP: <number>
FILES_MODIFIED: <number>
TESTS_STATUS: PASSING | FAILING | NOT_RUN
WORK_TYPE: IMPLEMENTATION | TESTING | DOCUMENTATION | REFACTORING | DEBUGGING
EXIT_SIGNAL: false | true
---END_RALPH_STATUS---

5. When FULLY complete, set EXIT_SIGNAL: true AND output: <promise>{}</promise>
6. Do NOT stop until the task is truly done

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

        // Check circuit breaker
        if state.circuit_breaker.is_tripped() {
            Self::clear_state(Some(&context.directory));
            UltraworkHook::deactivate(Some(&context.directory));
            return Ok(HookOutput::continue_with_message(format!(
                "[RALPH CIRCUIT BREAKER] Loop terminated: {}",
                state
                    .circuit_breaker
                    .trip_reason
                    .as_deref()
                    .unwrap_or("Unknown reason")
            )));
        }

        // Check for completion intent via transcript
        if let Some(last_response) = input.get_last_assistant_response() {
            let has_promise =
                Self::detect_completion_promise(&last_response, &state.completion_promise);
            let has_exit_signal = Self::detect_exit_signal(&last_response);

            if has_promise || has_exit_signal {
                let todo_result = TodoContinuationHook::check_incomplete_todos(
                    input.session_id.as_deref(),
                    &context.directory,
                    None,
                );

                let goals_result = Self::check_goals_from_config(&context.directory).await;

                let signals = Self::detect_completion_signals_with_goals(
                    &last_response,
                    &state.completion_promise,
                    Some(&todo_result),
                    goals_result.as_ref(),
                );

                // Fail-open: if no goals configured, config missing, or config parse error,
                // default to passing. Goals are optional and shouldn't block indefinitely.
                let goals_gate_passed = goals_result.as_ref().map(|g| g.all_passed).unwrap_or(true);

                let exit_allowed = if state.require_dual_condition {
                    signals.is_exit_allowed()
                        && signals.confidence >= state.min_confidence
                        && goals_gate_passed
                } else {
                    signals.confidence >= state.min_confidence && goals_gate_passed
                };

                if exit_allowed {
                    Self::clear_state(Some(&context.directory));
                    UltraworkHook::deactivate(Some(&context.directory));
                    return Ok(HookOutput {
                        should_continue: false,
                        message: Some(format!(
                            "[RALPH COMPLETE] All verification passed after {} iterations (confidence: {}%). Task finished successfully!",
                            state.iteration,
                            signals.confidence
                        )),
                        reason: None,
                        modified_input: None,
                    });
                } else {
                    let feedback =
                        Self::build_verification_feedback(&signals, &state, &goals_result);
                    let new_state = match Self::increment_iteration(Some(&context.directory)) {
                        Some(s) => s,
                        None => return Ok(HookOutput::pass()),
                    };
                    let message = Self::get_verification_failure_prompt(&new_state, &feedback);
                    return Ok(HookOutput::block_with_reason(message));
                }
            }
        }

        // Check max iterations (AFTER promise check)
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
        250
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
            min_confidence: 50,
            require_dual_condition: true,
            session_hours: 24,
            git_branch: None,
            circuit_breaker: CircuitBreakerState::default(),
            circuit_config: CircuitBreakerConfig::default(),
        };

        let prompt = RalphHook::get_continuation_prompt(&state);
        assert!(prompt.contains("ITERATION 3/10"));
        assert!(prompt.contains("<promise>DONE</promise>"));
        assert!(prompt.contains("Build the feature"));
    }

    #[test]
    fn test_detect_exit_signal() {
        let text = r#"---RALPH_STATUS---
STATUS: COMPLETE
EXIT_SIGNAL: true
---END_RALPH_STATUS---"#;
        assert!(RalphHook::detect_exit_signal(text));

        let text_false = "EXIT_SIGNAL: false";
        assert!(!RalphHook::detect_exit_signal(text_false));
    }

    #[test]
    fn test_ralph_status_block_roundtrip() {
        let block = RalphStatusBlock {
            status: RalphStatusValue::InProgress,
            tasks_completed_this_loop: 3,
            files_modified: 5,
            tests_status: TestsStatus::Passing,
            work_type: WorkType::Implementation,
            exit_signal: false,
        };
        let formatted = block.format();
        let parsed = RalphStatusBlock::parse(&formatted).unwrap();
        assert_eq!(parsed.tasks_completed_this_loop, 3);
        assert_eq!(parsed.files_modified, 5);
        assert!(!parsed.exit_signal);
    }

    #[test]
    fn test_completion_signals_dual_condition() {
        let mut signals = CompletionSignals::default();

        // Only subjective signal - should not allow exit
        signals.signals.push(CompletionSignal {
            signal_type: SignalType::PromiseToken,
            weight: 40,
            detected: true,
            evidence: None,
        });
        signals.calculate_confidence();
        assert!(!signals.is_exit_allowed());

        // Add objective signals
        signals.signals.push(CompletionSignal {
            signal_type: SignalType::TestsPassing,
            weight: 15,
            detected: true,
            evidence: None,
        });
        signals.signals.push(CompletionSignal {
            signal_type: SignalType::TodosComplete,
            weight: 20,
            detected: true,
            evidence: None,
        });
        signals.calculate_confidence();
        assert!(signals.is_exit_allowed());
        assert_eq!(signals.confidence, 75); // 40 + 15 + 20
    }

    #[test]
    fn test_ralph_progress_add_iteration() {
        let mut progress = RalphProgress::default();
        progress.add_iteration(IterationRecord {
            iteration: 1,
            timestamp: Utc::now(),
            output_size: 1000,
            confidence: 50,
        });
        assert_eq!(progress.iterations.len(), 1);
        assert_eq!(progress.iterations[0].iteration, 1);
    }

    #[test]
    fn test_ralph_progress_add_file_dedup() {
        let mut progress = RalphProgress::default();
        progress.add_file("src/main.rs");
        progress.add_file("src/main.rs"); // duplicate
        progress.add_file("src/lib.rs");
        assert_eq!(progress.files_modified.len(), 2);
    }

    #[test]
    fn test_ralph_progress_summary() {
        let mut progress = RalphProgress::default();
        progress.add_iteration(IterationRecord {
            iteration: 1,
            timestamp: Utc::now(),
            output_size: 1000,
            confidence: 50,
        });
        progress.add_file("src/main.rs");
        progress.add_learning("Use async/await for I/O");

        let summary = progress.get_summary();
        assert!(summary.contains("Iterations: 1"));
        assert!(summary.contains("Files: 1"));
        assert!(summary.contains("Learnings: 1"));
    }

    #[test]
    fn test_session_expiration() {
        let mut state = RalphState::default();
        state.session_hours = 24;
        state.started_at = Utc::now() - chrono::Duration::hours(25);
        assert!(RalphHook::is_session_expired(&state));

        state.started_at = Utc::now() - chrono::Duration::hours(12);
        assert!(!RalphHook::is_session_expired(&state));
    }

    #[test]
    fn test_ralph_options_default() {
        let opts = RalphOptions::default();
        assert_eq!(opts.max_iterations, 10);
        assert_eq!(opts.min_confidence, 50);
        assert!(opts.require_dual_condition);
    }

    #[test]
    fn test_detect_completion_signals_with_promise() {
        let signals = RalphHook::detect_completion_signals(
            "Work done. <promise>TASK COMPLETE</promise>",
            "TASK COMPLETE",
            None,
        );
        assert!(signals
            .signals
            .iter()
            .any(|s| s.signal_type == SignalType::PromiseToken && s.detected));
        assert!(signals.confidence >= 40);
    }

    #[test]
    fn test_detect_completion_signals_with_keywords() {
        let signals = RalphHook::detect_completion_signals(
            "All tasks complete and tests passed",
            "DONE",
            None,
        );
        // Should detect completion keywords and tests passing
        let keyword_detected = signals
            .signals
            .iter()
            .find(|s| s.signal_type == SignalType::CompletionKeywords)
            .map(|s| s.detected)
            .unwrap_or(false);
        let tests_detected = signals
            .signals
            .iter()
            .find(|s| s.signal_type == SignalType::TestsPassing)
            .map(|s| s.detected)
            .unwrap_or(false);
        assert!(keyword_detected);
        assert!(tests_detected);
    }

    #[test]
    fn test_ralph_status_block_parse_complete() {
        let text = r#"Some output
---RALPH_STATUS---
STATUS: COMPLETE
TASKS_COMPLETED_THIS_LOOP: 5
FILES_MODIFIED: 3
TESTS_STATUS: PASSING
WORK_TYPE: IMPLEMENTATION
EXIT_SIGNAL: true
---END_RALPH_STATUS---
More output"#;

        let block = RalphStatusBlock::parse(text).unwrap();
        assert_eq!(block.status, RalphStatusValue::Complete);
        assert_eq!(block.tasks_completed_this_loop, 5);
        assert_eq!(block.files_modified, 3);
        assert_eq!(block.tests_status, TestsStatus::Passing);
        assert_eq!(block.work_type, WorkType::Implementation);
        assert!(block.exit_signal);
    }

    #[test]
    fn test_completion_signals_confidence_capped() {
        let mut signals = CompletionSignals::default();
        // Add signals with weights totaling more than 100
        for _ in 0..5 {
            signals.signals.push(CompletionSignal {
                signal_type: SignalType::PromiseToken,
                weight: 40,
                detected: true,
                evidence: None,
            });
        }
        signals.calculate_confidence();
        assert_eq!(signals.confidence, 100); // Capped at 100
    }

    #[test]
    fn test_detect_exit_signal_various_formats() {
        assert!(RalphHook::detect_exit_signal(
            "---RALPH_STATUS---\nEXIT_SIGNAL: true\n---END_RALPH_STATUS---"
        ));
        assert!(RalphHook::detect_exit_signal(
            "---RALPH_STATUS---\nEXIT_SIGNAL:true\n---END_RALPH_STATUS---"
        ));
        assert!(!RalphHook::detect_exit_signal("EXIT_SIGNAL: true"));
        assert!(!RalphHook::detect_exit_signal(
            "---RALPH_STATUS---\nEXIT_SIGNAL: false\n---END_RALPH_STATUS---"
        ));
    }

    #[test]
    fn test_build_verification_feedback_low_confidence() {
        let mut signals = CompletionSignals::default();
        signals.confidence = 30;
        let state = RalphState {
            min_confidence: 50,
            require_dual_condition: false,
            ..Default::default()
        };

        let feedback = RalphHook::build_verification_feedback(&signals, &state, &None);
        assert!(feedback.contains("Confidence 30"));
        assert!(feedback.contains("below minimum threshold 50"));
    }

    #[test]
    fn test_build_verification_feedback_missing_objective_signals() {
        let mut signals = CompletionSignals::default();
        signals.confidence = 60;
        signals.signals.push(CompletionSignal {
            signal_type: SignalType::PromiseToken,
            weight: 40,
            detected: true,
            evidence: None,
        });
        signals.signals.push(CompletionSignal {
            signal_type: SignalType::TestsPassing,
            weight: 15,
            detected: true,
            evidence: None,
        });

        let state = RalphState {
            min_confidence: 50,
            require_dual_condition: true,
            ..Default::default()
        };

        let feedback = RalphHook::build_verification_feedback(&signals, &state, &None);
        assert!(feedback.contains("Need 2+ objective signals"));
    }

    #[test]
    fn test_build_verification_feedback_goals_failed() {
        let signals = CompletionSignals {
            confidence: 80,
            signals: vec![
                CompletionSignal {
                    signal_type: SignalType::PromiseToken,
                    weight: 40,
                    detected: true,
                    evidence: None,
                },
                CompletionSignal {
                    signal_type: SignalType::TestsPassing,
                    weight: 15,
                    detected: true,
                    evidence: None,
                },
                CompletionSignal {
                    signal_type: SignalType::TodosComplete,
                    weight: 20,
                    detected: true,
                    evidence: None,
                },
            ],
        };

        let state = RalphState {
            min_confidence: 50,
            require_dual_condition: true,
            ..Default::default()
        };

        let goals_result = uira_goals::VerificationResult {
            all_passed: false,
            results: vec![uira_goals::GoalCheckResult {
                name: "pixel-match".to_string(),
                score: 85.0,
                target: 99.0,
                passed: false,
                checked_at: Utc::now(),
                duration_ms: 100,
                error: None,
            }],
            checked_at: Utc::now(),
            iteration: 1,
        };

        let feedback =
            RalphHook::build_verification_feedback(&signals, &state, &Some(goals_result));
        assert!(feedback.contains("pixel-match"));
        assert!(feedback.contains("85.0%"));
        assert!(feedback.contains("99.0%"));
    }

    #[test]
    fn test_get_verification_failure_prompt() {
        let state = RalphState {
            active: true,
            iteration: 5,
            max_iterations: 10,
            completion_promise: "DONE".to_string(),
            prompt: Some("Build feature".to_string()),
            ..Default::default()
        };

        let feedback = "Goal 'coverage': 75% (target: 90%)";
        let prompt = RalphHook::get_verification_failure_prompt(&state, feedback);

        assert!(prompt.contains("ITERATION 5/10"));
        assert!(prompt.contains("VERIFICATION FAILED"));
        assert!(prompt.contains("coverage"));
        assert!(prompt.contains("<promise>DONE</promise>"));
        assert!(prompt.contains("Build feature"));
    }

    #[test]
    fn test_detect_completion_signals_with_goals_passing() {
        let goals_result = uira_goals::VerificationResult {
            all_passed: true,
            results: vec![uira_goals::GoalCheckResult {
                name: "test".to_string(),
                score: 100.0,
                target: 95.0,
                passed: true,
                checked_at: Utc::now(),
                duration_ms: 50,
                error: None,
            }],
            checked_at: Utc::now(),
            iteration: 1,
        };

        let signals = RalphHook::detect_completion_signals_with_goals(
            "<promise>DONE</promise>",
            "DONE",
            None,
            Some(&goals_result),
        );

        let goals_signal = signals
            .signals
            .iter()
            .find(|s| s.signal_type == SignalType::GoalsPassing)
            .unwrap();
        assert!(goals_signal.detected);
        assert!(goals_signal.evidence.as_ref().unwrap().contains("1/1"));
    }
}
