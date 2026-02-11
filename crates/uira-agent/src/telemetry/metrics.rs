use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::RwLock;

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct TokenMetrics {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_write_tokens: u64,
}

impl TokenMetrics {
    pub fn total(&self) -> u64 {
        self.input_tokens + self.output_tokens
    }

    pub fn add(&mut self, other: &TokenMetrics) {
        self.input_tokens += other.input_tokens;
        self.output_tokens += other.output_tokens;
        self.cache_read_tokens += other.cache_read_tokens;
        self.cache_write_tokens += other.cache_write_tokens;
    }
}

#[derive(Debug, Default)]
pub struct MetricsCollector {
    total_input_tokens: AtomicU64,
    total_output_tokens: AtomicU64,
    total_cache_read: AtomicU64,
    total_cache_write: AtomicU64,
    tool_calls: AtomicU64,
    tool_errors: AtomicU64,
    turns_completed: AtomicU64,
    sessions_started: AtomicU64,
    per_model_tokens: RwLock<HashMap<String, TokenMetrics>>,
}

impl MetricsCollector {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn record_tokens(&self, model: &str, metrics: &TokenMetrics) {
        self.total_input_tokens
            .fetch_add(metrics.input_tokens, Ordering::Relaxed);
        self.total_output_tokens
            .fetch_add(metrics.output_tokens, Ordering::Relaxed);
        self.total_cache_read
            .fetch_add(metrics.cache_read_tokens, Ordering::Relaxed);
        self.total_cache_write
            .fetch_add(metrics.cache_write_tokens, Ordering::Relaxed);

        if let Ok(mut per_model) = self.per_model_tokens.write() {
            per_model.entry(model.to_string()).or_default().add(metrics);
        }
    }

    pub fn record_tool_call(&self, success: bool) {
        self.tool_calls.fetch_add(1, Ordering::Relaxed);
        if !success {
            self.tool_errors.fetch_add(1, Ordering::Relaxed);
        }
    }

    pub fn record_turn_completed(&self) {
        self.turns_completed.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_session_started(&self) {
        self.sessions_started.fetch_add(1, Ordering::Relaxed);
    }

    pub fn total_tokens(&self) -> TokenMetrics {
        TokenMetrics {
            input_tokens: self.total_input_tokens.load(Ordering::Relaxed),
            output_tokens: self.total_output_tokens.load(Ordering::Relaxed),
            cache_read_tokens: self.total_cache_read.load(Ordering::Relaxed),
            cache_write_tokens: self.total_cache_write.load(Ordering::Relaxed),
        }
    }

    pub fn tool_stats(&self) -> (u64, u64) {
        (
            self.tool_calls.load(Ordering::Relaxed),
            self.tool_errors.load(Ordering::Relaxed),
        )
    }

    pub fn turns_completed(&self) -> u64 {
        self.turns_completed.load(Ordering::Relaxed)
    }

    pub fn sessions_started(&self) -> u64 {
        self.sessions_started.load(Ordering::Relaxed)
    }

    pub fn per_model_tokens(&self) -> HashMap<String, TokenMetrics> {
        self.per_model_tokens
            .read()
            .map(|m| m.clone())
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metrics_collector() {
        let collector = MetricsCollector::new();

        collector.record_tokens(
            "claude-sonnet",
            &TokenMetrics {
                input_tokens: 100,
                output_tokens: 50,
                cache_read_tokens: 20,
                cache_write_tokens: 10,
            },
        );

        let total = collector.total_tokens();
        assert_eq!(total.input_tokens, 100);
        assert_eq!(total.output_tokens, 50);
        assert_eq!(total.total(), 150);
    }

    #[test]
    fn test_tool_stats() {
        let collector = MetricsCollector::new();

        collector.record_tool_call(true);
        collector.record_tool_call(true);
        collector.record_tool_call(false);

        let (calls, errors) = collector.tool_stats();
        assert_eq!(calls, 3);
        assert_eq!(errors, 1);
    }
}
