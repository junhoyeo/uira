use std::borrow::Cow;
use std::collections::HashMap;
use std::path::Path;

use typos::tokens::{Identifier, Tokenizer, Word};
use typos::{Dictionary, Status};

use super::{Detector, Issue, RenderBudget, Scope, TyposConfig};

/// Dictionary that overlays user-configured extend-words on top of the built-in dictionary.
///
/// When `extend_words` has `key == value`, the word is treated as valid (ignored).
/// When `key != value`, the word is treated as a correction.
struct OverlayDictionary {
    extend_words: HashMap<String, String>,
}

impl OverlayDictionary {
    fn new(extend_words: HashMap<String, String>) -> Self {
        Self { extend_words }
    }

    fn check_word_str<'s>(&'s self, word_str: &str) -> Option<Status<'s>> {
        let lower = word_str.to_lowercase();

        // Check extend-words overlay first
        if let Some(replacement) = self.extend_words.get(&lower) {
            return if lower == *replacement {
                // key == value means "ignore this word"
                Some(Status::Valid)
            } else {
                // key != value means "correct to value"
                Some(Status::Corrections(vec![Cow::Owned(replacement.clone())]))
            };
        }

        // Fall through to built-in dictionary
        let key = unicase::UniCase::new(word_str);
        if let Some(corrections) = typos_dict::WORD.find(&key) {
            if corrections.is_empty() {
                Some(Status::Invalid)
            } else {
                Some(Status::Corrections(
                    corrections
                        .iter()
                        .map(|c| Cow::Owned(c.to_string()))
                        .collect(),
                ))
            }
        } else {
            None
        }
    }
}

impl Dictionary for OverlayDictionary {
    fn correct_ident<'s>(&'s self, _ident: Identifier<'_>) -> Option<Status<'s>> {
        // We don't do ident-level corrections; let the library split into words
        None
    }

    fn correct_word<'s>(&'s self, word: Word<'_>) -> Option<Status<'s>> {
        self.check_word_str(word.token())
    }
}

/// Native typos detector using the `typos` crate library (no subprocess).
pub struct TyposDetector {
    #[allow(dead_code)]
    config: TyposConfig,
    tokenizer: Tokenizer,
    dictionary: OverlayDictionary,
    exclude_patterns: Option<globset::GlobSet>,
}

impl TyposDetector {
    pub fn new(working_dir: &Path) -> Self {
        let config = TyposConfig::load(working_dir);
        let tokenizer = Tokenizer::new();
        let dictionary = OverlayDictionary::new(config.extend_words.clone());
        let exclude_patterns = config.compile_exclude_patterns();

        Self {
            config,
            tokenizer,
            dictionary,
            exclude_patterns,
        }
    }

    fn is_excluded(&self, path: &Path) -> bool {
        if let Some(ref patterns) = self.exclude_patterns {
            let path_str = path.to_string_lossy();
            patterns.is_match(path_str.as_ref())
        } else {
            false
        }
    }
}

fn byte_offset_to_line_col(content: &str, offset: usize) -> (usize, usize) {
    let before = &content[..offset.min(content.len())];
    let line = before.chars().filter(|&c| c == '\n').count() + 1;
    let last_newline = before.rfind('\n').map(|i| i + 1).unwrap_or(0);
    let line_slice = &content[last_newline..offset.min(content.len())];
    let col = line_slice.chars().count() + 1;
    (line, col)
}

fn extract_context_line(content: &str, offset: usize) -> Option<String> {
    if offset >= content.len() {
        return None;
    }
    let start = content[..offset].rfind('\n').map(|i| i + 1).unwrap_or(0);
    let end = content[offset..]
        .find('\n')
        .map(|i| offset + i)
        .unwrap_or(content.len());
    Some(content[start..end].to_string())
}

fn issue_id(path: &Path, byte_offset: usize, typo: &str) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let mut hasher = DefaultHasher::new();
    path.hash(&mut hasher);
    byte_offset.hash(&mut hasher);
    typo.hash(&mut hasher);
    format!("typo-{:016x}", hasher.finish())
}

