//! File Snapshot & Undo System
//!
//! Tracks file changes per tool execution, enabling undo of individual operations.
//! Maintains a stack of file states that can be rolled back.
//!
//! Inspired by forgecode's `fs_undo` and snapshot repository.

use crate::create_file_symlink_safe;
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::{Path, PathBuf};

/// A snapshot of a single file's state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileSnapshot {
    /// File path
    pub path: PathBuf,
    /// File content at snapshot time (None if file didn't exist)
    pub content: Option<String>,
    /// Timestamp of snapshot
    pub timestamp_secs: u64,
    /// Hash of content for quick comparison
    pub content_hash: Option<u64>,
}

/// A group of snapshots taken before a tool execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotGroup {
    /// Unique ID for this group
    pub id: String,
    /// Tool name that triggered the snapshots
    pub tool_name: String,
    /// Snapshots of all files that were modified
    pub snapshots: Vec<FileSnapshot>,
    /// Whether this group has been undone
    pub undone: bool,
}

/// Manages file snapshots and undo operations
#[derive(Debug, Default)]
pub struct FileSnapshotManager {
    /// Stack of snapshot groups (most recent first)
    groups: Vec<SnapshotGroup>,
    /// Maximum number of groups to retain
    max_groups: usize,
    /// Counter for generating IDs
    next_id: u64,
}

impl FileSnapshotManager {
    pub const fn new(max_groups: usize) -> Self {
        Self {
            groups: Vec::new(),
            max_groups,
            next_id: 0,
        }
    }

    /// Create a new snapshot group for an upcoming tool execution
    pub fn create_group(&mut self, tool_name: &str) -> String {
        self.next_id += 1;
        let id = format!("snap_{:06}", self.next_id);

        let group = SnapshotGroup {
            id: id.clone(),
            tool_name: tool_name.to_string(),
            snapshots: Vec::new(),
            undone: false,
        };

        self.groups.push(group);
        self.trim_to_max();

        id
    }

    /// Snapshot a file before modification
    pub fn snapshot_file(&mut self, group_id: &str, path: &Path) -> anyhow::Result<()> {
        let group = self
            .groups
            .iter_mut()
            .find(|g| g.id == group_id)
            .ok_or_else(|| anyhow::anyhow!("Snapshot group '{group_id}' not found"))?;

        // Don't snapshot the same file twice in a group
        if group.snapshots.iter().any(|s| s.path == path) {
            return Ok(());
        }

        let content = std::fs::read_to_string(path).ok();
        let content_hash = content.as_ref().map(|c| {
            use std::collections::hash_map::DefaultHasher;
            use std::hash::{Hash, Hasher};
            let mut hasher = DefaultHasher::new();
            c.hash(&mut hasher);
            hasher.finish()
        });

        group.snapshots.push(FileSnapshot {
            path: path.to_path_buf(),
            content,
            content_hash,
            timestamp_secs: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
        });

        Ok(())
    }

    /// Undo the most recent snapshot group
    pub fn undo_last(&mut self) -> Option<UndoResult> {
        let (group_id, tool_name, snapshots) = {
            let group = self.groups.iter_mut().rev().find(|g| !g.undone)?;
            group.undone = true;
            (
                group.id.clone(),
                group.tool_name.clone(),
                group.snapshots.clone(),
            )
        };

        let (restored, failed) = restore_snapshots_impl(&snapshots);

        Some(UndoResult {
            group_id,
            tool_name,
            restored,
            failed,
        })
    }

    /// Undo a specific snapshot group by ID
    pub fn undo_group(&mut self, group_id: &str) -> Option<UndoResult> {
        let (id, tool_name, snapshots) = {
            let group = self.groups.iter_mut().find(|g| g.id == group_id)?;
            if group.undone {
                return None;
            }
            group.undone = true;
            (
                group.id.clone(),
                group.tool_name.clone(),
                group.snapshots.clone(),
            )
        };

        let (restored, failed) = restore_snapshots_impl(&snapshots);

        Some(UndoResult {
            group_id: id,
            tool_name,
            restored,
            failed,
        })
    }

    /// List all snapshot groups
    pub fn list_groups(&self) -> Vec<&SnapshotGroup> {
        self.groups.iter().collect()
    }

    /// Get the most recent undoable group
    pub fn last_undoable(&self) -> Option<&SnapshotGroup> {
        self.groups.iter().rev().find(|g| !g.undone)
    }

    /// Get count of undoable groups
    pub fn undo_count(&self) -> usize {
        self.groups.iter().filter(|g| !g.undone).count()
    }

