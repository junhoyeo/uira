//! Recovery Hook
//!
//! Port of oh-my-claudecode's `src/hooks/recovery/*`.
//!
//! Provides a unified recovery system:
//! - Context window limit detection + retry state
//! - Edit tool error reminder injection
//! - Session recovery helpers for common structural errors

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use lazy_static::lazy_static;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::RwLock;

use crate::hook::{Hook, HookContext, HookResult};
use crate::types::{HookEvent, HookInput, HookOutput};

// =========================
// Constants (ported)
// =========================

pub const PLACEHOLDER_TEXT: &str = "[user interrupted]";

pub const CONTEXT_LIMIT_RECOVERY_MESSAGE: &str = r#"CONTEXT WINDOW LIMIT REACHED - IMMEDIATE ACTION REQUIRED

The conversation has exceeded the model's context window limit. To continue working effectively, you must take one of these actions:

1. SUMMARIZE THE CONVERSATION
   - Use the /compact command if available
   - Or provide a concise summary of what has been accomplished so far
   - Include key decisions, code changes, and remaining tasks

2. START A FRESH CONTEXT
   - If summarization isn't sufficient, suggest starting a new session
   - Provide a handoff message with essential context

3. REDUCE OUTPUT SIZE
   - When showing code, show only relevant portions
   - Use file paths and line numbers instead of full code blocks
   - Be more concise in explanations

IMPORTANT: Do not attempt to continue without addressing this limit.
The API will reject further requests until the context is reduced.

Current Status:
- Context limit exceeded
- Further API calls will fail until context is reduced
- Action required before continuing
"#;

pub const CONTEXT_LIMIT_SHORT_MESSAGE: &str =
    "Context window limit reached. Please use /compact to summarize the conversation or start a new session.";

pub const NON_EMPTY_CONTENT_RECOVERY_MESSAGE: &str = r#"API ERROR: Non-empty content validation failed.

This error typically occurs when:
- A message has empty text content
- The conversation structure is invalid

Suggested actions:
1. Continue with a new message
2. If the error persists, start a new session

The system will attempt automatic recovery.
"#;

pub const TRUNCATION_APPLIED_MESSAGE: &str = r#"CONTEXT OPTIMIZATION APPLIED

Some tool outputs have been truncated to fit within the context window.
The conversation can now continue normally.

If you need to see the full output of a previous tool call, you can:
- Re-run the specific command
- Ask to see a particular file or section

Continuing with the current task...
"#;

pub const RECOVERY_FAILED_MESSAGE: &str = r#"CONTEXT RECOVERY FAILED

All automatic recovery attempts have been exhausted.
Please start a new session to continue.

Before starting a new session:
1. Note what has been accomplished
2. Save any important code changes
3. Document the current state of the task

You can copy this conversation summary to continue in a new session.
"#;

pub const EDIT_ERROR_REMINDER: &str = r#"
[EDIT ERROR - IMMEDIATE ACTION REQUIRED]

You made an Edit mistake. STOP and do this NOW:

1. READ the file immediately to see its ACTUAL current state
2. VERIFY what the content really looks like (your assumption was wrong)
3. APOLOGIZE briefly to the user for the error
4. CONTINUE with corrected action based on the real file content

DO NOT attempt another edit until you've read and verified the file state.
"#;

pub const EDIT_ERROR_PATTERNS: &[&str] = &[
    "oldString and newString must be different",
    "oldString not found",
    "oldString found multiple times",
    "old_string not found",
    "old_string and new_string must be different",
];

lazy_static! {
    static ref TOKEN_LIMIT_PATTERNS: Vec<Regex> = vec![
        Regex::new(r"(\\d+)\\s*tokens?\\s*>\\s*(\\d+)\\s*maximum").unwrap(),
        Regex::new(r"prompt.*?(\\d+).*?tokens.*?exceeds.*?(\\d+)").unwrap(),
        Regex::new(r"(\\d+).*?tokens.*?limit.*?(\\d+)").unwrap(),
        Regex::new(r"context.*?length.*?(\\d+).*?maximum.*?(\\d+)").unwrap(),
        Regex::new(r"max.*?context.*?(\\d+).*?but.*?(\\d+)").unwrap(),
    ];
    static ref THINKING_BLOCK_ERROR_PATTERNS: Vec<Regex> = vec![
        Regex::new(r"thinking.*first block").unwrap(),
        Regex::new(r"first block.*thinking").unwrap(),
        Regex::new(r"must.*start.*thinking").unwrap(),
        Regex::new(r"thinking.*redacted_thinking").unwrap(),
        Regex::new(r"expected.*thinking.*found").unwrap(),
        Regex::new(r"thinking.*disabled.*cannot.*contain").unwrap(),
    ];
}

pub const TOKEN_LIMIT_KEYWORDS: &[&str] = &[
    "prompt is too long",
    "is too long",
    "context_length_exceeded",
    "max_tokens",
    "token limit",
    "context length",
    "too many tokens",
    "non-empty content",
];

// Session recovery part type sets
lazy_static! {
    static ref THINKING_TYPES: HashSet<&'static str> =
        ["thinking", "redacted_thinking", "reasoning"].into_iter().collect();
    static ref META_TYPES: HashSet<&'static str> = ["step-start", "step-finish"].into_iter().collect();
    static ref TOOL_PART_TYPES: HashSet<&'static str> =
        ["tool", "tool_use", "tool_result"].into_iter().collect();
}

// =========================
// Types (ported)
// =========================

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecoveryErrorType {
    ContextWindowLimit,
    EditError,
    ToolResultMissing,
    ThinkingBlockOrder,
    ThinkingDisabledViolation,
    EmptyContent,
}

impl RecoveryErrorType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::ContextWindowLimit => "context_window_limit",
            Self::EditError => "edit_error",
            Self::ToolResultMissing => "tool_result_missing",
            Self::ThinkingBlockOrder => "thinking_block_order",
            Self::ThinkingDisabledViolation => "thinking_disabled_violation",
            Self::EmptyContent => "empty_content",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecoveryResult {
    pub attempted: bool,
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(rename = "errorType", skip_serializing_if = "Option::is_none")]
    pub error_type: Option<String>,
}

