//! Turn-level snapshot for tracking file changes across agent turns.
//!
//! Captures git state before each agent turn and computes diffs after.
//! Enables verification ("did the agent actually change anything?"), undo
//! support, and doom-loop detection.
//!
//! Inspired by OpenCode's `snapshot.track()/patch()` pattern. Reuses
//! [`rustycode_core::ultrawork`] primitives where possible.

use std::path::Path;

// Re-export the core progress snapshot for convenience — callers can use
// either the core version (checks "did anything change?") or the richer
// TurnDiff from this module (shows *what* changed).
pub use rustycode_core::ultrawork::{
    git_head_rev, has_file_changes, has_progress, ProgressSnapshot,
};

/// Summary of what changed between a snapshot and the current state.
#[derive(Debug, Clone)]
pub struct TurnDiff {
    /// Number of files modified.
    pub modified: usize,
    /// Number of files added.
    pub added: usize,
    /// Number of files deleted.
    pub deleted: usize,
    /// Short file paths that changed (limited to 20 for display).
    pub files: Vec<String>,
}

impl TurnDiff {
    /// Returns `true` if no changes were detected.
    pub fn is_empty(&self) -> bool {
        self.modified == 0 && self.added == 0 && self.deleted == 0
    }

    /// Total number of files with any change.
    pub fn total_files(&self) -> usize {
        self.modified + self.added + self.deleted
    }

    /// Human-readable one-liner: "3 files changed (1 added, 2 modified)".
    pub fn summary(&self) -> String {
        if self.is_empty() {
            return "no changes detected".to_string();
        }

        let mut parts = Vec::new();
        if self.added > 0 {
            parts.push(format!("{} added", self.added));
        }
        if self.modified > 0 {
            parts.push(format!("{} modified", self.modified));
        }
        if self.deleted > 0 {
            parts.push(format!("{} deleted", self.deleted));
        }

        let total = self.total_files();
        format!(
            "{} file{} changed ({})",
            total,
            if total != 1 { "s" } else { "" },
            parts.join(", ")
        )
    }
}

/// Internal files that should not count as task progress.
/// Mirrors [`rustycode_core::ultrawork::INTERNAL_FILES`].
const INTERNAL_FILES: &[&str] = &[".rustycode_command_history", ".claude/"];

/// Captures git state at a point in time for later diffing.
///
/// Uses `git status --porcelain` for change detection (fast, no diff
/// computation). Falls back to file-mtime tracking when not in a git repo.
#[derive(Debug)]
pub struct TurnSnapshot {
    /// Git HEAD before the turn. `None` if not a git repo.
    head_before: Option<String>,
    /// Snapshot of file mtimes as a fallback for non-git directories.
    mtime_snapshot: MtimeMap,
}

/// Lightweight file-mtime map for non-git change detection.
#[derive(Debug, Clone)]
struct MtimeMap {
    entries: Vec<(String, std::time::SystemTime)>,
}

impl MtimeMap {
    fn take(cwd: &Path) -> Self {
        let mut entries = Vec::new();
        if let Ok(dir) = std::fs::read_dir(cwd) {
            for entry in dir.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if name.starts_with('.') || INTERNAL_FILES.iter().any(|f| name.starts_with(f)) {
                    continue;
                }
                if let Ok(meta) = entry.metadata() {
                    if let Ok(mtime) = meta.modified() {
                        entries.push((name, mtime));
                    }
                }
            }
        }
        Self { entries }
    }

    fn diff_since(&self, cwd: &Path) -> (usize, usize, usize) {
        let current = Self::take(cwd);
        let mut added = 0;
        let mut modified = 0;
        let mut deleted = 0;

        // Check for new/modified files
        for (name, mtime) in &current.entries {
            if let Some((_, old_mtime)) = self.entries.iter().find(|(n, _)| n == name) {
                if mtime != old_mtime {
                    modified += 1;
                }
            } else {
                added += 1;
            }
        }

        // Check for deleted files
        for (name, _) in &self.entries {
            if !current.entries.iter().any(|(n, _)| n == name) {
                deleted += 1;
            }
        }

        (added, modified, deleted)
    }
}

impl TurnSnapshot {
    /// Capture current state before an agent turn begins.
    pub fn take(cwd: &Path) -> Self {
        Self {
            head_before: git_head_rev(cwd),
            mtime_snapshot: MtimeMap::take(cwd),
        }
    }

    /// Compute what changed since this snapshot was taken.
    pub fn diff(&self, cwd: &Path) -> TurnDiff {
        // Try git first
        if let Some(git_diff) = self.git_diff(cwd) {
            return git_diff;
        }

        // Fallback to mtime comparison
        let (added, modified, deleted) = self.mtime_snapshot.diff_since(cwd);
        TurnDiff {
            modified,
            added,
            deleted,
            files: Vec::new(), // mtime fallback can't easily list changed files
        }
    }

