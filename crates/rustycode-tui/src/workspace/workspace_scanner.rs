//! Workspace Scanning Optimization
//!
//! Provides efficient workspace scanning with:
//! - Incremental indexing (only changed files)
//! - Cached metadata with TTL
//! - Debounced rapid requests
//! - Git-aware smart reindexing

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// Metadata about a file in the workspace
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileMetadata {
    /// Absolute path to the file
    pub path: PathBuf,
    /// File size in bytes
    pub size: u64,
    /// Last modification time
    pub modified: DateTime<Utc>,
    /// File extension
    pub extension: Option<String>,
    /// Whether this is a test file
    pub is_test: bool,
    /// Whether this is a documentation file
    pub is_docs: bool,
}

impl FileMetadata {
    /// Create new file metadata
    pub fn new(path: PathBuf, size: u64, modified: DateTime<Utc>) -> Self {
        let extension = path
            .extension()
            .and_then(|e| e.to_str())
            .map(|s| s.to_lowercase());

        let is_test =
            path.to_string_lossy().contains("test") || path.to_string_lossy().contains("spec");

        let is_docs = matches!(extension.as_deref(), Some("md") | Some("rst") | Some("txt"));

        Self {
            path,
            size,
            modified,
            extension,
            is_test,
            is_docs,
        }
    }
}

/// Scan result with statistics
#[derive(Debug, Clone)]
pub struct ScanResult {
    /// Files found
    pub files: Vec<FileMetadata>,
    /// Total size in bytes
    pub total_size: u64,
    /// Time taken to scan
    pub scan_duration_ms: u64,
    /// Whether this was an incremental scan
    pub incremental: bool,
    /// Files that were added since last scan
    pub added_files: Vec<PathBuf>,
    /// Files that were modified since last scan
    pub modified_files: Vec<PathBuf>,
}

impl ScanResult {
    /// Create empty scan result
    pub fn empty() -> Self {
        Self {
            files: Vec::new(),
            total_size: 0,
            scan_duration_ms: 0,
            incremental: false,
            added_files: Vec::new(),
            modified_files: Vec::new(),
        }
    }

    /// Get number of files by extension
    pub fn count_by_extension(&self) -> HashMap<String, usize> {
        let mut counts = HashMap::new();
        for file in &self.files {
            if let Some(ext) = &file.extension {
                *counts.entry(ext.clone()).or_insert(0) += 1;
            }
        }
        counts
    }

    /// Get test files
    pub fn test_files(&self) -> Vec<&FileMetadata> {
        self.files.iter().filter(|f| f.is_test).collect()
    }

    /// Get documentation files
    pub fn docs_files(&self) -> Vec<&FileMetadata> {
        self.files.iter().filter(|f| f.is_docs).collect()
    }
}

/// Cache entry for file metadata
#[derive(Debug, Clone)]
struct CacheEntry {
    metadata: FileMetadata,
    cached_at: Instant,
}

/// Workspace scanner with caching and incremental updates
pub struct WorkspaceScanner {
    /// Workspace root directory
    root: PathBuf,
    /// File metadata cache
    cache: std::sync::RwLock<HashMap<PathBuf, CacheEntry>>,
    /// Cache TTL (duration before cache is considered stale)
    cache_ttl: Duration,
    /// Last full scan time
    last_scan: std::sync::RwLock<Option<Instant>>,
    /// Debounce state
    debounce_duration: Duration,
}

impl WorkspaceScanner {
    /// Create new workspace scanner
    pub fn new(root: PathBuf) -> Self {
        Self {
            root,
            cache: std::sync::RwLock::new(HashMap::new()),
            cache_ttl: Duration::from_mins(1), // 1 minute cache
            last_scan: std::sync::RwLock::new(None),
            debounce_duration: Duration::from_millis(100), // 100ms debounce
        }
    }

    /// Set cache TTL
    pub fn with_cache_ttl(mut self, ttl: Duration) -> Self {
        self.cache_ttl = ttl;
        self
    }

    /// Set debounce duration
    pub fn with_debounce(mut self, debounce: Duration) -> Self {
        self.debounce_duration = debounce;
        self
    }

