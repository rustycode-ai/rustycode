// File-based logging module for RustyCode TUI
// Redirects debug messages from screen to log files

use anyhow::{Context, Result};
use chrono::Local;
use once_cell::sync::OnceCell;
use parking_lot::Mutex;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use tracing::Level;

/// Maximum log file size before rotation (10MB)
const MAX_LOG_SIZE: u64 = 10 * 1024 * 1024;

/// Log file path
static LOG_PATH: OnceCell<PathBuf> = OnceCell::new();

/// Global log writer (thread-safe)
static LOG_WRITER: OnceCell<Mutex<Option<LogWriter>>> = OnceCell::new();

/// Log level configured from environment
static LOG_LEVEL: OnceCell<Level> = OnceCell::new();

/// Log writer with rotation support
#[derive(Debug)]
struct LogWriter {
    file: File,
    current_size: u64,
}

impl LogWriter {
    /// Create a new log writer
    fn new(path: &PathBuf) -> Result<Self> {
        // Ensure parent directory exists
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).context("Failed to create log directory")?;
        }

        // Open or create log file
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .context("Failed to open log file")?;

        // Get current file size
        let metadata = file.metadata().context("Failed to get log file metadata")?;
        let current_size = metadata.len();

        Ok(Self { file, current_size })
    }

    /// Write a log entry
    fn write(&mut self, message: &str) -> Result<()> {
        // Check if rotation is needed
        if self.current_size > MAX_LOG_SIZE {
            self.rotate()?;
        }

        let timestamp = Local::now().format("%Y-%m-%d %H:%M:%S%.3f");
        let log_entry = format!("[{}] {}\n", timestamp, message);

        self.file
            .write_all(log_entry.as_bytes())
            .context("Failed to write to log file")?;

        self.file.flush().context("Failed to flush log file")?;

        self.current_size = self.current_size.saturating_add(log_entry.len() as u64);

        Ok(())
    }

    /// Rotate log file
    fn rotate(&mut self) -> Result<()> {
        let log_path = LOG_PATH.get().context("Log path not initialized")?;

        // Create backup filename with timestamp
        let timestamp = Local::now().format("%Y%m%d_%H%M%S");
        let backup_path = log_path.with_extension(format!("log.{}", timestamp));

        // Rename current log file
        std::fs::rename(log_path, &backup_path).context("Failed to rotate log file")?;

        // Create new log file
        self.file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(log_path)
            .context("Failed to create new log file after rotation")?;

        self.current_size = 0;

        // Keep only last 5 backup files
        self.cleanup_old_logs(&backup_path)?;

        Ok(())
    }

    /// Remove old log backups, keeping only the most recent 5
    fn cleanup_old_logs(&self, current_backup: &PathBuf) -> Result<()> {
        if let Some(parent) = current_backup.parent() {
            let mut backups = Vec::new();

            // Collect all backup files
            for entry in std::fs::read_dir(parent).context("Failed to read log directory")? {
                let entry = entry.context("Failed to read directory entry")?;
                let path = entry.path();

                // Check if it's a backup log file
                if path
                    .extension()
                    .and_then(|s| s.to_str())
                    .is_some_and(|ext| ext.starts_with("log.") && path != *current_backup)
                {
                    backups.push((path, entry.metadata().ok()));
                }
            }

            // Sort by modification time (newest first)
            backups.sort_by(|a, b| match (&a.1, &b.1) {
                (Some(meta_a), Some(meta_b)) => match (meta_a.modified(), meta_b.modified()) {
                    (Ok(time_a), Ok(time_b)) => time_a.cmp(&time_b).reverse(),
                    _ => std::cmp::Ordering::Equal,
                },
                _ => std::cmp::Ordering::Equal,
            });

            // Remove old backups (keep 5 most recent)
            for (path, _) in backups.into_iter().skip(5) {
                let _ = std::fs::remove_file(path); // Ignore errors
            }
        }

        Ok(())
    }
}

