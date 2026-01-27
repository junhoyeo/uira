use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CommentType {
    Line,
    Block,
    Docstring,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommentInfo {
    pub text: String,
    pub line_number: usize,
    pub file_path: String,
    pub comment_type: CommentType,
    pub is_docstring: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<HashMap<String, String>>,
}

impl CommentInfo {
    pub fn new(
        text: String,
        line_number: usize,
        file_path: String,
        comment_type: CommentType,
    ) -> Self {
        Self {
            text,
            line_number,
            file_path,
            comment_type,
            is_docstring: comment_type == CommentType::Docstring,
            metadata: None,
        }
    }

    pub fn normalized_text(&self) -> String {
        self.text.trim().to_lowercase()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_comment_info_creation() {
        let comment = CommentInfo::new(
            "// Hello".to_string(),
            1,
            "test.rs".to_string(),
            CommentType::Line,
        );
        assert_eq!(comment.line_number, 1);
        assert!(!comment.is_docstring);
    }

    #[test]
    fn test_normalized_text() {
        let comment = CommentInfo::new(
            "  // Hello World  ".to_string(),
            1,
            "test.rs".to_string(),
            CommentType::Line,
        );
        assert_eq!(comment.normalized_text(), "// hello world");
    }

    #[test]
    fn test_docstring_is_docstring() {
        let comment = CommentInfo::new(
            "\"\"\"Docstring\"\"\"".to_string(),
            1,
            "test.py".to_string(),
            CommentType::Docstring,
        );
        assert!(comment.is_docstring);
    }
}