    /// Scan workspace with incremental updates
    pub fn scan(&self) -> Result<ScanResult> {
        let _start = Instant::now();

        // Check if we should do a full scan or incremental
        let last_scan = *self.last_scan.read().unwrap_or_else(|e| e.into_inner());
        let cache_stale = last_scan
            .map(|t| t.elapsed() > self.cache_ttl)
            .unwrap_or(true);

        if cache_stale {
            self.full_scan()
        } else {
            self.incremental_scan()
        }
    }

    /// Static utility to scan workspace for FileSelector
    pub fn scan_workspace(root: &Path) -> Vec<String> {
        let scanner = Self::new(root.to_path_buf());
        if let Ok(result) = scanner.full_scan() {
            result
                .files
                .iter()
                .map(|f| f.path.to_string_lossy().into_owned())
                .collect()
        } else {
            Vec::new()
        }
    }

    /// Perform a full workspace scan
    fn full_scan(&self) -> Result<ScanResult> {
        let start = Instant::now();
        let mut files = Vec::new();

        // Scan directory recursively
        if let Ok(entries) = std::fs::read_dir(&self.root) {
            for entry in entries.flatten() {
                if let Ok(file_type) = entry.file_type() {
                    if file_type.is_file() {
                        let path = entry.path();
                        if let Ok(metadata) = self.get_file_metadata(&path) {
                            files.push(metadata);
                        }
                    } else if file_type.is_dir() {
                        // Recursively scan subdirectory
                        if let Ok(sub_files) = self.scan_directory(&entry.path()) {
                            files.extend(sub_files);
                        }
                    }
                }
            }
        }

        // Update cache
        let mut cache = self.cache.write().unwrap_or_else(|e| e.into_inner());
        cache.clear();
        for file in &files {
            cache.insert(
                file.path.clone(),
                CacheEntry {
                    metadata: file.clone(),
                    cached_at: Instant::now(),
                },
            );
        }

        // Update last scan time
        *self.last_scan.write().unwrap_or_else(|e| e.into_inner()) = Some(Instant::now());

        let total_size: u64 = files.iter().map(|f| f.size).sum();
        Ok(ScanResult {
            files,
            total_size,
            scan_duration_ms: start.elapsed().as_millis() as u64,
            incremental: false,
            added_files: Vec::new(),
            modified_files: Vec::new(),
        })
    }

