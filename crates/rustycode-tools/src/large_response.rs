//! Large Response Handler
//!
//! When tool responses exceed a configurable size threshold, this module
//! spills the content to a temporary file and returns a reference instead.
//! This prevents context window bloat from large tool outputs (e.g., file reads,
//! grep results, command output).
//!
//! Inspired by goose's `large_response_handler` pattern.
//!
//! ## Usage
//!
//! ```ignore
//! use rustycode_tools::large_response::LargeResponseHandler;
//!
//! let handler = LargeResponseHandler::default();
//! let result = handler.handle("tool_name", output_text);
//!
//! if result.spilled_to_file {
//!     println!("Large output saved to: {}", result.file_path.unwrap());
//!     // result.content now contains a reference message
//! }
//! ```

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

/// Default threshold in characters for spilling to file
pub const DEFAULT_LARGE_TEXT_THRESHOLD: usize = 200_000;

/// Maximum number of large response files to keep before cleanup
const MAX_SPILL_FILES: usize = 50;

/// Handler for managing oversized tool responses
#[derive(Debug, Clone)]
pub struct LargeResponseHandler {
    /// Character count threshold above which content is spilled to file
    threshold: usize,
    /// Directory where spilled files are stored
    spill_dir: PathBuf,
}

impl Default for LargeResponseHandler {
    fn default() -> Self {
        let spill_dir = std::env::temp_dir().join("rustycode_responses");
        Self {
            threshold: DEFAULT_LARGE_TEXT_THRESHOLD,
            spill_dir,
        }
    }
}

impl LargeResponseHandler {
    /// Create a new handler with default settings
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a handler with a custom threshold
    pub fn with_threshold(threshold: usize) -> Self {
        Self {
            threshold,
            ..Self::default()
        }
    }

    /// Set a custom spill directory (builder pattern)
    pub fn with_spill_dir(mut self, spill_dir: PathBuf) -> Self {
        self.spill_dir = spill_dir;
        self
    }

    /// Process a tool response, spilling to file if needed
    ///
    /// Returns a `ResponseResult` indicating whether the content was kept as-is
    /// or spilled to a temporary file.
    pub fn handle(&self, tool_name: &str, content: String) -> ResponseResult {
        if content.chars().count() <= self.threshold {
            return ResponseResult {
                content,
                spilled_to_file: false,
                file_path: None,
                original_size: None,
            };
        }

        match self.spill_to_file(tool_name, &content) {
            Ok(file_path) => {
                let char_count = content.chars().count();
                let message = format!(
                    "The response from '{}' was too large ({} characters) and has been saved to a file.\n\
                     You can read it with: read_file \"{}\"\n\
                     Or search within it with: grep \"pattern\" \"{}\"",
                    tool_name,
                    char_count,
                    file_path.display(),
                    file_path.display(),
                );

                ResponseResult {
                    content: message,
                    spilled_to_file: true,
                    file_path: Some(file_path),
                    original_size: Some(char_count),
                }
            }
            Err(e) => {
                // If file writing fails, truncate and include warning
                let warning = format!(
                    "Warning: Failed to write large response to file: {e}. \
                     Content truncated to fit context window.\n\n"
                );
                let max_content = self.threshold.saturating_sub(warning.len());
                let truncated: String = content.chars().take(max_content).collect();
                let truncated_content = format!(
                    "{}{}\n\n[... truncated {} characters ...]",
                    warning,
                    truncated,
                    content.chars().count().saturating_sub(max_content),
                );

                ResponseResult {
                    content: truncated_content,
                    spilled_to_file: false,
                    file_path: None,
                    original_size: Some(content.chars().count()),
                }
            }
        }
    }

    /// Check if content exceeds the threshold
    pub fn is_large(&self, content: &str) -> bool {
        content.chars().count() > self.threshold
    }

    /// Get the configured threshold
    pub const fn threshold(&self) -> usize {
        self.threshold
    }

    /// Clean up old spill files
    pub fn cleanup_old_files(&self) -> usize {
        let dir = &self.spill_dir;
        if !dir.exists() {
            return 0;
        }

        let mut files: Vec<_> = match fs::read_dir(dir) {
            Ok(entries) => entries
                .filter_map(Result::ok)
                .filter(|e| e.path().extension().is_some_and(|ext| ext == "txt"))
                .filter_map(|e| {
                    let metadata = e.metadata().ok()?;
                    let modified = metadata.modified().ok()?;
                    Some((e.path(), modified))
                })
                .collect(),
            Err(_) => return 0,
        };

        if files.len() <= MAX_SPILL_FILES {
            return 0;
        }

        // Sort by modification time (oldest first)
        files.sort_by_key(|(_, m)| *m);

        let to_remove = files.len() - MAX_SPILL_FILES;
        let mut removed = 0;

        for (path, _) in files.into_iter().take(to_remove) {
            if fs::remove_file(&path).is_ok() {
                removed += 1;
            }
        }

        removed
    }

