//! File read deduplication cache that tracks mtime and read count to prevent

use std::collections::HashMap;
use std::path::Path;
use std::time::SystemTime;

/// Cached metadata for a file read
#[derive(Debug, Clone)]
#[non_exhaustive]
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
#[non_exhaustive]
pub struct FileReadCache {
    /// Cache entries keyed by normalized absolute path (lowercase)
    entries: HashMap<String, FileReadEntry>,
    /// Maximum reads before warning
    warn_threshold: usize,
}

impl FileReadCache {
    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
            warn_threshold: 3,
        }
    }

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

    /// Returns cached entry if mtime unchanged, or None (evicts stale entries).
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

    /// Updates cache with current mtime and increments read count.
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

    /// Evicts the cache entry so the file is re-read on next access.
    pub fn invalidate(&mut self, path: &Path) {
        let key = self.normalize_key(path);
        self.entries.remove(&key);
    }

    /// Returns true if the file has been read >= warn_threshold times.
    pub fn should_warn(&self, path: &Path) -> bool {
        let key = self.normalize_key(path);
        self.entries
            .get(&key)
            .map(|e| e.read_count >= self.warn_threshold)
            .unwrap_or(false)
    }

    pub fn read_count(&self, path: &Path) -> usize {
        let key = self.normalize_key(path);
        self.entries.get(&key).map(|e| e.read_count).unwrap_or(0)
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

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
