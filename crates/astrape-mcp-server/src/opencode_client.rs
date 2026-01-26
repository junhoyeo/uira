//! OpenCode session-based API client.
//!
//! This module provides a clean interface to query any AI provider through
//! OpenCode's session message API. OpenCode handles all provider routing,
//! retry logic, timeouts, and authentication.

use crate::auth::{get_access_token, load_opencode_auth};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::json;

// ============================================================================
// Request structures (matching OpenCode API)
// ============================================================================

#[derive(Serialize)]
struct ChatBody {
    #[serde(rename = "modelID")]
    model_id: String,

    #[serde(rename = "providerID")]
    provider_id: String,

    parts: Vec<ChatPart>,

    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<serde_json::Value>,
}

#[derive(Serialize)]
struct ChatPart {
    #[serde(rename = "type")]
    part_type: String,
    text: String,
}

// ============================================================================
// Response structures
// ============================================================================

#[derive(Deserialize)]
struct Session {
    id: String,
}

#[derive(Deserialize)]
struct MessageInfo {
    #[allow(dead_code)]
    info: MessageMeta,
    parts: Vec<MessagePart>,
}

#[derive(Deserialize)]
struct MessageMeta {
    #[allow(dead_code)]
    id: String,
}

#[derive(Deserialize)]
struct MessagePart {
    #[serde(rename = "type")]
    part_type: String,
    text: Option<String>,
}

// ============================================================================
// Main API
// ============================================================================

/// Query any provider through OpenCode's session message API.
///
/// OpenCode handles all provider routing, retry logic, and authentication.
/// The model string can be in the format "provider/model-name" or just "model-name"
/// (defaults to openai provider).
pub async fn query(prompt: &str, model: &str, opencode_port: u16) -> Result<String, String> {
    // 1. Load auth store (for validation - OpenCode handles actual auth)
    let auth_store = load_opencode_auth()
        .await
        .map_err(|e| format!("Failed to load OpenCode auth: {}", e))?;

    // 2. Parse model string: "provider/model-name" or just "model-name"
    let (provider_id, model_id) = parse_model(model);

    // 3. Verify provider has auth configured
    let _ = get_access_token(&auth_store, &provider_id)
        .map_err(|e| format!("No auth for provider '{}': {}", provider_id, e))?;

    // 4. Create OpenCode session with timeout
    let timeout_secs = std::env::var("OPENCODE_TIMEOUT_SECS")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(120);

    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(timeout_secs))
        .build()
        .map_err(|e| format!("Failed to build HTTP client: {}", e))?;

    let session: Session = client
        .post(format!("http://localhost:{}/session", opencode_port))
        .send()
        .await
        .map_err(|e| format!("Failed to create session: {}", e))?
        .json()
        .await
        .map_err(|e| format!("Failed to parse session response: {}", e))?;

    // 5. Send message via session API (OpenCode routes to provider)
    let body = ChatBody {
        model_id,
        provider_id,
        parts: vec![ChatPart {
            part_type: "text".to_string(),
            text: prompt.to_string(),
        }],
        tools: None,
    };

    let response = client
        .post(format!(
            "http://localhost:{}/session/{}/message",
            opencode_port, session.id
        ))
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("Failed to send message: {}", e))?;

    // 6. Check status
    if !response.status().is_success() {
        let status = response.status();
        let error_text = response
            .text()
            .await
            .unwrap_or_else(|_| "Failed to read error".to_string());
        return Err(format!("OpenCode API error {}: {}", status, error_text));
    }

    // 7. Parse response
    let message: MessageInfo = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse message response: {}", e))?;

    // 8. Extract text from parts
    let combined_text = extract_text(&message.parts);

    // 9. Return in spawn_agent format
    serde_json::to_string(&json!({"result": combined_text}))
        .map_err(|e| format!("Failed to serialize result: {}", e))
}

// ============================================================================
// Helper functions
// ============================================================================

/// Parse model string into (provider_id, model_id).
///
/// Examples:
/// - "openai/gpt-4" → ("openai", "gpt-4")
/// - "google/gemini-pro" → ("google", "gemini-pro")
/// - "gpt-4" → ("openai", "gpt-4")
fn parse_model(model: &str) -> (String, String) {
    if let Some((provider, model_name)) = model.split_once('/') {
        (provider.to_string(), model_name.to_string())
    } else {
        // Default to openai for bare model names
        ("openai".to_string(), model.to_string())
    }
}

/// Extract text content from message parts.
fn extract_text(parts: &[MessagePart]) -> String {
    parts
        .iter()
        .filter(|p| p.part_type == "text")
        .filter_map(|p| p.text.as_ref())
        .map(|s| s.as_str())
        .collect::<Vec<_>>()
        .join("\n")
}
