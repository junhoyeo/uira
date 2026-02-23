use std::collections::HashSet;

use uira_comment_checker::{CommentDetector, CommentInfo};

use crate::tools::builtins::hashline;

pub fn build_comment_text_set(comments: &[CommentInfo]) -> HashSet<String> {
    comments.iter().map(|c| c.normalized_text()).collect()
}

pub fn filter_new_comments(
    old_comments: &[CommentInfo],
    new_comments: Vec<CommentInfo>,
) -> Vec<CommentInfo> {
    if old_comments.is_empty() {
        return new_comments;
    }

    let old_set = build_comment_text_set(old_comments);
    new_comments
        .into_iter()
        .filter(|c| !old_set.contains(&c.normalized_text()))
        .collect()
}

pub fn parse_edit_line_content(line_text: &str) -> String {
    hashline::parse_line_content(line_text)
}

pub fn detect_comments_from_edit_lines(
    detector: &CommentDetector,
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
            let content = parse_edit_line_content(line_text);
            comments.extend(detector.detect(&content, file_path, true));
        }
    }
    comments
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_edit_line_content_preserves_indentation() {
        assert_eq!(parse_edit_line_content("1#AB |     x"), "    x");
        assert_eq!(parse_edit_line_content("1#AB|    x"), "    x");
        assert_eq!(parse_edit_line_content("value = a | b"), "value = a | b");
    }
}
