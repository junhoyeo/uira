//! Token monitoring for context window management

use uira_types::TokenUsage;

#[derive(Debug, Clone)]
pub struct TokenMonitor {
    model_limit: usize,
    threshold: f64,
    protected_tokens: usize,
}

impl TokenMonitor {
    pub fn new(model_limit: usize) -> Self {
        Self {
            model_limit,
            threshold: 0.8,
            protected_tokens: 40_000,
        }
    }

    pub fn with_threshold(mut self, threshold: f64) -> Self {
        self.threshold = threshold.clamp(0.0, 1.0);
        self
    }

    pub fn with_protected_tokens(mut self, protected_tokens: usize) -> Self {
        self.protected_tokens = protected_tokens;
        self
    }

    pub fn is_overflow(&self, current_tokens: usize) -> bool {
        current_tokens > self.model_limit
    }

    pub fn needs_compaction(&self, current_tokens: usize) -> bool {
        let threshold_tokens = (self.model_limit as f64 * self.threshold) as usize;
        current_tokens >= threshold_tokens
    }

    pub fn compactable_tokens(&self, current_tokens: usize) -> usize {
        current_tokens.saturating_sub(self.protected_tokens)
    }

    pub fn usage_ratio(&self, current_tokens: usize) -> f64 {
        if self.model_limit == 0 {
            return 1.0;
        }
        current_tokens as f64 / self.model_limit as f64
    }

    pub fn model_limit(&self) -> usize {
        self.model_limit
    }

    pub fn threshold(&self) -> f64 {
        self.threshold
    }

    pub fn protected_tokens(&self) -> usize {
        self.protected_tokens
    }

    pub fn from_usage(usage: &TokenUsage, model_limit: usize) -> TokenMonitorSnapshot {
        let current = usage.input_tokens as usize;
        let monitor = Self::new(model_limit);
        TokenMonitorSnapshot {
            current_tokens: current,
            model_limit,
            usage_ratio: monitor.usage_ratio(current),
            needs_compaction: monitor.needs_compaction(current),
            is_overflow: monitor.is_overflow(current),
        }
    }
}

impl Default for TokenMonitor {
    fn default() -> Self {
        Self::new(100_000)
    }
}

#[derive(Debug, Clone)]
pub struct TokenMonitorSnapshot {
    pub current_tokens: usize,
    pub model_limit: usize,
    pub usage_ratio: f64,
    pub needs_compaction: bool,
    pub is_overflow: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_token_monitor_basic() {
        let monitor = TokenMonitor::new(100_000);
        assert!(!monitor.is_overflow(50_000));
        assert!(monitor.is_overflow(150_000));
    }

    #[test]
    fn test_needs_compaction() {
        let monitor = TokenMonitor::new(100_000).with_threshold(0.8);
        assert!(!monitor.needs_compaction(70_000));
        assert!(monitor.needs_compaction(80_000));
        assert!(monitor.needs_compaction(90_000));
    }

    #[test]
    fn test_compactable_tokens() {
        let monitor = TokenMonitor::new(100_000).with_protected_tokens(40_000);
        assert_eq!(monitor.compactable_tokens(80_000), 40_000);
        assert_eq!(monitor.compactable_tokens(30_000), 0);
    }

    #[test]
    fn test_usage_ratio() {
        let monitor = TokenMonitor::new(100_000);
        assert!((monitor.usage_ratio(50_000) - 0.5).abs() < 0.01);
        assert!((monitor.usage_ratio(100_000) - 1.0).abs() < 0.01);
    }
}