impl Detector for TyposDetector {
    fn name(&self) -> &'static str {
        "typos"
    }

    fn detect(&self, scope: &Scope) -> anyhow::Result<Vec<Issue>> {
        let mut issues = Vec::new();

        for path in &scope.paths {
            if self.is_excluded(path) {
                continue;
            }

            if let Ok(relative) = path.strip_prefix(&scope.working_dir) {
                if self.is_excluded(relative) {
                    continue;
                }
            }

            let content = match std::fs::read_to_string(path) {
                Ok(c) => c,
                Err(_) => continue,
            };

            for typo in typos::check_str(&content, &self.tokenizer, &self.dictionary) {
                let (line, col) = byte_offset_to_line_col(&content, typo.byte_offset);
                let context = extract_context_line(&content, typo.byte_offset);

                let suggestions: Vec<String> = match &typo.corrections {
                    Status::Corrections(corrections) => {
                        corrections.iter().map(|c| c.to_string()).collect()
                    }
                    _ => vec![],
                };

                let message = if suggestions.is_empty() {
                    format!("`{}` is misspelled", typo.typo)
                } else {
                    format!("`{}` -> {}", typo.typo, suggestions.join(", "))
                };

                issues.push(Issue {
                    id: issue_id(path, typo.byte_offset, &typo.typo),
                    path: path.clone(),
                    line,
                    col,
                    byte_offset: typo.byte_offset,
                    message,
                    suggestions,
                    context,
                });
            }
        }

        Ok(issues)
    }

    fn render_prompt(&self, issues: &[Issue], budget: &RenderBudget) -> String {
        if issues.is_empty() {
            return String::from("No typos found.");
        }

        let mut output = String::new();
        output.push_str(&format!("Found {} typo(s):\n\n", issues.len()));

        let mut by_file: std::collections::BTreeMap<&std::path::Path, Vec<&Issue>> =
            std::collections::BTreeMap::new();
        for issue in issues.iter().take(budget.max_issues) {
            by_file.entry(issue.path.as_path()).or_default().push(issue);
        }

        for (path, file_issues) in &by_file {
            output.push_str(&format!("## {}\n", path.display()));
            for issue in file_issues {
                output.push_str(&format!(
                    "  L{}:{} {}\n",
                    issue.line, issue.col, issue.message
                ));
                if budget.include_context {
                    if let Some(ref ctx) = issue.context {
                        output.push_str(&format!("    > {}\n", ctx.trim()));
                    }
                }
            }
            output.push('\n');
        }

        if issues.len() > budget.max_issues {
            output.push_str(&format!(
                "... and {} more typo(s) not shown.\n",
                issues.len() - budget.max_issues
            ));
        }

        output
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_byte_offset_to_line_col() {
        let content = "hello\nworld\nfoo";
        assert_eq!(byte_offset_to_line_col(content, 0), (1, 1));
        assert_eq!(byte_offset_to_line_col(content, 6), (2, 1));
        assert_eq!(byte_offset_to_line_col(content, 7), (2, 2));
        assert_eq!(byte_offset_to_line_col(content, 12), (3, 1));

        // Non-ASCII: multibyte UTF-8 characters
        // "cafe" at byte 7 (after 2-byte char)
        let utf8_content = "caf\u{00E9}\ncafe";
        // Line 1: "cafe" (4 chars, 5 bytes due to e-acute)
        // Line 2: "cafe" starts at byte 6
        assert_eq!(byte_offset_to_line_col(utf8_content, 6), (2, 1));
        assert_eq!(byte_offset_to_line_col(utf8_content, 7), (2, 2));
    }

    #[test]
    fn test_extract_context_line() {
        let content = "first line\nsecond line\nthird line";
        assert_eq!(
            extract_context_line(content, 0),
            Some("first line".to_string())
        );
        assert_eq!(
            extract_context_line(content, 11),
            Some("second line".to_string())
        );
        assert_eq!(
            extract_context_line(content, 23),
            Some("third line".to_string())
        );
    }

    #[test]
    fn test_overlay_dictionary_ignore_word() {
        let mut extend_words = HashMap::new();
        extend_words.insert("ratatui".to_string(), "ratatui".to_string());
        let dict = OverlayDictionary::new(extend_words);

        let status = dict.check_word_str("ratatui");
        assert!(matches!(status, Some(Status::Valid)));
    }

    #[test]
    fn test_overlay_dictionary_correct_word() {
        let mut extend_words = HashMap::new();
        extend_words.insert("teh".to_string(), "the".to_string());
        let dict = OverlayDictionary::new(extend_words);

        let status = dict.check_word_str("teh");
        match status {
            Some(Status::Corrections(corrections)) => {
                assert_eq!(corrections.len(), 1);
                assert_eq!(corrections[0].as_ref(), "the");
            }
            _ => panic!("Expected Corrections status"),
        }
    }

    #[test]
    fn test_overlay_dictionary_builtin_fallback() {
        let dict = OverlayDictionary::new(HashMap::new());

        let status = dict.check_word_str("abandonned");
        assert!(
            matches!(status, Some(Status::Corrections(_))),
            "Expected built-in dictionary to catch 'abandonned', got {:?}",
            status
        );
    }

    #[test]
    fn test_overlay_dictionary_unknown_word() {
        let dict = OverlayDictionary::new(HashMap::new());

        let status = dict.check_word_str("hello");
        assert!(status.is_none(), "Expected None for valid word 'hello'");
    }

    #[test]
    fn test_detect_finds_typos_in_content() {
        let tokenizer = Tokenizer::new();
        let dict = OverlayDictionary::new(HashMap::new());

        let content = "The abandonned building was empty.";
        let typos: Vec<_> = typos::check_str(content, &tokenizer, &dict).collect();

        assert!(!typos.is_empty(), "Should detect 'abandonned' as a typo");
        assert_eq!(typos[0].typo.as_ref(), "abandonned");
    }

    #[test]
    fn test_detect_respects_extend_words() {
        let tokenizer = Tokenizer::new();
        let mut extend_words = HashMap::new();
        extend_words.insert("abandonned".to_string(), "abandonned".to_string());
        let dict = OverlayDictionary::new(extend_words);

        let content = "The abandonned building was empty.";
        let typos: Vec<_> = typos::check_str(content, &tokenizer, &dict).collect();

        assert!(
            typos.is_empty(),
            "Should not flag 'abandonned' when it's in extend-words as ignored"
        );
    }

    #[test]
    fn test_render_prompt_basic() {
        let issues = vec![Issue {
            id: "test-1".to_string(),
            path: std::path::PathBuf::from("src/main.rs"),
            line: 5,
            col: 10,
            byte_offset: 42,
            message: "`teh` -> the".to_string(),
            suggestions: vec!["the".to_string()],
            context: Some("let teh = 42;".to_string()),
        }];

        let budget = RenderBudget {
            max_issues: 10,
            include_context: true,
        };

        let detector = TyposDetector {
            config: TyposConfig::default(),
            tokenizer: Tokenizer::new(),
            dictionary: OverlayDictionary::new(HashMap::new()),
            exclude_patterns: None,
        };

        let prompt = detector.render_prompt(&issues, &budget);
        assert!(prompt.contains("1 typo(s)"));
        assert!(prompt.contains("src/main.rs"));
        assert!(prompt.contains("`teh` -> the"));
        assert!(prompt.contains("let teh = 42;"));
    }

    #[test]
    fn test_render_prompt_empty() {
        let detector = TyposDetector {
            config: TyposConfig::default(),
            tokenizer: Tokenizer::new(),
            dictionary: OverlayDictionary::new(HashMap::new()),
            exclude_patterns: None,
        };

        let budget = RenderBudget {
            max_issues: 10,
            include_context: false,
        };

        let prompt = detector.render_prompt(&[], &budget);
        assert_eq!(prompt, "No typos found.");
    }

    #[test]
    fn test_issue_id_deterministic() {
        let path = Path::new("src/main.rs");
        let id1 = issue_id(path, 42, "teh");
        let id2 = issue_id(path, 42, "teh");
        assert_eq!(id1, id2);

        let id3 = issue_id(path, 43, "teh");
        assert_ne!(id1, id3);
    }

    #[test]
    fn test_detect_with_tempfile() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("test.rs");
        std::fs::write(&file_path, "fn mian() {}\n").unwrap();

        let scope = Scope {
            working_dir: dir.path().to_path_buf(),
            paths: vec![file_path],
        };

        let detector = TyposDetector::new(dir.path());
        let issues = detector.detect(&scope).unwrap();

        assert!(!issues.is_empty(), "Should detect 'mian' as a typo");
        assert!(
            issues[0].message.contains("mian"),
            "Issue message should mention 'mian'"
        );
    }
}
