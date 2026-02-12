//! OpenAI error classification with nested error parsing
//!
//! OpenAI returns errors in the format: `{"error": {"message": "...", "type": "...", "code": "..."}}`
//! This module provides pattern-based classification to map these to appropriate ProviderError variants.

use crate::error::ProviderError;
use lazy_static::lazy_static;
use regex::Regex;
use serde::Deserialize;

const DEFAULT_RATE_LIMIT_RETRY_MS: u64 = 60_000;
const PROVIDER_NAME: &str = "openai";

lazy_static! {
    // Context overflow patterns (OpenAI-specific)
    static ref CONTEXT_OVERFLOW_PATTERNS: Vec<Regex> = vec![
        Regex::new(r"(?i)context_length_exceeded").unwrap(),
        Regex::new(r"(?i)maximum context length").unwrap(),
        Regex::new(r"(?i)token limit").unwrap(),
        Regex::new(r"(?i)exceeds.*token.*limit").unwrap(),
        Regex::new(r"(?i)reduce.*(prompt|input|context)").unwrap(),
    ];

    // Rate limit patterns
    static ref RATE_LIMIT_PATTERNS: Vec<Regex> = vec![
        Regex::new(r"(?i)rate_limit_exceeded").unwrap(),
        Regex::new(r"(?i)rate[_\s]?limit").unwrap(),
        Regex::new(r"(?i)too many requests").unwrap(),
        Regex::new(r"(?i)resource_exhausted").unwrap(),
        Regex::new(r"(?i)quota exceeded").unwrap(),
    ];

    // Billing/quota patterns (OpenAI-specific)
    static ref BILLING_PATTERNS: Vec<Regex> = vec![
        Regex::new(r"(?i)insufficient_quota").unwrap(),
        Regex::new(r"(?i)billing").unwrap(),
        Regex::new(r"(?i)exceeded.*quota").unwrap(),
        Regex::new(r"(?i)payment required").unwrap(),
        Regex::new(r"(?i)credit balance").unwrap(),
        Regex::new(r"\b402\b").unwrap(),
    ];

    // Auth patterns
    static ref AUTH_PATTERNS: Vec<Regex> = vec![
        Regex::new(r"(?i)invalid[_\s]?api[_\s]?key").unwrap(),
        Regex::new(r"(?i)incorrect api key").unwrap(),
        Regex::new(r"(?i)invalid token").unwrap(),
        Regex::new(r"(?i)unauthorized").unwrap(),
        Regex::new(r"(?i)forbidden").unwrap(),
        Regex::new(r"(?i)access denied").unwrap(),
        Regex::new(r"(?i)token.*expired").unwrap(),
        Regex::new(r"(?i)authentication").unwrap(),
    ];

    // Timeout patterns
    static ref TIMEOUT_PATTERNS: Vec<Regex> = vec![
        Regex::new(r"(?i)\btimeout\b").unwrap(),
        Regex::new(r"(?i)timed out").unwrap(),
        Regex::new(r"(?i)deadline exceeded").unwrap(),
    ];

    // Overloaded patterns
    static ref OVERLOADED_PATTERNS: Vec<Regex> = vec![
        Regex::new(r"(?i)overloaded").unwrap(),
        Regex::new(r"(?i)server_error").unwrap(),
        Regex::new(r"(?i)service unavailable").unwrap(),
    ];

    // Token count extraction
    static ref TOKEN_RE: Regex =
        Regex::new(r"(\d[\d,]*)\s*tokens?\b.*?(\d[\d,]*)\s*(?:token|limit|maximum)\b").unwrap();
}

/// Flat error response format (rare but possible)
#[derive(Debug, Deserialize)]
struct FlatErrorResponse {
    message: Option<String>,
    #[serde(rename = "type")]
    error_type: Option<String>,
    code: Option<String>,
}

/// Nested error response format (standard OpenAI format)
/// `{"error": {"message": "...", "type": "...", "code": "..."}}`
#[derive(Debug, Deserialize)]
struct NestedErrorResponse {
    error: NestedError,
}

