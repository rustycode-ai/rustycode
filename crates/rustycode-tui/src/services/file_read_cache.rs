//! File read deduplication cache
//!
//! Prevents repeated reads of the same file by the AI model, which wastes
//! API tokens. The cache stores only metadata (read count, modification time)
//! to keep memory usage minimal. On cache hits, files are re-read from disk
//! to ensure fresh content.
//!
//! # Cache Behavior
//!
//! - **First read**: Cache metadata (mtime, readCount)
//! - **Cache hit**: Compare mtime, if unchanged increment readCount
//! - **mtime mismatch**: File was modified externally, evict and re-read
//! - **3+ reads**: Return warning to discourage repeated reads
//! - **Write operations**: Invalidate cache entry for modified file
//!
//! # Example
//!
//! ```rust,ignore
//! use rustycode_tui::file_read_cache::FileReadCache;
//! use std::path::Path;
//!
//! let mut cache = FileReadCache::new();
//!
//! // Check before reading
//! let path = Path::new("/path/to/file.txt");
//! if let Some(entry) = cache.check(path) {
//!     if entry.read_count >= 3 {
//!         // Warn model about repeated reads
//!     }
//! }
//!
//! // After reading successfully, update cache
//! cache.record_read(path, 1234567890);
//!
//! // On write, invalidate
//! cache.invalidate(path);
//! ```

use std::collections::HashMap;
use std::path::Path;
use std::time::SystemTime;

/// Cached metadata for a file read
#[derive(Debug, Clone)]
pub struct FileReadEntry {
    /// Number of times this file has been read
    pub read_count: usize,
    /// Last modification time when cached (for external change detection)
    pub mtime_ms: u64,
    /// Whether this file contained image content
    pub has_image_content: bool,
}

/// File read deduplication cache
///
/// Tracks files read during a session to prevent repeated reads
/// of unchanged files, which wastes API tokens.
#[derive(Debug, Clone, Default)]
pub struct FileReadCache {
    /// Cache entries keyed by normalized absolute path (lowercase)
    entries: HashMap<String, FileReadEntry>,
    /// Maximum reads before warning
    warn_threshold: usize,
}

impl FileReadCache {
    /// Create a new file read cache
    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
            warn_threshold: 3,
        }
    }

    /// Set the warning threshold for repeated reads
    pub fn with_warn_threshold(mut self, threshold: usize) -> Self {
        self.warn_threshold = threshold;
        self
    }

    /// Normalize a path for cache key lookup
    ///
    /// Converts to absolute path if relative, then lowercases for
    /// case-insensitive comparison on Windows and consistency.
    fn normalize_key(&self, path: &Path) -> String {
        // Try to canonicalize for absolute path
        let normalized = if let Ok(canonical) = path.canonicalize() {
            canonical
        } else {
            path.to_path_buf()
        };

        // Use string representation, lowercase for consistency
        normalized.to_string_lossy().to_lowercase().to_string()
    }

    /// Check if a file has been cached
    ///
    /// Returns `None` if not cached, or the cached entry if found.
    /// The entry will be validated against current file mtime.
    pub fn check(&mut self, path: &Path) -> Option<FileReadEntry> {
        let key = self.normalize_key(path);
        let cached = self.entries.get(&key).cloned()?;

        // Verify mtime hasn't changed (external file modification)
        if let Ok(mtime) = get_file_mtime_ms(path) {
            if mtime != cached.mtime_ms {
                // File was modified externally - evict cache
                self.entries.remove(&key);
                return None;
            }
        }

        Some(cached)
    }

    /// Record a successful file read
    ///
    /// Updates the cache entry with the current mtime and increments
    /// the read count. Should be called after successfully reading a file.
    pub fn record_read(&mut self, path: &Path, mtime_ms: u64, has_image_content: bool) {
        let key = self.normalize_key(path);

        let entry = self.entries.entry(key).or_insert_with(|| FileReadEntry {
            read_count: 0,
            mtime_ms,
            has_image_content,
        });

        // Update mtime (might have changed between reads) and increment
        entry.mtime_ms = mtime_ms;
        entry.read_count += 1;
        entry.has_image_content = has_image_content;
    }

    /// Invalidate a cache entry
    ///
    /// Should be called when a file is modified (write, edit, patch)
    /// to force a fresh read on next access.
    pub fn invalidate(&mut self, path: &Path) {
        let key = self.normalize_key(path);
        self.entries.remove(&key);
    }

    /// Check if a read should trigger a warning
    ///
    /// Returns true if the file has been read >= warn_threshold times.
    pub fn should_warn(&self, path: &Path) -> bool {
        let key = self.normalize_key(path);
        self.entries
            .get(&key)
            .map(|e| e.read_count >= self.warn_threshold)
            .unwrap_or(false)
    }

    /// Get the read count for a file
    pub fn read_count(&self, path: &Path) -> usize {
        let key = self.normalize_key(path);
        self.entries.get(&key).map(|e| e.read_count).unwrap_or(0)
    }

    /// Clear all cached entries
    pub fn clear(&mut self) {
        self.entries.clear();
    }

    /// Get the number of cached files
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Check if cache is empty
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// Get file modification time in milliseconds since Unix epoch
fn get_file_mtime_ms(path: &Path) -> Result<u64, std::io::Error> {
    use std::fs;

    let metadata = fs::metadata(path)?;
    let mtime = metadata.modified()?;
    let duration = mtime
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default();

    Ok(duration.as_millis() as u64)
}

