//! Comment Checker Hook
//!
//! Detects and warns about comments and docstrings in source code changes.
//! Runs on PostToolUse events for Write, Edit, MultiEdit, and NotebookEdit tools.
//!
//! Uses the uira-comment-checker library for tree-sitter based detection.

use async_trait::async_trait;
use std::collections::HashSet;
use std::path::Path;

use uira_comment_checker::{
    format_hook_message, CommentDetector, CommentInfo, FilterChain, LanguageRegistry,
};

use super::super::hook::{Hook, HookContext, HookResult};
use super::super::types::{HookEvent, HookInput, HookOutput};

pub const HOOK_NAME: &str = "comment-checker";

/// Tools that write/modify source files
const CHECKABLE_TOOLS: &[&str] = &["Write", "Edit", "MultiEdit", "NotebookEdit"];

/// Comment Checker Hook
///
/// Analyzes source code changes for comments and docstrings,
/// providing guidance on when comments are appropriate.
pub struct CommentCheckerHook {
    detector: CommentDetector,
    registry: LanguageRegistry,
}

impl CommentCheckerHook {
    pub fn new() -> Self {
        Self {
            detector: CommentDetector::new(),
            registry: LanguageRegistry::new(),
        }
    }

    fn get_extension(file_path: &str) -> String {
        let path = Path::new(file_path);
        match path.extension() {
            Some(ext) => ext.to_string_lossy().to_lowercase(),
            None => path
                .file_name()
                .map(|n| n.to_string_lossy().to_lowercase())
                .unwrap_or_default(),
        }
    }

    fn build_comment_text_set(comments: &[CommentInfo]) -> HashSet<String> {
        comments.iter().map(|c| c.normalized_text()).collect()
    }

    fn filter_new_comments(
        old_comments: &[CommentInfo],
        new_comments: Vec<CommentInfo>,
    ) -> Vec<CommentInfo> {
        if old_comments.is_empty() {
            return new_comments;
        }

        let old_set = Self::build_comment_text_set(old_comments);
        new_comments
            .into_iter()
            .filter(|c| !old_set.contains(&c.normalized_text()))
            .collect()
    }

    fn detect_new_comments_for_edit(
        &self,
        old_string: &str,
        new_string: &str,
        file_path: &str,
    ) -> Vec<CommentInfo> {
        let old_comments = self.detector.detect(old_string, file_path, true);
        let new_comments = self.detector.detect(new_string, file_path, true);
        Self::filter_new_comments(&old_comments, new_comments)
    }

    fn detect_comments_from_edit_lines(
        &self,
        file_path: &str,
        edits: &[serde_json::Value],
    ) -> Vec<CommentInfo> {
        let mut comments = Vec::new();
        for edit in edits {
            let Some(lines) = edit.get("lines").and_then(|v| v.as_array()) else {
                continue;
            };

            for line in lines {
                let Some(line_text) = line.as_str() else {
                    continue;
                };
                let content = if let Some((left, rhs)) = line_text.split_once('|') {
                    if Self::looks_like_hashline_prefix(left) {
                        rhs.trim_start()
                    } else {
                        line_text
                    }
                } else {
                    line_text
                };
                comments.extend(self.detector.detect(content, file_path, true));
            }
        }
        comments
    }

    fn looks_like_hashline_prefix(value: &str) -> bool {
        let raw = value.trim().trim_start_matches('L');
        let Some((line_part, hash_part)) = raw.split_once('#') else {
            return false;
        };
        if line_part
            .trim()
            .parse::<usize>()
            .ok()
            .filter(|n| *n > 0)
            .is_none()
        {
            return false;
        }
        let mut chars = hash_part.trim().chars();
        let first = chars.next();
        let second = chars.next();
        match (first, second, chars.next()) {
            (Some(a), Some(b), None) => a.is_ascii_alphabetic() && b.is_ascii_alphabetic(),
            _ => false,
        }
    }

