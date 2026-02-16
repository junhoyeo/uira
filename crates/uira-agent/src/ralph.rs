//! Ralph mode controller for native agent harness
//!
//! Wraps uira-hooks Ralph implementation with event streaming.

use crate::config::AgentGoalsConfig;
use crate::events::EventSender;
use crate::goals::GoalVerifier;
use chrono::Utc;
use std::path::Path;
use uira_orchestration::hooks::hooks::circuit_breaker::CircuitBreakerConfig;
use uira_orchestration::hooks::hooks::ralph::{RalphHook, RalphOptions, RalphState};
use uira_orchestration::hooks::VerificationResult;
use uira_orchestration::TodoStore;
use uira_core::{ThreadEvent, TodoStatus};

/// Ralph mode configuration
pub struct RalphConfig {
    pub max_iterations: u32,
    pub completion_promise: String,
    pub min_confidence: u32,
    pub require_dual_condition: bool,
    pub circuit_breaker: CircuitBreakerConfig,
}

impl Default for RalphConfig {
    fn default() -> Self {
        Self {
            max_iterations: 10,
            completion_promise: "TASK COMPLETE".into(),
            min_confidence: 50,
            require_dual_condition: true,
            circuit_breaker: CircuitBreakerConfig::default(),
        }
    }
}

/// Ralph mode controller
///
/// Controls ralph (self-referential work loop) mode with event streaming.
pub struct RalphController {
    state: RalphState,
    config: RalphConfig,
    directory: String,
    event_tx: Option<EventSender>,
    todo_store: TodoStore,
}

/// Decision from ralph completion check
pub enum RalphDecision {
    /// Continue with feedback for next iteration
    Continue { feedback: String },
    /// Task completed successfully
    Complete,
    /// Exit due to circuit breaker or max iterations
    Exit { reason: String },
}

impl RalphController {
    /// Activate ralph mode for a task
    pub fn activate(
        prompt: &str,
        session_id: Option<&str>,
        directory: &str,
        config: RalphConfig,
        todo_store: TodoStore,
    ) -> Option<Self> {
        let options = RalphOptions {
            max_iterations: config.max_iterations,
            completion_promise: config.completion_promise.clone(),
            min_confidence: config.min_confidence,
            require_dual_condition: config.require_dual_condition,
        };

        // RalphHook::activate returns bool
        let success = RalphHook::activate(prompt, session_id, Some(directory), Some(options));

        if !success {
            return None;
        }

        // Read back state that was written
        let state = RalphHook::read_state(Some(directory))?;

        Some(Self {
            state,
            config,
            directory: directory.to_string(),
            event_tx: None,
            todo_store,
        })
    }

    /// Load existing ralph state
    pub fn load(directory: &str, todo_store: TodoStore) -> Option<Self> {
        let state = RalphHook::read_state(Some(directory))?;
        if !state.active {
            return None;
        }

        Some(Self {
            config: RalphConfig {
                max_iterations: state.max_iterations,
                completion_promise: state.completion_promise.clone(),
                min_confidence: state.min_confidence,
                require_dual_condition: state.require_dual_condition,
                ..Default::default()
            },
            state,
            directory: directory.to_string(),
            event_tx: None,
            todo_store,
        })
    }

    /// Add event streaming
    pub fn with_events(mut self, tx: EventSender) -> Self {
        self.event_tx = Some(tx);
        self
    }

