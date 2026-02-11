//! Goal verification for agent execution

use crate::events::EventSender;
use futures::future::join_all;
use std::path::Path;
use uira_core::schema::GoalConfig;
use uira_hooks::{GoalCheckResult, GoalRunner, VerificationResult};
use uira_types::ThreadEvent;

/// Goal verifier for agent execution
///
/// Wraps uira-goals with event streaming and parallel execution support.
pub struct GoalVerifier {
    runner: GoalRunner,
    goals: Vec<GoalConfig>,
    event_tx: Option<EventSender>,
    parallel: bool,
}

impl GoalVerifier {
    /// Create a new goal verifier
    pub fn new(project_root: impl AsRef<Path>, goals: Vec<GoalConfig>) -> Self {
        Self {
            runner: GoalRunner::new(project_root),
            goals,
            event_tx: None,
            parallel: true, // parallel by default
        }
    }

    /// Add event streaming
    pub fn with_events(mut self, tx: EventSender) -> Self {
        self.event_tx = Some(tx);
        self
    }

    /// Set parallel execution mode
    pub fn with_parallel(mut self, parallel: bool) -> Self {
        self.parallel = parallel;
        self
    }

    /// Verify all goals
    ///
    /// Emits events during verification:
    /// - GoalVerificationStarted
    /// - GoalVerificationResult (for each goal)
    /// - GoalVerificationCompleted
    pub async fn verify_all(&self) -> VerificationResult {
        // Emit started event
        if let Some(tx) = &self.event_tx {
            let _ = tx
                .send(ThreadEvent::GoalVerificationStarted {
                    goals: self.goals.iter().map(|g| g.name.clone()).collect(),
                    method: if self.parallel {
                        "parallel"
                    } else {
                        "sequential"
                    }
                    .to_string(),
                })
                .await;
        }

        // Run goals
        let results = if self.parallel {
            self.verify_parallel().await
        } else {
            self.verify_sequential().await
        };

        let all_passed = results.iter().all(|r| r.passed);

        // Emit individual results
        if let Some(tx) = &self.event_tx {
            for result in &results {
                let _ = tx
                    .send(ThreadEvent::GoalVerificationResult {
                        goal: result.name.clone(),
                        score: result.score,
                        target: result.target,
                        passed: result.passed,
                        duration_ms: result.duration_ms,
                    })
                    .await;
            }
        }

        let passed_count = results.iter().filter(|r| r.passed).count();
        let total_count = results.len();

        let verification = VerificationResult {
            all_passed,
            results,
            checked_at: chrono::Utc::now(),
            iteration: 0,
        };

        // Emit completed event
        if let Some(tx) = &self.event_tx {
            let _ = tx
                .send(ThreadEvent::GoalVerificationCompleted {
                    all_passed,
                    passed_count,
                    total_count,
                })
                .await;
        }

        verification
    }

    /// Check if there are any goals to verify
    pub fn has_goals(&self) -> bool {
        !self.goals.is_empty()
    }

    /// Verify goals in parallel
    async fn verify_parallel(&self) -> Vec<GoalCheckResult> {
        let futures: Vec<_> = self
            .goals
            .iter()
            .map(|goal| self.runner.check_goal(goal))
            .collect();

        join_all(futures).await
    }

    /// Verify goals sequentially
    async fn verify_sequential(&self) -> Vec<GoalCheckResult> {
        let mut results = Vec::with_capacity(self.goals.len());

        for goal in &self.goals {
            let result = self.runner.check_goal(goal).await;
            results.push(result);
        }

        results
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_goal(name: &str, command: &str, target: f64) -> GoalConfig {
        GoalConfig {
            name: name.to_string(),
            workspace: None,
            command: command.to_string(),
            target,
            timeout_secs: 10,
            enabled: true,
            description: None,
        }
    }

    #[tokio::test]
    async fn test_goal_verifier_empty() {
        let verifier = GoalVerifier::new(".", vec![]);
        assert!(!verifier.has_goals());
    }

    #[tokio::test]
    async fn test_goal_verifier_with_goals() {
        let goals = vec![make_goal("test", "echo 100", 80.0)];
        let verifier = GoalVerifier::new(".", goals);
        assert!(verifier.has_goals());
    }

    #[tokio::test]
    async fn test_verify_all_sequential() {
        let goals = vec![
            make_goal("pass", "echo 90", 80.0),
            make_goal("fail", "echo 50", 80.0),
        ];
        let verifier = GoalVerifier::new(".", goals).with_parallel(false);
        let result = verifier.verify_all().await;

        assert!(!result.all_passed);
        assert_eq!(result.results.len(), 2);
        assert!(result.results[0].passed);
        assert!(!result.results[1].passed);
    }

    #[tokio::test]
    async fn test_verify_all_parallel() {
        let goals = vec![
            make_goal("pass1", "echo 90", 80.0),
            make_goal("pass2", "echo 100", 90.0),
        ];
        let verifier = GoalVerifier::new(".", goals).with_parallel(true);
        let result = verifier.verify_all().await;

        assert!(result.all_passed);
        assert_eq!(result.results.len(), 2);
    }
}
