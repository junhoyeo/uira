//! Retry logic with exponential backoff for initial connection attempts
//!
//! This module provides retry functionality for transient failures during
//! initial connection establishment. It does NOT retry mid-stream errors.

use crate::error::ProviderError;
use std::future::Future;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::time::{sleep, Duration};
use tracing::{debug, warn};

/// Configuration for retry behavior with exponential backoff
#[derive(Debug, Clone)]
pub struct RetryConfig {
    /// Maximum number of retry attempts (including initial attempt)
    pub max_attempts: u32,
    /// Initial delay in milliseconds before first retry
    pub initial_delay_ms: u64,
    /// Maximum delay in milliseconds between retries
    pub max_delay_ms: u64,
    /// Multiplier for exponential backoff (e.g., 2.0 doubles delay each time)
    pub backoff_multiplier: f64,
    /// Jitter factor (0.0-1.0) to add randomness and prevent thundering herd
    pub jitter_factor: f64,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            initial_delay_ms: 500,
            max_delay_ms: 60_000,
            backoff_multiplier: 2.0,
            jitter_factor: 0.1,
        }
    }
}

impl RetryConfig {
    /// Calculate delay for a given attempt with exponential backoff and jitter
    fn calculate_delay(&self, attempt: u32) -> u64 {
        // Calculate base delay with exponential backoff
        let base_delay =
            self.initial_delay_ms as f64 * self.backoff_multiplier.powi(attempt as i32 - 1);

        // Cap at max delay
        let capped_delay = base_delay.min(self.max_delay_ms as f64);

        // Add jitter using simple time-based pseudo-random
        let jitter = self.calculate_jitter(capped_delay);

        (capped_delay + jitter) as u64
    }

    /// Calculate jitter using time-based pseudo-random value
    fn calculate_jitter(&self, delay: f64) -> f64 {
        // Use current time as pseudo-random seed
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();

        // Simple pseudo-random: use lower bits of nanoseconds
        let random_factor = ((now % 1000) as f64) / 1000.0;

        // Apply jitter: random value between -jitter_factor and +jitter_factor
        let jitter_range = delay * self.jitter_factor;
        (random_factor * 2.0 - 1.0) * jitter_range
    }
}

