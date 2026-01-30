//! Atomic file writing utilities
//!
//! Provides crash-safe file writes using the temp→fsync→rename pattern.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::Path;

#[cfg(unix)]
use std::os::unix::fs::OpenOptionsExt;

/// Write content atomically to a file.
///
/// Uses the pattern: write to temp file → fsync → rename
/// This ensures the file is either fully written or unchanged on crash.
///
/// # Arguments
/// * `path` - Target file path
/// * `content` - Content to write
/// * `mode` - Optional Unix file permissions (defaults to 0o644)
pub fn atomic_write(path: &Path, content: &[u8], mode: Option<u32>) -> io::Result<()> {
    let parent = path.parent().ok_or_else(|| {
        io::Error::new(io::ErrorKind::InvalidInput, "Path has no parent directory")
    })?;

    // Ensure parent directory exists
    fs::create_dir_all(parent)?;

    // Create temp file in same directory (required for atomic rename)
    let temp_path = path.with_file_name(format!(
        ".{}.tmp.{}",
        path.file_name().unwrap_or_default().to_string_lossy(),
        std::process::id()
    ));

    // Write to temp file
    {
        let mut opts = OpenOptions::new();
        opts.write(true).create(true).truncate(true);

        #[cfg(unix)]
        {
            let m = mode.unwrap_or(0o644);
            opts.mode(m);
        }

        let mut file = opts.open(&temp_path)?;
        file.write_all(content)?;
        file.sync_all()?;
    }

    // Atomic rename
    fs::rename(&temp_path, path)?;

    // Sync parent directory for durability
    #[cfg(unix)]
    {
        if let Ok(dir) = File::open(parent) {
            let _ = dir.sync_all();
        }
    }

    Ok(())
}

/// Write content atomically with secure permissions (0o600).
///
/// Convenience wrapper for writing sensitive files like credentials.
pub fn atomic_write_secure(path: &Path, content: &[u8]) -> io::Result<()> {
    atomic_write(path, content, Some(0o600))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn test_atomic_write_creates_file() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test.txt");

        atomic_write(&path, b"hello world", None).unwrap();

        assert_eq!(fs::read_to_string(&path).unwrap(), "hello world");
    }

    #[test]
    fn test_atomic_write_overwrites_existing() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test.txt");

        atomic_write(&path, b"first", None).unwrap();
        atomic_write(&path, b"second", None).unwrap();

        assert_eq!(fs::read_to_string(&path).unwrap(), "second");
    }

    #[test]
    fn test_atomic_write_creates_parent_dirs() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("nested/dir/test.txt");

        atomic_write(&path, b"nested content", None).unwrap();

        assert_eq!(fs::read_to_string(&path).unwrap(), "nested content");
    }

    #[cfg(unix)]
    #[test]
    fn test_atomic_write_secure_permissions() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempdir().unwrap();
        let path = dir.path().join("secret.txt");

        atomic_write_secure(&path, b"secret").unwrap();

        let perms = fs::metadata(&path).unwrap().permissions();
        assert_eq!(perms.mode() & 0o777, 0o600);
    }

    #[test]
    fn test_no_temp_file_left_on_success() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test.txt");

        atomic_write(&path, b"content", None).unwrap();

        // Check no temp files remain
        let entries: Vec<_> = fs::read_dir(dir.path()).unwrap().collect();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].as_ref().unwrap().file_name(), "test.txt");
    }
}
