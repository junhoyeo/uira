use uira_config::schema::GoalConfig;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::Duration;
use thiserror::Error;
use tokio::process::Command;
use tokio::time::timeout;

#[derive(Error, Debug)]
pub enum GoalError {
    #[error("Command failed with exit code {0}")]
    CommandFailed(i32),

    #[error("Command timed out after {0} seconds")]
    Timeout(u64),

    #[error("Failed to parse score from output: {0}")]
    ParseError(String),

    #[error("Score {0} is out of valid range (0-100)")]
    ScoreOutOfRange(f64),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("Goal '{0}' not found")]
    GoalNotFound(String),
}

pub type GoalResult<T> = Result<T, GoalError>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoalCheckResult {
    pub name: String,
    pub score: f64,
    pub target: f64,
    pub passed: bool,
    pub checked_at: DateTime<Utc>,
    pub duration_ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl GoalCheckResult {
    pub fn success(name: String, score: f64, target: f64, duration_ms: u64) -> Self {
        Self {
            name,
            score,
            target,
            passed: score >= target,
            checked_at: Utc::now(),
            duration_ms,
            error: None,
        }
    }

    pub fn failure(name: String, target: f64, error: String) -> Self {
        Self {
            name,
            score: 0.0,
            target,
            passed: false,
            checked_at: Utc::now(),
            duration_ms: 0,
            error: Some(error),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationResult {
    pub all_passed: bool,
    pub results: Vec<GoalCheckResult>,
    pub checked_at: DateTime<Utc>,
    pub iteration: u32,
}

impl VerificationResult {
    pub fn summary(&self) -> String {
        let passed = self.results.iter().filter(|r| r.passed).count();
        let total = self.results.len();
        let details: Vec<String> = self
            .results
            .iter()
            .map(|r| {
                let status = if r.passed { "✓" } else { "✗" };
                format!("{} {}: {:.1}/{:.1}", status, r.name, r.score, r.target)
            })
            .collect();
        format!("Goals: {}/{} passed\n{}", passed, total, details.join("\n"))
    }
}

pub struct GoalRunner {
    project_root: PathBuf,
}

impl GoalRunner {
    pub fn new(project_root: impl AsRef<Path>) -> Self {
        Self {
            project_root: project_root.as_ref().to_path_buf(),
        }
    }

    pub async fn check_goal(&self, goal: &GoalConfig) -> GoalCheckResult {
        if !goal.enabled {
            return GoalCheckResult::success(goal.name.clone(), goal.target, goal.target, 0);
        }

        let start = std::time::Instant::now();

        match self.run_goal_command(goal).await {
            Ok(score) => {
                let duration_ms = start.elapsed().as_millis() as u64;
                GoalCheckResult::success(goal.name.clone(), score, goal.target, duration_ms)
            }
            Err(e) => GoalCheckResult::failure(goal.name.clone(), goal.target, e.to_string()),
        }
    }

    async fn run_goal_command(&self, goal: &GoalConfig) -> GoalResult<f64> {
        let working_dir = match &goal.workspace {
            Some(ws) => self.project_root.join(ws),
            None => self.project_root.clone(),
        };

        let timeout_duration = Duration::from_secs(goal.timeout_secs);

        let output = timeout(timeout_duration, async {
            Command::new("sh")
                .arg("-c")
                .arg(&goal.command)
                .current_dir(&working_dir)
                .output()
                .await
        })
        .await
        .map_err(|_| GoalError::Timeout(goal.timeout_secs))?
        .map_err(GoalError::IoError)?;

        if !output.status.success() {
            let code = output.status.code().unwrap_or(-1);
            return Err(GoalError::CommandFailed(code));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        self.parse_score(&stdout)
    }

    fn parse_score(&self, output: &str) -> GoalResult<f64> {
        let trimmed = output.trim();

        if trimmed.is_empty() {
            return Err(GoalError::ParseError("Empty output".to_string()));
        }

        for line in trimmed.lines().rev() {
            let line = line.trim();
            if let Ok(score) = line.parse::<f64>() {
                if !(0.0..=100.0).contains(&score) {
                    return Err(GoalError::ScoreOutOfRange(score));
                }
                return Ok(score);
            }
        }

        let truncated: String = trimmed.chars().take(100).collect();
        Err(GoalError::ParseError(format!(
            "Could not parse score from: {}",
            truncated
        )))
    }

    pub async fn check_all(&self, goals: &[GoalConfig]) -> VerificationResult {
        let mut results = Vec::with_capacity(goals.len());

        for goal in goals {
            let result = self.check_goal(goal).await;
            results.push(result);
        }

        let all_passed = results.iter().all(|r| r.passed);

        VerificationResult {
            all_passed,
            results,
            checked_at: Utc::now(),
            iteration: 0,
        }
    }

    pub async fn verify_until_complete(
        &self,
        goals: &[GoalConfig],
        options: VerifyOptions,
    ) -> VerificationResult {
        let mut iteration = 0;
        let start = std::time::Instant::now();

        loop {
            iteration += 1;

            let mut result = self.check_all(goals).await;
            result.iteration = iteration;

            if let Some(callback) = &options.on_progress {
                callback(&result);
            }

            if result.all_passed {
                return result;
            }

            if iteration >= options.max_iterations {
                return result;
            }

            if let Some(max_duration) = options.max_duration {
                if start.elapsed() >= max_duration {
                    return result;
                }
            }

            tokio::time::sleep(Duration::from_secs(options.check_interval_secs)).await;
        }
    }
}

pub struct VerifyOptions {
    pub check_interval_secs: u64,
    pub max_iterations: u32,
    pub max_duration: Option<Duration>,
    #[allow(clippy::type_complexity)]
    pub on_progress: Option<Box<dyn Fn(&VerificationResult) + Send + Sync>>,
}

impl Default for VerifyOptions {
    fn default() -> Self {
        Self {
            check_interval_secs: 30,
            max_iterations: 100,
            max_duration: None,
            on_progress: None,
        }
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
    async fn test_simple_score_command() {
        let runner = GoalRunner::new(".");
        let goal = make_goal("test", "echo 85.5", 80.0);
        let result = runner.check_goal(&goal).await;
        assert!(result.passed);
        assert!((result.score - 85.5).abs() < 0.01);
    }

    #[tokio::test]
    async fn test_failing_score() {
        let runner = GoalRunner::new(".");
        let goal = make_goal("test", "echo 50", 80.0);
        let result = runner.check_goal(&goal).await;
        assert!(!result.passed);
        assert!((result.score - 50.0).abs() < 0.01);
    }

    #[tokio::test]
    async fn test_command_failure() {
        let runner = GoalRunner::new(".");
        let goal = make_goal("test", "exit 1", 80.0);
        let result = runner.check_goal(&goal).await;
        assert!(!result.passed);
        assert!(result.error.is_some());
    }

    #[tokio::test]
    async fn test_invalid_output() {
        let runner = GoalRunner::new(".");
        let goal = make_goal("test", "echo 'not a number'", 80.0);
        let result = runner.check_goal(&goal).await;
        assert!(!result.passed);
        assert!(result.error.is_some());
    }

    #[tokio::test]
    async fn test_multiline_output() {
        let runner = GoalRunner::new(".");
        let goal = make_goal("test", "echo 'debug info'; echo 92.5", 80.0);
        let result = runner.check_goal(&goal).await;
        assert!(result.passed);
        assert!((result.score - 92.5).abs() < 0.01);
    }

    #[tokio::test]
    async fn test_disabled_goal() {
        let runner = GoalRunner::new(".");
        let mut goal = make_goal("test", "exit 1", 80.0);
        goal.enabled = false;
        let result = runner.check_goal(&goal).await;
        assert!(result.passed);
    }

    #[tokio::test]
    async fn test_score_out_of_range() {
        let runner = GoalRunner::new(".");
        let goal = make_goal("test", "echo 150", 80.0);
        let result = runner.check_goal(&goal).await;
        assert!(!result.passed);
        assert!(result.error.as_ref().unwrap().contains("out of"));
    }

    #[tokio::test]
    async fn test_check_all() {
        let runner = GoalRunner::new(".");
        let goals = vec![
            make_goal("pass1", "echo 90", 80.0),
            make_goal("pass2", "echo 100", 90.0),
            make_goal("fail1", "echo 50", 80.0),
        ];
        let result = runner.check_all(&goals).await;
        assert!(!result.all_passed);
        assert_eq!(result.results.len(), 3);
        assert_eq!(result.results.iter().filter(|r| r.passed).count(), 2);
    }

    #[test]
    fn test_parse_score_valid() {
        let runner = GoalRunner::new(".");
        assert!((runner.parse_score("85.5\n").unwrap() - 85.5).abs() < 0.01);
        assert!((runner.parse_score("100").unwrap() - 100.0).abs() < 0.01);
        assert!((runner.parse_score("0").unwrap() - 0.0).abs() < 0.01);
        assert!((runner.parse_score("debug\n92.5").unwrap() - 92.5).abs() < 0.01);
    }

    #[test]
    fn test_parse_score_invalid() {
        let runner = GoalRunner::new(".");
        assert!(runner.parse_score("").is_err());
        assert!(runner.parse_score("not a number").is_err());
        assert!(runner.parse_score("150").is_err());
        assert!(runner.parse_score("-10").is_err());
    }

    #[test]
    fn test_goal_check_result_success() {
        let result = GoalCheckResult::success("test".to_string(), 90.0, 80.0, 100);
        assert!(result.passed);
        assert!(result.error.is_none());
    }

    #[test]
    fn test_goal_check_result_failure() {
        let result = GoalCheckResult::failure("test".to_string(), 80.0, "timeout".to_string());
        assert!(!result.passed);
        assert!(result.error.is_some());
    }

    #[test]
    fn test_verification_summary() {
        let result = VerificationResult {
            all_passed: false,
            results: vec![
                GoalCheckResult::success("pass".to_string(), 90.0, 80.0, 100),
                GoalCheckResult::success("fail".to_string(), 50.0, 80.0, 100),
            ],
            checked_at: Utc::now(),
            iteration: 1,
        };
        let summary = result.summary();
        assert!(summary.contains("1/2 passed"));
        assert!(summary.contains("pass"));
        assert!(summary.contains("fail"));
    }
}