    /// Check if task should continue or complete
    pub async fn check_completion(
        &mut self,
        response_text: &str,
        goals_result: Option<&VerificationResult>,
    ) -> RalphDecision {
        self.emit_iteration_started().await;

        // Check circuit breaker
        if self.state.circuit_breaker.is_tripped() {
            let reason = self
                .state
                .circuit_breaker
                .trip_reason
                .clone()
                .unwrap_or_else(|| "Circuit breaker tripped".into());
            self.emit_circuit_break(&reason).await;
            self.clear();
            return RalphDecision::Exit { reason };
        }

        // Check max iterations
        if self.state.iteration >= self.state.max_iterations {
            let reason = format!("Max iterations ({}) reached", self.state.max_iterations);
            self.emit_circuit_break(&reason).await;
            self.clear();
            return RalphDecision::Exit { reason };
        }

        // Check todos
        let session_id = self.state.session_id.as_deref().unwrap_or("");
        let todos = self.todo_store.get(session_id).await;
        let incomplete_count = todos
            .iter()
            .filter(|t| matches!(t.status, TodoStatus::Pending | TodoStatus::InProgress))
            .count();
        let total = todos.len();
        let todo_counts = if total > 0 {
            Some((incomplete_count, total))
        } else {
            None
        };

        // Detect completion signals
        let signals = RalphHook::detect_completion_signals_with_goals(
            response_text,
            &self.state.completion_promise,
            todo_counts,
            goals_result,
        );

        // Check exit gate
        let goals_passed = goals_result.map(|r| r.all_passed).unwrap_or(true);
        let exit_allowed = if self.config.require_dual_condition {
            signals.is_exit_allowed()
                && signals.confidence >= self.state.min_confidence
                && goals_passed
        } else {
            signals.confidence >= self.state.min_confidence && goals_passed
        };

        if exit_allowed {
            self.clear();
            RalphDecision::Complete
        } else {
            // Build feedback using now-public function
            let feedback = RalphHook::build_verification_feedback(
                &signals,
                &self.state,
                &goals_result.cloned(),
            );
            self.emit_continuation(&feedback, signals.confidence).await;
            self.increment_iteration();
            RalphDecision::Continue { feedback }
        }
    }

    fn increment_iteration(&mut self) {
        self.state.iteration += 1;
        self.state.last_checked_at = Utc::now();
        RalphHook::write_state(&self.state, Some(&self.directory));
    }

    fn clear(&self) {
        RalphHook::clear_state(Some(&self.directory));
    }

    async fn emit_iteration_started(&self) {
        if let Some(ref tx) = self.event_tx {
            let _ = tx
                .send(ThreadEvent::RalphIterationStarted {
                    iteration: self.state.iteration,
                    max_iterations: self.state.max_iterations,
                    prompt: self.state.prompt.clone().unwrap_or_default(),
                })
                .await;
        }
    }

    async fn emit_continuation(&self, details: &str, confidence: u32) {
        if let Some(ref tx) = self.event_tx {
            let _ = tx
                .send(ThreadEvent::RalphContinuation {
                    reason: "verification_failed".into(),
                    confidence,
                    details: details.to_string(),
                })
                .await;
        }
    }

    async fn emit_circuit_break(&self, reason: &str) {
        if let Some(ref tx) = self.event_tx {
            let _ = tx
                .send(ThreadEvent::RalphCircuitBreak {
                    reason: reason.to_string(),
                    iteration: self.state.iteration,
                })
                .await;
        }
    }

    /// Check if ralph is active
    pub fn is_active(&self) -> bool {
        self.state.active
    }

    /// Get current iteration
    pub fn iteration(&self) -> u32 {
        self.state.iteration
    }

    /// Get max iterations
    pub fn max_iterations(&self) -> u32 {
        self.state.max_iterations
    }

    /// Create a GoalVerifier from agent goals config
    pub fn create_goal_verifier(&self, goals_config: &AgentGoalsConfig) -> Option<GoalVerifier> {
        if !goals_config.has_goals() {
            return None;
        }

        let verifier = GoalVerifier::new(Path::new(&self.directory), goals_config.goals.clone())
            .with_parallel(goals_config.parallel_check);

        // Add events if we have a sender
        if let Some(ref tx) = self.event_tx {
            Some(verifier.with_events(tx.clone()))
        } else {
            Some(verifier)
        }
    }

    /// Check completion with automatic goal verification
    ///
    /// This is a convenience method that runs goal verification if configured,
    /// then checks ralph completion.
    pub async fn check_completion_with_goals(
        &mut self,
        response_text: &str,
        goals_config: &AgentGoalsConfig,
    ) -> RalphDecision {
        let goals_result = if goals_config.has_goals() && goals_config.auto_verify {
            if let Some(verifier) = self.create_goal_verifier(goals_config) {
                Some(verifier.verify_all().await)
            } else {
                None
            }
        } else {
            None
        };

        self.check_completion(response_text, goals_result.as_ref())
            .await
    }
}