    fn trim_to_max(&mut self) {
        while self.groups.len() > self.max_groups {
            // Remove oldest non-undone group, or oldest overall
            if let Some(pos) = self.groups.iter().position(|g| g.undone) {
                self.groups.remove(pos);
            } else {
                let excess = self.groups.len() - self.max_groups;
                self.groups.drain(0..excess);
                break;
            }
        }
    }
}

/// Helper function to restore snapshots (standalone to avoid borrow checker issues)
fn restore_snapshots_impl(snapshots: &[FileSnapshot]) -> (Vec<PathBuf>, Vec<(PathBuf, String)>) {
    let mut restored = Vec::new();
    let mut failed = Vec::new();

    for snapshot in snapshots.iter().rev() {
        match restore_single_snapshot(snapshot) {
            Ok(()) => restored.push(snapshot.path.clone()),
            Err(e) => failed.push((snapshot.path.clone(), e.to_string())),
        }
    }

    (restored, failed)
}

/// Restore a single snapshot
fn restore_single_snapshot(snapshot: &FileSnapshot) -> anyhow::Result<()> {
    if let Some(content) = &snapshot.content {
        // Restore file to previous state using symlink-safe write
        let mut file = create_file_symlink_safe(&snapshot.path)
            .map_err(|e| anyhow::anyhow!("Failed to create file for restore: {e}"))?;
        file.write_all(content.as_bytes())?;
        file.sync_all()?;
    } else {
        // File didn't exist before - delete it
        if snapshot.path.exists() {
            std::fs::remove_file(&snapshot.path)?;
        }
    }
    Ok(())
}

/// Result of an undo operation
#[derive(Debug, Clone)]
pub struct UndoResult {
    pub group_id: String,
    pub tool_name: String,
    pub restored: Vec<PathBuf>,
    pub failed: Vec<(PathBuf, String)>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_create_group_and_snapshot() {
        let dir = TempDir::new().unwrap();
        let file_path = dir.path().join("test.txt");
        fs::write(&file_path, "original content").unwrap();

        let mut mgr = FileSnapshotManager::new(10);
        let group_id = mgr.create_group("write_file");

        mgr.snapshot_file(&group_id, &file_path).unwrap();

        // Modify the file
        fs::write(&file_path, "modified content").unwrap();

        // Undo should restore original
        let result = mgr.undo_last().unwrap();
        assert_eq!(result.tool_name, "write_file");
        assert!(result.restored.contains(&file_path));

        let content = fs::read_to_string(&file_path).unwrap();
        assert_eq!(content, "original content");
    }

    #[test]
    fn test_undo_deletes_new_file() {
        let dir = TempDir::new().unwrap();
        let file_path = dir.path().join("new_file.txt");
        // File doesn't exist yet

        let mut mgr = FileSnapshotManager::new(10);
        let group_id = mgr.create_group("write_file");

        // Snapshot non-existent file
        mgr.snapshot_file(&group_id, &file_path).unwrap();

        // Create the file
        fs::write(&file_path, "new content").unwrap();
        assert!(file_path.exists());

        // Undo should delete it
        mgr.undo_last().unwrap();
        assert!(!file_path.exists());
    }

    #[test]
    fn test_cannot_undo_twice() {
        let dir = TempDir::new().unwrap();
        let file_path = dir.path().join("test.txt");
        fs::write(&file_path, "content").unwrap();

        let mut mgr = FileSnapshotManager::new(10);
        let group_id = mgr.create_group("write_file");
        mgr.snapshot_file(&group_id, &file_path).unwrap();

        let first = mgr.undo_last();
        assert!(first.is_some());

        let second = mgr.undo_last();
        assert!(second.is_none()); // Already undone
    }

    #[test]
    fn test_undo_count() {
        let mut mgr = FileSnapshotManager::new(10);
        mgr.create_group("tool1");
        mgr.create_group("tool2");

        assert_eq!(mgr.undo_count(), 2);

        mgr.undo_last();
        assert_eq!(mgr.undo_count(), 1);
    }

    #[test]
    fn test_max_groups_limit() {
        let mut mgr = FileSnapshotManager::new(3);
        mgr.create_group("tool1");
        mgr.create_group("tool2");
        mgr.create_group("tool3");
        mgr.create_group("tool4"); // Should evict oldest

        assert_eq!(mgr.list_groups().len(), 3);
    }

    #[test]
    fn test_trim_to_max_bulk_eviction() {
        // Create a manager with max 2 groups, add 5 — should bulk-drain to 2
        let mut mgr = FileSnapshotManager::new(2);
        for i in 0..5 {
            mgr.create_group(&format!("tool{i}"));
        }
        assert_eq!(mgr.list_groups().len(), 2);
        assert_eq!(mgr.list_groups()[0].tool_name, "tool3");
        assert_eq!(mgr.list_groups()[1].tool_name, "tool4");
    }
}
