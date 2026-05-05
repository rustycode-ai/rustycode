//! File read state tracking for staleness detection.
//!
//! Records mtime and content hash on read; blocks writes to unread or modified files.

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::SystemTime;

use parking_lot::Mutex;

/// Why a staleness check failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StaleReason {
    /// The file has never been read in this session.
    NeverRead,
    /// The file was modified since it was last read (mtime changed).
    Modified {
        read_mtime_secs: u64,
        current_mtime_secs: u64,
    },
    /// Only a partial range of the file was read — cannot guarantee consistency.
    PartialRead,
}

impl std::fmt::Display for StaleReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NeverRead => write!(f, "file has not been read in this session — read it first before writing"),
            Self::Modified { read_mtime_secs, current_mtime_secs } => write!(
                f,
                "file was modified since last read (read mtime: {read_mtime_secs}s, current mtime: {current_mtime_secs}s) — re-read before writing"
            ),
            Self::PartialRead => write!(f, "only a partial range was read — read the full file before writing"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ReadRecord {
    pub mtime: SystemTime,
    /// First 8 hex characters of the SHA-256 content hash.
    pub hash_prefix: String,
    pub is_partial: bool,
}

/// Thread-safe map of file paths to their last-read mtime and content hash.
#[derive(Debug, Default)]
pub struct FileReadState {
    reads: Mutex<HashMap<PathBuf, ReadRecord>>,
}

impl FileReadState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record that a file was read with the given mtime and content hash.
    pub fn record_read(
        &self,
        path: PathBuf,
        mtime: SystemTime,
        hash_prefix: String,
        is_partial: bool,
    ) {
        let record = ReadRecord {
            mtime,
            hash_prefix,
            is_partial,
        };
        self.reads.lock().insert(path, record);
    }

    /// Remove the read record for a path (e.g., after a successful write).
    pub fn invalidate(&self, path: &PathBuf) {
        self.reads.lock().remove(path);
    }

    /// Check whether a file is safe to write.
    ///
    /// Returns `Ok(())` if the file was previously read and has not been
    /// modified since. Returns `Err(StaleReason)` explaining why the write
    /// should be blocked.
    ///
    /// **Special case**: if the file has no read record, we check whether it
    /// exists on disk. If it doesn't exist (new file), we allow the write
    /// without requiring a prior read.
    pub fn check_stale(
        &self,
        path: &PathBuf,
        current_mtime: Option<SystemTime>,
    ) -> Result<(), StaleReason> {
        let record = self.reads.lock().get(path).cloned();
        let Some(record) = record else {
            return Err(StaleReason::NeverRead);
        };

        if record.is_partial {
            return Err(StaleReason::PartialRead);
        }

        let Some(current) = current_mtime else {
            return Ok(());
        };

        if record.mtime != current {
            let read_secs = duration_to_secs(record.mtime);
            let current_secs = duration_to_secs(current);
            return Err(StaleReason::Modified {
                read_mtime_secs: read_secs,
                current_mtime_secs: current_secs,
            });
        }

        Ok(())
    }

    pub fn has_been_read(&self, path: &PathBuf) -> bool {
        self.reads.lock().contains_key(path)
    }
}

/// Convert a `SystemTime` to seconds since Unix epoch.
/// Returns 0 on error (before epoch).
fn duration_to_secs(t: SystemTime) -> u64 {
    t.duration_since(SystemTime::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_and_check_fresh() {
        let state = FileReadState::new();
        let path = PathBuf::from("/tmp/test.rs");
        let now = SystemTime::now();

        state.record_read(path.clone(), now, "abcd1234".into(), false);
        assert!(state.check_stale(&path, Some(now)).is_ok());
    }

    #[test]
    fn check_stale_never_read() {
        let state = FileReadState::new();
        let path = PathBuf::from("/tmp/never.rs");
        let result = state.check_stale(&path, Some(SystemTime::now()));
        assert_eq!(result.unwrap_err(), StaleReason::NeverRead);
    }

    #[test]
    fn check_stale_modified() {
        let state = FileReadState::new();
        let path = PathBuf::from("/tmp/modified.rs");
        let read_time = SystemTime::now();

        state.record_read(path.clone(), read_time, "abcd1234".into(), false);

        let later = read_time + std::time::Duration::from_secs(5);
        let result = state.check_stale(&path, Some(later));
        assert!(matches!(result.unwrap_err(), StaleReason::Modified { .. }));
    }

    #[test]
    fn check_stale_partial_read() {
        let state = FileReadState::new();
        let path = PathBuf::from("/tmp/partial.rs");
        let now = SystemTime::now();

        state.record_read(path.clone(), now, "abcd1234".into(), true);
        assert_eq!(
            state.check_stale(&path, Some(now)).unwrap_err(),
            StaleReason::PartialRead
        );
    }

    #[test]
    fn invalidate_removes_record() {
        let state = FileReadState::new();
        let path = PathBuf::from("/tmp/invalidate.rs");
        let now = SystemTime::now();

        state.record_read(path.clone(), now, "abcd1234".into(), false);
        assert!(state.has_been_read(&path));

        state.invalidate(&path);
        assert!(!state.has_been_read(&path));
        assert_eq!(
            state.check_stale(&path, Some(now)).unwrap_err(),
            StaleReason::NeverRead
        );
    }

    #[test]
    fn check_stale_file_deleted_after_read() {
        let state = FileReadState::new();
        let path = PathBuf::from("/tmp/deleted.rs");
        let now = SystemTime::now();

        state.record_read(path.clone(), now, "abcd1234".into(), false);
        assert!(state.check_stale(&path, None).is_ok());
    }

    #[test]
    fn stale_reason_display() {
        assert!(StaleReason::NeverRead.to_string().contains("not been read"));
        assert!(StaleReason::PartialRead
            .to_string()
            .contains("partial range"));
        let modified = StaleReason::Modified {
            read_mtime_secs: 100,
            current_mtime_secs: 200,
        };
        let msg = modified.to_string();
        assert!(msg.contains("100") && msg.contains("200"));
    }
}
