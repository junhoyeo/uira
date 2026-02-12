//! Error classification based on status codes and message content
//!
//! This module provides pattern-based classification of API errors to map
//! HTTP responses and error messages to appropriate ProviderError variants.

use crate::error::ProviderError;
use lazy_static::lazy_static;
use regex::Regex;

const DEFAULT_RATE_LIMIT_RETRY_MS: u64 = 60_000;

lazy_static! {
    // Context overflow patterns
    static ref CONTEXT_OVERFLOW_PATTERNS: Vec<Regex> = vec![
        Regex::new(r"(?i)request_too_large").unwrap(),
        Regex::new(r"(?i)context length exceeded").unwrap(),
        Regex::new(r"(?i)prompt is too long").unwrap(),
        Regex::new(r"(?i)exceeds model context window").unwrap(),
        Regex::new(r"(?i)maximum context length").unwrap(),
    ];

    // Rate limit patterns
    static ref RATE_LIMIT_PATTERNS: Vec<Regex> = vec![
        Regex::new(r"(?i)rate[_\s]?limit").unwrap(),
        Regex::new(r"(?i)too many requests").unwrap(),
        Regex::new(r"(?i)quota exceeded").unwrap(),
    ];

    // Overloaded patterns
    static ref OVERLOADED_PATTERNS: Vec<Regex> = vec![
        Regex::new(r"(?i)overloaded_error").unwrap(),
        Regex::new(r"(?i)\boverloaded\b").unwrap(),
    ];

    // Billing patterns
    static ref BILLING_PATTERNS: Vec<Regex> = vec![
        Regex::new(r"\b402\b").unwrap(),
        Regex::new(r"(?i)payment required").unwrap(),
        Regex::new(r"(?i)insufficient credits").unwrap(),
        Regex::new(r"(?i)billing").unwrap(),
    ];

    // Auth patterns
    static ref AUTH_PATTERNS: Vec<Regex> = vec![
        Regex::new(r"(?i)invalid[_\s]?api[_\s]?key").unwrap(),
        Regex::new(r"(?i)unauthorized").unwrap(),
        Regex::new(r"(?i)token.*expired").unwrap(),
        Regex::new(r"(?i)authentication failed").unwrap(),
    ];

    // Timeout patterns
    static ref TIMEOUT_PATTERNS: Vec<Regex> = vec![
        Regex::new(r"(?i)\btimeout\b").unwrap(),
        Regex::new(r"(?i)timed out").unwrap(),
    ];

    // Message ordering patterns
    static ref MESSAGE_ORDERING_PATTERNS: Vec<Regex> = vec![
        Regex::new(r"(?i)incorrect role information").unwrap(),
        Regex::new(r"(?i)roles must alternate").unwrap(),
    ];

    // Tool input patterns
    static ref TOOL_INPUT_PATTERNS: Vec<Regex> = vec![
        Regex::new(r"(?i)tool_(?:use|call)\.(?:input|arguments).*required").unwrap(),
    ];

    // Image error patterns
    static ref IMAGE_ERROR_PATTERNS: Vec<Regex> = vec![
        Regex::new(r"(?i)image dimensions exceed").unwrap(),
        Regex::new(r"(?i)image exceeds.*mb").unwrap(),
        Regex::new(r"(?i)image.*too large").unwrap(),
    ];
}

/// Check if message matches context overflow patterns
fn is_context_overflow(message: &str) -> bool {
    CONTEXT_OVERFLOW_PATTERNS
        .iter()
        .any(|pattern| pattern.is_match(message))
}

/// Check if message matches rate limit patterns
fn is_rate_limit(message: &str) -> bool {
    RATE_LIMIT_PATTERNS
        .iter()
        .any(|pattern| pattern.is_match(message))
}

/// Check if message matches overloaded patterns
fn is_overloaded(message: &str) -> bool {
    OVERLOADED_PATTERNS
        .iter()
        .any(|pattern| pattern.is_match(message))
}

/// Check if message matches billing patterns
fn is_billing(message: &str) -> bool {
    BILLING_PATTERNS
        .iter()
        .any(|pattern| pattern.is_match(message))
}

/// Check if message matches auth patterns
fn is_auth(message: &str) -> bool {
    AUTH_PATTERNS
        .iter()
        .any(|pattern| pattern.is_match(message))
}

