#![cfg_attr(
    test,
    allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::uninlined_format_args
    )
)]
//! Config file backup rotation.
//!
//! Automatically creates timestamped backups before config writes and
//! rotates them to keep at most `max_backups` copies. Includes throttling
//! to avoid excessive backups during rapid successive saves, and corruption
//! detection so broken backups don't displace good ones.

use std::fs;
use std::path::PathBuf;
use std::time::{Duration, SystemTime};
use tracing::warn;

/// Default maximum number of backups to retain per config file.
const DEFAULT_MAX_BACKUPS: usize = 5;

/// Default minimum interval between backup creations for the same file.
const DEFAULT_THROTTLE: Duration = Duration::from_mins(1);

/// Manages backup rotation for a single config file.
#[derive(Debug, Clone)]
pub struct ConfigBackup {
    /// Path to the config file being backed up.
    config_path: PathBuf,
    /// Directory where backups are stored.
    backup_dir: PathBuf,
    /// Maximum number of backups to retain.
    max_backups: usize,
    /// Minimum time between backups for the same file.
    throttle: Duration,
}

impl ConfigBackup {
    /// Create a new backup manager for the given config file.
    ///
    /// Backups are stored in a `.backup` directory alongside the config file.
    pub fn new(config_path: impl Into<PathBuf>) -> Self {
        let config_path = config_path.into();
        let backup_dir = config_path.with_extension("backup_dir");
        Self {
            config_path,
            backup_dir,
            max_backups: DEFAULT_MAX_BACKUPS,
            throttle: DEFAULT_THROTTLE,
        }
    }

    /// Set a custom max backup count.
    pub fn with_max_backups(mut self, n: usize) -> Self {
        self.max_backups = n.max(1);
        self
    }

    /// Set a custom throttle interval.
    pub const fn with_throttle(mut self, duration: Duration) -> Self {
        self.throttle = duration;
        self
    }

    /// Create a backup of the current config file if warranted.
    ///
    /// Returns `Ok(Some(backup_path))` if a backup was created, `Ok(None)` if
    /// skipped (throttled or file unchanged), or an error on failure.
    pub fn create_backup(&self) -> Result<Option<PathBuf>, BackupError> {
        if !self.config_path.exists() {
            return Ok(None);
        }

        // Ensure backup directory exists
        fs::create_dir_all(&self.backup_dir)
            .map_err(|e| BackupError::Io(self.backup_dir.clone(), e))?;

        // Throttle: skip if last backup is recent enough
        if let Some(last) = self.most_recent_backup()? {
            if let Ok(metadata) = fs::metadata(&last) {
                if let Ok(modified) = metadata.modified() {
                    if modified.elapsed().unwrap_or_default() < self.throttle {
                        return Ok(None);
                    }
                }
            }
        }

        // Check if content changed since last backup
        let current_content = fs::read_to_string(&self.config_path)
            .map_err(|e| BackupError::Io(self.config_path.clone(), e))?;

        if let Some(last) = self.most_recent_backup()? {
            if let Ok(last_content) = fs::read_to_string(&last) {
                if last_content == current_content {
                    return Ok(None); // Unchanged
                }
            }
        }

        // Create timestamped backup (with collision suffix if needed)
        let timestamp = format_timestamp(SystemTime::now());
        let stem = self
            .config_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("config");
        let ext = self
            .config_path
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("json");
        let backup_path = self.unique_backup_path(stem, &timestamp, ext);

        // Validate current file isn't corrupted (basic check: valid UTF-8 and non-empty)
        if current_content.is_empty() {
            return Err(BackupError::Corrupted(self.config_path.clone()));
        }

        let tmp_backup = backup_path.with_extension("bak.tmp");
        fs::write(&tmp_backup, &current_content)
            .map_err(|e| BackupError::Io(tmp_backup.clone(), e))?;
        fs::rename(&tmp_backup, &backup_path)
            .map_err(|e| BackupError::Io(backup_path.clone(), e))?;

        // Rotate: remove oldest backups exceeding max_backups
        self.rotate()?;

        Ok(Some(backup_path))
    }

    /// Generate a unique backup path, appending a counter if the name collides.
    fn unique_backup_path(&self, stem: &str, timestamp: &str, ext: &str) -> PathBuf {
        let base = format!("{stem}.{timestamp}.{ext}");
        let mut path = self.backup_dir.join(&base);
        if !path.exists() {
            return path;
        }
        for i in 1..100 {
            let name = format!("{stem}.{timestamp}-{i}.{ext}");
            path = self.backup_dir.join(&name);
            if !path.exists() {
                return path;
            }
        }
        // Fallback: use the base path (will overwrite)
        self.backup_dir.join(&base)
    }

    /// Rotate backups, keeping only the most recent `max_backups`.
    fn rotate(&self) -> Result<(), BackupError> {
        let mut backups = self.list_backups()?;

        if backups.len() <= self.max_backups {
            return Ok(());
        }

        // Sort newest first
        backups.sort_by_key(|b| std::cmp::Reverse(b.1));

        // Remove oldest entries beyond the limit
        for (path, _) in backups.iter().skip(self.max_backups) {
            if let Err(e) = fs::remove_file(path) {
                warn!("Failed to remove old backup {:?}: {}", path, e);
            }
        }

        Ok(())
    }