impl RecoveryResult {
    pub fn not_attempted() -> Self {
        Self {
            attempted: false,
            success: false,
            message: None,
            error_type: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParsedTokenLimitError {
    #[serde(rename = "currentTokens")]
    pub current_tokens: u64,
    #[serde(rename = "maxTokens")]
    pub max_tokens: u64,
    #[serde(rename = "requestId", skip_serializing_if = "Option::is_none")]
    pub request_id: Option<String>,
    #[serde(rename = "errorType")]
    pub error_type: String,
    #[serde(rename = "providerID", skip_serializing_if = "Option::is_none")]
    pub provider_id: Option<String>,
    #[serde(rename = "modelID", skip_serializing_if = "Option::is_none")]
    pub model_id: Option<String>,
    #[serde(rename = "messageIndex", skip_serializing_if = "Option::is_none")]
    pub message_index: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryState {
    pub attempt: u32,
    #[serde(rename = "lastAttemptTime")]
    pub last_attempt_time_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TruncateState {
    #[serde(rename = "truncateAttempt")]
    pub truncate_attempt: u32,
    #[serde(rename = "lastTruncatedPartId", skip_serializing_if = "Option::is_none")]
    pub last_truncated_part_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecoveryConfig {
    #[serde(rename = "contextWindowRecovery", skip_serializing_if = "Option::is_none")]
    pub context_window_recovery: Option<bool>,
    #[serde(rename = "editErrorRecovery", skip_serializing_if = "Option::is_none")]
    pub edit_error_recovery: Option<bool>,
    #[serde(rename = "sessionRecovery", skip_serializing_if = "Option::is_none")]
    pub session_recovery: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detailed: Option<bool>,
    #[serde(rename = "customMessages", skip_serializing_if = "Option::is_none")]
    pub custom_messages: Option<HashMap<String, String>>,
    #[serde(rename = "autoResume", skip_serializing_if = "Option::is_none")]
    pub auto_resume: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub debug: Option<bool>,
}

pub const RETRY_CONFIG_MAX_ATTEMPTS: u32 = 2;
pub const RETRY_CONFIG_INITIAL_DELAY_MS: u64 = 2000;
pub const RETRY_CONFIG_BACKOFF_FACTOR: u32 = 2;
pub const RETRY_CONFIG_MAX_DELAY_MS: u64 = 30000;

pub const TRUNCATE_CONFIG_MAX_TRUNCATE_ATTEMPTS: u32 = 20;
pub const TRUNCATE_CONFIG_MIN_OUTPUT_SIZE_TO_TRUNCATE: usize = 500;
pub const TRUNCATE_CONFIG_TARGET_TOKEN_RATIO: f64 = 0.5;
pub const TRUNCATE_CONFIG_CHARS_PER_TOKEN: u32 = 4;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MessageData {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub info: Option<MessageInfoData>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parts: Option<Vec<MessagePartData>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MessageInfoData {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    #[serde(rename = "sessionID", skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MessagePartData {
    #[serde(rename = "type")]
    pub part_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thinking: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input: Option<serde_json::Value>,
    #[serde(rename = "callID", skip_serializing_if = "Option::is_none")]
    pub call_id: Option<String>,
}

// Storage types (matching TS)

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredMessageMeta {
    pub id: String,
    #[serde(rename = "sessionID")]
    pub session_id: String,
    pub role: String,
    #[serde(rename = "parentID", skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub time: Option<StoredMessageTime>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredMessageTime {
    pub created: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredTextPart {
    pub id: String,
    #[serde(rename = "sessionID")]
    pub session_id: String,
    #[serde(rename = "messageID")]
    pub message_id: String,
    #[serde(rename = "type")]
    pub part_type: String,
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub synthetic: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ignored: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredToolPart {
    pub id: String,
    #[serde(rename = "sessionID")]
    pub session_id: String,
    #[serde(rename = "messageID")]
    pub message_id: String,
    #[serde(rename = "type")]
    pub part_type: String,
    #[serde(rename = "callID")]
    pub call_id: String,
    pub tool: String,
    pub state: StoredToolState,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredToolState {
    pub status: String,
    pub input: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredReasoningPart {
    pub id: String,
    #[serde(rename = "sessionID")]
    pub session_id: String,
    #[serde(rename = "messageID")]
    pub message_id: String,
    #[serde(rename = "type")]
    pub part_type: String,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum StoredPart {
    Text(StoredTextPart),
    Tool(StoredToolPart),
    Reasoning(StoredReasoningPart),
    Other(serde_json::Value),
}

impl StoredPart {
    pub fn part_type(&self) -> Option<&str> {
        match self {
            StoredPart::Text(p) => Some(p.part_type.as_str()),
            StoredPart::Tool(p) => Some(p.part_type.as_str()),
            StoredPart::Reasoning(p) => Some(p.part_type.as_str()),
            StoredPart::Other(v) => v.get("type").and_then(|t| t.as_str()),
        }
    }
}

// =========================
// Helpers
// =========================

fn now_ms() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn format_with_commas(mut n: u64) -> String {
    // Simple 3-digit grouping. Matches TS `toLocaleString()` well enough.
    if n < 1000 {
        return n.to_string();
    }
    let mut parts = Vec::new();
    while n >= 1000 {
        parts.push(format!("{:03}", n % 1000));
        n /= 1000;
    }
    parts.push(n.to_string());
    parts.reverse();
    parts.join(",")
}

fn json_to_lower_string(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::String(s) => s.to_lowercase(),
        _ => v.to_string().to_lowercase(),
    }
}

fn extract_message_index_from_text(text: &str) -> Option<usize> {
    lazy_static! {
        static ref MSG_IDX_RE: Regex = Regex::new(r"messages\\.(\\d+)").unwrap();
    }
    MSG_IDX_RE
        .captures(text)
        .and_then(|c| c.get(1))
        .and_then(|m| m.as_str().parse::<usize>().ok())
}

fn is_thinking_block_error(text: &str) -> bool {
    let lower = text.to_lowercase();
    THINKING_BLOCK_ERROR_PATTERNS
        .iter()
        .any(|re| re.is_match(&lower))
}

fn is_token_limit_error(text: &str) -> bool {
    if is_thinking_block_error(text) {
        return false;
    }
    let lower = text.to_lowercase();
    TOKEN_LIMIT_KEYWORDS
        .iter()
        .any(|kw| lower.contains(&kw.to_lowercase()))
}

fn extract_tokens_from_message(message: &str) -> Option<(u64, u64)> {
    for re in TOKEN_LIMIT_PATTERNS.iter() {
        if let Some(caps) = re.captures(message) {
            let n1 = caps.get(1)?.as_str().parse::<u64>().ok()?;
            let n2 = caps.get(2)?.as_str().parse::<u64>().ok()?;
            if n1 > n2 {
                return Some((n1, n2));
            }
            return Some((n2, n1));
        }
    }
    None
}

// =========================
// Context window limit recovery
// =========================

#[derive(Debug, Clone)]
struct SessionState {
    retry_state: RetryState,
    truncate_state: TruncateState,
    last_error_time_ms: u64,
    error_count: u32,
}

const STATE_TTL_MS: u64 = 300_000;

lazy_static! {
    static ref SESSION_STATES: RwLock<HashMap<String, SessionState>> = RwLock::new(HashMap::new());
}

fn get_session_state(session_id: &str) -> SessionState {
    let now = now_ms();
    let mut states = SESSION_STATES.write().unwrap();

    if let Some(existing) = states.get(session_id) {
        if now.saturating_sub(existing.last_error_time_ms) <= STATE_TTL_MS {
            return existing.clone();
        }
    }

    let state = SessionState {
        retry_state: RetryState {
            attempt: 0,
            last_attempt_time_ms: 0,
        },
        truncate_state: TruncateState {
            truncate_attempt: 0,
            last_truncated_part_id: None,
        },
        last_error_time_ms: now,
        error_count: 0,
    };

    states.insert(session_id.to_string(), state.clone());
    state
}

fn set_session_state(session_id: &str, state: SessionState) {
    let mut states = SESSION_STATES.write().unwrap();
    states.insert(session_id.to_string(), state);
}

pub fn parse_token_limit_error(err: &serde_json::Value) -> Option<ParsedTokenLimitError> {
    // string errors
    if let serde_json::Value::String(s) = err {
        let lower = s.to_lowercase();
        if lower.contains("non-empty content") {
            return Some(ParsedTokenLimitError {
                current_tokens: 0,
                max_tokens: 0,
                request_id: None,
                error_type: "non-empty content".to_string(),
                provider_id: None,
                model_id: None,
                message_index: extract_message_index_from_text(&lower),
            });
        }
        if is_token_limit_error(&lower) {
            let (current, max) = extract_tokens_from_message(&lower).unwrap_or((0, 0));
            return Some(ParsedTokenLimitError {
                current_tokens: current,
                max_tokens: max,
                request_id: None,
                error_type: "token_limit_exceeded_string".to_string(),
                provider_id: None,
                model_id: None,
                message_index: None,
            });
        }
        return None;
    }

    // non-object
    if !err.is_object() {
        return None;
    }

    let mut text_sources: Vec<String> = Vec::new();

    let response_body = err
        .get("data")
        .and_then(|d| d.get("responseBody"))
        .and_then(|v| v.as_str());
    let err_message = err.get("message").and_then(|v| v.as_str());
    let err_error = err.get("error");
    let nested_err_msg = err
        .get("error")
        .and_then(|e| e.get("error"))
        .and_then(|n| n.get("message"))
        .and_then(|v| v.as_str());

    if let Some(s) = response_body {
        text_sources.push(s.to_string());
    }
    if let Some(s) = err_message {
        text_sources.push(s.to_string());
    }
    if let Some(s) = err_error.and_then(|e| e.get("message")).and_then(|v| v.as_str()) {
        text_sources.push(s.to_string());
    }
    for key in ["body", "details", "reason", "description"] {
        if let Some(s) = err.get(key).and_then(|v| v.as_str()) {
            text_sources.push(s.to_string());
        }
    }
    if let Some(s) = nested_err_msg {
        text_sources.push(s.to_string());
    }
    if let Some(s) = err
        .get("data")
        .and_then(|d| d.get("message"))
        .and_then(|v| v.as_str())
    {
        text_sources.push(s.to_string());
    }
    if let Some(s) = err
        .get("data")
        .and_then(|d| d.get("error"))
        .and_then(|v| v.as_str())
    {
        text_sources.push(s.to_string());
    }

    if text_sources.is_empty() {
        let json_str = err.to_string();
        if is_token_limit_error(&json_str) {
            text_sources.push(json_str);
        }
    }

    let combined_text = text_sources.join(" ");
    if !is_token_limit_error(&combined_text) {
        return None;
    }

    // Try structured response body: `data: {..}` stream or direct json
    if let Some(body) = response_body {
        // Look for embedded json blocks.
        // TS uses regex patterns; we replicate the same intent without lookaround.
        let mut candidates: Vec<&str> = Vec::new();

        // SSE-style: `data: { ... }` (take last occurrence)
        if let Some(idx) = body.rfind("data:") {
            let tail = body[idx + 5..].trim();
            if tail.starts_with('{') {
                candidates.push(tail);
            }
        }

        // Any JSON-looking substring that includes an `error` object
        if body.contains("\"error\"") {
            if let (Some(start), Some(end)) = (body.find('{'), body.rfind('}')) {
                if end > start {
                    candidates.push(&body[start..=end]);
                }
            }
        }

        for cand in candidates {
            if let Ok(json_val) = serde_json::from_str::<serde_json::Value>(cand) {
                let msg = json_val
                    .get("error")
                    .and_then(|e| e.get("message"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("");

                if let Some((current, max)) = extract_tokens_from_message(msg) {
                    return Some(ParsedTokenLimitError {
                        current_tokens: current,
                        max_tokens: max,
                        request_id: json_val
                            .get("request_id")
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string()),
                        error_type: json_val
                            .get("error")
                            .and_then(|e| e.get("type"))
                            .and_then(|v| v.as_str())
                            .unwrap_or("token_limit_exceeded")
                            .to_string(),
                        provider_id: None,
                        model_id: None,
                        message_index: None,
                    });
                }
            }
        }

        // Bedrock-style error body
        if let Ok(json_val) = serde_json::from_str::<serde_json::Value>(body) {
            if let Some(msg) = json_val.get("message").and_then(|v| v.as_str()) {
                if is_token_limit_error(msg) {
                    return Some(ParsedTokenLimitError {
                        current_tokens: 0,
                        max_tokens: 0,
                        request_id: None,
                        error_type: "bedrock_input_too_long".to_string(),
                        provider_id: None,
                        model_id: None,
                        message_index: None,
                    });
                }
            }
        }
    }

    // Extract tokens from any text source
    for text in &text_sources {
        if let Some((current, max)) = extract_tokens_from_message(text) {
            return Some(ParsedTokenLimitError {
                current_tokens: current,
                max_tokens: max,
                request_id: None,
                error_type: "token_limit_exceeded".to_string(),
                provider_id: None,
                model_id: None,
                message_index: None,
            });
        }
    }

    if combined_text.to_lowercase().contains("non-empty content") {
        return Some(ParsedTokenLimitError {
            current_tokens: 0,
            max_tokens: 0,
            request_id: None,
            error_type: "non-empty content".to_string(),
            provider_id: None,
            model_id: None,
            message_index: extract_message_index_from_text(&combined_text),
        });
    }

    Some(ParsedTokenLimitError {
        current_tokens: 0,
        max_tokens: 0,
        request_id: None,
        error_type: "token_limit_exceeded_unknown".to_string(),
        provider_id: None,
        model_id: None,
        message_index: None,
    })
}

pub fn contains_token_limit_error(text: &str) -> bool {
    is_token_limit_error(text)
}

fn generate_recovery_message(
    parsed: Option<&ParsedTokenLimitError>,
    mut state: SessionState,
    config: Option<&RecoveryConfig>,
) -> (SessionState, Option<String>, Option<String>) {
    if let Some(custom) = config
        .and_then(|c| c.custom_messages.as_ref())
        .and_then(|m| m.get("context_window_limit"))
    {
        return (state, Some(custom.clone()), parsed.map(|p| p.error_type.clone()));
    }

    if parsed
        .and_then(|p| Some(p.error_type.as_str()))
        .map(|t| t.contains("non-empty content"))
        .unwrap_or(false)
    {
        return (
            state,
            Some(NON_EMPTY_CONTENT_RECOVERY_MESSAGE.to_string()),
            Some("non-empty content".to_string()),
        );
    }

    state.retry_state.attempt += 1;
    state.retry_state.last_attempt_time_ms = now_ms();

    if state.retry_state.attempt > RETRY_CONFIG_MAX_ATTEMPTS {
        return (
            state,
            Some(RECOVERY_FAILED_MESSAGE.to_string()),
            Some("recovery_exhausted".to_string()),
        );
    }

    let detailed = config.and_then(|c| c.detailed).unwrap_or(true);

    if detailed {
        let mut msg = CONTEXT_LIMIT_RECOVERY_MESSAGE.to_string();
        if let Some(p) = parsed {
            if p.current_tokens > 0 && p.max_tokens > 0 {
                msg.push_str("\nToken Details:\n");
                msg.push_str(&format!(
                    "- Current: {} tokens\n- Maximum: {} tokens\n- Over limit by: {} tokens\n",
                    format_with_commas(p.current_tokens),
                    format_with_commas(p.max_tokens),
                    format_with_commas(p.current_tokens.saturating_sub(p.max_tokens))
                ));
            }
        }
        return (
            state,
            Some(msg),
            Some(
                parsed
                    .map(|p| p.error_type.clone())
                    .unwrap_or_else(|| "token_limit_exceeded".to_string()),
            ),
        );
    }

    (
        state,
        Some(CONTEXT_LIMIT_SHORT_MESSAGE.to_string()),
        Some(
            parsed
                .map(|p| p.error_type.clone())
                .unwrap_or_else(|| "token_limit_exceeded".to_string()),
        ),
    )
}

pub fn handle_context_window_recovery(
    session_id: &str,
    error: &serde_json::Value,
    config: Option<&RecoveryConfig>,
) -> RecoveryResult {
    let parsed = parse_token_limit_error(error);
    if parsed.is_none() {
        return RecoveryResult::not_attempted();
    }

    let mut state = get_session_state(session_id);
    state.last_error_time_ms = now_ms();
    state.error_count += 1;

    let (new_state, message, error_type) =
        generate_recovery_message(parsed.as_ref(), state, config);
    set_session_state(session_id, new_state);

    RecoveryResult {
        attempted: true,
        success: message.is_some(),
        message,
        error_type,
    }
}

pub fn detect_context_limit_error(text: &str) -> bool {
    contains_token_limit_error(text)
}

// =========================
// Edit error recovery
// =========================

pub fn detect_edit_error(output: &str) -> bool {
    let output_lower = output.to_lowercase();
    EDIT_ERROR_PATTERNS
        .iter()
        .any(|pat| output_lower.contains(&pat.to_lowercase()))
}

pub fn inject_edit_error_recovery(output: &str) -> String {
    if detect_edit_error(output) {
        format!("{}{}", output, EDIT_ERROR_REMINDER)
    } else {
        output.to_string()
    }
}

pub fn handle_edit_error_recovery(tool_name: &str, output: &str) -> RecoveryResult {
    if tool_name.to_lowercase() != "edit" {
        return RecoveryResult::not_attempted();
    }

    if detect_edit_error(output) {
        return RecoveryResult {
            attempted: true,
            success: true,
            message: Some(EDIT_ERROR_REMINDER.to_string()),
            error_type: Some(RecoveryErrorType::EditError.as_str().to_string()),
        };
    }

    RecoveryResult::not_attempted()
}

pub fn process_edit_output(tool_name: &str, output: &str) -> String {
    if tool_name.to_lowercase() != "edit" {
        return output.to_string();
    }
    inject_edit_error_recovery(output)
}

// =========================
// Session recovery (storage operations)
// =========================

#[derive(Debug, Clone)]
pub struct RecoveryStorage {
    pub message_storage: PathBuf,
    pub part_storage: PathBuf,
}

impl RecoveryStorage {
    pub fn from_default_paths() -> Self {
        let data_dir = std::env::var("XDG_DATA_HOME")
            .ok()
            .map(PathBuf::from)
            .or_else(|| dirs::home_dir().map(|h| h.join(".local").join("share")))
            .unwrap_or_else(|| PathBuf::from("."));

        let claude_code_storage = data_dir.join("claude-code").join("storage");
        Self {
            message_storage: claude_code_storage.join("message"),
            part_storage: claude_code_storage.join("part"),
        }
    }

    pub fn get_message_dir(&self, session_id: &str) -> Option<PathBuf> {
        if !self.message_storage.exists() {
            return None;
        }

        let direct = self.message_storage.join(session_id);
        if direct.exists() {
            return Some(direct);
        }

        let entries = fs::read_dir(&self.message_storage).ok()?;
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let candidate = path.join(session_id);
            if candidate.exists() {
                return Some(candidate);
            }
        }

        None
    }

    pub fn read_messages(&self, session_id: &str) -> Vec<StoredMessageMeta> {
        let Some(message_dir) = self.get_message_dir(session_id) else {
            return Vec::new();
        };

        let mut messages = Vec::new();
        let Ok(entries) = fs::read_dir(message_dir) else {
            return Vec::new();
        };

        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            if let Ok(content) = fs::read_to_string(&path) {
                if let Ok(msg) = serde_json::from_str::<StoredMessageMeta>(&content) {
                    messages.push(msg);
                }
            }
        }

        messages.sort_by(|a, b| {
            let a_time = a.time.as_ref().map(|t| t.created).unwrap_or(0);
            let b_time = b.time.as_ref().map(|t| t.created).unwrap_or(0);
            if a_time != b_time {
                return a_time.cmp(&b_time);
            }
            a.id.cmp(&b.id)
        });

        messages
    }

    pub fn read_parts(&self, message_id: &str) -> Vec<StoredPart> {
        let part_dir = self.part_storage.join(message_id);
        if !part_dir.exists() {
            return Vec::new();
        }

        let Ok(entries) = fs::read_dir(&part_dir) else {
            return Vec::new();
        };

        let mut parts = Vec::new();
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            if let Ok(content) = fs::read_to_string(&path) {
                if let Ok(part) = serde_json::from_str::<StoredPart>(&content) {
                    parts.push(part);
                }
            }
        }
        parts
    }

    pub fn has_content(part: &StoredPart) -> bool {
        let Some(t) = part.part_type() else {
            return false;
        };
        if THINKING_TYPES.contains(t) {
            return false;
        }
        if META_TYPES.contains(t) {
            return false;
        }

        if t == "text" {
            match part {
                StoredPart::Text(p) => !p.text.trim().is_empty(),
                _ => false,
            }
        } else if t == "tool" || t == "tool_use" || t == "tool_result" {
            true
        } else {
            false
        }
    }

    pub fn message_has_content(&self, message_id: &str) -> bool {
        self.read_parts(message_id).iter().any(Self::has_content)
    }

    pub fn find_empty_messages(&self, session_id: &str) -> Vec<String> {
        let messages = self.read_messages(session_id);
        let mut empty = Vec::new();
        for msg in messages {
            if !self.message_has_content(&msg.id) {
                empty.push(msg.id);
            }
        }
        empty
    }

    pub fn find_empty_message_by_index(&self, session_id: &str, target_index: isize) -> Option<String> {
        let messages = self.read_messages(session_id);
        let indices_to_try = [
            target_index,
            target_index - 1,
            target_index + 1,
            target_index - 2,
            target_index + 2,
            target_index - 3,
            target_index - 4,
            target_index - 5,
        ];

        for idx in indices_to_try {
            if idx < 0 {
                continue;
            }
            let idx = idx as usize;
            if idx >= messages.len() {
                continue;
            }
            let msg = &messages[idx];
            if !self.message_has_content(&msg.id) {
                return Some(msg.id.clone());
            }
        }

        None
    }

    pub fn find_messages_with_thinking_blocks(&self, session_id: &str) -> Vec<String> {
        let messages = self.read_messages(session_id);
        let mut result = Vec::new();
        for msg in messages {
            if msg.role != "assistant" {
                continue;
            }
            let parts = self.read_parts(&msg.id);
            if parts.iter().any(|p| p.part_type().map(|t| THINKING_TYPES.contains(t)).unwrap_or(false)) {
                result.push(msg.id);
            }
        }
        result
    }

    pub fn find_messages_with_thinking_only(&self, session_id: &str) -> Vec<String> {
        let messages = self.read_messages(session_id);
        let mut result = Vec::new();
        for msg in messages {
            if msg.role != "assistant" {
                continue;
            }
            let parts = self.read_parts(&msg.id);
            if parts.is_empty() {
                continue;
            }
            let has_thinking = parts.iter().any(|p| {
                p.part_type()
                    .map(|t| THINKING_TYPES.contains(t))
                    .unwrap_or(false)
            });
            let has_text_content = parts.iter().any(Self::has_content);
            if has_thinking && !has_text_content {
                result.push(msg.id);
            }
        }
        result
    }

    pub fn find_messages_with_orphan_thinking(&self, session_id: &str) -> Vec<String> {
        let messages = self.read_messages(session_id);
        let mut result = Vec::new();
        for msg in messages {
            if msg.role != "assistant" {
                continue;
            }
            let mut parts = self.read_parts(&msg.id);
            if parts.is_empty() {
                continue;
            }
            // Sort by id to match TS lexicographic ordering.
            parts.sort_by(|a, b| {
                let a_id = match a {
                    StoredPart::Text(p) => p.id.as_str(),
                    StoredPart::Tool(p) => p.id.as_str(),
                    StoredPart::Reasoning(p) => p.id.as_str(),
                    StoredPart::Other(v) => v.get("id").and_then(|vv| vv.as_str()).unwrap_or(""),
                };
                let b_id = match b {
                    StoredPart::Text(p) => p.id.as_str(),
                    StoredPart::Tool(p) => p.id.as_str(),
                    StoredPart::Reasoning(p) => p.id.as_str(),
                    StoredPart::Other(v) => v.get("id").and_then(|vv| vv.as_str()).unwrap_or(""),
                };
                a_id.cmp(b_id)
            });
            let first = &parts[0];
            let first_is_thinking = first
                .part_type()
                .map(|t| THINKING_TYPES.contains(t))
                .unwrap_or(false);
            if !first_is_thinking {
                result.push(msg.id);
            }
        }
        result
    }

    pub fn find_messages_with_empty_text_parts(&self, session_id: &str) -> Vec<String> {
        let messages = self.read_messages(session_id);
        let mut result = Vec::new();
        for msg in messages {
            let parts = self.read_parts(&msg.id);
            let has_empty_text = parts.iter().any(|p| match p {
                StoredPart::Text(tp) => tp.part_type == "text" && tp.text.trim().is_empty(),
                _ => false,
            });
            if has_empty_text {
                result.push(msg.id);
            }
        }
        result
    }

    pub fn find_message_by_index_needing_thinking(
        &self,
        session_id: &str,
        target_index: usize,
    ) -> Option<String> {
        let messages = self.read_messages(session_id);
        if target_index >= messages.len() {
            return None;
        }
        let msg = &messages[target_index];
        if msg.role != "assistant" {
            return None;
        }
        let mut parts = self.read_parts(&msg.id);
        if parts.is_empty() {
            return None;
        }
        parts.sort_by(|a, b| {
            let a_id = match a {
                StoredPart::Text(p) => p.id.as_str(),
                StoredPart::Tool(p) => p.id.as_str(),
                StoredPart::Reasoning(p) => p.id.as_str(),
                StoredPart::Other(v) => v.get("id").and_then(|vv| vv.as_str()).unwrap_or(""),
            };
            let b_id = match b {
                StoredPart::Text(p) => p.id.as_str(),
                StoredPart::Tool(p) => p.id.as_str(),
                StoredPart::Reasoning(p) => p.id.as_str(),
                StoredPart::Other(v) => v.get("id").and_then(|vv| vv.as_str()).unwrap_or(""),
            };
            a_id.cmp(b_id)
        });
        let first = &parts[0];
        let first_is_thinking = first
            .part_type()
            .map(|t| THINKING_TYPES.contains(t))
            .unwrap_or(false);
        if !first_is_thinking {
            Some(msg.id.clone())
        } else {
            None
        }
    }

    fn find_last_thinking_content(&self, session_id: &str, before_message_id: &str) -> String {
        let messages = self.read_messages(session_id);
        let current_idx = messages
            .iter()
            .position(|m| m.id == before_message_id);
        let Some(current_idx) = current_idx else {
            return String::new();
        };

        for msg in messages[..current_idx].iter().rev() {
            if msg.role != "assistant" {
                continue;
            }
            let parts = self.read_parts(&msg.id);
            for part in parts {
                let Some(t) = part.part_type() else {
                    continue;
                };
                if !THINKING_TYPES.contains(t) {
                    continue;
                }
                match part {
                    StoredPart::Reasoning(p) => {
                        if !p.text.trim().is_empty() {
                            return p.text;
                        }
                    }
                    StoredPart::Other(v) => {
                        let thinking = v
                            .get("thinking")
                            .and_then(|vv| vv.as_str())
                            .or_else(|| v.get("text").and_then(|vv| vv.as_str()))
                            .unwrap_or("");
                        if !thinking.trim().is_empty() {
                            return thinking.to_string();
                        }
                    }
                    _ => {}
                }
            }
        }

        String::new()
    }

    pub fn prepend_thinking_part(&self, session_id: &str, message_id: &str) -> bool {
        let part_dir = self.part_storage.join(message_id);
        if fs::create_dir_all(&part_dir).is_err() {
            return false;
        }

        let previous_thinking = self.find_last_thinking_content(session_id, message_id);
        let part_id = "prt_0000000000_thinking";
        let part = serde_json::json!({
            "id": part_id,
            "sessionID": session_id,
            "messageID": message_id,
            "type": "thinking",
            "thinking": if previous_thinking.trim().is_empty() {
                "[Continuing from previous reasoning]"
            } else {
                previous_thinking.as_str()
            },
            "synthetic": true,
        });

        fs::write(part_dir.join(format!("{}.json", part_id)), serde_json::to_string_pretty(&part).unwrap_or_default()).is_ok()
    }

    pub fn strip_thinking_parts(&self, message_id: &str) -> bool {
        let part_dir = self.part_storage.join(message_id);
        if !part_dir.exists() {
            return false;
        }

        let Ok(entries) = fs::read_dir(&part_dir) else {
            return false;
        };

        let mut any_removed = false;
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            if let Ok(content) = fs::read_to_string(&path) {
                if let Ok(part) = serde_json::from_str::<serde_json::Value>(&content) {
                    if part
                        .get("type")
                        .and_then(|v| v.as_str())
                        .map(|t| THINKING_TYPES.contains(t))
                        .unwrap_or(false)
                    {
                        let _ = fs::remove_file(&path);
                        any_removed = true;
                    }
                }
            }
        }
        any_removed
    }

    pub fn replace_empty_text_parts(&self, message_id: &str, replacement_text: &str) -> bool {
        let part_dir = self.part_storage.join(message_id);
        if !part_dir.exists() {
            return false;
        }

        let Ok(entries) = fs::read_dir(&part_dir) else {
            return false;
        };

        let mut any_replaced = false;
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            if let Ok(content) = fs::read_to_string(&path) {
                if let Ok(mut part) = serde_json::from_str::<serde_json::Value>(&content) {
                    if part.get("type").and_then(|v| v.as_str()) != Some("text") {
                        continue;
                    }
                    let current_text = part.get("text").and_then(|v| v.as_str()).unwrap_or("");
                    if current_text.trim().is_empty() {
                        part["text"] = serde_json::Value::String(replacement_text.to_string());
                        part["synthetic"] = serde_json::Value::Bool(true);
                        if fs::write(&path, serde_json::to_string_pretty(&part).unwrap_or_default()).is_ok() {
                            any_replaced = true;
                        }
                    }
                }
            }
        }
        any_replaced
    }

    pub fn inject_text_part(&self, session_id: &str, message_id: &str, text: &str) -> bool {
        let part_dir = self.part_storage.join(message_id);
        if fs::create_dir_all(&part_dir).is_err() {
            return false;
        }

        let part_id = generate_part_id();
        let part = serde_json::json!({
            "id": part_id,
            "sessionID": session_id,
            "messageID": message_id,
            "type": "text",
            "text": text,
            "synthetic": true,
        });
        fs::write(
            part_dir.join(format!("{}.json", part_id)),
            serde_json::to_string_pretty(&part).unwrap_or_default(),
        )
        .is_ok()
    }
}

lazy_static! {
    static ref PART_COUNTER: AtomicUsize = AtomicUsize::new(0);
}

pub fn generate_part_id() -> String {
    let ms = now_ms();
    let c = PART_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("prt_{:x}{:x}", ms, c)
}

// =========================
// Session recovery (error handling)
// =========================

pub type SessionRecoveryErrorType = Option<RecoveryErrorType>;

fn get_error_message_value(error: &serde_json::Value) -> String {
    if error.is_null() {
        return String::new();
    }
    if let Some(s) = error.as_str() {
        return s.to_lowercase();
    }

    for candidate in [
        error.get("data"),
        error.get("error"),
        Some(error),
        error
            .get("data")
            .and_then(|d| d.as_object())
            .and_then(|d| d.get("error")),
    ] {
        if let Some(obj) = candidate {
            if let Some(msg) = obj.get("message").and_then(|m| m.as_str()) {
                if !msg.is_empty() {
                    return msg.to_lowercase();
                }
            }
        }
    }

    json_to_lower_string(error)
}

fn detect_session_error_type(error: &serde_json::Value) -> SessionRecoveryErrorType {
    let msg = get_error_message_value(error);

    if msg.contains("tool_use") && msg.contains("tool_result") {
        return Some(RecoveryErrorType::ToolResultMissing);
    }

    if msg.contains("thinking")
        && (msg.contains("first block")
            || msg.contains("must start with")
            || msg.contains("preceeding")
            || msg.contains("final block")
            || msg.contains("cannot be thinking")
            || (msg.contains("expected") && msg.contains("found")))
    {
        return Some(RecoveryErrorType::ThinkingBlockOrder);
    }

    if msg.contains("thinking is disabled") && msg.contains("cannot contain") {
        return Some(RecoveryErrorType::ThinkingDisabledViolation);
    }

    if msg.contains("empty") && (msg.contains("content") || msg.contains("message")) {
        return Some(RecoveryErrorType::EmptyContent);
    }

    None
}

pub fn is_recoverable_error(error: &serde_json::Value) -> bool {
    detect_session_error_type(error).is_some()
}

fn extract_tool_use_ids(parts: &[MessagePartData]) -> Vec<String> {
    parts
        .iter()
        .filter(|p| p.part_type == "tool_use")
        .filter_map(|p| p.id.clone())
        .collect()
}

async fn recover_tool_result_missing(
    storage: &RecoveryStorage,
    session_id: &str,
    failed_msg: &MessageData,
) -> bool {
    let mut parts = failed_msg.parts.clone().unwrap_or_default();
    if parts.is_empty() {
        if let Some(msg_id) = failed_msg.info.as_ref().and_then(|i| i.id.clone()) {
            let stored = storage.read_parts(&msg_id);
            parts = stored
                .into_iter()
                .filter_map(|p| match p {
                    StoredPart::Tool(t) => Some(MessagePartData {
                        part_type: "tool_use".to_string(),
                        id: Some(t.call_id.clone()),
                        name: Some(t.tool.clone()),
                        input: Some(t.state.input.clone()),
                        ..Default::default()
                    }),
                    StoredPart::Other(v) => {
                        let t = v.get("type").and_then(|vv| vv.as_str()).unwrap_or("");
                        if t == "tool" {
                            Some(MessagePartData {
                                part_type: "tool_use".to_string(),
                                id: v.get("callID")
                                    .and_then(|vv| vv.as_str())
                                    .map(|s| s.to_string())
                                    .or_else(|| v.get("id").and_then(|vv| vv.as_str()).map(|s| s.to_string())),
                                name: v.get("tool").and_then(|vv| vv.as_str()).map(|s| s.to_string()),
                                input: v.get("state").and_then(|st| st.get("input")).cloned(),
                                ..Default::default()
                            })
                        } else {
                            None
                        }
                    }
                    _ => None,
                })
                .collect();
        }
    }

    let tool_use_ids = extract_tool_use_ids(&parts);
    if tool_use_ids.is_empty() {
        return false;
    }

    // NOTE: Parity with TS placeholder behavior: return true if we found IDs.
    let _ = (storage, session_id);
    true
}

async fn recover_thinking_block_order(
    storage: &RecoveryStorage,
    session_id: &str,
    error: &serde_json::Value,
) -> bool {
    let target_index = extract_message_index_from_text(&get_error_message_value(error));
    if let Some(idx) = target_index {
        if let Some(target_msg_id) = storage.find_message_by_index_needing_thinking(session_id, idx)
        {
            return storage.prepend_thinking_part(session_id, &target_msg_id);
        }
    }

    let orphan = storage.find_messages_with_orphan_thinking(session_id);
    if orphan.is_empty() {
        return false;
    }

    let mut any = false;
    for msg_id in orphan {
        if storage.prepend_thinking_part(session_id, &msg_id) {
            any = true;
        }
    }
    any
}

async fn recover_thinking_disabled_violation(storage: &RecoveryStorage, session_id: &str) -> bool {
    let msgs = storage.find_messages_with_thinking_blocks(session_id);
    if msgs.is_empty() {
        return false;
    }
    let mut any = false;
    for msg_id in msgs {
        if storage.strip_thinking_parts(&msg_id) {
            any = true;
        }
    }
    any
}

async fn recover_empty_content_message(
    storage: &RecoveryStorage,
    session_id: &str,
    failed_msg: &MessageData,
    error: &serde_json::Value,
) -> bool {
    let target_index = extract_message_index_from_text(&get_error_message_value(error))
        .map(|i| i as isize);
    let failed_id = failed_msg.info.as_ref().and_then(|i| i.id.clone());
    let mut any = false;

    let msgs_with_empty_text = storage.find_messages_with_empty_text_parts(session_id);
    for msg_id in msgs_with_empty_text {
        if storage.replace_empty_text_parts(&msg_id, PLACEHOLDER_TEXT) {
            any = true;
        }
    }

    let thinking_only = storage.find_messages_with_thinking_only(session_id);
    for msg_id in thinking_only {
        if storage.inject_text_part(session_id, &msg_id, PLACEHOLDER_TEXT) {
            any = true;
        }
    }

    if let Some(idx) = target_index {
        if let Some(target_msg_id) = storage.find_empty_message_by_index(session_id, idx) {
            if storage.replace_empty_text_parts(&target_msg_id, PLACEHOLDER_TEXT) {
                return true;
            }
            if storage.inject_text_part(session_id, &target_msg_id, PLACEHOLDER_TEXT) {
                return true;
            }
        }
    }

    if let Some(id) = failed_id {
        if storage.replace_empty_text_parts(&id, PLACEHOLDER_TEXT) {
            return true;
        }
        if storage.inject_text_part(session_id, &id, PLACEHOLDER_TEXT) {
            return true;
        }
    }

    let empty_ids = storage.find_empty_messages(session_id);
    for msg_id in empty_ids {
        if storage.replace_empty_text_parts(&msg_id, PLACEHOLDER_TEXT) {
            any = true;
        }
        if storage.inject_text_part(session_id, &msg_id, PLACEHOLDER_TEXT) {
            any = true;
        }
    }

    any
}

pub async fn handle_session_recovery(
    session_id: &str,
    error: &serde_json::Value,
    failed_message: Option<&MessageData>,
    config: Option<&RecoveryConfig>,
    storage: Option<&RecoveryStorage>,
) -> RecoveryResult {
    let Some(error_type) = detect_session_error_type(error) else {
        return RecoveryResult::not_attempted();
    };

    let storage = storage
        .cloned()
        .unwrap_or_else(RecoveryStorage::from_default_paths);
    let failed = failed_message.cloned().unwrap_or_default();

    let success = match error_type {
        RecoveryErrorType::ToolResultMissing => {
            recover_tool_result_missing(&storage, session_id, &failed).await
        }
        RecoveryErrorType::ThinkingBlockOrder => {
            recover_thinking_block_order(&storage, session_id, error).await
        }
        RecoveryErrorType::ThinkingDisabledViolation => {
            recover_thinking_disabled_violation(&storage, session_id).await
        }
        RecoveryErrorType::EmptyContent => {
            recover_empty_content_message(&storage, session_id, &failed, error).await
        }
        _ => false,
    };

    let default_message = match error_type {
        RecoveryErrorType::ToolResultMissing => "Injecting cancelled tool results...",
        RecoveryErrorType::ThinkingBlockOrder => "Fixing message structure...",
        RecoveryErrorType::ThinkingDisabledViolation => "Stripping thinking blocks...",
        RecoveryErrorType::EmptyContent => "Adding placeholder content...",
        _ => "Session recovery attempted",
    };

    let message = config
        .and_then(|c| c.custom_messages.as_ref())
        .and_then(|m| m.get(error_type.as_str()))
        .cloned()
        .or_else(|| {
            if success {
                Some(default_message.to_string())
            } else {
                None
            }
        });

    RecoveryResult {
        attempted: true,
        success,
        message,
        error_type: Some(error_type.as_str().to_string()),
    }
}

// =========================
// Unified recovery entry points
// =========================

pub async fn handle_recovery(input: RecoveryInput<'_>) -> RecoveryResult {
    // Priority 1: Context Window Limit
    if let Some(err) = input.error {
        let ctx = handle_context_window_recovery(input.session_id, err, input.config);
        if ctx.attempted && ctx.success {
            return ctx;
        }
    }

    // Priority 2: Session Recovery
    if let Some(err) = input.error {
        let session = handle_session_recovery(
            input.session_id,
            err,
            input.message,
            input.config,
            input.storage,
        )
        .await;
        if session.attempted && session.success {
            return session;
        }
    }

    // Priority 3: Edit Error Recovery
    if let (Some(tool), Some(output)) = (input.tool_name, input.tool_output) {
        let edit = handle_edit_error_recovery(tool, output);
        if edit.attempted && edit.success {
            return edit;
        }
    }

    RecoveryResult::not_attempted()
}

pub struct RecoveryInput<'a> {
    pub session_id: &'a str,
    pub error: Option<&'a serde_json::Value>,
    pub tool_name: Option<&'a str>,
    pub tool_output: Option<&'a str>,
    pub message: Option<&'a MessageData>,
    pub config: Option<&'a RecoveryConfig>,
    pub storage: Option<&'a RecoveryStorage>,
}

pub fn detect_recoverable_error(error: &serde_json::Value) -> (bool, Option<String>) {
    if let Some(parsed) = parse_token_limit_error(error) {
        let _ = parsed;
        return (true, Some("context_window_limit".to_string()));
    }
    if let Some(t) = detect_session_error_type(error) {
        return (true, Some(t.as_str().to_string()));
    }
    (false, None)
}

// =========================
// Hook wrapper
// =========================

#[derive(Debug, Clone, Default)]
pub struct RecoveryHook {
    config: Option<RecoveryConfig>,
}

impl RecoveryHook {
    pub fn new() -> Self {
        Self { config: None }
    }

    pub fn with_config(config: RecoveryConfig) -> Self {
        Self {
            config: Some(config),
        }
    }
}

#[async_trait]
impl Hook for RecoveryHook {
    fn name(&self) -> &str {
        "recovery"
    }

    fn events(&self) -> &[HookEvent] {
        &[HookEvent::Stop, HookEvent::PostToolUse]
    }

    async fn execute(
        &self,
        event: HookEvent,
        input: &HookInput,
        _context: &HookContext,
    ) -> HookResult {
        let session_id = input.session_id.as_deref().unwrap_or("");
        if session_id.is_empty() {
            return Ok(HookOutput::pass());
        }

        match event {
            HookEvent::PostToolUse => {
                let tool = input.tool_name.as_deref().unwrap_or("");
                let output = input
                    .tool_output
                    .as_ref()
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                if tool.is_empty() || output.is_empty() {
                    return Ok(HookOutput::pass());
                }

                let result = handle_edit_error_recovery(tool, output);
                if result.attempted && result.success {
                    return Ok(HookOutput::continue_with_message(
                        result.message.unwrap_or_else(|| EDIT_ERROR_REMINDER.to_string()),
                    ));
                }
                Ok(HookOutput::pass())
            }
            HookEvent::Stop => {
                let Some(err) = input.extra.get("error") else {
                    return Ok(HookOutput::pass());
                };
                let result = handle_recovery(RecoveryInput {
                    session_id,
                    error: Some(err),
                    tool_name: None,
                    tool_output: None,
                    message: None,
                    config: self.config.as_ref(),
                    storage: None,
                })
                .await;

                if result.attempted && result.success {
                    // Context limit errors are blocking in TS; emulate by blocking the stop.
                    if result
                        .error_type
                        .as_deref()
                        .map(|t| t.contains("token") || t.contains("context"))
                        .unwrap_or(false)
                    {
                        return Ok(HookOutput::block_with_reason(
                            result.message.unwrap_or_default(),
                        ));
                    }
                    return Ok(HookOutput::continue_with_message(
                        result.message.unwrap_or_default(),
                    ));
                }
                Ok(HookOutput::pass())
            }
            _ => Ok(HookOutput::pass()),
        }
    }
}

// =========================
// Tests
// =========================

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_detect_edit_error() {
        assert!(detect_edit_error("oldString not found"));
        assert!(!detect_edit_error("all good"));
    }

    #[test]
    fn test_process_edit_output_injects_reminder() {
        let out = process_edit_output("edit", "oldString not found");
        assert!(out.contains("EDIT ERROR"));
    }

    #[test]
    fn test_parse_token_limit_error_string() {
        let err = serde_json::Value::String("prompt is too long: 12000 tokens > 8000 maximum"
            .to_string());
        let parsed = parse_token_limit_error(&err).unwrap();
        assert!(parsed.current_tokens >= parsed.max_tokens);
        assert!(parsed.error_type.contains("token"));
    }

    #[test]
    fn test_handle_context_window_recovery_retry_exhaustion() {
        let session = "s1";
        let err = serde_json::Value::String("context length exceeded".to_string());

        let r1 = handle_context_window_recovery(session, &err, None);
        assert!(r1.attempted);
        assert!(r1.success);

        let r2 = handle_context_window_recovery(session, &err, None);
        assert!(r2.attempted);

        let r3 = handle_context_window_recovery(session, &err, None);
        assert!(r3.attempted);
        assert!(r3.message.unwrap_or_default().contains("RECOVERY FAILED"));
    }

    #[test]
    fn test_storage_inject_and_find_empty_messages() {
        let dir = tempdir().unwrap();
        let storage = RecoveryStorage {
            message_storage: dir.path().join("message"),
            part_storage: dir.path().join("part"),
        };

        // Create message meta file
        let session_id = "sess";
        let message_dir = storage.message_storage.join(session_id);
        fs::create_dir_all(&message_dir).unwrap();
        let msg = StoredMessageMeta {
            id: "m1".to_string(),
            session_id: session_id.to_string(),
            role: "assistant".to_string(),
            parent_id: None,
            time: Some(StoredMessageTime { created: 1, completed: None }),
            error: None,
        };
        fs::write(
            message_dir.join("m1.json"),
            serde_json::to_string_pretty(&msg).unwrap(),
        )
        .unwrap();

        // No parts => empty
        assert_eq!(storage.find_empty_messages(session_id), vec!["m1".to_string()]);

        // Inject text part => not empty
        assert!(storage.inject_text_part(session_id, "m1", "hello"));
        assert!(storage.find_empty_messages(session_id).is_empty());
    }
}
