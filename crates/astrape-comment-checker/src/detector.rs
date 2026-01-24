use streaming_iterator::StreamingIterator;
use tree_sitter::{Parser, QueryCursor};

use crate::languages::{get_comment_query, LanguageRegistry};
use crate::models::{CommentInfo, CommentType};

pub struct CommentDetector {
    registry: LanguageRegistry,
}

impl CommentDetector {
    pub fn new() -> Self {
        Self {
            registry: LanguageRegistry::new(),
        }
    }

    pub fn detect(
        &self,
        content: &str,
        file_path: &str,
        include_docstrings: bool,
    ) -> Vec<CommentInfo> {
        let lang_name = match self.registry.get_language_name(file_path) {
            Some(name) => name,
            None => return Vec::new(),
        };

        let language = match self.registry.get_language(lang_name) {
            Some(lang) => lang,
            None => return Vec::new(),
        };

        let mut parser = Parser::new();
        if parser.set_language(&language).is_err() {
            return Vec::new();
        }

        let tree = match parser.parse(content, None) {
            Some(tree) => tree,
            None => return Vec::new(),
        };

        let query = match get_comment_query(lang_name, language) {
            Some(q) => q,
            None => return Vec::new(),
        };

        let mut cursor = QueryCursor::new();
        let mut matches = cursor.matches(&query, tree.root_node(), content.as_bytes());

        let mut comments = Vec::new();
        while let Some(m) = matches.next() {
            for capture in m.captures {
                let node = capture.node;
                let text = node.utf8_text(content.as_bytes()).unwrap_or("").to_string();
                let line_number = node.start_position().row + 1;
                let node_type = node.kind();

                let comment_type = self.determine_comment_type(&text, node_type);
                let is_docstring = comment_type == CommentType::Docstring;

                if is_docstring && !include_docstrings {
                    continue;
                }

                comments.push(CommentInfo::new(
                    text,
                    line_number,
                    file_path.to_string(),
                    comment_type,
                ));
            }
        }

        comments
    }

    fn determine_comment_type(&self, text: &str, node_type: &str) -> CommentType {
        let stripped = text.trim();

        if node_type == "line_comment" {
            return CommentType::Line;
        }
        if node_type == "block_comment" {
            return CommentType::Block;
        }

        if stripped.starts_with("\"\"\"") || stripped.starts_with("'''") {
            return CommentType::Docstring;
        }

        if stripped.starts_with("//") || stripped.starts_with("#") {
            return CommentType::Line;
        }

        if stripped.starts_with("/*") || stripped.starts_with("<!--") || stripped.starts_with("--")
        {
            return CommentType::Block;
        }

        CommentType::Line
    }
}

impl Default for CommentDetector {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_python_comments() {
        let detector = CommentDetector::new();
        let code = r#"
# This is a comment
x = 1  # inline comment
"#;
        let comments = detector.detect(code, "test.py", false);
        assert!(!comments.is_empty());
        assert!(comments
            .iter()
            .any(|c| c.text.contains("This is a comment")));
    }

    #[test]
    fn test_detect_rust_comments() {
        let detector = CommentDetector::new();
        let code = r#"
// Line comment
/* Block comment */
fn main() {}
"#;
        let comments = detector.detect(code, "test.rs", false);
        assert!(!comments.is_empty());
    }

    #[test]
    fn test_unsupported_language() {
        let detector = CommentDetector::new();
        let comments = detector.detect("some content", "test.unknown", false);
        assert!(comments.is_empty());
    }
}
