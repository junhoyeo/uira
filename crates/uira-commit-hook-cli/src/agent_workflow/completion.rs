use regex::Regex;

pub struct CompletionDetector {
    pattern: Regex,
    summary_pattern: Regex,
}

impl CompletionDetector {
    pub fn new() -> Self {
        // Use (?s) dotall mode so .* matches newlines in multi-line summaries
        let pattern = Regex::new(r"(?s)<DONE\s*(?:/\s*>|>.*?</DONE>)").unwrap();
        let summary_pattern = Regex::new(r"(?s)<DONE\s*>(.*?)</DONE>").unwrap();
        Self {
            pattern,
            summary_pattern,
        }
    }

    pub fn is_done(&self, text: &str) -> bool {
        self.pattern.is_match(text)
    }

    pub fn extract_summary(&self, text: &str) -> Option<String> {
        self.summary_pattern
            .captures(text)
            .and_then(|cap| cap.get(1))
            .map(|m| m.as_str().trim().to_string())
            .filter(|s| !s.is_empty())
    }
}

impl Default for CompletionDetector {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_done_detection() {
        let detector = CompletionDetector::new();

        assert!(detector.is_done("All fixes applied. <DONE/>"));
        assert!(detector.is_done("Complete! <DONE />"));
        assert!(detector.is_done("<DONE>Fixed 3 typos</DONE>"));

        assert!(!detector.is_done("Working on fixes..."));
        assert!(!detector.is_done("DONE but not tagged"));
    }

    #[test]
    fn test_summary_extraction() {
        let detector = CompletionDetector::new();

        assert_eq!(
            detector.extract_summary("<DONE>Fixed 3 typos</DONE>"),
            Some("Fixed 3 typos".to_string())
        );
        // Whitespace after DONE should still extract summary
        assert_eq!(
            detector.extract_summary("<DONE >Fixed with whitespace</DONE>"),
            Some("Fixed with whitespace".to_string())
        );
        assert_eq!(detector.extract_summary("<DONE/>"), None);
        assert_eq!(detector.extract_summary("<DONE></DONE>"), None);
    }

    #[test]
    fn test_multiline_summary() {
        let detector = CompletionDetector::new();

        let multiline = "<DONE>Fixed issues:\n- Typo in foo.rs\n- Missing import in bar.rs</DONE>";
        assert!(detector.is_done(multiline));
        assert_eq!(
            detector.extract_summary(multiline),
            Some("Fixed issues:\n- Typo in foo.rs\n- Missing import in bar.rs".to_string())
        );
    }
}
