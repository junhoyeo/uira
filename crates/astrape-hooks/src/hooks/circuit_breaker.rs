//! Circuit Breaker for Ralph Loop
//!
//! State machine: CLOSED -> HALF_OPEN -> OPEN
//! - CLOSED: Normal operation, monitoring for failures
//! - HALF_OPEN: Detected potential stagnation, need confirmation
//! - OPEN: Circuit tripped, loop should terminate

use regex::Regex;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum CircuitState {
    #[default]
    Closed,
    HalfOpen,
    Open,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CircuitBreakerConfig {
    /// Consecutive no-progress iterations before tripping
    pub no_progress_threshold: u32,
    /// Same error occurrences before tripping
    pub same_error_threshold: u32,
    /// Output decline percentage (0-100) before warning
    pub output_decline_threshold: u32,
}

impl Default for CircuitBreakerConfig {
    fn default() -> Self {
        Self {
            no_progress_threshold: 3,
            same_error_threshold: 5,
            output_decline_threshold: 70,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CircuitBreakerState {
    pub state: CircuitState,
    pub consecutive_no_progress: u32,
    pub error_history: Vec<String>,
    pub output_sizes: Vec<usize>,
    pub trip_reason: Option<String>,
}

impl CircuitBreakerState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record iteration metrics and check for trip conditions
    pub fn record_iteration(
        &mut self,
        files_changed: u32,
        tests_changed: bool,
        output_size: usize,
        error: Option<&str>,
        config: &CircuitBreakerConfig,
    ) -> CircuitState {
        // Track output size
        self.output_sizes.push(output_size);
        if self.output_sizes.len() > 10 {
            self.output_sizes.remove(0);
        }

        // Check no-progress
        if files_changed == 0 && !tests_changed {
            self.consecutive_no_progress += 1;
        } else {
            self.consecutive_no_progress = 0;
        }

        // Check same error
        if let Some(err) = error {
            let normalized = Self::normalize_error(err);
            self.error_history.push(normalized.clone());
            if self.error_history.len() > 10 {
                self.error_history.remove(0);
            }

            // Count consecutive same errors
            let same_count = self
                .error_history
                .iter()
                .rev()
                .take_while(|e| **e == normalized)
                .count();

            if same_count >= config.same_error_threshold as usize {
                self.state = CircuitState::Open;
                self.trip_reason = Some(format!("Same error {} times: {}", same_count, err));
                return self.state;
            }
        }

        // Check no-progress threshold
        if self.consecutive_no_progress >= config.no_progress_threshold {
            self.state = CircuitState::Open;
            self.trip_reason = Some(format!(
                "No progress for {} iterations",
                self.consecutive_no_progress
            ));
            return self.state;
        }

        // Check output decline (need at least 6 samples for meaningful comparison)
        if self.output_sizes.len() >= 6 {
            let avg_recent: usize = self.output_sizes.iter().rev().take(3).sum::<usize>() / 3;
            let earlier_count = self.output_sizes.len() - 3;
            let avg_earlier: usize =
                self.output_sizes.iter().take(earlier_count).sum::<usize>() / earlier_count;

            if avg_earlier > 0 {
                let decline = 100 - (avg_recent * 100 / avg_earlier);
                if decline >= config.output_decline_threshold as usize {
                    self.state = CircuitState::HalfOpen;
                }
            }
        }

        self.state
    }

    fn normalize_error(error: &str) -> String {
        let mut result = error.to_string();
        // Remove ISO timestamp
        if let Ok(re) = Regex::new(r"\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}") {
            result = re.replace_all(&result, "").to_string();
        }
        // Remove line:col
        if let Ok(re) = Regex::new(r":\d+:\d+") {
            result = re.replace_all(&result, "").to_string();
        }
        result.trim().to_lowercase()
    }

    pub fn is_tripped(&self) -> bool {
        self.state == CircuitState::Open
    }

    pub fn reset(&mut self) {
        self.state = CircuitState::Closed;
        self.consecutive_no_progress = 0;
        self.error_history.clear();
        self.trip_reason = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_circuit_initial_state() {
        let cb = CircuitBreakerState::new();
        assert_eq!(cb.state, CircuitState::Closed);
        assert!(!cb.is_tripped());
    }

    #[test]
    fn test_circuit_breaker_no_progress() {
        let mut cb = CircuitBreakerState::new();
        let config = CircuitBreakerConfig::default();

        // 3 iterations with no progress should trip
        for _ in 0..3 {
            cb.record_iteration(0, false, 100, None, &config);
        }
        assert!(cb.is_tripped());
        assert!(cb.trip_reason.as_ref().unwrap().contains("No progress"));
    }

    #[test]
    fn test_circuit_breaker_same_error() {
        let mut cb = CircuitBreakerState::new();
        let config = CircuitBreakerConfig::default();

        // 5 same errors should trip
        for _ in 0..5 {
            cb.record_iteration(1, true, 100, Some("Error: undefined"), &config);
        }
        assert!(cb.is_tripped());
        assert!(cb.trip_reason.as_ref().unwrap().contains("Same error"));
    }

    #[test]
    fn test_error_normalization() {
        let error1 = "Error at 2024-01-15T10:30:00 in main.rs:42:10";
        let error2 = "Error at 2024-01-16T11:45:00 in main.rs:42:10";
        assert_eq!(
            CircuitBreakerState::normalize_error(error1),
            CircuitBreakerState::normalize_error(error2)
        );
    }

    #[test]
    fn test_reset() {
        let mut cb = CircuitBreakerState::new();
        cb.state = CircuitState::Open;
        cb.trip_reason = Some("test".to_string());
        cb.consecutive_no_progress = 5;

        cb.reset();
        assert_eq!(cb.state, CircuitState::Closed);
        assert!(cb.trip_reason.is_none());
        assert_eq!(cb.consecutive_no_progress, 0);
    }

    #[test]
    fn test_progress_resets_counter() {
        let mut cb = CircuitBreakerState::new();
        let config = CircuitBreakerConfig::default();

        // 2 no-progress iterations
        cb.record_iteration(0, false, 100, None, &config);
        cb.record_iteration(0, false, 100, None, &config);
        assert_eq!(cb.consecutive_no_progress, 2);

        // Progress resets counter
        cb.record_iteration(1, false, 100, None, &config);
        assert_eq!(cb.consecutive_no_progress, 0);
    }
}
