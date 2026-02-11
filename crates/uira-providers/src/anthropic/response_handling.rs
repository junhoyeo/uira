//! Utilities for handling Anthropic API responses

use serde::Deserialize;

/// Extract retry-after delay from response headers
/// Returns delay in milliseconds
pub fn extract_retry_after(headers: &reqwest::header::HeaderMap) -> Option<u64> {
    headers
        .get("retry-after")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<u64>().ok())
        .map(|secs| secs * 1000)
}

pub async fn parse_error_body(response: reqwest::Response) -> String {
    if let Ok(text) = response.text().await {
        if let Ok(error) = serde_json::from_str::<ErrorResponse>(&text) {
            return error.to_string();
        }
        // Anthropic wraps errors as {"error": {"type": ..., "message": ...}}
        if let Ok(wrapper) = serde_json::from_str::<NestedErrorResponse>(&text) {
            return wrapper.error.to_string();
        }
        text
    } else {
        "Failed to read error response body".to_string()
    }
}

#[derive(Debug, Deserialize)]
struct ErrorResponse {
    #[serde(alias = "type")]
    error_type: Option<String>,
    #[serde(alias = "message", alias = "error")]
    error_message: Option<String>,
    #[serde(alias = "code")]
    error_code: Option<String>,
}

#[derive(Debug, Deserialize)]
struct NestedErrorResponse {
    error: ErrorResponse,
}

impl std::fmt::Display for ErrorResponse {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut parts = Vec::new();

        if let Some(code) = &self.error_code {
            parts.push(format!("code: {}", code));
        }
        if let Some(error_type) = &self.error_type {
            parts.push(format!("type: {}", error_type));
        }
        if let Some(message) = &self.error_message {
            parts.push(format!("message: {}", message));
        }

        if parts.is_empty() {
            write!(f, "Unknown error")
        } else {
            write!(f, "{}", parts.join(", "))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use reqwest::header::{HeaderMap, HeaderValue};

    #[test]
    fn test_extract_retry_after_present() {
        let mut headers = HeaderMap::new();
        headers.insert("retry-after", HeaderValue::from_static("60"));

        assert_eq!(extract_retry_after(&headers), Some(60000));
    }

    #[test]
    fn test_extract_retry_after_missing() {
        let headers = HeaderMap::new();
        assert_eq!(extract_retry_after(&headers), None);
    }

    #[test]
    fn test_extract_retry_after_invalid() {
        let mut headers = HeaderMap::new();
        headers.insert("retry-after", HeaderValue::from_static("invalid"));

        assert_eq!(extract_retry_after(&headers), None);
    }

    #[test]
    fn test_error_response_display() {
        let error = ErrorResponse {
            error_type: Some("rate_limit_error".to_string()),
            error_message: Some("Too many requests".to_string()),
            error_code: Some("429".to_string()),
        };

        let display = error.to_string();
        assert!(display.contains("code: 429"));
        assert!(display.contains("type: rate_limit_error"));
        assert!(display.contains("message: Too many requests"));
    }

    #[test]
    fn test_nested_error_response_deserializes() {
        let json = r#"{"error":{"type":"overloaded_error","message":"Overloaded"}}"#;
        let wrapper: NestedErrorResponse = serde_json::from_str(json).unwrap();
        let display = wrapper.error.to_string();
        assert!(display.contains("type: overloaded_error"));
        assert!(display.contains("message: Overloaded"));
    }
}
