//! Agent memo filter - detects "memo-style" comments left by AI agents.

use once_cell::sync::Lazy;
use regex::Regex;

use crate::models::CommentInfo;

/// Patterns that indicate "agent memo" comments - describing what changed or how something was implemented.
/// These are typically signs of an AI agent leaving notes for itself or the user.
static AGENT_MEMO_PATTERNS: Lazy<Vec<Regex>> = Lazy::new(|| {
    vec![
        // English patterns
        Regex::new(r"(?i)^[\s#/*-]*changed?\s+(from|to)\b").unwrap(),
        Regex::new(r"(?i)^[\s#/*-]*modified?\s+(from|to)?\b").unwrap(),
        Regex::new(r"(?i)^[\s#/*-]*updated?\s+(from|to)?\b").unwrap(),
        Regex::new(r"(?i)^[\s#/*-]*refactor(ed|ing)?\b").unwrap(),
        Regex::new(r"(?i)^[\s#/*-]*moved?\s+(from|to)\b").unwrap(),
        Regex::new(r"(?i)^[\s#/*-]*renamed?\s+(from|to)?\b").unwrap(),
        Regex::new(r"(?i)^[\s#/*-]*replaced?\b").unwrap(),
        Regex::new(r"(?i)^[\s#/*-]*removed?\b").unwrap(),
        Regex::new(r"(?i)^[\s#/*-]*deleted?\b").unwrap(),
        Regex::new(r"(?i)^[\s#/*-]*added?\b").unwrap(),
        Regex::new(r"(?i)^[\s#/*-]*implemented?\b").unwrap(),
        Regex::new(r"(?i)^[\s#/*-]*this\s+(implements?|adds?|removes?|changes?|fixes?)\b").unwrap(),
        Regex::new(r"(?i)^[\s#/*-]*here\s+we\b").unwrap(),
        Regex::new(r"(?i)^[\s#/*-]*now\s+(we|this|it)\b").unwrap(),
        Regex::new(r"(?i)^[\s#/*-]*previously\b").unwrap(),
        Regex::new(r"(?i)^[\s#/*-]*before\s+this\b").unwrap(),
        Regex::new(r"(?i)^[\s#/*-]*after\s+this\b").unwrap(),
        Regex::new(r"(?i)^[\s#/*-]*was\s+changed\b").unwrap(),
        Regex::new(r"(?i)^[\s#/*-]*implementation\s+(of|note)\b").unwrap(),
        Regex::new(r"(?i)^[\s#/*-]*note:\s*\w").unwrap(),
        Regex::new(r"(?i)^[\s#/*-]*[a-z]+\s*->\s*[a-z]+").unwrap(),
        Regex::new(r"(?i)^[\s#/*-]*converted?\s+(from|to)\b").unwrap(),
        Regex::new(r"(?i)^[\s#/*-]*migrated?\s+(from|to)?\b").unwrap(),
        Regex::new(r"(?i)^[\s#/*-]*switched?\s+(from|to)\b").unwrap(),
        // Korean patterns
        Regex::new(r"(?i)여기(서|에서)\s*").unwrap(),
        Regex::new(r"(?i)(으로|로)\s*(바뀜|변경|변환)").unwrap(),
        Regex::new(r"(?i)구현(임|함|했|된|됨)").unwrap(),
        Regex::new(r"(?i)추가(함|했|된|됨)").unwrap(),
        Regex::new(r"(?i)삭제(함|했|된|됨)").unwrap(),
        Regex::new(r"(?i)수정(함|했|된|됨)").unwrap(),
        Regex::new(r"(?i)변경(함|했|된|됨)").unwrap(),
        Regex::new(r"(?i)리팩(터|토)링").unwrap(),
        Regex::new(r"(?i)이전(에는|엔)").unwrap(),
        Regex::new(r"(?i)기존(에는|엔|의)").unwrap(),
        Regex::new(r"(?i)에서\s+\S+\s*(으로|로)\b").unwrap(),
    ]
});

/// Filter for detecting "agent memo" comments.
pub struct AgentMemoFilter;

impl AgentMemoFilter {
    pub fn new() -> Self {
        Self
    }

    /// Checks if a comment is an "agent memo" - a comment that describes what changed or how
    /// something was implemented, typically left by AI agents.
    pub fn is_agent_memo(&self, comment: &CommentInfo) -> bool {
        let mut text = comment.text.trim().to_string();

        // Strip common comment prefixes
        for prefix in &["#", "//", "/*", "--", "*"] {
            if text.starts_with(prefix) {
                text = text[prefix.len()..].trim().to_string();
            }
        }

        // Check against all patterns
        for pattern in AGENT_MEMO_PATTERNS.iter() {
            if pattern.is_match(&text) {
                return true;
            }
        }

        false
    }
}

impl Default for AgentMemoFilter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::CommentType;

    fn make_comment(text: &str) -> CommentInfo {
        CommentInfo::new(
            text.to_string(),
            1,
            "test.py".to_string(),
            CommentType::Line,
        )
    }

    #[test]
    fn test_detects_changed_from() {
        let filter = AgentMemoFilter::new();
        assert!(filter.is_agent_memo(&make_comment("# Changed from old_value to new_value")));
    }

    #[test]
    fn test_detects_modified() {
        let filter = AgentMemoFilter::new();
        assert!(filter.is_agent_memo(&make_comment("// Modified to use new implementation")));
    }

    #[test]
    fn test_detects_added() {
        let filter = AgentMemoFilter::new();
        assert!(filter.is_agent_memo(&make_comment("# Added new feature")));
    }

    #[test]
    fn test_detects_implemented() {
        let filter = AgentMemoFilter::new();
        assert!(filter.is_agent_memo(&make_comment("// Implemented the new algorithm")));
    }

    #[test]
    fn test_detects_korean_patterns() {
        let filter = AgentMemoFilter::new();
        assert!(filter.is_agent_memo(&make_comment("# 여기서 값이 변경됨")));
        assert!(filter.is_agent_memo(&make_comment("# 구현함")));
        assert!(filter.is_agent_memo(&make_comment("# 추가됨")));
    }

    #[test]
    fn test_regular_comment_not_agent_memo() {
        let filter = AgentMemoFilter::new();
        assert!(!filter.is_agent_memo(&make_comment("# Calculate the sum of values")));
        assert!(!filter.is_agent_memo(&make_comment("// TODO: fix this later")));
        assert!(!filter.is_agent_memo(&make_comment("# This function does X")));
    }

    #[test]
    fn test_detects_arrow_pattern() {
        let filter = AgentMemoFilter::new();
        assert!(filter.is_agent_memo(&make_comment("# foo -> bar")));
    }

    #[test]
    fn test_detects_note_pattern() {
        let filter = AgentMemoFilter::new();
        assert!(filter.is_agent_memo(&make_comment("# Note: this was changed")));
    }
}
