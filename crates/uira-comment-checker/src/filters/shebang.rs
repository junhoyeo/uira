use super::CommentFilter;
use crate::models::CommentInfo;

pub struct ShebangFilter;

impl CommentFilter for ShebangFilter {
    fn should_skip(&self, comment: &CommentInfo) -> bool {
        comment.text.trim().starts_with("#!")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::CommentType;

    #[test]
    fn test_shebang_filter() {
        let filter = ShebangFilter;

        let shebang = CommentInfo::new(
            "#!/usr/bin/env python3".to_string(),
            1,
            "test.py".to_string(),
            CommentType::Line,
        );
        assert!(filter.should_skip(&shebang));

        let normal = CommentInfo::new(
            "# This is a normal comment".to_string(),
            2,
            "test.py".to_string(),
            CommentType::Line,
        );
        assert!(!filter.should_skip(&normal));
    }
}