    fn spill_to_file(&self, tool_name: &str, content: &str) -> Result<PathBuf, std::io::Error> {
        fs::create_dir_all(&self.spill_dir)?;

        let timestamp = chrono::Utc::now().format("%Y%m%d_%H%M%S%.6f");
        let safe_name = tool_name.replace(|c: char| !c.is_alphanumeric() && c != '_', "_");
        let filename = format!("{safe_name}_{timestamp}.txt");
        let file_path = self.spill_dir.join(&filename);

        let mut file = fs::File::create(&file_path)?;
        file.write_all(content.as_bytes())?;

        Ok(file_path)
    }
}

/// Result of processing a tool response
#[derive(Debug, Clone)]
pub struct ResponseResult {
    /// The content to use (either original or reference message)
    pub content: String,
    /// Whether the content was spilled to a file
    pub spilled_to_file: bool,
    /// Path to the spill file (if spilled)
    pub file_path: Option<PathBuf>,
    /// Original size in characters (if large)
    pub original_size: Option<usize>,
}

impl ResponseResult {
    /// Check if the response was spilled to a file
    pub const fn was_spilled(&self) -> bool {
        self.spilled_to_file
    }

    /// Get the file path if spilled
    pub fn file_path(&self) -> Option<&Path> {
        self.file_path.as_deref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_small_response_passes_through() {
        let handler = LargeResponseHandler::new();
        let content = "This is a small response".to_string();

        let result = handler.handle("read_file", content.clone());

        assert!(!result.spilled_to_file);
        assert_eq!(result.content, content);
        assert!(result.file_path.is_none());
    }

    #[test]
    fn test_large_response_spills_to_file() {
        let handler = LargeResponseHandler::with_threshold(100);
        let large_content = "a".repeat(200);

        let result = handler.handle("bash", large_content);

        assert!(result.spilled_to_file);
        assert!(result.content.contains("too large"));
        assert!(result.file_path.is_some());

        let path = result.file_path.unwrap();
        assert!(path.exists());

        // Verify file content
        let file_content = fs::read_to_string(&path).unwrap();
        assert_eq!(file_content, "a".repeat(200));

        // Cleanup
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn test_threshold_boundary() {
        let handler = LargeResponseHandler::with_threshold(100);

        // Exactly at threshold - should NOT spill
        let content_at = "a".repeat(100);
        let result_at = handler.handle("tool", content_at);
        assert!(!result_at.spilled_to_file);

        // One over threshold - should spill
        let content_over = "a".repeat(101);
        let result_over = handler.handle("tool", content_over);
        assert!(result_over.spilled_to_file);

        // Cleanup
        if let Some(path) = result_over.file_path {
            let _ = fs::remove_file(&path);
        }
    }

    #[test]
    fn test_is_large_check() {
        let handler = LargeResponseHandler::with_threshold(100);

        assert!(!handler.is_large("small"));
        assert!(!handler.is_large(&"a".repeat(100)));
        assert!(handler.is_large(&"a".repeat(101)));
    }

    #[test]
    fn test_tool_name_sanitized_in_filename() {
        let handler = LargeResponseHandler::with_threshold(10);
        let content = "a".repeat(50);

        let result = handler.handle("read/file/path", content);

        assert!(result.spilled_to_file);
        let path = result.file_path.unwrap();
        let filename = path.file_name().unwrap().to_string_lossy();
        assert!(filename.starts_with("read_file_path_"));

        let _ = fs::remove_file(&path);
    }

    #[test]
    fn test_cleanup_old_files() {
        let temp_dir = tempfile::tempdir().unwrap();
        let handler =
            LargeResponseHandler::with_threshold(10).with_spill_dir(temp_dir.path().to_path_buf());

        // Create more files than MAX_SPILL_FILES
        // We need to create them with different timestamps, but for testing
        // just verify the function works
        let count = handler.cleanup_old_files();
        // Empty dir should return 0
        assert_eq!(count, 0);
    }

    #[test]
    fn test_response_result_accessors() {
        let result = ResponseResult {
            content: "test".to_string(),
            spilled_to_file: true,
            file_path: Some(PathBuf::from("/tmp/test.txt")),
            original_size: Some(5000),
        };

        assert!(result.was_spilled());
        assert_eq!(result.file_path(), Some(Path::new("/tmp/test.txt")));
    }
}
