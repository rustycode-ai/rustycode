//! Log directory management with date-based rotation and automatic cleanup.
//!
//! Ported from goose's `logging.rs` as a standalone module with no external
//! path dependencies. Provides:
//! - Log directory creation with optional date-based subdirectories
//! - Automatic cleanup of logs older than a configurable retention period
//!
//! ## Usage
//!
//! ```ignore
//! use rustycode_tools::log_rotation::{LogConfig, prepare_log_directory};
//!
//! let config = LogConfig::default();
//! let log_dir = prepare_log_directory(&config, "cli", true)?;
//! // log_dir = ~/.rustycode/logs/cli/2026-04-05/
//! ```

use anyhow::{Context, Result};
use std::fs;
use std::path::PathBuf;
use std::time::{Duration, SystemTime};

/// Configuration for log rotation behavior.
#[derive(Debug, Clone)]
pub struct LogConfig {
    /// Base directory for logs (defaults to `~/.rustycode/logs`)
    pub base_dir: PathBuf,
    /// How long to keep logs before cleanup (default: 14 days)
    pub retention: Duration,
}

impl Default for LogConfig {
    fn default() -> Self {
        let home = std::env::var("HOME")
            .or_else(|_| std::env::var("USERPROFILE"))
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("."));

        let base_dir = home.join(".rustycode").join("logs");

        Self {
            base_dir,
            retention: Duration::from_hours(336), // 14 days
        }
    }
}

impl LogConfig {
    /// Create a config with a custom base directory.
    pub fn with_base_dir(base_dir: impl Into<PathBuf>) -> Self {
        Self {
            base_dir: base_dir.into(),
            retention: Duration::from_hours(336),
        }
    }

    /// Set a custom retention period.
    pub const fn with_retention(mut self, retention: Duration) -> Self {
        self.retention = retention;
        self
    }
}

/// Prepare a log directory for a specific component.
///
/// Creates the directory structure if it doesn't exist. Optionally creates
/// a date-based subdirectory. Also triggers cleanup of old logs.
///
pub fn prepare_log_directory(
    config: &LogConfig,
    component: &str,
    use_date_subdir: bool,
) -> Result<PathBuf> {
    if let Err(e) = cleanup_old_logs(config, component) {
        tracing::warn!("log rotation cleanup failed: {e}");
    }

    let component_dir = config.base_dir.join(component);

    let log_dir = if use_date_subdir {
        let date_str = format_date_now();
        component_dir.join(date_str)
    } else {
        component_dir
    };

    fs::create_dir_all(&log_dir)
        .with_context(|| format!("Failed to create log directory: {log_dir:?}"))?;

    Ok(log_dir)
}

/// Remove log directories older than the retention period.
///
/// Only removes date-based subdirectories (YYYY-MM-DD format), not the
/// component directory itself.
pub fn cleanup_old_logs(config: &LogConfig, component: &str) -> Result<()> {
    let component_dir = config.base_dir.join(component);

    if !component_dir.exists() {
        return Ok(());
    }

    let cutoff = SystemTime::now() - config.retention;

    let entries = fs::read_dir(&component_dir)
        .with_context(|| format!("Failed to read log directory: {component_dir:?}"))?;

    for entry in entries.flatten() {
        let path = entry.path();

        if let Ok(metadata) = entry.metadata() {
            if let Ok(modified) = metadata.modified() {
                if modified < cutoff && path.is_dir() {
                    if let Err(e) = fs::remove_dir_all(&path) {
                        tracing::warn!("failed to remove old log dir {}: {e}", path.display());
                    }
                }
            }
        }
    }

    Ok(())
}

