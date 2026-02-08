use std::collections::HashMap;
use std::path::Path;

/// Configuration loaded from `_typos.toml` files.
///
/// Parses the `[default.extend-words]` and `[files.extend-exclude]` sections.
/// When `extend_words` has `key == value`, it means "ignore this word".
/// When `key != value`, it means "correct key to value".
#[derive(Debug, Clone, Default)]
pub struct TyposConfig {
    /// Words to extend (ignore or correct).
    /// Key is the word, value is the correction (or same word to ignore).
    pub extend_words: HashMap<String, String>,
    /// File patterns to exclude from typo checking.
    pub extend_exclude: Vec<String>,
}

impl TyposConfig {
    /// Load configuration from `_typos.toml` in the given working directory.
    ///
    /// If the file doesn't exist or fails to parse, returns default configuration.
    pub fn load(working_dir: &Path) -> Self {
        let config_path = working_dir.join("_typos.toml");

        match std::fs::read_to_string(&config_path) {
            Ok(content) => Self::parse(&content),
            Err(_) => Self::default(),
        }
    }

    /// Parse TOML content into TyposConfig.
    ///
    /// Handles missing sections gracefully by returning defaults.
    fn parse(content: &str) -> Self {
        match toml::from_str::<toml::Table>(content) {
            Ok(table) => {
                let extend_words = Self::extract_extend_words(&table);
                let extend_exclude = Self::extract_extend_exclude(&table);

                Self {
                    extend_words,
                    extend_exclude,
                }
            }
            Err(_) => Self::default(),
        }
    }

    /// Extract `[default.extend-words]` section as HashMap<String, String>.
    fn extract_extend_words(table: &toml::Table) -> HashMap<String, String> {
        table
            .get("default")
            .and_then(|default| default.as_table())
            .and_then(|default_table| default_table.get("extend-words"))
            .and_then(|extend_words| extend_words.as_table())
            .map(|extend_words_table| {
                extend_words_table
                    .iter()
                    .filter_map(|(key, value)| value.as_str().map(|v| (key.clone(), v.to_string())))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Extract `[files.extend-exclude]` section as Vec<String>.
    fn extract_extend_exclude(table: &toml::Table) -> Vec<String> {
        table
            .get("files")
            .and_then(|files| files.as_table())
            .and_then(|files_table| files_table.get("extend-exclude"))
            .and_then(|extend_exclude| extend_exclude.as_array())
            .map(|array| {
                array
                    .iter()
                    .filter_map(|item| item.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Compile `extend_exclude` patterns into a GlobSet for efficient matching.
    ///
    /// Returns None if patterns fail to compile.
    pub fn compile_exclude_patterns(&self) -> Option<globset::GlobSet> {
        let mut glob_set_builder = globset::GlobSetBuilder::new();

        for pattern in &self.extend_exclude {
            if let Ok(glob) = globset::Glob::new(pattern) {
                glob_set_builder.add(glob);
            }
        }

        glob_set_builder.build().ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_sample_config() {
        let content = r#"
[default.extend-words]
trivias = "trivias"
ratatui = "ratatui"
unparseable = "unparseable"
teh = "teh"

[files]
extend-exclude = ["*.lock", "target/"]
"#;

        let config = TyposConfig::parse(content);

        // Check extend_words
        assert_eq!(config.extend_words.len(), 4);
        assert_eq!(
            config.extend_words.get("trivias"),
            Some(&"trivias".to_string())
        );
        assert_eq!(
            config.extend_words.get("ratatui"),
            Some(&"ratatui".to_string())
        );
        assert_eq!(
            config.extend_words.get("unparseable"),
            Some(&"unparseable".to_string())
        );
        assert_eq!(config.extend_words.get("teh"), Some(&"teh".to_string()));

        // Check extend_exclude
        assert_eq!(config.extend_exclude.len(), 2);
        assert_eq!(config.extend_exclude[0], "*.lock");
        assert_eq!(config.extend_exclude[1], "target/");
    }

    #[test]
    fn test_parse_missing_sections() {
        let content = r#"
[some_other_section]
key = "value"
"#;

        let config = TyposConfig::parse(content);

        assert!(config.extend_words.is_empty());
        assert!(config.extend_exclude.is_empty());
    }

    #[test]
    fn test_parse_partial_config() {
        let content = r#"
[default.extend-words]
word1 = "word1"
word2 = "correction"
"#;

        let config = TyposConfig::parse(content);

        assert_eq!(config.extend_words.len(), 2);
        assert_eq!(config.extend_words.get("word1"), Some(&"word1".to_string()));
        assert_eq!(
            config.extend_words.get("word2"),
            Some(&"correction".to_string())
        );
        assert!(config.extend_exclude.is_empty());
    }

    #[test]
    fn test_parse_invalid_toml() {
        let content = r#"
[default.extend-words
invalid toml
"#;

        let config = TyposConfig::parse(content);

        // Should return defaults on parse error
        assert!(config.extend_words.is_empty());
        assert!(config.extend_exclude.is_empty());
    }

    #[test]
    fn test_default_config() {
        let config = TyposConfig::default();

        assert!(config.extend_words.is_empty());
        assert!(config.extend_exclude.is_empty());
    }

    #[test]
    fn test_compile_exclude_patterns() {
        let config = TyposConfig {
            extend_words: HashMap::new(),
            extend_exclude: vec!["*.lock".to_string(), "target/".to_string()],
        };

        let glob_set = config.compile_exclude_patterns();
        assert!(glob_set.is_some());
    }

    #[test]
    fn test_compile_exclude_patterns_empty() {
        let config = TyposConfig::default();

        let glob_set = config.compile_exclude_patterns();
        assert!(glob_set.is_some());
    }
}
