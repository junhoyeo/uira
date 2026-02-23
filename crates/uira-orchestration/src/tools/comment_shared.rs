use std::collections::HashSet;

use uira_comment_checker::{CommentDetector, CommentInfo};

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

pub fn looks_like_hashline_prefix(value: &str) -> bool {
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

pub fn parse_edit_line_content(line_text: &str) -> String {
    if let Some((left, rhs)) = line_text.split_once(" | ") {
        if looks_like_hashline_prefix(left) {
            return rhs.to_string();
        }
    }
    if let Some((left, rhs)) = line_text.split_once('|') {
        if looks_like_hashline_prefix(left) {
            return rhs.to_string();
        }
    }
    line_text.to_string()
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
