use async_trait::async_trait;
use lazy_static::lazy_static;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::RwLock;

use crate::hook::{Hook, HookContext, HookResult};
use crate::types::{HookEvent, HookInput, HookOutput};

pub const DEFAULT_THRESHOLD: f64 = 0.85;
pub const CRITICAL_THRESHOLD: f64 = 0.95;
pub const MIN_TOKENS_FOR_COMPACTION: u64 = 50_000;
pub const COMPACTION_COOLDOWN_MS: u64 = 60_000;
pub const MAX_WARNINGS: u32 = 3;
pub const CHARS_PER_TOKEN: usize = 4;

pub fn claude_default_context_limit() -> u64 {
    if matches!(
        std::env::var("ANTHROPIC_1M_CONTEXT").ok().as_deref(),
        Some("true")
    ) || matches!(
        std::env::var("VERTEX_ANTHROPIC_1M_CONTEXT").ok().as_deref(),
        Some("true")
    ) {
        1_000_000
    } else {
        200_000
    }
}

pub const CONTEXT_WARNING_MESSAGE: &str = r#"CONTEXT WINDOW WARNING - APPROACHING LIMIT

Your context usage is getting high. Consider these actions to prevent hitting the limit:

1. USE COMPACT COMMAND
   - Run /compact to summarize the conversation
   - This frees up context space while preserving important information

2. BE MORE CONCISE
   - Show only relevant code portions
   - Use file paths instead of full code blocks
   - Summarize instead of repeating information

3. FOCUS YOUR REQUESTS
   - Work on one task at a time
   - Complete current tasks before starting new ones
   - Avoid unnecessary back-and-forth

Current Status: Context usage is high but recoverable.
Action recommended: Use /compact when convenient.
"#;

pub const CONTEXT_CRITICAL_MESSAGE: &str = r#"CRITICAL: CONTEXT WINDOW ALMOST FULL

Your context usage is critically high. Immediate action required:

1. COMPACT NOW
   - Run /compact immediately to summarize the conversation
   - Without compaction, the next few messages may fail

2. AVOID LARGE OUTPUTS
   - Do not show full files
   - Use summaries instead of detailed outputs
   - Be as concise as possible

3. PREPARE FOR SESSION HANDOFF
   - If compaction doesn't help enough, prepare to continue in a new session
   - Note your current progress and next steps

WARNING: Further messages may fail if context is not reduced.
Action required: Run /compact now.
"#;