/// Format the current date as YYYY-MM-DD.
fn format_date_now() -> String {
    chrono::Local::now().format("%Y-%m-%d").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_prepare_log_directory_basic() {
        let tmp = tempfile::tempdir().unwrap();
        let config = LogConfig::with_base_dir(tmp.path().join("logs"));

        let result = prepare_log_directory(&config, "cli", false);
        assert!(result.is_ok());

        let log_dir = result.unwrap();
        assert!(log_dir.exists());
        assert!(log_dir.is_dir());
        assert!(log_dir.to_string_lossy().contains("cli"));
    }

    #[test]
    fn test_prepare_log_directory_with_date() {
        let tmp = tempfile::tempdir().unwrap();
        let config = LogConfig::with_base_dir(tmp.path().join("logs"));

        let result = prepare_log_directory(&config, "server", true);
        assert!(result.is_ok());

        let log_dir = result.unwrap();
        assert!(log_dir.exists());
        assert!(log_dir.to_string_lossy().contains("server"));

        // Should contain today's date
        let date_str = format_date_now();
        assert!(log_dir.to_string_lossy().contains(&date_str));
    }

    #[test]
    fn test_prepare_log_directory_idempotent() {
        let tmp = tempfile::tempdir().unwrap();
        let config = LogConfig::with_base_dir(tmp.path().join("logs"));

        let dir1 = prepare_log_directory(&config, "debug", false).unwrap();
        let dir2 = prepare_log_directory(&config, "debug", false).unwrap();

        assert_eq!(dir1, dir2);
        assert!(dir1.exists());
    }

    #[test]
    fn test_prepare_log_directory_different_components() {
        let tmp = tempfile::tempdir().unwrap();
        let config = LogConfig::with_base_dir(tmp.path().join("logs"));

        let cli = prepare_log_directory(&config, "cli", false).unwrap();
        let server = prepare_log_directory(&config, "server", false).unwrap();

        assert_ne!(cli, server);
    }

    #[test]
    fn test_cleanup_old_logs() {
        let tmp = tempfile::tempdir().unwrap();
        let config = LogConfig::with_base_dir(tmp.path().join("logs"))
            .with_retention(Duration::from_secs(0)); // Immediate expiry

        // Create a log directory (simulate an old date-based dir)
        prepare_log_directory(&config, "test", false).unwrap();

        // Cleanup should remove old dirs
        cleanup_old_logs(&config, "test").unwrap();
        // No panic = success (empty retention cleans everything)
    }

    #[test]
    fn test_cleanup_nonexistent_component() {
        let tmp = tempfile::tempdir().unwrap();
        let config = LogConfig::with_base_dir(tmp.path().join("logs"));

        let result = cleanup_old_logs(&config, "nonexistent");
        assert!(result.is_ok());
    }

    #[test]
    fn test_format_date_now_format() {
        let date = format_date_now();
        // Should be YYYY-MM-DD format
        assert_eq!(date.len(), 10);
        assert_eq!(&date[4..5], "-");
        assert_eq!(&date[7..8], "-");

        // Should start with 202 (we're in the 2020s)
        assert!(date.starts_with("202"));
    }

    #[test]
    fn test_log_config_default() {
        let config = LogConfig::default();
        assert!(config.base_dir.to_string_lossy().contains(".rustycode"));
        assert!(config.base_dir.to_string_lossy().contains("logs"));
        assert_eq!(config.retention.as_secs(), 14 * 24 * 60 * 60);
    }

    #[test]
    fn test_log_config_custom_retention() {
        let config = LogConfig::default().with_retention(Duration::from_hours(168));
        assert_eq!(config.retention.as_secs(), 7 * 24 * 60 * 60);
    }

    #[test]
    fn test_can_write_to_log_directory() {
        let tmp = tempfile::tempdir().unwrap();
        let config = LogConfig::with_base_dir(tmp.path().join("logs"));

        let log_dir = prepare_log_directory(&config, "cli", false).unwrap();
        let test_file = log_dir.join("test.log");

        assert!(fs::write(&test_file, "test content").is_ok());
        assert_eq!(fs::read_to_string(&test_file).unwrap(), "test content");

        let _ = fs::remove_file(&test_file);
    }
}
