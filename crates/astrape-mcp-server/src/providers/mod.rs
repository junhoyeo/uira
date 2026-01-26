pub mod gemini;
pub mod openai;

pub use gemini::GeminiProvider;
pub use openai::OpenAIProvider;

use std::time::Duration;
use tokio::time::timeout;

pub trait Provider {
    fn query(
        &self,
        prompt: &str,
        model: &str,
    ) -> impl std::future::Future<Output = Result<String, String>> + Send;
}

/// Retry HTTP request with exponential backoff
pub async fn retry_with_backoff<F, Fut, T>(operation: F, max_retries: u32) -> Result<T, String>
where
    F: Fn() -> Fut,
    Fut: std::future::Future<Output = Result<T, String>>,
{
    let mut attempt = 0;
    let timeout_secs = std::env::var("PROVIDER_TIMEOUT_SECS")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(120);

    loop {
        attempt += 1;

        let result = timeout(Duration::from_secs(timeout_secs), operation()).await;

        match result {
            Ok(Ok(value)) => return Ok(value),
            Ok(Err(e)) if attempt >= max_retries => {
                return Err(format!("Failed after {} attempts: {}", max_retries, e));
            }
            Ok(Err(e)) => {
                let backoff_secs = 2u64.pow(attempt - 1); // 1s, 2s, 4s
                tracing::warn!(
                    attempt = attempt,
                    max_retries = max_retries,
                    backoff_secs = backoff_secs,
                    error = %e,
                    "Request failed, retrying after backoff"
                );
                tokio::time::sleep(Duration::from_secs(backoff_secs)).await;
            }
            Err(_) => {
                return Err(format!(
                    "Request timed out after {}s (attempt {}/{})",
                    timeout_secs, attempt, max_retries
                ));
            }
        }
    }
}