/// Format a warning message for repeated file reads
pub fn format_repeated_read_warning(path: &Path, read_count: usize) -> String {
    format!(
        "[DUPLICATE READ] You have already read '{}' {} times in this conversation. \
         The content has not changed since your last read. \
         Please use the information you already have and proceed with your task.",
        path.display(),
        read_count
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::Write;
    use tempfile::TempDir;

    #[test]
    fn test_cache_miss_initially() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        File::create(&file_path)
            .unwrap()
            .write_all(b"hello")
            .unwrap();

        let mut cache = FileReadCache::new();
        assert!(cache.check(&file_path).is_none());
        assert_eq!(cache.read_count(&file_path), 0);
    }

    #[test]
    fn test_cache_hit_after_record() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        File::create(&file_path)
            .unwrap()
            .write_all(b"hello")
            .unwrap();

        let mut cache = FileReadCache::new();

        // First read - not cached
        assert!(cache.check(&file_path).is_none());

        // Record the read
        let mtime = get_file_mtime_ms(&file_path).unwrap();
        cache.record_read(&file_path, mtime, false);

        // Second read - cached
        let entry = cache.check(&file_path);
        assert!(entry.is_some());
        assert_eq!(entry.unwrap().read_count, 1);
    }

    #[test]
    fn test_invalidate() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        File::create(&file_path)
            .unwrap()
            .write_all(b"hello")
            .unwrap();

        let mut cache = FileReadCache::new();
        let mtime = get_file_mtime_ms(&file_path).unwrap();
        cache.record_read(&file_path, mtime, false);

        assert!(cache.check(&file_path).is_some());

        // Invalidate
        cache.invalidate(&file_path);
        assert!(cache.check(&file_path).is_none());
    }

    #[test]
    fn test_read_count_increments() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        File::create(&file_path)
            .unwrap()
            .write_all(b"hello")
            .unwrap();

        let mut cache = FileReadCache::new();
        let mtime = get_file_mtime_ms(&file_path).unwrap();

        cache.record_read(&file_path, mtime, false);
        assert_eq!(cache.read_count(&file_path), 1);

        // Check then record increments
        cache.check(&file_path);
        cache.record_read(&file_path, mtime, false);
        assert_eq!(cache.read_count(&file_path), 2);

        cache.record_read(&file_path, mtime, false);
        assert_eq!(cache.read_count(&file_path), 3);
    }

    #[test]
    fn test_warn_threshold() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        File::create(&file_path)
            .unwrap()
            .write_all(b"hello")
            .unwrap();

        let mut cache = FileReadCache::new();
        let mtime = get_file_mtime_ms(&file_path).unwrap();

        // Below threshold
        cache.record_read(&file_path, mtime, false);
        assert!(!cache.should_warn(&file_path));

        cache.record_read(&file_path, mtime, false);
        assert!(!cache.should_warn(&file_path));

        // At threshold
        cache.record_read(&file_path, mtime, false);
        assert!(cache.should_warn(&file_path));
    }

    #[test]
    fn test_mtime_invalidation() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        File::create(&file_path)
            .unwrap()
            .write_all(b"hello")
            .unwrap();

        let mut cache = FileReadCache::new();
        let mtime = get_file_mtime_ms(&file_path).unwrap();
        cache.record_read(&file_path, mtime, false);

        // Cache hit
        assert!(cache.check(&file_path).is_some());

        // Wait and modify file
        std::thread::sleep(std::time::Duration::from_millis(10));
        File::create(&file_path)
            .unwrap()
            .write_all(b"hello world")
            .unwrap();

        // Cache miss due to mtime change
        assert!(cache.check(&file_path).is_none());
    }

    #[test]
    fn test_clear() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        File::create(&file_path)
            .unwrap()
            .write_all(b"hello")
            .unwrap();

        let mut cache = FileReadCache::new();
        let mtime = get_file_mtime_ms(&file_path).unwrap();
        cache.record_read(&file_path, mtime, false);

        assert!(!cache.is_empty());
        assert_eq!(cache.len(), 1);

        cache.clear();
        assert!(cache.is_empty());
        assert_eq!(cache.len(), 0);
    }
}