#[derive(Debug, Deserialize)]
struct NestedError {
    message: Option<String>,
    #[serde(rename = "type")]
    error_type: Option<String>,
    code: Option<String>,
    param: Option<String>,
}

#[derive(Debug, Default)]
pub struct OpenAIErrorInfo {
    pub message: String,
    #[allow(dead_code)]
    pub error_type: Option<String>,
    pub code: Option<String>,
    #[allow(dead_code)]
    pub param: Option<String>,
}

/// Parse OpenAI error body, trying nested format first then flat
pub fn parse_error_body(body: &str) -> OpenAIErrorInfo {
    // Try nested format first (standard OpenAI format)
    if let Ok(nested) = serde_json::from_str::<NestedErrorResponse>(body) {
        return OpenAIErrorInfo {
            message: nested.error.message.unwrap_or_else(|| body.to_string()),
            error_type: nested.error.error_type,
            code: nested.error.code,
            param: nested.error.param,
        };
    }

    // Try flat format as fallback
    if let Ok(flat) = serde_json::from_str::<FlatErrorResponse>(body) {
        if flat.message.is_some() || flat.error_type.is_some() || flat.code.is_some() {
            return OpenAIErrorInfo {
                message: flat.message.unwrap_or_else(|| body.to_string()),
                error_type: flat.error_type,
                code: flat.code,
                param: None,
            };
        }
    }

    // Return raw body if parsing fails
    OpenAIErrorInfo {
        message: body.to_string(),
        error_type: None,
        code: None,
        param: None,
    }
}

/// Check if message matches context overflow patterns
fn is_context_overflow(message: &str, code: Option<&str>) -> bool {
    if code == Some("context_length_exceeded") {
        return true;
    }
    CONTEXT_OVERFLOW_PATTERNS
        .iter()
        .any(|pattern| pattern.is_match(message))
}

/// Check if message matches rate limit patterns
fn is_rate_limit(message: &str, code: Option<&str>) -> bool {
    if code == Some("rate_limit_exceeded") {
        return true;
    }
    RATE_LIMIT_PATTERNS
        .iter()
        .any(|pattern| pattern.is_match(message))
}

/// Check if message matches billing patterns
fn is_billing(message: &str, code: Option<&str>) -> bool {
    if code == Some("insufficient_quota") {
        return true;
    }
    BILLING_PATTERNS
        .iter()
        .any(|pattern| pattern.is_match(message))
}

