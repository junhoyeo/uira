//! XML builder for comment output.

use crate::models::CommentInfo;

/// Builds a `<comments>` XML block for a given file and its comments.
/// Returns an XML formatted string with comments, or empty string if no comments provided.
pub fn build_comments_xml(comments: &[CommentInfo], file_path: &str) -> String {
    if comments.is_empty() {
        return String::new();
    }

    let mut sb = String::new();
    sb.push_str(&format!("<comments file=\"{}\">\n", file_path));

    for comment in comments {
        sb.push_str(&format!(
            "\t<comment line-number=\"{}\">{}</comment>\n",
            comment.line_number, comment.text
        ));
    }

    sb.push_str("</comments>");
    sb
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::CommentType;

    #[test]
    fn test_empty_comments() {
        let result = build_comments_xml(&[], "test.py");
        assert!(result.is_empty());
    }

    #[test]
    fn test_single_comment() {
        let comments = vec![CommentInfo::new(
            "# Test comment".to_string(),
            5,
            "test.py".to_string(),
            CommentType::Line,
        )];
        let result = build_comments_xml(&comments, "test.py");
        assert!(result.contains("<comments file=\"test.py\">"));
        assert!(result.contains("<comment line-number=\"5\"># Test comment</comment>"));
        assert!(result.contains("</comments>"));
    }

    #[test]
    fn test_multiple_comments() {
        let comments = vec![
            CommentInfo::new(
                "# First".to_string(),
                1,
                "test.py".to_string(),
                CommentType::Line,
            ),
            CommentInfo::new(
                "# Second".to_string(),
                10,
                "test.py".to_string(),
                CommentType::Line,
            ),
        ];
        let result = build_comments_xml(&comments, "test.py");
        assert!(result.contains("line-number=\"1\""));
        assert!(result.contains("line-number=\"10\""));
    }
}