pub const COMPACTION_SUCCESS_MESSAGE: &str =
    "Context compacted successfully. Session can continue normally.";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CompactionAction {
    None,
    Warn,
    Compact,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextUsageResult {
    #[serde(rename = "totalTokens")]
    pub total_tokens: u64,
    #[serde(rename = "usageRatio")]
    pub usage_ratio: f64,
    #[serde(rename = "isWarning")]
    pub is_warning: bool,
    #[serde(rename = "isCritical")]
    pub is_critical: bool,
    pub action: CompactionAction,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PreemptiveCompactionConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    #[serde(rename = "warningThreshold", skip_serializing_if = "Option::is_none")]
    pub warning_threshold: Option<f64>,
    #[serde(rename = "criticalThreshold", skip_serializing_if = "Option::is_none")]
    pub critical_threshold: Option<f64>,
    #[serde(rename = "cooldownMs", skip_serializing_if = "Option::is_none")]
    pub cooldown_ms: Option<u64>,
    #[serde(rename = "maxWarnings", skip_serializing_if = "Option::is_none")]
    pub max_warnings: Option<u32>,
    #[serde(rename = "customMessage", skip_serializing_if = "Option::is_none")]
    pub custom_message: Option<String>,
}

/// Estimate tokens from text content.
///
/// Matches oh-my-claudecode/src/hooks/preemptive-compaction/index.ts
pub fn estimate_tokens(text: &str) -> u64 {
    text.len().div_ceil(CHARS_PER_TOKEN) as u64
}

pub fn analyze_context_usage(
    content: &str,
    config: Option<&PreemptiveCompactionConfig>,
) -> ContextUsageResult {
    let warning_threshold = config
        .and_then(|c| c.warning_threshold)
        .unwrap_or(DEFAULT_THRESHOLD);
    let critical_threshold = config
        .and_then(|c| c.critical_threshold)
        .unwrap_or(CRITICAL_THRESHOLD);
    let context_limit = claude_default_context_limit();

    let total_tokens = estimate_tokens(content);
    let usage_ratio = total_tokens as f64 / context_limit as f64;

    let is_warning = usage_ratio >= warning_threshold;
    let is_critical = usage_ratio >= critical_threshold;

    let action = if is_critical {
        CompactionAction::Compact
    } else if is_warning {
        CompactionAction::Warn
    } else {
        CompactionAction::None
    };

    ContextUsageResult {
        total_tokens,
        usage_ratio,
        is_warning,
        is_critical,
        action,
    }
}

#[derive(Debug, Clone, Default)]
struct SessionState {
    last_warning_time: u64,
    warning_count: u32,
    estimated_tokens: u64,
}

lazy_static! {
    static ref DEBUG: bool = matches!(
        std::env::var("PREEMPTIVE_COMPACTION_DEBUG").ok().as_deref(),
        Some("1")
    );
    static ref DEBUG_FILE: PathBuf = std::env::temp_dir().join("preemptive-compaction-debug.log");
    static ref SESSION_STATES: RwLock<HashMap<String, SessionState>> = RwLock::new(HashMap::new());
    static ref LAST_CLEANUP_TIME: RwLock<u64> = RwLock::new(0);
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn debug_log(msg: &str) {
    if !*DEBUG {
        return;
    }

    let line = format!(
        "[{}] [preemptive-compaction] {}\n",
        chrono::Utc::now().to_rfc3339(),
        msg
    );
    let _ = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&*DEBUG_FILE)
        .and_then(|mut f| std::io::Write::write_all(&mut f, line.as_bytes()));
}

fn maybe_cleanup_session_states() {
    let now = now_ms();

    // TS behavior: cleanup runs every 5 minutes via setInterval.
    // We approximate by running cleanup at most once per 5 minutes.
    let should_run = {
        match LAST_CLEANUP_TIME.read() {
            Ok(last) => {
                if *last == 0 {
                    false
                } else {
                    now.saturating_sub(*last) >= 5 * 60 * 1000
                }
            }
            Err(_) => false,
        }
    };

    if !should_run {
        if let Ok(mut last) = LAST_CLEANUP_TIME.write() {
            if *last == 0 {
                *last = now;
            }
        }
        return;
    }

    if let Ok(mut last) = LAST_CLEANUP_TIME.write() {
        *last = now;
    }

    let max_age = 30 * 60 * 1000;
    let Ok(mut states) = SESSION_STATES.write() else {
        return;
    };

    states.retain(|_, state| now.saturating_sub(state.last_warning_time) <= max_age);
}

fn get_session_state(session_id: &str) -> SessionState {
    let Ok(states) = SESSION_STATES.read() else {
        return SessionState::default();
    };
    states.get(session_id).cloned().unwrap_or_default()
}

fn set_session_state(session_id: &str, state: SessionState) {
    let Ok(mut states) = SESSION_STATES.write() else {
        return;
    };
    states.insert(session_id.to_string(), state);
}

fn should_show_warning(session_id: &str, config: Option<&PreemptiveCompactionConfig>) -> bool {
    let state = get_session_state(session_id);
    let cooldown_ms = config
        .and_then(|c| c.cooldown_ms)
        .unwrap_or(COMPACTION_COOLDOWN_MS);
    let max_warnings = config.and_then(|c| c.max_warnings).unwrap_or(MAX_WARNINGS);

    let now = now_ms();

    if state.last_warning_time != 0 && now.saturating_sub(state.last_warning_time) < cooldown_ms {
        debug_log("skipping warning - cooldown active");
        return false;
    }

    if state.warning_count >= max_warnings {
        debug_log("skipping warning - max reached");
        return false;
    }

    true
}

fn record_warning(session_id: &str) {
    let mut state = get_session_state(session_id);
    state.last_warning_time = now_ms();
    state.warning_count += 1;
    set_session_state(session_id, state);
}

pub fn get_session_token_estimate(session_id: &str) -> u64 {
    get_session_state(session_id).estimated_tokens
}

pub fn reset_session_token_estimate(session_id: &str) {
    let mut state = get_session_state(session_id);
    state.estimated_tokens = 0;
    state.warning_count = 0;
    state.last_warning_time = 0;
    set_session_state(session_id, state);
}

fn extract_tool_response_text(tool_output: &serde_json::Value) -> Option<String> {
    // astrape-core ToolResponse shape: { output?: string }
    if let Some(obj) = tool_output.as_object() {
        if let Some(output) = obj.get("output").and_then(|v| v.as_str()) {
            return Some(output.to_string());
        }
        // legacy-ish: { tool_response: { output } }
        if let Some(inner) = obj.get("tool_response") {
            if let Some(output) = inner.get("output").and_then(|v| v.as_str()) {
                return Some(output.to_string());
            }
        }
    }
    tool_output.as_str().map(|s| s.to_string())
}

pub struct PreemptiveCompactionHook {
    config: PreemptiveCompactionConfig,
}

impl PreemptiveCompactionHook {
    pub fn new(config: Option<PreemptiveCompactionConfig>) -> Self {
        Self {
            config: config.unwrap_or_default(),
        }
    }

    fn enabled(&self) -> bool {
        self.config.enabled != Some(false)
    }

    fn post_tool_use(
        &self,
        session_id: &str,
        tool_name: &str,
        tool_response: &str,
    ) -> Option<String> {
        maybe_cleanup_session_states();

        let tool_lower = tool_name.to_lowercase();
        let large_output_tools = ["read", "grep", "glob", "bash", "webfetch"];
        if !large_output_tools.contains(&tool_lower.as_str()) {
            return None;
        }

        let response_tokens = estimate_tokens(tool_response);
        let mut state = get_session_state(session_id);
        state.estimated_tokens += response_tokens;
        set_session_state(session_id, state.clone());

        // Equivalent to analyzing: "x".repeat(estimated_tokens * CHARS_PER_TOKEN)
        let usage_ratio = state.estimated_tokens as f64 / claude_default_context_limit() as f64;
        let warning_threshold = self.config.warning_threshold.unwrap_or(DEFAULT_THRESHOLD);
        let critical_threshold = self.config.critical_threshold.unwrap_or(CRITICAL_THRESHOLD);

        let is_warning = usage_ratio >= warning_threshold;
        let is_critical = usage_ratio >= critical_threshold;

        if !is_warning {
            return None;
        }

        if !should_show_warning(session_id, Some(&self.config)) {
            return None;
        }

        record_warning(session_id);

        if let Some(custom) = &self.config.custom_message {
            return Some(custom.clone());
        }

        Some(if is_critical {
            CONTEXT_CRITICAL_MESSAGE.to_string()
        } else {
            CONTEXT_WARNING_MESSAGE.to_string()
        })
    }

    fn stop(&self, session_id: &str) {
        let mut state = get_session_state(session_id);
        if state.warning_count > 0 {
            state.warning_count = 0;
            set_session_state(session_id, state);
        }
    }
}

impl Default for PreemptiveCompactionHook {
    fn default() -> Self {
        Self::new(None)
    }
}

#[async_trait]
impl Hook for PreemptiveCompactionHook {
    fn name(&self) -> &str {
        "preemptive-compaction"
    }

    fn events(&self) -> &[HookEvent] {
        &[HookEvent::PostToolUse, HookEvent::Stop]
    }

    async fn execute(
        &self,
        event: HookEvent,
        input: &HookInput,
        _context: &HookContext,
    ) -> HookResult {
        if !self.enabled() {
            return Ok(HookOutput::pass());
        }

        match event {
            HookEvent::PostToolUse => {
                let Some(session_id) = input.session_id.as_deref() else {
                    return Ok(HookOutput::pass());
                };
                let Some(tool_name) = input.tool_name.as_deref() else {
                    return Ok(HookOutput::pass());
                };
                let Some(tool_output) = input.tool_output.as_ref() else {
                    return Ok(HookOutput::pass());
                };
                let Some(tool_response) = extract_tool_response_text(tool_output) else {
                    return Ok(HookOutput::pass());
                };

                match self.post_tool_use(session_id, tool_name, &tool_response) {
                    Some(msg) => Ok(HookOutput::continue_with_message(msg)),
                    None => Ok(HookOutput::pass()),
                }
            }
            HookEvent::Stop => {
                if let Some(session_id) = input.session_id.as_deref() {
                    self.stop(session_id);
                }
                Ok(HookOutput::pass())
            }
            _ => Ok(HookOutput::pass()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn test_estimate_tokens() {
        assert_eq!(estimate_tokens(""), 0);
        assert_eq!(estimate_tokens("abcd"), 1);
        assert_eq!(estimate_tokens("abcde"), 2);
    }

    #[test]
    fn test_analyze_context_usage_with_custom_thresholds() {
        let cfg = PreemptiveCompactionConfig {
            warning_threshold: Some(0.0),
            critical_threshold: Some(1.0),
            ..Default::default()
        };

        let res = analyze_context_usage("x", Some(&cfg));
        assert!(res.is_warning);
        assert!(!res.is_critical);
        assert_eq!(res.action, CompactionAction::Warn);

        let cfg = PreemptiveCompactionConfig {
            warning_threshold: Some(0.0),
            critical_threshold: Some(0.0),
            ..Default::default()
        };
        let res = analyze_context_usage("x", Some(&cfg));
        assert!(res.is_warning);
        assert!(res.is_critical);
        assert_eq!(res.action, CompactionAction::Compact);
    }

    #[tokio::test]
    async fn test_hook_emits_warning_for_large_output_tools() {
        reset_session_token_estimate("sess");
        let hook = PreemptiveCompactionHook::new(Some(PreemptiveCompactionConfig {
            warning_threshold: Some(0.0),
            critical_threshold: Some(1.0),
            cooldown_ms: Some(0),
            max_warnings: Some(3),
            ..Default::default()
        }));

        let input = HookInput {
            session_id: Some("sess".to_string()),
            prompt: None,
            message: None,
            parts: None,
            tool_name: Some("read".to_string()),
            tool_input: None,
            tool_output: Some(serde_json::json!({"output": "abcd"})),
            directory: None,
            stop_reason: None,
            user_requested: None,
            extra: HashMap::new(),
        };
        let ctx = HookContext::new(Some("sess".to_string()), "/tmp".to_string());

        let out = hook
            .execute(HookEvent::PostToolUse, &input, &ctx)
            .await
            .unwrap();

        assert!(out.should_continue);
        assert!(out.message.unwrap().contains("CONTEXT WINDOW WARNING"));
    }

    #[tokio::test]
    async fn test_hook_respects_cooldown() {
        reset_session_token_estimate("sess-cooldown");

        let hook = PreemptiveCompactionHook::new(Some(PreemptiveCompactionConfig {
            warning_threshold: Some(0.0),
            critical_threshold: Some(1.0),
            // Must be larger than the delta between calls, but smaller than now_ms() - 0.
            // Epoch ms in 2026 is ~1.7e12, so 1e12 ensures the first warning passes.
            cooldown_ms: Some(1_000_000_000_000),
            max_warnings: Some(3),
            ..Default::default()
        }));

        let mk_input = || HookInput {
            session_id: Some("sess-cooldown".to_string()),
            prompt: None,
            message: None,
            parts: None,
            tool_name: Some("read".to_string()),
            tool_input: None,
            tool_output: Some(serde_json::json!({"output": "abcd"})),
            directory: None,
            stop_reason: None,
            user_requested: None,
            extra: HashMap::new(),
        };
        let ctx = HookContext::new(Some("sess-cooldown".to_string()), "/tmp".to_string());

        let out1 = hook
            .execute(HookEvent::PostToolUse, &mk_input(), &ctx)
            .await
            .unwrap();
        assert!(out1.message.is_some());

        let out2 = hook
            .execute(HookEvent::PostToolUse, &mk_input(), &ctx)
            .await
            .unwrap();
        assert!(out2.message.is_none());
    }
}
