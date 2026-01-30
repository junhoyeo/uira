use async_trait::async_trait;
use regex::Regex;

use crate::hook::{Hook, HookContext, HookResult};
use crate::types::{HookEvent, HookInput, HookOutput};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeywordType {
    Ralph,
    Autopilot,
    Ultrawork,
    Ultrathink,
    Search,
    Analyze,
}

impl KeywordType {
    pub fn name(&self) -> &'static str {
        match self {
            Self::Ralph => "ralph",
            Self::Autopilot => "autopilot",
            Self::Ultrawork => "ultrawork",
            Self::Ultrathink => "ultrathink",
            Self::Search => "search",
            Self::Analyze => "analyze",
        }
    }

    pub fn message(&self) -> &'static str {
        match self {
            Self::Ralph => "[RALPH MODE ACTIVATED]\n\nYou are now in Ralph mode - a self-referential loop that continues until completion.\nYou MUST work until the task is fully complete. Do not stop until done.",
            Self::Autopilot => "[AUTOPILOT MODE ACTIVATED]\n\nYou are now in Autopilot mode - fully autonomous execution from idea to completion.\nHandle all phases: expansion, planning, execution, QA, and validation.",
            Self::Ultrawork => "[ULTRAWORK MODE ACTIVATED]\n\nYou are now in Ultrawork mode - maximum parallel agent execution.\nDelegate aggressively. Fire multiple agents simultaneously. Never wait.",
            Self::Ultrathink => "[ULTRATHINK MODE ACTIVATED]\n\nYou are now in enhanced thinking mode.\nUse extended thinking for complex reasoning tasks.",
            Self::Search => "[SEARCH MODE ACTIVATED]\n\nYou are in search mode - focus on finding and locating information.\nUse appropriate search tools and agents.",
            Self::Analyze => "[ANALYZE MODE ACTIVATED]\n\nYou are in analysis mode - deep investigation and understanding.\nPerform thorough analysis and provide detailed insights.",
        }
    }

    const PRIORITY: [KeywordType; 6] = [
        KeywordType::Ralph,
        KeywordType::Autopilot,
        KeywordType::Ultrawork,
        KeywordType::Ultrathink,
        KeywordType::Search,
        KeywordType::Analyze,
    ];

    pub fn priority_order() -> &'static [KeywordType] {
        &Self::PRIORITY
    }
}

#[derive(Debug, Clone)]
pub struct DetectedKeyword {
    pub keyword_type: KeywordType,
    pub keyword: String,
    pub position: usize,
}

pub struct KeywordDetectorHook {
    ralph_pattern: Regex,
    autopilot_pattern: Regex,
    ultrawork_pattern: Regex,
    ultrathink_pattern: Regex,
    search_pattern: Regex,
    analyze_pattern: Regex,
    code_block_pattern: Regex,
    inline_code_pattern: Regex,
}

impl KeywordDetectorHook {
    pub fn new() -> Self {
        Self {
            ralph_pattern: Regex::new(r"(?i)\b(ralph|don't stop|must complete|until done)\b")
                .unwrap(),
            autopilot_pattern: Regex::new(
                r"(?i)\b(autopilot|auto pilot|auto-pilot|autonomous|full auto|fullsend)\b",
            )
            .unwrap(),
            ultrawork_pattern: Regex::new(r"(?i)\b(ultrawork|ulw)\b").unwrap(),
            ultrathink_pattern: Regex::new(r"(?i)\b(ultrathink|think)\b").unwrap(),
            search_pattern: Regex::new(
                r"(?i)\b(search|find|locate|lookup|explore|discover|scan|grep|query|browse|detect|trace|seek|track|pinpoint|hunt)\b|where\s+is|show\s+me|list\s+all",
            )
            .unwrap(),
            analyze_pattern: Regex::new(
                r"(?i)\b(analyze|analyse|investigate|examine|research|study|deep.?dive|inspect|audit|evaluate|assess|review|diagnose|scrutinize|dissect|debug|comprehend|interpret|breakdown|understand)\b|why\s+is|how\s+does|how\s+to",
            )
            .unwrap(),
            code_block_pattern: Regex::new(r"```[\s\S]*?```|~~~[\s\S]*?~~~").unwrap(),
            inline_code_pattern: Regex::new(r"`[^`]+`").unwrap(),
        }
    }

    fn remove_code_blocks(&self, text: &str) -> String {
        let mut result = self.code_block_pattern.replace_all(text, "").to_string();
        result = self
            .inline_code_pattern
            .replace_all(&result, "")
            .to_string();
        result
    }

