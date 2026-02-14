//! Cost estimation utilities for model usage

/// Model pricing per 1M tokens (input)
const HAIKU_INPUT: f64 = 0.25;
/// Model pricing per 1M tokens (output)
const HAIKU_OUTPUT: f64 = 1.25;
/// Model pricing per 1M tokens (input)
const SONNET_INPUT: f64 = 3.0;
/// Model pricing per 1M tokens (output)
const SONNET_OUTPUT: f64 = 15.0;
/// Model pricing per 1M tokens (input)
const OPUS_INPUT: f64 = 15.0;
/// Model pricing per 1M tokens (output)
const OPUS_OUTPUT: f64 = 75.0;

/// Cost estimator for model usage
pub struct CostEstimator;

impl CostEstimator {
    /// Estimate token count from text (rough approximation: ~4 chars per token)
    pub fn estimate_tokens(text: &str) -> usize {
        // Basic estimation: ~4 characters per token
        // This is a rough approximation; actual tokenization varies by model
        (text.chars().count() as f64 / 4.0).ceil() as usize
    }

    /// Estimate cost in USD for given token counts and model
    ///
    /// # Arguments
    /// * `input_tokens` - Number of input tokens
    /// * `output_tokens` - Number of output tokens
    /// * `model` - Model name (haiku, sonnet, opus, or specific model IDs)
    ///
    /// # Returns
    /// Cost in USD
    pub fn estimate_cost(input_tokens: usize, output_tokens: usize, model: &str) -> f64 {
        let (input_price, output_price) = Self::get_pricing(model);

        let input_cost = (input_tokens as f64 / 1_000_000.0) * input_price;
        let output_cost = (output_tokens as f64 / 1_000_000.0) * output_price;

        input_cost + output_cost
    }

    /// Get pricing for a specific model
    fn get_pricing(model: &str) -> (f64, f64) {
        let model_lower = model.to_lowercase();

        if model_lower.contains("haiku") {
            (HAIKU_INPUT, HAIKU_OUTPUT)
        } else if model_lower.contains("sonnet") {
            (SONNET_INPUT, SONNET_OUTPUT)
        } else if model_lower.contains("opus") {
            (OPUS_INPUT, OPUS_OUTPUT)
        } else if model_lower.contains("gpt-5-nano") || model_lower.contains("opencode") {
            // Assume ultra-cheap for specialized models
            (0.1, 0.5)
        } else {
            // Default to Sonnet pricing if unknown
            (SONNET_INPUT, SONNET_OUTPUT)
        }
    }

    /// Format cost as a human-readable string
    pub fn format_cost(cost: f64) -> String {
        if cost < 0.01 {
            format!("${:.4}", cost)
        } else if cost < 1.0 {
            format!("${:.3}", cost)
        } else {
            format!("${:.2}", cost)
        }
    }

    /// Estimate cost from text directly
    pub fn estimate_text_cost(input_text: &str, output_text: &str, model: &str) -> f64 {
        let input_tokens = Self::estimate_tokens(input_text);
        let output_tokens = Self::estimate_tokens(output_text);
        Self::estimate_cost(input_tokens, output_tokens, model)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_token_estimation() {
        let text = "Hello, world!";
        let tokens = CostEstimator::estimate_tokens(text);
        // "Hello, world!" is 13 chars, so ~3-4 tokens
        assert!(tokens >= 3 && tokens <= 4);
    }

    #[test]
    fn test_cost_estimation_haiku() {
        // 1M input tokens, 1M output tokens
        let cost = CostEstimator::estimate_cost(1_000_000, 1_000_000, "haiku");
        assert_eq!(cost, HAIKU_INPUT + HAIKU_OUTPUT);
    }

    #[test]
    fn test_cost_estimation_sonnet() {
        // 1M input tokens, 1M output tokens
        let cost = CostEstimator::estimate_cost(1_000_000, 1_000_000, "sonnet");
        assert_eq!(cost, SONNET_INPUT + SONNET_OUTPUT);
    }

    #[test]
    fn test_cost_estimation_opus() {
        // 1M input tokens, 1M output tokens
        let cost = CostEstimator::estimate_cost(1_000_000, 1_000_000, "opus");
        assert_eq!(cost, OPUS_INPUT + OPUS_OUTPUT);
    }

    #[test]
    fn test_format_cost() {
        assert_eq!(CostEstimator::format_cost(0.001), "$0.0010");
        assert_eq!(CostEstimator::format_cost(0.1), "$0.100");
        assert_eq!(CostEstimator::format_cost(1.5), "$1.50");
    }
}