    /// List all backups sorted by timestamp (oldest first).
    fn list_backups(&self) -> Result<Vec<(PathBuf, SystemTime)>, BackupError> {
        if !self.backup_dir.exists() {
            return Ok(Vec::new());
        }

        let mut backups = Vec::new();
        let entries = fs::read_dir(&self.backup_dir)
            .map_err(|e| BackupError::Io(self.backup_dir.clone(), e))?;

        let stem = self
            .config_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("config");

        for entry in entries.flatten() {
            let path = entry.path();
            let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
            if name.starts_with(stem) {
                if let Ok(metadata) = fs::metadata(&path) {
                    if let Ok(modified) = metadata.modified() {
                        backups.push((path, modified));
                    }
                }
            }
        }

        backups.sort_by_key(|(_, t)| *t);
        Ok(backups)
    }

    /// Get the most recent backup path.
    fn most_recent_backup(&self) -> Result<Option<PathBuf>, BackupError> {
        let backups = self.list_backups()?;
        Ok(backups.into_iter().last().map(|(p, _)| p))
    }

    /// Restore the most recent backup, replacing the current config.
    ///
    /// Returns the path of the restored backup.
    pub fn restore_latest(&self) -> Result<PathBuf, BackupError> {
        let backup = self
            .most_recent_backup()?
            .ok_or_else(|| BackupError::NoBackup(self.config_path.clone()))?;

        let content =
            fs::read_to_string(&backup).map_err(|e| BackupError::Io(backup.clone(), e))?;

        // Validate backup content isn't corrupted
        if content.is_empty() {
            return Err(BackupError::Corrupted(backup));
        }

        let tmp_path = self.config_path.with_extension("json.tmp");
        fs::write(&tmp_path, &content).map_err(|e| BackupError::Io(tmp_path.clone(), e))?;
        fs::rename(&tmp_path, &self.config_path)
            .map_err(|e| BackupError::Io(self.config_path.clone(), e))?;

        Ok(backup)
    }

    /// Get the number of existing backups.
    pub fn backup_count(&self) -> usize {
        self.list_backups().map_or(0, |b| b.len())
    }
}

/// Format a `SystemTime` as a sortable timestamp string: `YYYYMMDD_HHMMSS`
fn format_timestamp(t: SystemTime) -> String {
    let duration = t.duration_since(SystemTime::UNIX_EPOCH).unwrap_or_default();
    #[allow(clippy::cast_possible_wrap)]
    let secs = duration.as_secs() as i64;
    // Simple formatting without chrono dependency
    let days = secs / 86400;
    let time_of_day = secs % 86400;
    let hours = time_of_day / 3600;
    let minutes = (time_of_day % 3600) / 60;
    let seconds = time_of_day % 60;

    // Convert days since epoch to year/month/day
    let (year, month, day) = days_to_date(days);
    format!("{year:04}{month:02}{day:02}_{hours:02}{minutes:02}{seconds:02}")
}

