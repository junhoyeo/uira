use super::CommentFilter;
use crate::models::CommentInfo;

const BDD_KEYWORDS: &[&str] = &[
    "given",
    "when",
    "then",
    "arrange",
    "act",
    "assert",
    "when & then",
    "when&then",
];

pub struct BddFilter;

impl CommentFilter for BddFilter {
    fn should_skip(&self, comment: &CommentInfo) -> bool {
        let mut normalized = comment.text.trim().to_lowercase();

        for prefix in &["#", "//", "--"] {
            if normalized.starts_with(prefix) {
                normalized = normalized[prefix.len()..].trim().to_string();
                break;
            }
        }

        BDD_KEYWORDS.contains(&normalized.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::CommentType;

    #[test]
    fn test_bdd_keywords() {
        let filter = BddFilter;

        let given = CommentInfo::new(
            "# given".to_string(),
            1,
            "test.py".to_string(),
            CommentType::Line,
        );
        assert!(filter.should_skip(&given));

        let when = CommentInfo::new(
            "// when".to_string(),
            1,
            "test.js".to_string(),
            CommentType::Line,
        );
        assert!(filter.should_skip(&when));

        let normal = CommentInfo::new(
            "// This is a normal comment".to_string(),
            1,
            "test.js".to_string(),
            CommentType::Line,
        );
        assert!(!filter.should_skip(&normal));
    }
}
