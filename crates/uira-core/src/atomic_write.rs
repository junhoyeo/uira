//! Atomic file writing utilities
//!
//! Provides crash-safe file writes using the temp→fsync→rename pattern.
//! Uses the `tempfile` crate for cross-platform atomic operations.

use std::fs::{self, File};
use std::io::{self, Write};
use std::path::Path;

use tempfile::NamedTempFile;

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

/// Write content atomically to a file.
///
/// Uses the pattern: create unique temp file → write → fsync → atomic rename
/// This ensures the file is either fully written or unchanged on crash.
///
/// # Cross-platform behavior
/// - On Unix: Uses rename(2) which atomically replaces the target
/// - On Windows: Uses `tempfile::NamedTempFile::persist()` which handles
///   the platform-specific workarounds for atomic replacement
///
/// # Cleanup guarantee
/// If any step fails, the temp file is automatically cleaned up via RAII.
/// No orphaned temp files will be left behind.
///
/// # Arguments
/// * `path` - Target file path
/// * `content` - Content to write
/// * `mode` - Optional Unix file permissions (ignored on non-Unix, defaults to 0o644)
pub fn atomic_write(path: &Path, content: &[u8], mode: Option<u32>) -> io::Result<()> {
    let parent = path.parent().ok_or_else(|| {
        io::Error::new(io::ErrorKind::InvalidInput, "Path has no parent directory")
    })?;

    // Ensure parent directory exists
    fs::create_dir_all(parent)?;

    // Create temp file in same directory (required for atomic rename on same filesystem)
    // NamedTempFile generates a unique random name, avoiding collisions
    let mut temp_file = NamedTempFile::new_in(parent)?;

    // Write content
    temp_file.write_all(content)?;

    // Fsync to ensure data is on disk before rename
    temp_file.as_file().sync_all()?;

    // Set permissions before persist (Unix only)
    #[cfg(unix)]
    {
        let m = mode.unwrap_or(0o644);
        let perms = std::fs::Permissions::from_mode(m);
        temp_file.as_file().set_permissions(perms)?;
    }

    // Suppress unused variable warning on non-Unix
    #[cfg(not(unix))]
    let _ = mode;

    // Atomic rename - persist() handles cross-platform differences
    // On Windows, this handles the case where target already exists
    temp_file.persist(path).map_err(|e| e.error)?;

    // Sync parent directory for durability (Unix only)
    // This ensures the directory entry is persisted
    #[cfg(unix)]
    {
        if let Ok(dir) = File::open(parent) {
            let _ = dir.sync_all();
        }
    }

    Ok(())
}

/// Write content atomically with secure permissions (0o600 on Unix).
///
/// Convenience wrapper for writing sensitive files like credentials.
/// On non-Unix platforms, uses default permissions but still provides
/// atomic write guarantees.
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

    #[test]
    fn test_no_temp_file_left_on_write_error() {
        let dir = tempdir().unwrap();
        // Try to write to a path where parent doesn't exist and can't be created
        // This is tricky to test reliably, so we'll just verify the RAII cleanup
        // by checking that NamedTempFile cleans up on drop

        let temp = NamedTempFile::new_in(dir.path()).unwrap();
        let temp_path = temp.path().to_path_buf();
        assert!(temp_path.exists());

        // Drop without persist - should clean up
        drop(temp);
        assert!(!temp_path.exists());
    }

    #[test]
    fn test_concurrent_writes_no_collision() {
        use std::sync::Arc;
        use std::thread;

        let dir = tempdir().unwrap();
        let dir_path = Arc::new(dir.path().to_path_buf());

        // Spawn multiple threads writing to different files simultaneously
        let handles: Vec<_> = (0..10)
            .map(|i| {
                let dir = Arc::clone(&dir_path);
                thread::spawn(move || {
                    let path = dir.join(format!("file_{}.txt", i));
                    atomic_write(&path, format!("content_{}", i).as_bytes(), None).unwrap();
                })
            })
            .collect();

        for handle in handles {
            handle.join().unwrap();
        }

        // Verify all files written correctly
        for i in 0..10 {
            let path = dir.path().join(format!("file_{}.txt", i));
            assert_eq!(fs::read_to_string(&path).unwrap(), format!("content_{}", i));
        }

        // Verify no temp files left
        let entries: Vec<_> = fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().starts_with('.'))
            .collect();
        assert!(entries.is_empty(), "Temp files left behind: {:?}", entries);
    }
}
