//! Comment checker integration for post-tool-use processing
//!
//! This module provides comment detection for tools that write source code,
//! warning agents when they add comments or docstrings.

use std::collections::HashSet;
use std::path::Path;
use uira_comment_checker::{
    format_hook_message, CommentDetector, CommentInfo, FilterChain, LanguageRegistry,
};

/// Tools that write/modify source files
const CHECKABLE_TOOLS: &[&str] = &["Write", "Edit", "MultiEdit", "NotebookEdit"];

/// Comment checker for post-tool-use processing
pub struct CommentChecker {
    detector: CommentDetector,
    registry: LanguageRegistry,
}

impl CommentChecker {
    pub fn new() -> Self {
        Self {
            detector: CommentDetector::new(),
            registry: LanguageRegistry::new(),
        }
    }

    /// Check if a tool is one that should be checked for comments
    pub fn should_check_tool(&self, tool_name: &str) -> bool {
        CHECKABLE_TOOLS.contains(&tool_name)
    }

    /// Check for new comments after a Write tool call
    pub fn check_write(&self, file_path: &str, content: &str) -> Option<String> {
        if !self.is_supported_file(file_path) {
            return None;
        }

        let comments = self.detector.detect(content, file_path, true);
        self.format_if_any(comments)
    }

    /// Check for new comments after an Edit tool call
    pub fn check_edit(
        &self,
        file_path: &str,
        old_string: &str,
        new_string: &str,
    ) -> Option<String> {
        if !self.is_supported_file(file_path) {
            return None;
        }

        let old_comments = self.detector.detect(old_string, file_path, true);
        let new_comments = self.detector.detect(new_string, file_path, true);
        let only_new = Self::filter_new_comments(&old_comments, new_comments);

        self.format_if_any(only_new)
    }

    /// Check the result of a tool execution and return a warning if comments were added
    pub fn check_tool_result(
        &self,
        tool_name: &str,
        tool_input: &serde_json::Value,
    ) -> Option<String> {
        if !self.should_check_tool(tool_name) {
            return None;
        }

        let file_path = tool_input
            .get("file_path")
            .or_else(|| tool_input.get("filePath"))
            .and_then(|v| v.as_str())?;

        match tool_name {
            "Write" | "NotebookEdit" => {
                let content = tool_input.get("content").and_then(|v| v.as_str())?;
                self.check_write(file_path, content)
            }
            "Edit" => {
                let old_string = tool_input
                    .get("old_string")
                    .or_else(|| tool_input.get("oldString"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let new_string = tool_input
                    .get("new_string")
                    .or_else(|| tool_input.get("newString"))
                    .and_then(|v| v.as_str())?;

                self.check_edit(file_path, old_string, new_string)
            }
            "MultiEdit" => {
                let edits = tool_input.get("edits").and_then(|v| v.as_array())?;

                let mut all_comments = Vec::new();
                for edit in edits {
                    let old_string = edit
                        .get("old_string")
                        .or_else(|| edit.get("oldString"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    let new_string = edit
                        .get("new_string")
                        .or_else(|| edit.get("newString"))
                        .and_then(|v| v.as_str());

                    if let Some(new_str) = new_string {
                        let old_comments = self.detector.detect(old_string, file_path, true);
                        let new_comments = self.detector.detect(new_str, file_path, true);
                        let only_new = Self::filter_new_comments(&old_comments, new_comments);
                        all_comments.extend(only_new);
                    }
                }

                self.format_if_any(all_comments)
            }
            _ => None,
        }
    }

    fn is_supported_file(&self, file_path: &str) -> bool {
        let path = Path::new(file_path);
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_lowercase())
            .unwrap_or_default();

        self.registry.is_supported(&ext)
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

    fn format_if_any(&self, comments: Vec<CommentInfo>) -> Option<String> {
        if comments.is_empty() {
            return None;
        }

        // Apply filters
        let filter_chain = FilterChain::new();
        let filtered: Vec<CommentInfo> = comments
            .into_iter()
            .filter(|c| !filter_chain.should_skip(c))
            .collect();

        if filtered.is_empty() {
            return None;
        }

        Some(format_hook_message(&filtered, None))
    }
}

impl Default for CommentChecker {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_should_check_tool() {
        let checker = CommentChecker::new();
        assert!(checker.should_check_tool("Write"));
        assert!(checker.should_check_tool("Edit"));
        assert!(!checker.should_check_tool("Read"));
        assert!(!checker.should_check_tool("Bash"));
    }

    #[test]
    fn test_check_write_with_comment() {
        let checker = CommentChecker::new();
        let result = checker.check_write("test.py", "# This is a comment\nx = 1");
        assert!(result.is_some());
    }

    #[test]
    fn test_check_write_no_comment() {
        let checker = CommentChecker::new();
        let result = checker.check_write("test.py", "x = 1\ny = 2");
        assert!(result.is_none());
    }

    #[test]
    fn test_check_edit_new_comment() {
        let checker = CommentChecker::new();
        let result = checker.check_edit("test.py", "x = 1", "# Comment\nx = 1");
        assert!(result.is_some());
    }

    #[test]
    fn test_check_edit_existing_comment() {
        let checker = CommentChecker::new();
        let result = checker.check_edit("test.py", "# Comment\nx = 1", "# Comment\nx = 2");
        assert!(result.is_none());
    }

    #[test]
    fn test_check_tool_result() {
        let checker = CommentChecker::new();
        let result = checker.check_tool_result(
            "Write",
            &json!({
                "file_path": "test.py",
                "content": "# New comment\nx = 1"
            }),
        );
        assert!(result.is_some());
    }
}
