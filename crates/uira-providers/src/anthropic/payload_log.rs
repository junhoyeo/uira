//! Optional JSONL request payload logging for debugging and auditing
//!
//! Enable via `UIRA_ANTHROPIC_PAYLOAD_LOG=true` environment variable.
//! Logs are written to `~/.local/share/uira/logs/anthropic-payload.jsonl` by default.
//! Override path with `UIRA_ANTHROPIC_PAYLOAD_LOG_FILE`.

use chrono::Utc;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;

/// Payload log event structure
#[derive(Debug, Clone, Serialize)]
pub struct PayloadLogEvent {
    pub ts: String,
    pub stage: String, // "request" | "usage"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payload_digest: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Payload logger with environment-based opt-in
pub struct PayloadLogger {
    enabled: bool,
    log_path: PathBuf,
}

impl PayloadLogger {
    /// Create logger from environment variables
    ///
    /// Checks `UIRA_ANTHROPIC_PAYLOAD_LOG` (true/1 to enable)
    /// and `UIRA_ANTHROPIC_PAYLOAD_LOG_FILE` (custom path)
    pub fn from_env() -> Self {
        let enabled = std::env::var("UIRA_ANTHROPIC_PAYLOAD_LOG")
            .map(|v| v == "true" || v == "1")
            .unwrap_or(false);

        let log_path = std::env::var("UIRA_ANTHROPIC_PAYLOAD_LOG_FILE")
            .map(PathBuf::from)
            .unwrap_or_else(|_| Self::default_log_path());

        Self { enabled, log_path }
    }

    /// Default log path: ~/.local/share/uira/logs/anthropic-payload.jsonl
    fn default_log_path() -> PathBuf {
        let mut path = dirs::data_local_dir().unwrap_or_else(|| PathBuf::from("."));
        path.push("uira");
        path.push("logs");
        path.push("anthropic-payload.jsonl");
        path
    }

    /// Log a request payload with SHA-256 digest
    ///
    /// Redacts secrets (API keys, tokens) before hashing
    pub fn log_request(
        &self,
        session_id: Option<String>,
        provider: &str,
        model_id: &str,
        payload: &serde_json::Value,
    ) {
        if !self.enabled {
            return;
        }

        // Redact secrets before hashing
        let redacted = self.redact_secrets(payload);
        let digest = self.compute_digest(&redacted);

        let event = PayloadLogEvent {
            ts: Utc::now().to_rfc3339(),
            stage: "request".to_string(),
            session_id,
            provider: Some(provider.to_string()),
            model_id: Some(model_id.to_string()),
            payload_digest: Some(digest),
            error: None,
        };

        self.write_event(&event);
    }

    /// Log usage information
    pub fn log_usage(
        &self,
        session_id: Option<String>,
        provider: &str,
        model_id: &str,
        input_tokens: u64,
        output_tokens: u64,
    ) {
        if !self.enabled {
            return;
        }

        let usage_payload = serde_json::json!({
            "input_tokens": input_tokens,
            "output_tokens": output_tokens,
        });

        let digest = self.compute_digest(&usage_payload);

        let event = PayloadLogEvent {
            ts: Utc::now().to_rfc3339(),
            stage: "usage".to_string(),
            session_id,
            provider: Some(provider.to_string()),
            model_id: Some(model_id.to_string()),
            payload_digest: Some(digest),
            error: None,
        };

        self.write_event(&event);
    }

    /// Log an error
    pub fn log_error(&self, session_id: Option<String>, error: &str) {
        if !self.enabled {
            return;
        }

        let event = PayloadLogEvent {
            ts: Utc::now().to_rfc3339(),
            stage: "error".to_string(),
            session_id,
            provider: None,
            model_id: None,
            payload_digest: None,
            error: Some(error.to_string()),
        };

        self.write_event(&event);
    }

    /// Redact secrets from payload
    fn redact_secrets(&self, payload: &serde_json::Value) -> serde_json::Value {
        let mut redacted = payload.clone();

        // Redact common secret fields
        if let Some(obj) = redacted.as_object_mut() {
            let secret_keys = ["api_key", "apiKey", "token", "authorization", "secret"];
            for key in &secret_keys {
                if obj.contains_key(*key) {
                    obj.insert(key.to_string(), serde_json::json!("[REDACTED]"));
                }
            }
        }

        redacted
    }

    /// Compute SHA-256 digest of payload
    fn compute_digest(&self, payload: &serde_json::Value) -> String {
        let json_str = serde_json::to_string(payload).unwrap_or_default();
        let mut hasher = Sha256::new();
        hasher.update(json_str.as_bytes());
        let result = hasher.finalize();
        hex::encode(result)
    }

    /// Write event to JSONL file
    fn write_event(&self, event: &PayloadLogEvent) {
        // Ensure log directory exists
        if let Some(parent) = self.log_path.parent() {
            let _ = fs::create_dir_all(parent);
        }

        // Append event as JSONL
        if let Ok(mut file) = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.log_path)
        {
            if let Ok(json) = serde_json::to_string(event) {
                let _ = writeln!(file, "{}", json);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_redact_secrets() {
        let logger = PayloadLogger {
            enabled: false,
            log_path: PathBuf::from("/tmp/test.jsonl"),
        };

        let payload = serde_json::json!({
            "model": "claude-3",
            "api_key": "sk-ant-secret123",
            "messages": [{"role": "user", "content": "hello"}]
        });

        let redacted = logger.redact_secrets(&payload);
        assert_eq!(redacted["api_key"], "[REDACTED]");
        assert_eq!(redacted["model"], "claude-3");
    }

    #[test]
    fn test_compute_digest() {
        let logger = PayloadLogger {
            enabled: false,
            log_path: PathBuf::from("/tmp/test.jsonl"),
        };

        let payload = serde_json::json!({"test": "data"});
        let digest = logger.compute_digest(&payload);

        // SHA-256 produces 64 hex characters
        assert_eq!(digest.len(), 64);
    }

    #[test]
    fn test_disabled_by_default() {
        std::env::remove_var("UIRA_ANTHROPIC_PAYLOAD_LOG");
        let logger = PayloadLogger::from_env();
        assert!(!logger.enabled);
    }
}