/// Initialize the logging system
pub fn init() -> Result<()> {
    // Get log directory from environment or use default
    let log_dir = std::env::var("RUSTYCODE_LOG_DIR").unwrap_or_else(|_| {
        dirs::home_dir()
            .map(|h| h.join(".rustycode"))
            .unwrap_or_else(|| PathBuf::from(".rustycode"))
            .to_string_lossy()
            .to_string()
    });

    let log_path = PathBuf::from(log_dir).join("debug.log");

    // Store log path globally
    let _ = LOG_PATH.set(log_path.clone());

    // Initialize log level from environment
    let log_level = std::env::var("RUSTYCODE_LOG")
        .unwrap_or_else(|_| "info".to_string())
        .to_lowercase();

    let level = match log_level.as_str() {
        "debug" => Level::DEBUG,
        "info" => Level::INFO,
        "warn" => Level::WARN,
        "error" => Level::ERROR,
        "trace" => Level::TRACE,
        _ => Level::INFO,
    };

    let _ = LOG_LEVEL.set(level);

    // Initialize log writer
    let writer = LogWriter::new(&log_path)
        .with_context(|| format!("Failed to initialize log writer: {:?}", log_path))?;

    let _ = LOG_WRITER.set(Mutex::new(Some(writer)));

    // Initialize tracing subscriber to file
    // Use try_init() to avoid errors if subscriber is already set
    let log_path_clone = log_path.clone();
    let subscriber = tracing_subscriber::FmtSubscriber::builder()
        .with_max_level(level)
        .with_writer(std::sync::Mutex::new(
            OpenOptions::new()
                .create(true)
                .append(true)
                .open(&log_path_clone)
                .context("Failed to open log file for tracing")?,
        ))
        .finish();

    // Only set global default if not already set (allows running in tests/repl)
    let _ = tracing::subscriber::set_global_default(subscriber);

    Ok(())
}

/// Get the configured log level
pub fn log_level() -> Level {
    *LOG_LEVEL.get().unwrap_or(&Level::INFO)
}

/// Write a debug log message
pub fn debug_log(message: &str) {
    write_log(Level::DEBUG, message);
}

/// Write an info log message
pub fn info_log(message: &str) {
    write_log(Level::INFO, message);
}

/// Internal log writer
fn write_log(level: Level, message: &str) {
    if let Some(writer_guard) = LOG_WRITER.get() {
        let mut writer_opt = writer_guard.lock();

        if let Some(writer) = writer_opt.as_mut() {
            let level_str = match level {
                Level::DEBUG => "DEBUG",
                Level::INFO => "INFO",
                Level::WARN => "WARN",
                Level::ERROR => "ERROR",
                Level::TRACE => "TRACE",
            };

            let _ = writer.write(&format!("[{}] {}", level_str, message));
        }
    }
}

/// Check if debug logging is enabled
pub fn is_debug_enabled() -> bool {
    log_level() >= Level::DEBUG
}

/// Macro for convenient debug logging
#[macro_export]
macro_rules! debug_log {
    ($($arg:tt)*) => {
        if $crate::logging::is_debug_enabled() {
            $crate::logging::debug_log(&format!($($arg)*));
        }
    };
}

/// Macro for convenient info logging
#[macro_export]
macro_rules! info_log {
    ($($arg:tt)*) => {
        $crate::logging::info_log(&format!($($arg)*));
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_log_level_parsing() {
        // Test log level parsing
        let levels = vec![
            ("debug", Level::DEBUG),
            ("info", Level::INFO),
            ("warn", Level::WARN),
            ("error", Level::ERROR),
            ("DEBUG", Level::DEBUG),
            ("INFO", Level::INFO),
        ];

        for (input, expected) in levels {
            let level = match input.to_lowercase().as_str() {
                "debug" => Level::DEBUG,
                "info" => Level::INFO,
                "warn" => Level::WARN,
                "error" => Level::ERROR,
                _ => Level::INFO,
            };
            assert_eq!(level, expected);
        }
    }

    #[test]
    fn test_log_rotation_size() {
        // Verify rotation threshold
        assert_eq!(MAX_LOG_SIZE, 10 * 1024 * 1024);
    }
}
