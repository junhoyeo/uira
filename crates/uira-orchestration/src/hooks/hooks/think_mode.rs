//! Think Mode Hook
//!
//! Activates extended thinking/reasoning mode when users include
//! think keywords in their prompts. Supports multilingual detection.

use async_trait::async_trait;
use lazy_static::lazy_static;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::RwLock;

use super::super::hook::{Hook, HookContext, HookResult};
use super::super::types::{HookEvent, HookInput, HookOutput};

lazy_static! {
    static ref THINK_MODE_STATE: RwLock<HashMap<String, ThinkModeState>> =
        RwLock::new(HashMap::new());

    // Code block patterns
    static ref CODE_BLOCK_PATTERN: Regex = Regex::new(r"```[\s\S]*?```").unwrap();
    static ref INLINE_CODE_PATTERN: Regex = Regex::new(r"`[^`]+`").unwrap();

    // English patterns
    static ref ULTRATHINK_PATTERN: Regex = Regex::new(r"(?i)\bultrathink\b").unwrap();
    static ref THINK_PATTERN: Regex = Regex::new(r"(?i)\bthink\b").unwrap();
}

const MULTILINGUAL_KEYWORDS: &[&str] = &[
    // Korean
    "생각",
    "고민",
    "검토",
    "제대로",
    // Chinese (Simplified & Traditional)
    "思考",
    "考虑",
    "考慮",
    // Japanese
    "考え",
    "熟考",
    // Hindi
    "सोच",
    "विचार",
    // Arabic
    "تفكير",
    "تأمل",
    // Bengali
    "চিন্তা",
    "ভাবনা",
    // Russian
    "думать",
    "думай",
    "размышлять",
    "размышляй",
    // Portuguese
    "pensar",
    "pense",
    "refletir",
    "reflita",
    // Spanish
    "piensa",
    "reflexionar",
    "reflexiona",
    // French
    "penser",
    "réfléchir",
    "réfléchis",
    // German
    "denken",
    "denk",
    "nachdenken",
    // Vietnamese
    "suy nghĩ",
    "cân nhắc",
    // Turkish
    "düşün",
    "düşünmek",
    // Italian
    "pensare",
    "pensa",
    "riflettere",
    "rifletti",
    // Thai
    "คิด",
    "พิจารณา",
    // Polish
    "myśl",
    "myśleć",
    "zastanów",
    // Dutch
    "nadenken",
    // Indonesian/Malay
    "berpikir",
    "pikir",
    "pertimbangkan",
    // Ukrainian
    "думати",
    "роздумувати",
    // Greek
    "σκέψου",
    "σκέφτομαι",
    // Czech
    "myslet",
    "mysli",
    "přemýšlet",
    // Romanian
    "gândește",
    "gândi",
    "reflectă",
    // Swedish
    "tänka",
    "tänk",
    "fundera",
    // Hungarian
    "gondolkodj",
    "gondolkodni",
    // Finnish
    "ajattele",
    "ajatella",
    "pohdi",
    // Danish
    "tænk",
    "tænke",
    "overvej",
    // Norwegian
    "tenk",
    "tenke",
    "gruble",
    // Hebrew
    "חשוב",
    "לחשוב",
    "להרהר",
];

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ThinkModeState {
    pub requested: bool,
    pub model_switched: bool,
    pub thinking_config_injected: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelRef {
    pub provider_id: String,
    pub model_id: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ThinkingConfig {
    Anthropic {
        thinking: AnthropicThinking,
        #[serde(rename = "maxTokens")]
        max_tokens: u32,
    },
    AmazonBedrock {
        #[serde(rename = "reasoningConfig")]
        reasoning_config: BedrockReasoning,
        #[serde(rename = "maxTokens")]
        max_tokens: u32,
    },
    Google {
        #[serde(rename = "providerOptions")]
        provider_options: GoogleProviderOptions,
    },
    OpenAI {
        reasoning_effort: String,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AnthropicThinking {
    #[serde(rename = "type")]
    pub thinking_type: String,
    #[serde(rename = "budgetTokens")]
    pub budget_tokens: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BedrockReasoning {
    #[serde(rename = "type")]
    pub reasoning_type: String,
    #[serde(rename = "budgetTokens")]
    pub budget_tokens: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GoogleProviderOptions {
    pub google: GoogleThinkingOptions,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GoogleThinkingOptions {
    #[serde(rename = "thinkingConfig")]
    pub thinking_config: GoogleThinkingConfig,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GoogleThinkingConfig {
    #[serde(rename = "thinkingLevel")]
    pub thinking_level: String,
}

lazy_static! {
    pub static ref THINKING_CONFIGS: HashMap<&'static str, ThinkingConfig> = {
        let mut m = HashMap::new();
        m.insert(
            "anthropic",
            ThinkingConfig::Anthropic {
                thinking: AnthropicThinking {
                    thinking_type: "enabled".to_string(),
                    budget_tokens: 64000,
                },
                max_tokens: 128000,
            },
        );
        m.insert(
            "amazon-bedrock",
            ThinkingConfig::AmazonBedrock {
                reasoning_config: BedrockReasoning {
                    reasoning_type: "enabled".to_string(),
                    budget_tokens: 32000,
                },
                max_tokens: 64000,
            },
        );
        m.insert(
            "google",
            ThinkingConfig::Google {
                provider_options: GoogleProviderOptions {
                    google: GoogleThinkingOptions {
                        thinking_config: GoogleThinkingConfig {
                            thinking_level: "HIGH".to_string(),
                        },
                    },
                },
            },
        );
        m.insert(
            "openai",
            ThinkingConfig::OpenAI {
                reasoning_effort: "high".to_string(),
            },
        );
        m
    };
}

lazy_static! {
    static ref HIGH_VARIANT_MAP: HashMap<&'static str, &'static str> = {
        let mut m = HashMap::new();
        // Claude
        m.insert("claude-sonnet-4-5", "claude-sonnet-4-5-high");
        m.insert("claude-opus-4-5", "claude-opus-4-5-high");
        m.insert("claude-3-5-sonnet", "claude-3-5-sonnet-high");
        m.insert("claude-3-opus", "claude-3-opus-high");
        // GPT-4
        m.insert("gpt-4", "gpt-4-high");
        m.insert("gpt-4-turbo", "gpt-4-turbo-high");
        m.insert("gpt-4o", "gpt-4o-high");
        // GPT-5
        m.insert("gpt-5", "gpt-5-high");
        m.insert("gpt-5-mini", "gpt-5-mini-high");
        // Gemini
        m.insert("gemini-2-pro", "gemini-2-pro-high");
        m.insert("gemini-3-pro", "gemini-3-pro-high");
        m.insert("gemini-3-flash", "gemini-3-flash-high");
        m
    };

    static ref ALREADY_HIGH: std::collections::HashSet<&'static str> = {
        HIGH_VARIANT_MAP.values().copied().collect()
    };

    static ref THINKING_CAPABLE_MODELS: HashMap<&'static str, Vec<&'static str>> = {
        let mut m = HashMap::new();
        m.insert("anthropic", vec!["claude-sonnet-4", "claude-opus-4", "claude-3"]);
        m.insert("amazon-bedrock", vec!["claude", "anthropic"]);
        m.insert("google", vec!["gemini-2", "gemini-3"]);
        m.insert("openai", vec!["gpt-4", "gpt-5", "o1", "o3"]);
        m
    };
}

pub struct ThinkModeHook;

impl ThinkModeHook {
    pub fn new() -> Self {
        Self
    }

    fn remove_code_blocks(text: &str) -> String {
        let without_fenced = CODE_BLOCK_PATTERN.replace_all(text, "");
        INLINE_CODE_PATTERN
            .replace_all(&without_fenced, "")
            .to_string()
    }

    pub fn detect_think_keyword(text: &str) -> bool {
        let text_without_code = Self::remove_code_blocks(text);

        // Check English patterns
        if ULTRATHINK_PATTERN.is_match(&text_without_code)
            || THINK_PATTERN.is_match(&text_without_code)
        {
            return true;
        }

        // Check multilingual keywords (case-insensitive)
        let text_lower = text_without_code.to_lowercase();
        MULTILINGUAL_KEYWORDS
            .iter()
            .any(|kw| text_lower.contains(&kw.to_lowercase()))
    }

    pub fn detect_ultrathink_keyword(text: &str) -> bool {
        let text_without_code = Self::remove_code_blocks(text);
        ULTRATHINK_PATTERN.is_match(&text_without_code)
    }

    pub fn extract_prompt_text(parts: &[MessagePart]) -> String {
        parts
            .iter()
            .filter(|p| p.part_type == "text")
            .filter_map(|p| p.text.as_ref())
            .cloned()
            .collect::<Vec<_>>()
            .join("")
    }

    fn extract_model_prefix(model_id: &str) -> (String, String) {
        if let Some(slash_idx) = model_id.find('/') {
            (
                model_id[..=slash_idx].to_string(),
                model_id[slash_idx + 1..].to_string(),
            )
        } else {
            (String::new(), model_id.to_string())
        }
    }

    fn normalize_model_id(model_id: &str) -> String {
        let re = Regex::new(r"\.(\d+)").unwrap();
        re.replace_all(model_id, "-$1").to_string()
    }

    pub fn get_high_variant(model_id: &str) -> Option<String> {
        let normalized = Self::normalize_model_id(model_id);
        let (prefix, base) = Self::extract_model_prefix(&normalized);

        if ALREADY_HIGH.contains(base.as_str()) || base.ends_with("-high") {
            return None;
        }

        HIGH_VARIANT_MAP
            .get(base.as_str())
            .map(|high_base| format!("{}{}", prefix, high_base))
    }

    pub fn is_already_high_variant(model_id: &str) -> bool {
        let normalized = Self::normalize_model_id(model_id);
        let (_, base) = Self::extract_model_prefix(&normalized);
        ALREADY_HIGH.contains(base.as_str()) || base.ends_with("-high")
    }

    fn resolve_provider(provider_id: &str, model_id: &str) -> String {
        if provider_id == "github-copilot" {
            let model_lower = model_id.to_lowercase();
            if model_lower.contains("claude") {
                return "anthropic".to_string();
            }
            if model_lower.contains("gemini") {
                return "google".to_string();
            }
            if model_lower.contains("gpt")
                || model_lower.contains("o1")
                || model_lower.contains("o3")
            {
                return "openai".to_string();
            }
        }
        provider_id.to_string()
    }

    pub fn get_thinking_config(
        provider_id: &str,
        model_id: &str,
    ) -> Option<&'static ThinkingConfig> {
        let normalized = Self::normalize_model_id(model_id);
        let (_, base) = Self::extract_model_prefix(&normalized);

        if Self::is_already_high_variant(&normalized) {
            return None;
        }

        let resolved_provider = Self::resolve_provider(provider_id, model_id);

        let config = THINKING_CONFIGS.get(resolved_provider.as_str())?;
        let capable_patterns = THINKING_CAPABLE_MODELS.get(resolved_provider.as_str())?;

        let base_lower = base.to_lowercase();
        let is_capable = capable_patterns
            .iter()
            .any(|pattern| base_lower.contains(&pattern.to_lowercase()));

        if is_capable {
            Some(config)
        } else {
            None
        }
    }

    pub fn get_claude_thinking_config(budget_tokens: Option<u32>) -> ThinkingConfig {
        ThinkingConfig::Anthropic {
            thinking: AnthropicThinking {
                thinking_type: "enabled".to_string(),
                budget_tokens: budget_tokens.unwrap_or(64000),
            },
            max_tokens: 128000,
        }
    }

    // State management
    pub fn clear_state(session_id: &str) {
        if let Ok(mut state) = THINK_MODE_STATE.write() {
            state.remove(session_id);
        }
    }

    pub fn get_state(session_id: &str) -> Option<ThinkModeState> {
        THINK_MODE_STATE
            .read()
            .ok()
            .and_then(|s| s.get(session_id).cloned())
    }

    pub fn is_active(session_id: &str) -> bool {
        Self::get_state(session_id)
            .map(|s| s.requested)
            .unwrap_or(false)
    }

    pub fn process_prompt(session_id: &str, prompt_text: &str) -> ThinkModeState {
        let state = if Self::detect_think_keyword(prompt_text) {
            ThinkModeState {
                requested: true,
                ..Default::default()
            }
        } else {
            ThinkModeState::default()
        };

        if let Ok(mut states) = THINK_MODE_STATE.write() {
            states.insert(session_id.to_string(), state.clone());
        }

        state
    }

    pub fn process_with_model(
        session_id: &str,
        parts: &[MessagePart],
        model: Option<&ModelRef>,
    ) -> ThinkModeState {
        let prompt_text = Self::extract_prompt_text(parts);

        let mut state = ThinkModeState::default();

        if !Self::detect_think_keyword(&prompt_text) {
            if let Ok(mut states) = THINK_MODE_STATE.write() {
                states.insert(session_id.to_string(), state.clone());
            }
            return state;
        }

        state.requested = true;

        if let Some(model_ref) = model {
            state.provider_id = Some(model_ref.provider_id.clone());
            state.model_id = Some(model_ref.model_id.clone());

            if !Self::is_already_high_variant(&model_ref.model_id) {
                if Self::get_high_variant(&model_ref.model_id).is_some() {
                    state.model_switched = true;
                }

                if Self::get_thinking_config(&model_ref.provider_id, &model_ref.model_id).is_some()
                {
                    state.thinking_config_injected = true;
                }
            }
        }

        if let Ok(mut states) = THINK_MODE_STATE.write() {
            states.insert(session_id.to_string(), state.clone());
        }

        state
    }

    pub fn should_activate(prompt: &str) -> bool {
        Self::detect_think_keyword(prompt)
    }

    pub fn should_activate_ultrathink(prompt: &str) -> bool {
        Self::detect_ultrathink_keyword(prompt)
    }
}

impl Default for ThinkModeHook {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Hook for ThinkModeHook {
    fn name(&self) -> &str {
        "think-mode"
    }

    fn events(&self) -> &[HookEvent] {
        &[HookEvent::UserPromptSubmit]
    }

    async fn execute(
        &self,
        _event: HookEvent,
        input: &HookInput,
        _context: &HookContext,
    ) -> HookResult {
        // Check for think mode triggers in the user prompt
        if let Some(prompt) = &input.prompt {
            let should_activate = Self::should_activate(prompt);
            let should_activate_ultra = Self::should_activate_ultrathink(prompt);

            if should_activate || should_activate_ultra {
                // Think mode detected - this would typically trigger model/config changes
                // For now, just pass through
                return Ok(HookOutput::pass());
            }
        }

        Ok(HookOutput::pass())
    }
}

#[derive(Debug, Clone)]
pub struct MessagePart {
    pub part_type: String,
    pub text: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_think_keyword_english() {
        assert!(ThinkModeHook::detect_think_keyword(
            "Please think about this"
        ));
        assert!(ThinkModeHook::detect_think_keyword("THINK carefully"));
        assert!(ThinkModeHook::detect_think_keyword("ultrathink mode"));
    }

    #[test]
    fn test_detect_think_keyword_multilingual() {
        // Korean
        assert!(ThinkModeHook::detect_think_keyword("이것에 대해 생각해봐"));
        // Chinese
        assert!(ThinkModeHook::detect_think_keyword("请思考一下"));
        // Japanese
        assert!(ThinkModeHook::detect_think_keyword("考えてください"));
        // Russian
        assert!(ThinkModeHook::detect_think_keyword("думай об этом"));
    }

    #[test]
    fn test_detect_think_keyword_ignores_code() {
        // Should not trigger inside code blocks
        assert!(!ThinkModeHook::detect_think_keyword(
            "```\nfunction think() {}\n```"
        ));
        assert!(!ThinkModeHook::detect_think_keyword("Use `think` function"));
    }

    #[test]
    fn test_detect_ultrathink() {
        assert!(ThinkModeHook::detect_ultrathink_keyword(
            "ultrathink please"
        ));
        assert!(!ThinkModeHook::detect_ultrathink_keyword("just think"));
    }

    #[test]
    fn test_get_high_variant() {
        assert_eq!(
            ThinkModeHook::get_high_variant("claude-sonnet-4-5"),
            Some("claude-sonnet-4-5-high".to_string())
        );
        assert_eq!(
            ThinkModeHook::get_high_variant("gpt-4"),
            Some("gpt-4-high".to_string())
        );
        // Already high
        assert_eq!(
            ThinkModeHook::get_high_variant("claude-sonnet-4-5-high"),
            None
        );
        // Unknown model
        assert_eq!(ThinkModeHook::get_high_variant("unknown-model"), None);
    }

    #[test]
    fn test_get_high_variant_with_prefix() {
        assert_eq!(
            ThinkModeHook::get_high_variant("vertex_ai/claude-sonnet-4-5"),
            Some("vertex_ai/claude-sonnet-4-5-high".to_string())
        );
    }

    #[test]
    fn test_is_already_high_variant() {
        assert!(ThinkModeHook::is_already_high_variant(
            "claude-sonnet-4-5-high"
        ));
        assert!(ThinkModeHook::is_already_high_variant("custom-model-high"));
        assert!(!ThinkModeHook::is_already_high_variant("claude-sonnet-4-5"));
    }

    #[test]
    fn test_get_thinking_config() {
        let config = ThinkModeHook::get_thinking_config("anthropic", "claude-sonnet-4-5");
        assert!(config.is_some());

        let config = ThinkModeHook::get_thinking_config("openai", "gpt-4");
        assert!(config.is_some());

        // Already high - no config needed
        let config = ThinkModeHook::get_thinking_config("anthropic", "claude-sonnet-4-5-high");
        assert!(config.is_none());

        // Unknown provider
        let config = ThinkModeHook::get_thinking_config("unknown", "some-model");
        assert!(config.is_none());
    }

    #[test]
    fn test_resolve_github_copilot() {
        assert_eq!(
            ThinkModeHook::get_thinking_config("github-copilot", "claude-3-5-sonnet"),
            ThinkModeHook::get_thinking_config("anthropic", "claude-3-5-sonnet")
        );
    }

    #[test]
    fn test_extract_prompt_text() {
        let parts = vec![
            MessagePart {
                part_type: "text".to_string(),
                text: Some("Hello ".to_string()),
            },
            MessagePart {
                part_type: "image".to_string(),
                text: None,
            },
            MessagePart {
                part_type: "text".to_string(),
                text: Some("world".to_string()),
            },
        ];

        let result = ThinkModeHook::extract_prompt_text(&parts);
        assert_eq!(result, "Hello world");
    }

    #[test]
    fn test_state_management() {
        let session_id = "test-session-state";

        // Initially not active
        assert!(!ThinkModeHook::is_active(session_id));

        // Process a think request
        let state = ThinkModeHook::process_prompt(session_id, "please think about this");
        assert!(state.requested);
        assert!(ThinkModeHook::is_active(session_id));

        // Get state
        let retrieved = ThinkModeHook::get_state(session_id);
        assert!(retrieved.is_some());
        assert!(retrieved.unwrap().requested);

        // Clear state
        ThinkModeHook::clear_state(session_id);
        assert!(!ThinkModeHook::is_active(session_id));
    }

    #[test]
    fn test_process_with_model() {
        let session_id = "test-session-model";
        let parts = vec![MessagePart {
            part_type: "text".to_string(),
            text: Some("think carefully".to_string()),
        }];
        let model = ModelRef {
            provider_id: "anthropic".to_string(),
            model_id: "claude-sonnet-4-5".to_string(),
        };

        let state = ThinkModeHook::process_with_model(session_id, &parts, Some(&model));

        assert!(state.requested);
        assert!(state.model_switched);
        assert!(state.thinking_config_injected);
        assert_eq!(state.provider_id, Some("anthropic".to_string()));

        ThinkModeHook::clear_state(session_id);
    }

    #[test]
    fn test_claude_thinking_config() {
        let config = ThinkModeHook::get_claude_thinking_config(None);
        match config {
            ThinkingConfig::Anthropic {
                thinking,
                max_tokens,
            } => {
                assert_eq!(thinking.budget_tokens, 64000);
                assert_eq!(max_tokens, 128000);
            }
            _ => panic!("Expected Anthropic config"),
        }

        let config = ThinkModeHook::get_claude_thinking_config(Some(32000));
        match config {
            ThinkingConfig::Anthropic { thinking, .. } => {
                assert_eq!(thinking.budget_tokens, 32000);
            }
            _ => panic!("Expected Anthropic config"),
        }
    }

    #[test]
    fn test_should_activate() {
        assert!(ThinkModeHook::should_activate("think about this"));
        assert!(!ThinkModeHook::should_activate("do something"));

        assert!(ThinkModeHook::should_activate_ultrathink("ultrathink mode"));
        assert!(!ThinkModeHook::should_activate_ultrathink("think mode"));
    }
}
