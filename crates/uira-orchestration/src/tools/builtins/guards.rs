//! File read guards module
#![allow(dead_code)]
//!
//! Provides utilities for safely reading files with checks for:
//! - Binary file detection
//! - File size limits
//! - Ignored paths (node_modules, .git, etc.)

use std::io;
use std::path::Path;

/// Maximum file size for safe reading (10 MB)
pub const DEFAULT_MAX_FILE_SIZE: u64 = 10 * 1024 * 1024;

/// Error type for file guard operations
#[derive(Debug, thiserror::Error)]
pub enum FileGuardError {
    #[error("file not found: {0}")]
    NotFound(String),

    #[error("file appears to be binary: {0}")]
    BinaryFile(String),

    #[error("file too large ({size} bytes > {max} bytes): {path}")]
    TooLarge { path: String, size: u64, max: u64 },

    #[error("file is in an ignored path: {0}")]
    IgnoredPath(String),

    #[error("IO error: {0}")]
    Io(#[from] io::Error),
}

/// Detects if a file is binary by checking for null bytes and non-printable characters.
///
/// Returns `Ok(true)` if the file appears to be binary, `Ok(false)` if it appears to be text.
/// Empty files are considered text (not binary).
pub fn is_binary_file(path: &Path) -> io::Result<bool> {
    let mut file = std::fs::File::open(path)?;
    let mut buffer = [0u8; 8192];
    let bytes_read = io::Read::read(&mut file, &mut buffer)?;

    // Empty files are not binary
    if bytes_read == 0 {
        return Ok(false);
    }

    let content = &buffer[..bytes_read];

    // Check for null bytes (strong indicator of binary)
    if content.contains(&0) {
        return Ok(true);
    }

    // Count non-printable characters (excluding common whitespace)
    let non_printable_count = content
        .iter()
        .filter(|&&b| {
            // Consider bytes < 0x20 as non-printable, except for tab (0x09), newline (0x0A), carriage return (0x0D)
            b < 0x20 && b != 0x09 && b != 0x0A && b != 0x0D
        })
        .count();

    // If more than 30% of bytes are non-printable, consider it binary
    let non_printable_ratio = non_printable_count as f64 / bytes_read as f64;
    Ok(non_printable_ratio > 0.3)
}

/// Checks if a path matches common ignore patterns.
///
/// Returns `true` if the path should be ignored, `false` otherwise.
pub fn is_ignored_path(path: &Path) -> bool {
    let path_str = path.to_string_lossy();

    // Check for node_modules
    if path_str.contains("/node_modules/") || path_str.starts_with("node_modules/") {
        return true;
    }

    // Check for .git
    if path_str.contains("/.git/") || path_str.starts_with(".git/") {
        return true;
    }

    // Check for Rust target directory
    if path_str.contains("/target/") || path_str.starts_with("target/") {
        return true;
    }

    // Check for Python cache
    if path_str.contains("/__pycache__/") {
        return true;
    }

    // Check for Python virtual environments
    if path_str.contains("/.venv/") || path_str.contains("/venv/") {
        return true;
    }

    // Check for Python compiled files
    if path_str.ends_with(".pyc") || path_str.ends_with(".pyo") {
        return true;
    }

    // Check for dist directories with JS files
    if path_str.contains("/dist/") && (path_str.ends_with(".js") || path_str.ends_with(".js.map")) {
        return true;
    }

    // Check for Next.js build directory
    if path_str.contains("/.next/") {
        return true;
    }

    false
}

/// Checks file guards without checking ignored paths.
///
/// Verifies:
/// - File exists
/// - File is not too large
/// - File is not binary
pub fn check_file_guards(path: &Path, max_size: u64) -> Result<(), FileGuardError> {
    let path_str = path.to_string_lossy().to_string();

    // Check if file exists
    if !path.exists() {
        return Err(FileGuardError::NotFound(path_str));
    }

    // Check file size
    let metadata = std::fs::metadata(path)?;
    if metadata.len() > max_size {
        return Err(FileGuardError::TooLarge {
            path: path_str,
            size: metadata.len(),
            max: max_size,
        });
    }

    // Check if binary
    if is_binary_file(path)? {
        return Err(FileGuardError::BinaryFile(path_str));
    }

    Ok(())
}

/// Checks file guards including ignored paths.
///
/// Verifies:
/// - File is not in an ignored path
/// - File exists
/// - File is not too large
/// - File is not binary
pub fn check_file_guards_full(path: &Path, max_size: u64) -> Result<(), FileGuardError> {
    let path_str = path.to_string_lossy().to_string();

    // Check if path is ignored
    if is_ignored_path(path) {
        return Err(FileGuardError::IgnoredPath(path_str));
    }

    // Run standard guards
    check_file_guards(path, max_size)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_is_binary_null_bytes() {
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(b"Hello\x00World").unwrap();
        file.flush().unwrap();

        let result = is_binary_file(file.path()).unwrap();
        assert!(result, "File with null bytes should be detected as binary");
    }

    #[test]
    fn test_is_binary_text_file() {
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(b"Hello, World!\nThis is a text file.")
            .unwrap();
        file.flush().unwrap();

        let result = is_binary_file(file.path()).unwrap();
        assert!(!result, "Normal text file should not be detected as binary");
    }

    #[test]
    fn test_is_binary_empty_file() {
        let mut file = NamedTempFile::new().unwrap();
        file.flush().unwrap();

        let result = is_binary_file(file.path()).unwrap();
        assert!(!result, "Empty file should not be detected as binary");
    }

    #[test]
    fn test_is_binary_high_nonprintable() {
        let mut file = NamedTempFile::new().unwrap();
        // Create content with >30% non-printable characters
        let mut content = vec![0x01u8; 100]; // 100 non-printable bytes
        content.extend_from_slice(b"text"); // 4 printable bytes
        file.write_all(&content).unwrap();
        file.flush().unwrap();

        let result = is_binary_file(file.path()).unwrap();
        assert!(
            result,
            "File with high non-printable ratio should be detected as binary"
        );
    }

    #[test]
    fn test_is_ignored_node_modules() {
        let path = Path::new("src/node_modules/foo.js");
        assert!(is_ignored_path(path), "node_modules path should be ignored");

        let path = Path::new("node_modules/package/index.js");
        assert!(
            is_ignored_path(path),
            "node_modules at root should be ignored"
        );
    }

    #[test]
    fn test_is_ignored_git() {
        let path = Path::new(".git/config");
        assert!(is_ignored_path(path), ".git path should be ignored");

        let path = Path::new("src/.git/objects");
        assert!(
            is_ignored_path(path),
            ".git in subdirectory should be ignored"
        );
    }

    #[test]
    fn test_is_ignored_target() {
        let path = Path::new("target/debug/foo");
        assert!(is_ignored_path(path), "target directory should be ignored");

        let path = Path::new("src/target/release/binary");
        assert!(
            is_ignored_path(path),
            "target in subdirectory should be ignored"
        );
    }

    #[test]
    fn test_is_ignored_normal_path() {
        let path = Path::new("src/main.rs");
        assert!(!is_ignored_path(path), "Normal path should not be ignored");

        let path = Path::new("README.md");
        assert!(!is_ignored_path(path), "README should not be ignored");
    }

    #[test]
    fn test_check_guards_valid_file() {
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(b"Valid text content").unwrap();
        file.flush().unwrap();

        let result = check_file_guards(file.path(), DEFAULT_MAX_FILE_SIZE);
        assert!(result.is_ok(), "Valid text file should pass guards");
    }

    #[test]
    fn test_check_guards_too_large() {
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(b"x").unwrap();
        file.flush().unwrap();

        let result = check_file_guards(file.path(), 0); // Set max size to 0
        assert!(
            matches!(result, Err(FileGuardError::TooLarge { .. })),
            "File over limit should fail"
        );
    }

    #[test]
    fn test_check_guards_binary() {
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(b"Binary\x00Content").unwrap();
        file.flush().unwrap();

        let result = check_file_guards(file.path(), DEFAULT_MAX_FILE_SIZE);
        assert!(
            matches!(result, Err(FileGuardError::BinaryFile(_))),
            "Binary file should fail"
        );
    }

    #[test]
    fn test_check_guards_not_found() {
        let path = Path::new("/nonexistent/path/to/file.txt");
        let result = check_file_guards(path, DEFAULT_MAX_FILE_SIZE);
        assert!(
            matches!(result, Err(FileGuardError::NotFound(_))),
            "Nonexistent file should fail"
        );
    }

    #[test]
    fn test_check_guards_full_ignored_path() {
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(b"Valid content").unwrap();
        file.flush().unwrap();

        // Create a path that looks like it's in node_modules
        let path_str = format!("node_modules/{}", file.path().display());
        let path = Path::new(&path_str);

        let result = check_file_guards_full(path, DEFAULT_MAX_FILE_SIZE);
        assert!(
            matches!(result, Err(FileGuardError::IgnoredPath(_))),
            "Ignored path should fail in full check"
        );
    }

    #[test]
    fn test_check_guards_full_valid_file() {
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(b"Valid text content").unwrap();
        file.flush().unwrap();

        let result = check_file_guards_full(file.path(), DEFAULT_MAX_FILE_SIZE);
        assert!(
            result.is_ok(),
            "Valid text file in normal path should pass full guards"
        );
    }
}