    fn check_comments(&self, input: &HookInput) -> Option<String> {
        let tool_name = input.tool_name.as_deref()?;

        if !CHECKABLE_TOOLS.contains(&tool_name) {
            return None;
        }

        let tool_input = input.tool_input.as_ref()?;

        // Extract file_path
        let file_path = tool_input
            .get("file_path")
            .or_else(|| tool_input.get("filePath"))
            .and_then(|v| v.as_str())?;

        if file_path.is_empty() {
            return None;
        }

        // Check if file type is supported
        let ext = Self::get_extension(file_path);
        if !self.registry.is_supported(&ext) {
            return None;
        }

        let comments = match tool_name {
            "Edit" => {
                let new_string = tool_input
                    .get("new_string")
                    .or_else(|| tool_input.get("newString"))
                    .and_then(|v| v.as_str())
                    .filter(|s| !s.is_empty());

                if let Some(new_string) = new_string {
                    let old_string = tool_input
                        .get("old_string")
                        .or_else(|| tool_input.get("oldString"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("");

                    self.detect_new_comments_for_edit(old_string, new_string, file_path)
                } else {
                    let edits = tool_input
                        .get("edits")
                        .and_then(|v| v.as_array())
                        .filter(|arr| !arr.is_empty())?;

                    self.detect_comments_from_edit_lines(file_path, edits)
                }
            }
            "MultiEdit" => {
                let edits = tool_input
                    .get("edits")
                    .and_then(|v| v.as_array())
                    .filter(|arr| !arr.is_empty())?;

                let mut all_comments = Vec::new();
                for edit in edits {
                    let new_string = edit
                        .get("new_string")
                        .or_else(|| edit.get("newString"))
                        .and_then(|v| v.as_str())
                        .filter(|s| !s.is_empty());

                    if let Some(new_str) = new_string {
                        let old_string = edit
                            .get("old_string")
                            .or_else(|| edit.get("oldString"))
                            .and_then(|v| v.as_str())
                            .unwrap_or("");

                        let edit_comments =
                            self.detect_new_comments_for_edit(old_string, new_str, file_path);
                        all_comments.extend(edit_comments);
                    } else {
                        all_comments.extend(self.detect_comments_from_edit_lines(
                            file_path,
                            std::slice::from_ref(edit),
                        ));
                    }
                }
                all_comments
            }
            "Write" | "NotebookEdit" => {
                let content = tool_input
                    .get("content")
                    .and_then(|v| v.as_str())
                    .filter(|s| !s.is_empty())?;

                self.detector.detect(content, file_path, true)
            }
            _ => return None,
        };

        if comments.is_empty() {
            return None;
        }

        // Apply filters (FilterChain is created per-call as it contains non-Send types)
        let filter_chain = FilterChain::new();
        let filtered: Vec<CommentInfo> = comments
            .into_iter()
            .filter(|c| !filter_chain.should_skip(c))
            .collect();

        if filtered.is_empty() {
            return None;
        }

        // Format the warning message
        Some(format_hook_message(&filtered, None))
    }
}

impl Default for CommentCheckerHook {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Hook for CommentCheckerHook {
    fn name(&self) -> &str {
        HOOK_NAME
    }

    fn events(&self) -> &[HookEvent] {
        &[HookEvent::PostToolUse]
    }

    async fn execute(
        &self,
        _event: HookEvent,
        input: &HookInput,
        _context: &HookContext,
    ) -> HookResult {
        if let Some(message) = self.check_comments(input) {
            Ok(HookOutput::continue_with_message(message))
        } else {
            Ok(HookOutput::pass())
        }
    }

    fn priority(&self) -> i32 {
        // Run after other PostToolUse hooks
        -10
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn make_input(tool_name: &str, file_path: &str, content: &str) -> HookInput {
        let mut tool_input = serde_json::Map::new();
        tool_input.insert("file_path".to_string(), serde_json::json!(file_path));
        tool_input.insert("content".to_string(), serde_json::json!(content));

        HookInput {
            session_id: Some("test-session".to_string()),
            prompt: None,
            message: None,
            parts: None,
            tool_name: Some(tool_name.to_string()),
            tool_input: Some(serde_json::Value::Object(tool_input)),
            tool_output: None,
            directory: None,
            stop_reason: None,
            user_requested: None,
            transcript_path: None,
            extra: HashMap::new(),
        }
    }

    fn make_edit_input(
        tool_name: &str,
        file_path: &str,
        old_string: &str,
        new_string: &str,
    ) -> HookInput {
        let mut tool_input = serde_json::Map::new();
        tool_input.insert("file_path".to_string(), serde_json::json!(file_path));
        tool_input.insert("old_string".to_string(), serde_json::json!(old_string));
        tool_input.insert("new_string".to_string(), serde_json::json!(new_string));

        HookInput {
            session_id: Some("test-session".to_string()),
            prompt: None,
            message: None,
            parts: None,
            tool_name: Some(tool_name.to_string()),
            tool_input: Some(serde_json::Value::Object(tool_input)),
            tool_output: None,
            directory: None,
            stop_reason: None,
            user_requested: None,
            transcript_path: None,
            extra: HashMap::new(),
        }
    }

    #[test]
    fn test_ignores_non_checkable_tools() {
        let hook = CommentCheckerHook::new();
        let input = make_input("Read", "test.py", "# comment");

        assert!(hook.check_comments(&input).is_none());
    }

    #[test]
    fn test_ignores_non_code_files() {
        let hook = CommentCheckerHook::new();
        let input = make_input("Write", "README.md", "# This is markdown");

        assert!(hook.check_comments(&input).is_none());
    }

    #[test]
    fn test_detects_comments_in_python() {
        let hook = CommentCheckerHook::new();
        let input = make_input("Write", "test.py", "# This is a comment\nx = 1");

        let result = hook.check_comments(&input);
        assert!(result.is_some());
        assert!(result.unwrap().contains("COMMENT"));
    }

    #[test]
    fn test_detects_new_comments_in_edit() {
        let hook = CommentCheckerHook::new();
        let input = make_edit_input("Edit", "test.py", "x = 1", "# New comment\nx = 1");

        let result = hook.check_comments(&input);
        assert!(result.is_some());
    }

    #[test]
    fn test_ignores_existing_comments_in_edit() {
        let hook = CommentCheckerHook::new();
        let input = make_edit_input(
            "Edit",
            "test.py",
            "# Existing comment\nx = 1",
            "# Existing comment\nx = 2",
        );

        // Should not trigger since the comment already existed
        let result = hook.check_comments(&input);
        assert!(result.is_none());
    }

    #[test]
    fn test_no_comments_no_warning() {
        let hook = CommentCheckerHook::new();
        let input = make_input("Write", "test.py", "x = 1\ny = 2");

        assert!(hook.check_comments(&input).is_none());
    }

    #[test]
    fn test_detects_comments_in_hashline_edit_payload() {
        let hook = CommentCheckerHook::new();
        let mut tool_input = serde_json::Map::new();
        tool_input.insert("file_path".to_string(), serde_json::json!("test.py"));
        tool_input.insert(
            "edits".to_string(),
            serde_json::json!([
                {
                    "op": "replace",
                    "pos": "1#ZZ",
                    "end": "1#ZZ",
                    "lines": ["1#ZZ | # inserted comment"]
                }
            ]),
        );

        let input = HookInput {
            session_id: Some("test-session".to_string()),
            prompt: None,
            message: None,
            parts: None,
            tool_name: Some("Edit".to_string()),
            tool_input: Some(serde_json::Value::Object(tool_input)),
            tool_output: None,
            directory: None,
            stop_reason: None,
            user_requested: None,
            transcript_path: None,
            extra: HashMap::new(),
        };

        assert!(hook.check_comments(&input).is_some());
    }

    #[test]
    fn test_detects_comments_in_non_hashline_pipe_edit_payload() {
        let hook = CommentCheckerHook::new();
        let mut tool_input = serde_json::Map::new();
        tool_input.insert("file_path".to_string(), serde_json::json!("test.py"));
        tool_input.insert(
            "edits".to_string(),
            serde_json::json!([
                {
                    "op": "replace",
                    "pos": "1#ZZ",
                    "end": "1#ZZ",
                    "lines": ["value = a | b  # comment"]
                }
            ]),
        );

        let input = HookInput {
            session_id: Some("test-session".to_string()),
            prompt: None,
            message: None,
            parts: None,
            tool_name: Some("Edit".to_string()),
            tool_input: Some(serde_json::Value::Object(tool_input)),
            tool_output: None,
            directory: None,
            stop_reason: None,
            user_requested: None,
            transcript_path: None,
            extra: HashMap::new(),
        };

        assert!(hook.check_comments(&input).is_some());
    }
}