/// Convert days since Unix epoch to (year, month, day).
fn days_to_date(total_days: i64) -> (i64, i64, i64) {
    let mut y = 1970;
    let mut remaining = total_days;

    // Find the correct year by walking forward
    loop {
        let days_in_year = if is_leap_year(y) { 366 } else { 365 };
        if remaining < days_in_year {
            break;
        }
        remaining -= days_in_year;
        y += 1;
    }

    let leap = is_leap_year(y);
    let month_days = [
        31,
        if leap { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];

    let mut m = 0;
    for &md in &month_days {
        if remaining < md {
            break;
        }
        remaining -= md;
        m += 1;
    }

    (y, m + 1, remaining + 1)
}

const fn is_leap_year(y: i64) -> bool {
    (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
}

/// Errors that can occur during backup operations.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum BackupError {
    #[error("IO error for {0}: {1}")]
    Io(PathBuf, std::io::Error),
    #[error("File appears corrupted (empty): {0}")]
    Corrupted(PathBuf),
    #[error("No backup available for {0}")]
    NoBackup(PathBuf),
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup_temp_dir() -> tempfile::TempDir {
        tempfile::tempdir().unwrap()
    }

    #[test]
    fn format_timestamp_produces_valid_output() {
        // 2024-01-15 12:30:45 UTC
        let t = SystemTime::UNIX_EPOCH + Duration::from_secs(1_705_320_645);
        let ts = format_timestamp(t);
        // Should be a 15-char string: YYYYMMDD_HHMMSS
        assert_eq!(ts.len(), 15);
        assert!(ts.contains('_'));
    }

    #[test]
    fn days_to_date_known_values() {
        // 1970-01-01
        assert_eq!(days_to_date(0), (1970, 1, 1));
        // 2000-01-01 (10957 days)
        assert_eq!(days_to_date(10957), (2000, 1, 1));
        // 2024-01-01
        assert_eq!(days_to_date(19723), (2024, 1, 1));
    }

    #[test]
    fn creates_backup_on_first_call() {
        let tmp = setup_temp_dir();
        let config_path = tmp.path().join("config.json");
        fs::write(&config_path, r#"{"model": "opus"}"#).unwrap();

        let backup = ConfigBackup::new(&config_path).with_throttle(Duration::ZERO);

        let result = backup.create_backup().unwrap();
        assert!(result.is_some());
        assert_eq!(backup.backup_count(), 1);

        // Verify backup content matches
        let backup_path = result.unwrap();
        let content = fs::read_to_string(&backup_path).unwrap();
        assert_eq!(content, r#"{"model": "opus"}"#);
    }

    #[test]
    fn skips_backup_if_unchanged() {
        let tmp = setup_temp_dir();
        let config_path = tmp.path().join("config.json");
        fs::write(&config_path, r#"{"model": "opus"}"#).unwrap();

        let backup = ConfigBackup::new(&config_path).with_throttle(Duration::ZERO);

        backup.create_backup().unwrap();

        // Second call with same content should skip
        let result = backup.create_backup().unwrap();
        assert!(result.is_none());
        assert_eq!(backup.backup_count(), 1);
    }

    #[test]
    fn creates_new_backup_when_content_changes() {
        let tmp = setup_temp_dir();
        let config_path = tmp.path().join("config.json");

        let backup = ConfigBackup::new(&config_path).with_throttle(Duration::ZERO);

        fs::write(&config_path, "v1").unwrap();
        backup.create_backup().unwrap();

        fs::write(&config_path, "v2").unwrap();
        backup.create_backup().unwrap();

        fs::write(&config_path, "v3").unwrap();
        backup.create_backup().unwrap();

        assert_eq!(backup.backup_count(), 3);
    }

    #[test]
    fn rotates_old_backups() {
        let tmp = setup_temp_dir();
        let config_path = tmp.path().join("config.json");

        let backup = ConfigBackup::new(&config_path)
            .with_max_backups(3)
            .with_throttle(Duration::ZERO);

        for i in 0..5 {
            fs::write(&config_path, format!("version {}", i)).unwrap();
            backup.create_backup().unwrap();
            // Small sleep to ensure different timestamps
            std::thread::sleep(Duration::from_millis(10));
        }

        assert_eq!(backup.backup_count(), 3);
    }

    #[test]
    fn skips_if_config_file_missing() {
        let tmp = setup_temp_dir();
        let config_path = tmp.path().join("nonexistent.json");

        let backup = ConfigBackup::new(&config_path);
        let result = backup.create_backup().unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn rejects_empty_config_as_corrupted() {
        let tmp = setup_temp_dir();
        let config_path = tmp.path().join("config.json");
        fs::write(&config_path, "").unwrap();

        let backup = ConfigBackup::new(&config_path).with_throttle(Duration::ZERO);

        let result = backup.create_backup();
        assert!(result.is_err());
    }

    #[test]
    fn restore_latest_backup() {
        let tmp = setup_temp_dir();
        let config_path = tmp.path().join("config.json");

        let backup = ConfigBackup::new(&config_path).with_throttle(Duration::ZERO);

        fs::write(&config_path, "original").unwrap();
        backup.create_backup().unwrap();

        fs::write(&config_path, "modified").unwrap();

        let _restored = backup.restore_latest().unwrap();
        let content = fs::read_to_string(&config_path).unwrap();
        assert_eq!(content, "original");
    }

    #[test]
    fn restore_fails_when_no_backup() {
        let tmp = setup_temp_dir();
        let config_path = tmp.path().join("config.json");
        let backup = ConfigBackup::new(&config_path);
        assert!(backup.restore_latest().is_err());
    }

    #[test]
    fn throttle_skips_rapid_saves() {
        let tmp = setup_temp_dir();
        let config_path = tmp.path().join("config.json");

        let backup = ConfigBackup::new(&config_path).with_throttle(Duration::from_hours(1)); // 1 hour throttle

        fs::write(&config_path, "v1").unwrap();
        backup.create_backup().unwrap();
        assert_eq!(backup.backup_count(), 1);

        // Change content but throttle should block
        fs::write(&config_path, "v2").unwrap();
        let result = backup.create_backup().unwrap();
        assert!(result.is_none());
        assert_eq!(backup.backup_count(), 1);
    }

    #[test]
    fn restore_latest_uses_atomic_write() {
        let tmp = setup_temp_dir();
        let config_path = tmp.path().join("config.json");
        fs::write(&config_path, "original").unwrap();

        let backup = ConfigBackup::new(&config_path).with_throttle(Duration::ZERO);
        backup.create_backup().unwrap();

        // Corrupt the config
        fs::write(&config_path, "corrupted").unwrap();

        // Restore latest should work atomically
        backup.restore_latest().unwrap();
        assert_eq!(fs::read_to_string(&config_path).unwrap(), "original");

        // No tmp file should remain
        assert!(
            !tmp.path().join("config.json.tmp").exists(),
            "No tmp file should remain after restore"
        );
    }
}