    fn detect_keywords(&self, text: &str) -> Vec<DetectedKeyword> {
        let cleaned = self.remove_code_blocks(text);
        let mut detected = Vec::new();

        let patterns = [
            (KeywordType::Ralph, &self.ralph_pattern),
            (KeywordType::Autopilot, &self.autopilot_pattern),
            (KeywordType::Ultrawork, &self.ultrawork_pattern),
            (KeywordType::Ultrathink, &self.ultrathink_pattern),
            (KeywordType::Search, &self.search_pattern),
            (KeywordType::Analyze, &self.analyze_pattern),
        ];

        for (keyword_type, pattern) in patterns {
            if let Some(mat) = pattern.find(&cleaned) {
                detected.push(DetectedKeyword {
                    keyword_type,
                    keyword: mat.as_str().to_string(),
                    position: mat.start(),
                });
            }
        }

        detected
    }

    fn get_primary_keyword(&self, text: &str) -> Option<DetectedKeyword> {
        let detected = self.detect_keywords(text);

        if detected.is_empty() {
            return None;
        }

        for keyword_type in KeywordType::priority_order() {
            if let Some(kw) = detected.iter().find(|d| d.keyword_type == *keyword_type) {
                return Some(kw.clone());
            }
        }

        detected.first().cloned()
    }

    pub fn detect_and_message(&self, text: &str) -> Option<&'static str> {
        self.get_primary_keyword(text)
            .map(|kw| kw.keyword_type.message())
    }
}

impl Default for KeywordDetectorHook {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Hook for KeywordDetectorHook {
    fn name(&self) -> &str {
        "keyword-detector"
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
        let prompt_text = input.get_prompt_text();

        if prompt_text.is_empty() {
            return Ok(HookOutput::pass());
        }

        if let Some(keyword) = self.get_primary_keyword(&prompt_text) {
            return Ok(HookOutput::continue_with_message(
                keyword.keyword_type.message(),
            ));
        }

        Ok(HookOutput::pass())
    }

    fn priority(&self) -> i32 {
        100
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn test_remove_code_blocks() {
        let hook = KeywordDetectorHook::new();

        let text = "Some text ```code block``` more text";
        let cleaned = hook.remove_code_blocks(text);
        assert_eq!(cleaned, "Some text  more text");

        let text_inline = "Some `inline code` text";
        let cleaned_inline = hook.remove_code_blocks(text_inline);
        assert_eq!(cleaned_inline, "Some  text");
    }

    #[test]
    fn test_detect_ralph_keyword() {
        let hook = KeywordDetectorHook::new();
        let keywords = hook.detect_keywords("Please ralph this task");

        assert_eq!(keywords.len(), 1);
        assert_eq!(keywords[0].keyword_type, KeywordType::Ralph);
    }

    #[test]
    fn test_detect_ultrawork_keyword() {
        let hook = KeywordDetectorHook::new();
        let keywords = hook.detect_keywords("ultrawork: implement this feature");

        assert_eq!(keywords.len(), 1);
        assert_eq!(keywords[0].keyword_type, KeywordType::Ultrawork);
    }

    #[test]
    fn test_detect_search_keyword() {
        let hook = KeywordDetectorHook::new();
        let keywords = hook.detect_keywords("search for the implementation");

        assert_eq!(keywords.len(), 1);
        assert_eq!(keywords[0].keyword_type, KeywordType::Search);
    }

    #[test]
    fn test_priority_order() {
        let hook = KeywordDetectorHook::new();
        let text = "ralph and search for this";
        let primary = hook.get_primary_keyword(text);

        assert!(primary.is_some());
        assert_eq!(primary.unwrap().keyword_type, KeywordType::Ralph);
    }

    #[test]
    fn test_no_keywords() {
        let hook = KeywordDetectorHook::new();
        let keywords = hook.detect_keywords("just a normal prompt");

        assert!(keywords.is_empty());
    }

    #[tokio::test]
    async fn test_hook_execution_with_keyword() {
        let hook = KeywordDetectorHook::new();
        let input = HookInput {
            session_id: None,
            prompt: Some("ultrawork: build this feature".to_string()),
            message: None,
            parts: None,
            tool_name: None,
            tool_input: None,
            tool_output: None,
            directory: None,
            stop_reason: None,
            user_requested: None,
            transcript_path: None,
            extra: HashMap::new(),
        };
        let context = HookContext::new(None, "/tmp".to_string());

        let result = hook
            .execute(HookEvent::UserPromptSubmit, &input, &context)
            .await
            .unwrap();

        assert!(result.should_continue);
        assert!(result.message.is_some());
        assert!(result.message.unwrap().contains("ULTRAWORK"));
    }

    #[tokio::test]
    async fn test_hook_execution_without_keyword() {
        let hook = KeywordDetectorHook::new();
        let input = HookInput {
            session_id: None,
            prompt: Some("just a normal prompt".to_string()),
            message: None,
            parts: None,
            tool_name: None,
            tool_input: None,
            tool_output: None,
            directory: None,
            stop_reason: None,
            user_requested: None,
            transcript_path: None,
            extra: HashMap::new(),
        };
        let context = HookContext::new(None, "/tmp".to_string());

        let result = hook
            .execute(HookEvent::UserPromptSubmit, &input, &context)
            .await
            .unwrap();

        assert!(result.should_continue);
        assert!(result.message.is_none());
    }
}
