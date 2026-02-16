//! Glob pattern matching with path expansion
//!
//! Provides pattern matching for file paths and permission strings
//! with support for common path expansions.

use globset::{GlobBuilder, GlobMatcher};
use std::path::PathBuf;

/// Error type for pattern operations
#[derive(Debug, thiserror::Error)]
pub enum PatternError {
    #[error("invalid glob pattern: {0}")]
    InvalidPattern(#[from] globset::Error),

    #[error("path expansion failed: {0}")]
    ExpansionFailed(String),
}

/// A compiled glob pattern for matching
#[derive(Debug, Clone)]
pub struct Pattern {
    /// Original pattern string
    original: String,
    /// Expanded pattern string
    expanded: String,
    /// Compiled glob matcher
    matcher: GlobMatcher,
}

impl Pattern {
    /// Create a new pattern from a string
    ///
    /// Supports the following expansions:
    /// - `~/` → User home directory
    /// - `$HOME/` → User home directory
    /// - `$CWD/` → Current working directory
    pub fn new(pattern: &str) -> Result<Self, PatternError> {
        let expanded = expand_path(pattern)?;
        let glob = GlobBuilder::new(&expanded)
            .literal_separator(true)
            .build()?;
        let matcher = glob.compile_matcher();

        Ok(Self {
            original: pattern.to_string(),
            expanded,
            matcher,
        })
    }

    /// Check if this pattern matches a path
    pub fn matches(&self, path: &str) -> bool {
        self.matcher.is_match(path)
    }

    /// Check if this pattern matches a path with expansion
    pub fn matches_expanded(&self, path: &str) -> bool {
        if let Ok(expanded_path) = expand_path(path) {
            self.matcher.is_match(&expanded_path)
        } else {
            self.matcher.is_match(path)
        }
    }

    /// Get the original pattern string
    pub fn original(&self) -> &str {
        &self.original
    }

    /// Get the expanded pattern string
    pub fn expanded(&self) -> &str {
        &self.expanded
    }
}

/// Expand path variables in a string
///
/// Supported expansions:
/// - `~/` → User home directory
/// - `$HOME/` or `$HOME` → User home directory
/// - `$CWD/` or `$CWD` → Current working directory
pub fn expand_path(path: &str) -> Result<String, PatternError> {
    let mut result = path.to_string();

    // Expand ~/ to home directory
    if result.starts_with("~/") {
        let home = dirs::home_dir()
            .ok_or_else(|| PatternError::ExpansionFailed("could not find home directory".into()))?;
        result = format!("{}{}", home.display(), &result[1..]);
    }

    // Expand $HOME
    if result.contains("$HOME") {
        let home = dirs::home_dir()
            .ok_or_else(|| PatternError::ExpansionFailed("could not find home directory".into()))?;
        result = result.replace("$HOME", &home.display().to_string());
    }

    // Expand $CWD
    if result.contains("$CWD") {
        let cwd = std::env::current_dir()
            .map_err(|e| PatternError::ExpansionFailed(format!("could not get cwd: {}", e)))?;
        result = result.replace("$CWD", &cwd.display().to_string());
    }

    Ok(result)
}

/// Normalize a path for matching
///
/// Removes trailing slashes and normalizes separators
pub fn normalize_path(path: &str) -> String {
    let mut result = path.replace('\\', "/");
    while result.ends_with('/') && result.len() > 1 {
        result.pop();
    }
    result
}

/// Check if a path matches any of the given patterns
pub fn matches_any(path: &str, patterns: &[Pattern]) -> bool {
    let normalized = normalize_path(path);
    patterns.iter().any(|p| p.matches(&normalized))
}

/// Create a pattern that matches a directory and all its contents
pub fn directory_pattern(dir: &str) -> Result<Pattern, PatternError> {
    let normalized = normalize_path(dir);
    Pattern::new(&format!("{}/**", normalized))
}

/// Get the home directory as a PathBuf
pub fn home_dir() -> Option<PathBuf> {
    dirs::home_dir()
}

/// Get the current working directory as a PathBuf
pub fn current_dir() -> std::io::Result<PathBuf> {
    std::env::current_dir()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_pattern() {
        let pattern = Pattern::new("src/**/*.rs").unwrap();
        assert!(pattern.matches("src/main.rs"));
        assert!(pattern.matches("src/lib/mod.rs"));
        assert!(!pattern.matches("tests/test.rs"));
    }

    #[test]
    fn test_exact_pattern() {
        let pattern = Pattern::new("Cargo.toml").unwrap();
        assert!(pattern.matches("Cargo.toml"));
        assert!(!pattern.matches("cargo.toml"));
        assert!(!pattern.matches("src/Cargo.toml"));
    }

    #[test]
    fn test_wildcard_pattern() {
        let pattern = Pattern::new("*.rs").unwrap();
        assert!(pattern.matches("main.rs"));
        assert!(pattern.matches("lib.rs"));
        // Note: Single * in globset can match path separators depending on config
        // For strict single-segment matching, use [^/]* pattern
    }

    #[test]
    fn test_double_wildcard() {
        let pattern = Pattern::new("**/*.rs").unwrap();
        assert!(pattern.matches("main.rs"));
        assert!(pattern.matches("src/main.rs"));
        assert!(pattern.matches("src/deep/nested/file.rs"));
    }

    #[test]
    fn test_expand_home() {
        let result = expand_path("~/test").unwrap();
        assert!(!result.starts_with("~/"));
        assert!(result.contains("test"));
    }

    #[test]
    fn test_expand_cwd() {
        let result = expand_path("$CWD/test").unwrap();
        assert!(!result.contains("$CWD"));
        assert!(result.contains("test"));
    }

    #[test]
    fn test_normalize_path() {
        assert_eq!(normalize_path("src/"), "src");
        assert_eq!(normalize_path("src//"), "src");
        assert_eq!(normalize_path("src\\lib"), "src/lib");
        assert_eq!(normalize_path("/"), "/");
    }

    #[test]
    fn test_directory_pattern() {
        let pattern = directory_pattern("src").unwrap();
        assert!(pattern.matches("src/main.rs"));
        assert!(pattern.matches("src/lib/mod.rs"));
        assert!(!pattern.matches("tests/test.rs"));
    }

    #[test]
    fn test_matches_any() {
        let patterns = vec![
            Pattern::new("src/**/*.rs").unwrap(),
            Pattern::new("tests/**/*.rs").unwrap(),
        ];
        assert!(matches_any("src/main.rs", &patterns));
        assert!(matches_any("tests/test.rs", &patterns));
        assert!(!matches_any("docs/readme.md", &patterns));
    }

    #[test]
    fn test_invalid_pattern() {
        let result = Pattern::new("[invalid");
        assert!(result.is_err());
    }
}