/// Check if message matches auth patterns
fn is_auth(message: &str, code: Option<&str>) -> bool {
    if code == Some("invalid_api_key") {
        return true;
    }
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

/// Check if message matches overloaded patterns
fn is_overloaded(message: &str) -> bool {
    OVERLOADED_PATTERNS
        .iter()
        .any(|pattern| pattern.is_match(message))
}

/// Parse token counts from error message
fn parse_token_counts(message: &str) -> (u64, u64) {
    if let Some(caps) = TOKEN_RE.captures(message) {
        let parse = |s: &str| s.replace(',', "").parse::<u64>().unwrap_or(0);
        return (parse(&caps[1]), parse(&caps[2]));
    }
    (0, 0)
}

/// Classify an OpenAI API error based on status code and parsed error info
///
/// # Arguments
/// * `status` - HTTP status code
/// * `body` - Raw error response body
///
/// # Returns
/// Appropriate `ProviderError` variant based on classification
pub fn classify_error(status: u16, body: &str) -> ProviderError {
    let info = parse_error_body(body);
    let message = &info.message;
    let code = info.code.as_deref();

    // Check status code first (most reliable signal)
    match status {
        // HTTP 402 - Payment Required
        402 => {
            return ProviderError::PaymentRequired {
                message: message.clone(),
            };
        }
        // HTTP 429 - Rate Limited
        429 => {
            return ProviderError::RateLimited {
                retry_after_ms: DEFAULT_RATE_LIMIT_RETRY_MS,
            };
        }
        // HTTP 401/403 - Auth errors
        401 | 403 => {
            return ProviderError::AuthenticationFailed(message.clone());
        }
        _ => {}
    }

    // Check error code and message patterns
    if is_context_overflow(message, code) {
        let (used, limit) = parse_token_counts(message);
        return ProviderError::ContextExceeded { used, limit };
    }

    if is_rate_limit(message, code) {
        return ProviderError::RateLimited {
            retry_after_ms: DEFAULT_RATE_LIMIT_RETRY_MS,
        };
    }

    if is_billing(message, code) {
        return ProviderError::PaymentRequired {
            message: message.clone(),
        };
    }

    if is_auth(message, code) {
        return ProviderError::AuthenticationFailed(message.clone());
    }

    if is_timeout(message) {
        return ProviderError::Timeout {
            message: message.clone(),
        };
    }

    if is_overloaded(message) {
        return ProviderError::Unavailable {
            provider: PROVIDER_NAME.to_string(),
        };
    }

    // Server errors (5xx)
    if status >= 500 {
        return ProviderError::Unavailable {
            provider: PROVIDER_NAME.to_string(),
        };
    }

    // Default to InvalidResponse for unclassified errors
    ProviderError::InvalidResponse(message.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_nested_error() {
        let body = r#"{"error": {"message": "Rate limit exceeded", "type": "rate_limit_error", "code": "rate_limit_exceeded"}}"#;
        let info = parse_error_body(body);
        assert_eq!(info.message, "Rate limit exceeded");
        assert_eq!(info.error_type.as_deref(), Some("rate_limit_error"));
        assert_eq!(info.code.as_deref(), Some("rate_limit_exceeded"));
    }

    #[test]
    fn test_parse_flat_error() {
        let body = r#"{"message": "Invalid API key", "type": "invalid_request_error", "code": "invalid_api_key"}"#;
        let info = parse_error_body(body);
        assert_eq!(info.message, "Invalid API key");
        assert_eq!(info.code.as_deref(), Some("invalid_api_key"));
    }

    #[test]
    fn test_parse_raw_body() {
        let body = "Something went wrong";
        let info = parse_error_body(body);
        assert_eq!(info.message, "Something went wrong");
        assert!(info.code.is_none());
    }

    #[test]
    fn test_classify_rate_limit_by_status() {
        let err = classify_error(429, r#"{"error": {"message": "Rate limit exceeded"}}"#);
        assert!(matches!(err, ProviderError::RateLimited { .. }));
    }

    #[test]
    fn test_classify_rate_limit_by_code() {
        let err = classify_error(
            400,
            r#"{"error": {"message": "Please retry", "code": "rate_limit_exceeded"}}"#,
        );
        assert!(matches!(err, ProviderError::RateLimited { .. }));
    }

    #[test]
    fn test_classify_context_overflow() {
        let err = classify_error(
            400,
            r#"{"error": {"message": "This model's maximum context length is 8192 tokens", "code": "context_length_exceeded"}}"#,
        );
        assert!(matches!(err, ProviderError::ContextExceeded { .. }));
    }

    #[test]
    fn test_classify_auth_error() {
        let err = classify_error(
            401,
            r#"{"error": {"message": "Invalid API key provided", "type": "invalid_request_error"}}"#,
        );
        assert!(matches!(err, ProviderError::AuthenticationFailed(_)));
    }

    #[test]
    fn test_classify_billing_error() {
        let err = classify_error(
            402,
            r#"{"error": {"message": "You exceeded your current quota", "code": "insufficient_quota"}}"#,
        );
        assert!(matches!(err, ProviderError::PaymentRequired { .. }));
    }

    #[test]
    fn test_classify_server_error() {
        let err = classify_error(500, r#"{"error": {"message": "Internal server error"}}"#);
        assert!(matches!(err, ProviderError::Unavailable { .. }));
    }

    #[test]
    fn test_classify_unrecognized_error() {
        let err = classify_error(400, r#"{"error": {"message": "Unknown error occurred"}}"#);
        assert!(matches!(err, ProviderError::InvalidResponse(_)));
    }
}