    /// Quick check: did anything change at all?
    pub fn has_changes(&self, cwd: &Path) -> bool {
        if has_file_changes(cwd) {
            return true;
        }
        if let Some(ref before) = self.head_before {
            if let Some(after) = git_head_rev(cwd) {
                if before != &after {
                    return true;
                }
            }
        }
        let (added, modified, deleted) = self.mtime_snapshot.diff_since(cwd);
        added + modified + deleted > 0
    }

    /// Parse `git status --porcelain` into a TurnDiff.
    fn git_diff(&self, cwd: &Path) -> Option<TurnDiff> {
        let output = std::process::Command::new("git")
            .args(["status", "--porcelain"])
            .current_dir(cwd)
            .output()
            .ok()?;

        if !output.status.success() {
            return None;
        }

        let status = String::from_utf8_lossy(&output.stdout);
        let mut added = 0;
        let mut modified = 0;
        let mut deleted = 0;
        let mut files = Vec::new();

        for line in status.lines() {
            let path = line.trim_start_matches(|c: char| {
                c.is_whitespace() || c == '?' || c == 'A' || c == 'M' || c == 'D'
            });

            // Skip internal files
            if INTERNAL_FILES.iter().any(|f| path.starts_with(f)) {
                continue;
            }

            let line_bytes = line.as_bytes();
            let (x, _y) = (
                line_bytes.first().copied().unwrap_or(b' '),
                line_bytes.get(1).copied().unwrap_or(b' '),
            );

            match x {
                b'?' => {
                    added += 1;
                }
                b'A' => {
                    added += 1;
                }
                b'D' => {
                    deleted += 1;
                }
                b'M' | b'R' => {
                    modified += 1;
                }
                _ => {
                    modified += 1;
                }
            }

            if files.len() < 20 {
                files.push(path.to_string());
            }
        }

        Some(TurnDiff {
            modified,
            added,
            deleted,
            files,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_turn_diff_empty() {
        let diff = TurnDiff {
            modified: 0,
            added: 0,
            deleted: 0,
            files: Vec::new(),
        };
        assert!(diff.is_empty());
        assert_eq!(diff.summary(), "no changes detected");
    }

    #[test]
    fn test_turn_diff_summary_added_modified() {
        let diff = TurnDiff {
            modified: 2,
            added: 1,
            deleted: 0,
            files: vec!["a.rs".into(), "b.rs".into(), "c.rs".into()],
        };
        assert!(!diff.is_empty());
        assert_eq!(diff.total_files(), 3);
        assert_eq!(diff.summary(), "3 files changed (1 added, 2 modified)");
    }

    #[test]
    fn test_turn_diff_summary_all_types() {
        let diff = TurnDiff {
            modified: 1,
            added: 1,
            deleted: 1,
            files: vec!["a.rs".into(), "b.rs".into(), "c.rs".into()],
        };
        assert_eq!(
            diff.summary(),
            "3 files changed (1 added, 1 modified, 1 deleted)"
        );
    }

    #[test]
    fn test_turn_snapshot_take_in_git_repo() {
        let cwd = std::env::current_dir().unwrap();
        let snap = TurnSnapshot::take(&cwd);
        assert!(snap.head_before.is_some());
    }

    #[test]
    fn test_turn_snapshot_has_changes_no_panic() {
        // Just verify it doesn't panic — result depends on working tree state
        let cwd = std::env::current_dir().unwrap();
        let snap = TurnSnapshot::take(&cwd);
        let _ = snap.has_changes(&cwd);
    }

    #[test]
    fn test_turn_snapshot_diff_no_panic() {
        let cwd = std::env::current_dir().unwrap();
        let snap = TurnSnapshot::take(&cwd);
        let diff = snap.diff(&cwd);
        // Just verify the struct is populated without panic
        let _ = diff.summary();
    }

    #[test]
    fn test_mtime_map_detects_new_file() {
        let tmp = tempfile::tempdir().unwrap();
        let snap = MtimeMap::take(tmp.path());
        // Create a new file
        std::fs::write(tmp.path().join("new_file.txt"), "content").unwrap();
        let (added, modified, deleted) = snap.diff_since(tmp.path());
        assert_eq!(added, 1);
        assert_eq!(modified, 0);
        assert_eq!(deleted, 0);
    }

    #[test]
    fn test_mtime_map_detects_deletion() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("to_delete.txt"), "content").unwrap();
        let snap = MtimeMap::take(tmp.path());
        std::fs::remove_file(tmp.path().join("to_delete.txt")).unwrap();
        let (added, modified, deleted) = snap.diff_since(tmp.path());
        assert_eq!(added, 0);
        assert_eq!(modified, 0);
        assert_eq!(deleted, 1);
    }

    #[test]
    fn test_internal_files_excluded() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir(tmp.path().join(".claude")).unwrap();
        std::fs::write(tmp.path().join(".claude/config.json"), "{}").unwrap();
        let snap = MtimeMap::take(tmp.path());
        // .claude/ should be excluded
        assert!(snap.entries.is_empty());
    }
}