/// Check if message matches timeout patterns
fn is_timeout(message: &str) -> bool {
    TIMEOUT_PATTERNS
        .iter()
        .any(|pattern| pattern.is_match(message))
}

/// Check if message matches message ordering patterns
fn is_message_ordering(message: &str) -> bool {
    MESSAGE_ORDERING_PATTERNS
        .iter()
        .any(|pattern| pattern.is_match(message))
}

/// Check if message matches tool input patterns
fn is_tool_input(message: &str) -> bool {
    TOOL_INPUT_PATTERNS
        .iter()
        .any(|pattern| pattern.is_match(message))
}

/// Check if message matches image error patterns
fn is_image_error(message: &str) -> bool {
    IMAGE_ERROR_PATTERNS
        .iter()
        .any(|pattern| pattern.is_match(message))
}

fn parse_token_counts(message: &str) -> (u64, u64) {
    lazy_static::lazy_static! {
        static ref TOKEN_RE: Regex =
            Regex::new(r"(\d[\d,]*)\s*tokens?\b.*?(\d[\d,]*)\s*(?:token|limit|maximum)\b").unwrap();
    }
    if let Some(caps) = TOKEN_RE.captures(message) {
        let parse = |s: &str| s.replace(',', "").parse::<u64>().unwrap_or(0);
        return (parse(&caps[1]), parse(&caps[2]));
    }
    (0, 0)
}

