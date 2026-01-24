use regex::Regex;

pub struct PatternMatcher {
    pattern: String,
    regex: Option<Regex>,
}

impl PatternMatcher {
    pub fn new(pattern: &str) -> Self {
        let regex = if pattern.contains('*') || pattern.contains('?') {
            let regex_pattern = pattern
                .replace('.', "\\.")
                .replace('*', ".*")
                .replace('?', ".");
            Regex::new(&format!("^{}$", regex_pattern)).ok()
        } else {
            None
        };

        Self {
            pattern: pattern.to_string(),
            regex,
        }
    }

    pub fn matches(&self, text: &str) -> bool {
        if let Some(regex) = &self.regex {
            regex.is_match(text)
        } else {
            text.contains(&self.pattern)
        }
    }

    pub fn matches_tool(&self, tool_name: &str) -> bool {
        if self.pattern == "*" {
            return true;
        }
        self.matches(tool_name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_exact_match() {
        let matcher = PatternMatcher::new("test.rs");
        assert!(matcher.matches("test.rs"));
        assert!(matcher.matches("path/to/test.rs"));
        assert!(!matcher.matches("test.ts"));
    }

    #[test]
    fn test_glob_match() {
        let matcher = PatternMatcher::new("*.rs");
        assert!(matcher.matches("test.rs"));
        assert!(matcher.matches("main.rs"));
        assert!(!matcher.matches("test.ts"));
    }

    #[test]
    fn test_wildcard_tool_match() {
        let matcher = PatternMatcher::new("*");
        assert!(matcher.matches_tool("any_tool"));
        assert!(matcher.matches_tool("bash"));
    }

    #[test]
    fn test_specific_tool_match() {
        let matcher = PatternMatcher::new("bash");
        assert!(matcher.matches_tool("bash"));
        assert!(!matcher.matches_tool("edit"));
    }

    #[test]
    fn test_pattern_with_question_mark() {
        let matcher = PatternMatcher::new("test?.rs");
        assert!(matcher.matches("test1.rs"));
        assert!(matcher.matches("testa.rs"));
        assert!(!matcher.matches("test12.rs"));
    }
}