/// Retry an async operation with exponential backoff
///
/// This function wraps an operation and retries it on retryable errors.
/// It respects `retry_after_ms` hints from rate limit errors and adds
/// jitter to prevent thundering herd problems.
///
/// # Arguments
/// * `config` - Retry configuration
/// * `operation` - Async operation to retry (must be FnMut to allow multiple calls)
///
/// # Returns
/// * `Ok(T)` - Operation succeeded
/// * `Err(ProviderError)` - Operation failed after all retries
///
/// # Example
/// ```ignore
/// let config = RetryConfig::default();
/// let result = with_retry(&config, || async {
///     make_api_call().await
/// }).await?;
/// ```
pub async fn with_retry<T, F, Fut>(
    config: &RetryConfig,
    mut operation: F,
) -> Result<T, ProviderError>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T, ProviderError>>,
{
    let mut attempt = 1;

    loop {
        debug!(
            attempt = attempt,
            max_attempts = config.max_attempts,
            "Attempting operation"
        );

        match operation().await {
            Ok(result) => {
                if attempt > 1 {
                    debug!(attempt = attempt, "Operation succeeded after retry");
                }
                return Ok(result);
            }
            Err(err) => {
                // Check if we should retry
                if !err.is_retryable() {
                    debug!(
                        error = ?err,
                        "Error is not retryable, failing immediately"
                    );
                    return Err(err);
                }

                // Check if we've exhausted retries
                if attempt >= config.max_attempts {
                    warn!(
                        attempt = attempt,
                        max_attempts = config.max_attempts,
                        error = ?err,
                        "Max retry attempts reached, failing"
                    );
                    return Err(err);
                }

                // Calculate delay (respect retry_after_ms from rate limits)
                let delay_ms = if let Some(retry_after) = err.retry_after_ms() {
                    debug!(
                        retry_after_ms = retry_after,
                        "Using retry_after hint from rate limit error"
                    );
                    retry_after
                } else {
                    config.calculate_delay(attempt)
                };

                warn!(
                    attempt = attempt,
                    max_attempts = config.max_attempts,
                    delay_ms = delay_ms,
                    error = ?err,
                    "Operation failed, retrying after delay"
                );

                // Wait before retry
                sleep(Duration::from_millis(delay_ms)).await;

                attempt += 1;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = RetryConfig::default();
        assert_eq!(config.max_attempts, 3);
        assert_eq!(config.initial_delay_ms, 500);
        assert_eq!(config.max_delay_ms, 60_000);
        assert_eq!(config.backoff_multiplier, 2.0);
        assert_eq!(config.jitter_factor, 0.1);
    }

    #[test]
    fn test_calculate_delay_exponential() {
        let config = RetryConfig {
            max_attempts: 5,
            initial_delay_ms: 100,
            max_delay_ms: 10_000,
            backoff_multiplier: 2.0,
            jitter_factor: 0.0, // No jitter for predictable test
        };

        // First retry: 100ms
        let delay1 = config.calculate_delay(1);
        assert_eq!(delay1, 100);

        // Second retry: 200ms
        let delay2 = config.calculate_delay(2);
        assert_eq!(delay2, 200);

        // Third retry: 400ms
        let delay3 = config.calculate_delay(3);
        assert_eq!(delay3, 400);
    }

    #[test]
    fn test_calculate_delay_capped() {
        let config = RetryConfig {
            max_attempts: 10,
            initial_delay_ms: 1000,
            max_delay_ms: 5_000,
            backoff_multiplier: 2.0,
            jitter_factor: 0.0,
        };

        // Should cap at max_delay_ms
        let delay = config.calculate_delay(10);
        assert_eq!(delay, 5_000);
    }

    #[tokio::test]
    async fn test_retry_success_first_attempt() {
        let config = RetryConfig::default();

        let result = with_retry(&config, || async { Ok::<_, ProviderError>(42) }).await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 42);
    }

    #[tokio::test]
    async fn test_retry_success_after_retries() {
        use std::sync::atomic::{AtomicU32, Ordering};
        use std::sync::Arc;

        let config = RetryConfig {
            max_attempts: 3,
            initial_delay_ms: 10,
            ..Default::default()
        };
        let call_count = Arc::new(AtomicU32::new(0));
        let call_count_clone = call_count.clone();

        let result = with_retry(&config, move || {
            let count = call_count_clone.fetch_add(1, Ordering::SeqCst) + 1;
            async move {
                if count < 3 {
                    Err(ProviderError::Timeout {
                        message: "test timeout".to_string(),
                    })
                } else {
                    Ok::<_, ProviderError>(42)
                }
            }
        })
        .await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 42);
        assert_eq!(call_count.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn test_retry_non_retryable_error() {
        use std::sync::atomic::{AtomicU32, Ordering};
        use std::sync::Arc;

        let config = RetryConfig::default();
        let call_count = Arc::new(AtomicU32::new(0));
        let call_count_clone = call_count.clone();

        let result = with_retry(&config, move || {
            call_count_clone.fetch_add(1, Ordering::SeqCst);
            async move {
                Err::<i32, _>(ProviderError::PaymentRequired {
                    message: "test".to_string(),
                })
            }
        })
        .await;

        assert!(result.is_err());
        assert_eq!(call_count.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn test_retry_max_attempts_exceeded() {
        use std::sync::atomic::{AtomicU32, Ordering};
        use std::sync::Arc;

        let config = RetryConfig {
            max_attempts: 2,
            initial_delay_ms: 10,
            ..Default::default()
        };
        let call_count = Arc::new(AtomicU32::new(0));
        let call_count_clone = call_count.clone();

        let result = with_retry(&config, move || {
            call_count_clone.fetch_add(1, Ordering::SeqCst);
            async move {
                Err::<i32, _>(ProviderError::Timeout {
                    message: "test".to_string(),
                })
            }
        })
        .await;

        assert!(result.is_err());
        assert_eq!(call_count.load(Ordering::SeqCst), 2);
    }
}
