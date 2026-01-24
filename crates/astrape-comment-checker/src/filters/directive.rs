use super::CommentFilter;
use crate::models::CommentInfo;

const DIRECTIVE_PREFIXES: &[&str] = &[
    "type:",
    "noqa",
    "pyright:",
    "ruff:",
    "mypy:",
    "pylint:",
    "flake8:",
    "pyre:",
    "pytype:",
    "eslint-disable",
    "eslint-ignore",
    "prettier-ignore",
    "ts-ignore",
    "ts-expect-error",
    "clippy:",
    "allow",
    "deny",
    "warn",
    "forbid",
];

pub struct DirectiveFilter;

impl CommentFilter for DirectiveFilter {
    fn should_skip(&self, comment: &CommentInfo) -> bool {
        let mut normalized = comment.text.trim().to_lowercase();

        for prefix in &["#", "//", "/*", "--"] {
            if normalized.starts_with(prefix) {
                normalized = normalized[prefix.len()..].trim().to_string();
                break;
            }
        }

        if normalized.starts_with('@') {
            normalized = normalized[1..].trim().to_string();
        }

        DIRECTIVE_PREFIXES.iter().any(|d| normalized.starts_with(d))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::CommentType;

    #[test]
    fn test_directive_filter() {
        let filter = DirectiveFilter;

        let ts_ignore = CommentInfo::new(
            "// @ts-ignore".to_string(),
            1,
            "test.ts".to_string(),
            CommentType::Line,
        );
        assert!(filter.should_skip(&ts_ignore));

        let noqa = CommentInfo::new(
            "# noqa: E501".to_string(),
            1,
            "test.py".to_string(),
            CommentType::Line,
        );
        assert!(filter.should_skip(&noqa));

        let clippy = CommentInfo::new(
            "// clippy::allow".to_string(),
            1,
            "test.rs".to_string(),
            CommentType::Line,
        );
        assert!(filter.should_skip(&clippy));

        let normal = CommentInfo::new(
            "// This is a normal comment".to_string(),
            1,
            "test.js".to_string(),
            CommentType::Line,
        );
        assert!(!filter.should_skip(&normal));
    }
}