/// Classify an API error based on status code and message content
///
/// # Arguments
/// * `provider` - Provider name (used for `Unavailable { provider }` errors)
/// * `status` - HTTP status code
/// * `message` - Error message from the API
///
/// # Returns
/// Appropriate `ProviderError` variant based on classification
pub fn classify_error(provider: &str, status: u16, message: &str) -> ProviderError {
    // Check status code first
    match status {
        // HTTP 402 - Payment Required
        402 => {
            return ProviderError::PaymentRequired {
                message: message.to_string(),
            }
        }
        // HTTP 429 - Rate Limited
        429 => {
            return ProviderError::RateLimited {
                retry_after_ms: DEFAULT_RATE_LIMIT_RETRY_MS,
            };
        }
        // HTTP 401/403 - Auth errors
        401 | 403 => {
            return ProviderError::AuthenticationFailed(message.to_string());
        }
        _ => {}
    }

    // Check message patterns
    if is_context_overflow(message) {
        let (used, limit) = parse_token_counts(message);
        return ProviderError::ContextExceeded { used, limit };
    }

    if is_rate_limit(message) {
        return ProviderError::RateLimited {
            retry_after_ms: DEFAULT_RATE_LIMIT_RETRY_MS,
        };
    }

    if is_overloaded(message) {
        return ProviderError::Unavailable {
            provider: provider.to_string(),
        };
    }

    if is_billing(message) {
        return ProviderError::PaymentRequired {
            message: message.to_string(),
        };
    }

    if is_auth(message) {
        return ProviderError::AuthenticationFailed(message.to_string());
    }

    if is_timeout(message) {
        return ProviderError::Timeout {
            message: message.to_string(),
        };
    }

    if is_message_ordering(message) {
        return ProviderError::MessageOrderingConflict;
    }

    if is_tool_input(message) {
        return ProviderError::ToolCallInputMissing;
    }

    if is_image_error(message) {
        return ProviderError::ImageError {
            message: message.to_string(),
        };
    }

    if status >= 500 {
        return ProviderError::Unavailable {
            provider: provider.to_string(),
        };
    }

    // Default to InvalidResponse for unclassified errors
    ProviderError::InvalidResponse(message.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    const PROVIDER: &str = "anthropic";

    #[test]
    fn test_context_overflow_detection() {
        assert!(is_context_overflow("request_too_large"));
        assert!(is_context_overflow("Context length exceeded"));
        assert!(is_context_overflow("Your prompt is too long"));
        assert!(is_context_overflow("exceeds model context window"));
        assert!(!is_context_overflow("some other error"));
    }

    #[test]
    fn test_rate_limit_detection() {
        assert!(is_rate_limit("rate limit exceeded"));
        assert!(is_rate_limit("rate_limit"));
        assert!(is_rate_limit("Too many requests"));
        assert!(is_rate_limit("quota exceeded"));
        assert!(!is_rate_limit("some other error"));
    }

    #[test]
    fn test_overloaded_detection() {
        assert!(is_overloaded("overloaded_error"));
        assert!(is_overloaded("The service is overloaded"));
        assert!(!is_overloaded("some other error"));
    }

    #[test]
    fn test_billing_detection() {
        assert!(is_billing("402 payment required"));
        assert!(is_billing("Payment Required"));
        assert!(is_billing("insufficient credits"));
        assert!(!is_billing("some other error"));
    }

    #[test]
    fn test_auth_detection() {
        assert!(is_auth("invalid_api_key"));
        assert!(is_auth("Invalid API key"));
        assert!(is_auth("Unauthorized"));
        assert!(is_auth("token has expired"));
        assert!(!is_auth("some other error"));
    }

    #[test]
    fn test_timeout_detection() {
        assert!(is_timeout("Request timeout"));
        assert!(is_timeout("Connection timed out"));
        assert!(!is_timeout("some other error"));
    }

    #[test]
    fn test_message_ordering_detection() {
        assert!(is_message_ordering("incorrect role information"));
        assert!(is_message_ordering("Roles must alternate"));
        assert!(!is_message_ordering("some other error"));
    }

    #[test]
    fn test_tool_input_detection() {
        assert!(is_tool_input("tool_use.input is required"));
        assert!(is_tool_input("tool_call.arguments required"));
        assert!(!is_tool_input("some other error"));
    }

    #[test]
    fn test_image_error_detection() {
        assert!(is_image_error("image dimensions exceed maximum"));
        assert!(is_image_error("Image exceeds 5MB"));
        assert!(!is_image_error("some other error"));
    }

    #[test]
    fn test_classify_by_status_code() {
        // HTTP 402 - Payment Required
        let err = classify_error(PROVIDER, 402, "payment required");
        assert!(matches!(err, ProviderError::PaymentRequired { .. }));

        // HTTP 429 - Rate Limited
        let err = classify_error(PROVIDER, 429, "too many requests");
        assert!(matches!(err, ProviderError::RateLimited { .. }));

        // HTTP 529 - Overloaded (maps to Unavailable)
        let err = classify_error(PROVIDER, 529, "service overloaded");
        assert!(matches!(err, ProviderError::Unavailable { .. }));

        let err = classify_error(PROVIDER, 503, "internal server error");
        assert!(matches!(err, ProviderError::Unavailable { .. }));

        // HTTP 401 - Auth Failed
        let err = classify_error(PROVIDER, 401, "unauthorized");
        assert!(matches!(err, ProviderError::AuthenticationFailed(_)));
    }

    #[test]
    fn test_classify_by_message_pattern() {
        // Context overflow
        let err = classify_error(PROVIDER, 400, "context length exceeded");
        assert!(matches!(err, ProviderError::ContextExceeded { .. }));

        // Rate limit
        let err = classify_error(PROVIDER, 500, "rate limit exceeded");
        assert!(matches!(err, ProviderError::RateLimited { .. }));

        // Overloaded
        let err = classify_error(PROVIDER, 503, "overloaded_error");
        assert!(matches!(err, ProviderError::Unavailable { .. }));

        // Billing
        let err = classify_error(PROVIDER, 400, "insufficient credits");
        assert!(matches!(err, ProviderError::PaymentRequired { .. }));

        // Auth
        let err = classify_error(PROVIDER, 400, "invalid_api_key");
        assert!(matches!(err, ProviderError::AuthenticationFailed(_)));

        // Timeout
        let err = classify_error(PROVIDER, 408, "request timeout");
        assert!(matches!(err, ProviderError::Timeout { .. }));

        // Message ordering
        let err = classify_error(PROVIDER, 400, "roles must alternate");
        assert!(matches!(err, ProviderError::MessageOrderingConflict));

        // Tool input
        let err = classify_error(PROVIDER, 400, "tool_use.input is required");
        assert!(matches!(err, ProviderError::ToolCallInputMissing));

        // Image error
        let err = classify_error(PROVIDER, 400, "image dimensions exceed maximum");
        assert!(matches!(err, ProviderError::ImageError { .. }));
    }

    #[test]
    fn test_unclassified_error() {
        let err = classify_error(PROVIDER, 499, "unknown error");
        assert!(matches!(err, ProviderError::InvalidResponse(_)));
    }
}