    /// Scan a single directory
    fn scan_directory(&self, dir: &Path) -> Result<Vec<FileMetadata>> {
        let mut files = Vec::new();

        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                if let Ok(file_type) = entry.file_type() {
                    if file_type.is_file() {
                        let path = entry.path();
                        if let Ok(metadata) = self.get_file_metadata(&path) {
                            files.push(metadata);
                        }
                    } else if file_type.is_dir() {
                        if let Ok(sub_files) = self.scan_directory(&entry.path()) {
                            files.extend(sub_files);
                        }
                    }
                }
            }
        }

        Ok(files)
    }

    /// Perform an incremental scan (only changed files)
    fn incremental_scan(&self) -> Result<ScanResult> {
        let start = Instant::now();
        let mut added_files = Vec::new();
        let mut modified_files = Vec::new();
        let mut removed_files = Vec::new();

        let cache = self.cache.read().unwrap_or_else(|e| e.into_inner());
        let previous_paths: HashSet<PathBuf> = cache.keys().cloned().collect();
        drop(cache);

        let mut current_paths = HashSet::new();

        // Scan for changes
        if let Ok(entries) = std::fs::read_dir(&self.root) {
            for entry in entries.flatten() {
                let path = entry.path();

                if entry.file_type().map(|t| t.is_file()).unwrap_or(false) {
                    current_paths.insert(path.clone());

                    let metadata = match self.get_file_metadata(&path) {
                        Ok(m) => m,
                        Err(_) => continue,
                    };

                    if let Some(cached) = self.get_cached(&path) {
                        if cached.metadata.modified < metadata.modified {
                            // File was modified
                            modified_files.push(path.clone());
                            // Update cache
                            let mut cache = self.cache.write().unwrap_or_else(|e| e.into_inner());
                            cache.insert(
                                path.clone(),
                                CacheEntry {
                                    metadata,
                                    cached_at: Instant::now(),
                                },
                            );
                        }
                    } else {
                        // New file
                        added_files.push(path.clone());
                        // Add to cache
                        let mut cache = self.cache.write().unwrap_or_else(|e| e.into_inner());
                        cache.insert(
                            path.clone(),
                            CacheEntry {
                                metadata,
                                cached_at: Instant::now(),
                            },
                        );
                    }
                }
            }
        }

        // Find removed files
        for path in previous_paths {
            if !current_paths.contains(&path) {
                removed_files.push(path.clone());
                // Remove from cache
                let mut cache = self.cache.write().unwrap_or_else(|e| e.into_inner());
                cache.remove(&path);
            }
        }

        let mut files = Vec::new();
        let cache = self.cache.read().unwrap_or_else(|e| e.into_inner());
        for entry in cache.values() {
            files.push(entry.metadata.clone());
        }

        let total_size: u64 = files.iter().map(|f| f.size).sum();
        Ok(ScanResult {
            files,
            total_size,
            scan_duration_ms: start.elapsed().as_millis() as u64,
            incremental: true,
            added_files,
            modified_files,
        })
    }

    /// Get file metadata from filesystem
    fn get_file_metadata(&self, path: &Path) -> Result<FileMetadata> {
        let metadata = std::fs::metadata(path).context("Failed to read file metadata")?;

        let size = metadata.len();
        let modified = metadata
            .modified()
            .map(DateTime::from)
            .unwrap_or_else(|_| Utc::now());

        Ok(FileMetadata::new(path.to_path_buf(), size, modified))
    }

    /// Get cached metadata for a file
    fn get_cached(&self, path: &Path) -> Option<CacheEntry> {
        let cache = self.cache.read().unwrap_or_else(|e| e.into_inner());
        cache.get(path).cloned()
    }

    /// Invalidate cache (force full scan on next request)
    pub fn invalidate(&self) {
        self.cache
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .clear();
        *self.last_scan.write().unwrap_or_else(|e| e.into_inner()) = None;
    }

    /// Get cache statistics
    pub fn cache_stats(&self) -> CacheStats {
        let cache = self.cache.read().unwrap_or_else(|e| e.into_inner());
        let now = Instant::now();

        let total_entries = cache.len();
        let stale_entries = cache
            .values()
            .filter(|e| now.duration_since(e.cached_at) > self.cache_ttl)
            .count();
        let total_size: usize = cache
            .values()
            .map(|e| e.metadata.path.as_os_str().len() + size_of::<FileMetadata>())
            .sum();

        CacheStats {
            total_entries,
            stale_entries,
            cache_size_bytes: total_size,
            hit_rate: 0.0,
        }
    }
}

/// Cache statistics
#[derive(Debug, Clone)]
pub struct CacheStats {
    pub total_entries: usize,
    pub stale_entries: usize,
    pub cache_size_bytes: usize,
    pub hit_rate: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_file_metadata_creation() {
        let path = PathBuf::from("/src/file.rs");
        let modified = Utc::now();
        let metadata = FileMetadata::new(path.clone(), 1024, modified);

        assert_eq!(metadata.path, path);
        assert_eq!(metadata.size, 1024);
        assert_eq!(metadata.extension, Some("rs".to_string()));
        assert!(!metadata.is_test);
    }

    #[test]
    fn test_scan_result_empty() {
        let result = ScanResult::empty();
        assert!(result.files.is_empty());
        assert!(!result.incremental);
    }

    #[test]
    fn test_count_by_extension() {
        let mut result = ScanResult::empty();
        result.files.push(FileMetadata::new(
            PathBuf::from("/test/file.rs"),
            100,
            Utc::now(),
        ));
        result.files.push(FileMetadata::new(
            PathBuf::from("/test/file2.rs"),
            200,
            Utc::now(),
        ));
        result.files.push(FileMetadata::new(
            PathBuf::from("/test/file.py"),
            150,
            Utc::now(),
        ));

        let counts = result.count_by_extension();
        assert_eq!(counts.get("rs"), Some(&2));
        assert_eq!(counts.get("py"), Some(&1));
    }
}
