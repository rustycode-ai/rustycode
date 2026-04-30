//! Per-file undo/redo stack for agent file edits.
//!
//! Records individual file operations and provides lightweight undo/redo
//! without requiring full workspace snapshots. Uses the existing
//! `SnapshotManager` shadow git repo for before-state capture when available,
//! falling back to in-memory content storage.

use crate::error::{CoreError, Result};
use crate::snapshot::SnapshotManager;
use std::collections::VecDeque;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

// ... (keep the rest of the file)

/// Maximum number of operations retained per file.
const DEFAULT_MAX_DEPTH: usize = 50;

/// A single recorded file edit operation.
#[derive(Debug, Clone)]
pub struct EditOperation {
    /// File path relative to workspace root.
    pub path: PathBuf,
    /// Content of the file before the edit.
    pub before: Option<String>,
    /// Timestamp of the edit.
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

/// Per-file undo/redo history.
pub struct EditHistory {
    /// Undo stack — most recent operation is at the back.
    undo_stack: VecDeque<EditOperation>,
    /// Redo stack — operations that were undone and can be re-applied.
    redo_stack: VecDeque<EditOperation>,
    /// Maximum depth per stack.
    max_depth: usize,
    /// Workspace root for resolving relative paths.
    workspace_root: PathBuf,
}

impl EditHistory {
    /// Create a new edit history for the given workspace.
    pub fn new(workspace_root: PathBuf) -> Self {
        Self {
            undo_stack: VecDeque::with_capacity(DEFAULT_MAX_DEPTH),
            redo_stack: VecDeque::with_capacity(DEFAULT_MAX_DEPTH),
            max_depth: DEFAULT_MAX_DEPTH,
            workspace_root,
        }
    }

    /// Create with a custom max depth.
    pub fn with_max_depth(workspace_root: PathBuf, max_depth: usize) -> Self {
        Self {
            undo_stack: VecDeque::with_capacity(max_depth),
            redo_stack: VecDeque::with_capacity(max_depth),
            max_depth,
            workspace_root,
        }
    }

    /// Record a file edit for potential undo.
    ///
    /// Call this **before** the edit is applied. Reads the current file content
    /// as the "before" state. If the file doesn't exist yet, `before` will be
    /// `None` (indicating a newly created file that can be deleted on undo).
    ///
    /// Clears the redo stack (new edits invalidate redo history).
    pub fn record_before_edit(&mut self, relative_path: &Path) -> Result<()> {
        let absolute = self.workspace_root.join(relative_path);
        let before = if absolute.exists() {
            Some(fs::read_to_string(&absolute).map_err(|e| {
                CoreError::Io(std::io::Error::new(
                    e.kind(),
                    format!(
                        "failed to read {} for undo snapshot: {}",
                        absolute.display(),
                        e
                    ),
                ))
            })?)
        } else {
            None
        };

        self.push_undo(EditOperation {
            path: relative_path.to_path_buf(),
            before,
            timestamp: chrono::Utc::now(),
        });

        // New edits invalidate redo history
        self.redo_stack.clear();

        Ok(())
    }

    /// Undo the most recent edit.
    ///
    /// Returns the reverted operation (moved to redo stack), or `None` if
    /// nothing to undo.
    pub fn undo(&mut self) -> Option<EditOperation> {
        let op = self.undo_stack.pop_back()?;

        let absolute = self.workspace_root.join(&op.path);

        // Read current content for redo before reverting
        let current_content = if absolute.exists() {
            fs::read_to_string(&absolute).ok()
        } else {
            None
        };

        // Revert to before state
        match &op.before {
            Some(content) => {
                if let Err(e) = fs::write(&absolute, content) {
                    tracing::warn!("Undo write failed for {}: {}", op.path.display(), e);
                    // Put it back on undo stack
                    self.undo_stack.push_back(op);
                    return None;
                }
            }
            None => {
                // File was newly created — delete it
                if absolute.exists() {
                    if let Err(e) = fs::remove_file(&absolute) {
                        tracing::warn!("Undo delete failed for {}: {}", op.path.display(), e);
                        self.undo_stack.push_back(op);
                        return None;
                    }
                }
            }
        }

        // Push to redo stack with current content as the "before" for redo
        let redo_op = EditOperation {
            path: op.path.clone(),
            before: current_content,
            timestamp: chrono::Utc::now(),
        };
        self.push_redo(redo_op);

        Some(op)
    }

    /// Redo the most recently undone edit.
    ///
    /// Returns the re-applied operation (moved back to undo stack), or `None`
    /// if nothing to redo.
    pub fn redo(&mut self) -> Option<EditOperation> {
        let op = self.redo_stack.pop_back()?;

        let absolute = self.workspace_root.join(&op.path);

        // Read current content for undo before re-applying
        let current_content = if absolute.exists() {
            fs::read_to_string(&absolute).ok()
        } else {
            None
        };

        // Re-apply the edit (restore the content that was there before undo)
        match &op.before {
            Some(content) => {
                if let Err(e) = fs::write(&absolute, content) {
                    tracing::warn!("Redo write failed for {}: {}", op.path.display(), e);
                    self.redo_stack.push_back(op);
                    return None;
                }
            }
            None => {
                // The file was deleted by undo — it should be recreated
                // But we don't have the content. This means the original
                // edit was a creation that was undone. Redo can't recreate
                // without content. Skip this redo.
                tracing::warn!(
                    "Redo cannot recreate file {} — no content stored",
                    op.path.display()
                );
                return None;
            }
        }

        // Push back to undo stack
        let undo_op = EditOperation {
            path: op.path.clone(),
            before: current_content,
            timestamp: chrono::Utc::now(),
        };
        self.push_undo(undo_op);

        Some(op)
    }

    /// Whether there are operations that can be undone.
    pub fn can_undo(&self) -> bool {
        !self.undo_stack.is_empty()
    }

    /// Whether there are operations that can be redone.
    pub fn can_redo(&self) -> bool {
        !self.redo_stack.is_empty()
    }

    /// Number of operations in the undo stack.
    pub fn undo_count(&self) -> usize {
        self.undo_stack.len()
    }

    /// Number of operations in the redo stack.
    pub fn redo_count(&self) -> usize {
        self.redo_stack.len()
    }

    /// Get a summary of the most recent undo-able operation.
    pub fn peek_undo(&self) -> Option<&EditOperation> {
        self.undo_stack.back()
    }

    /// Get a summary of the most recent redo-able operation.
    pub fn peek_redo(&self) -> Option<&EditOperation> {
        self.redo_stack.back()
    }

    /// Clear all history.
    pub fn clear(&mut self) {
        self.undo_stack.clear();
        self.redo_stack.clear();
    }

    fn push_undo(&mut self, op: EditOperation) {
        if self.undo_stack.len() >= self.max_depth {
            self.undo_stack.pop_front();
        }
        self.undo_stack.push_back(op);
    }

    fn push_redo(&mut self, op: EditOperation) {
        if self.redo_stack.len() >= self.max_depth {
            self.redo_stack.pop_front();
        }
        self.redo_stack.push_back(op);
    }
}

/// Thread-safe wrapper around `EditHistory`.
///
/// Use this when the edit history needs to be shared across threads (e.g.,
/// between the runtime and TUI).
pub struct SharedEditHistory {
    inner: Mutex<EditHistory>,
}

impl SharedEditHistory {
    pub fn new(workspace_root: PathBuf) -> Self {
        Self {
            inner: Mutex::new(EditHistory::new(workspace_root)),
        }
    }

    pub fn record_before_edit(&self, relative_path: &Path) -> Result<()> {
        self.inner
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .record_before_edit(relative_path)
    }

    pub fn undo(&self) -> Option<EditOperation> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner()).undo()
    }

    pub fn redo(&self) -> Option<EditOperation> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner()).redo()
    }

    pub fn can_undo(&self) -> bool {
        self.inner
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .can_undo()
    }

    pub fn can_redo(&self) -> bool {
        self.inner
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .can_redo()
    }

    pub fn undo_count(&self) -> usize {
        self.inner
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .undo_count()
    }

    pub fn redo_count(&self) -> usize {
        self.inner
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .redo_count()
    }

    pub fn peek_undo(&self) -> Option<EditOperation> {
        self.inner
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .peek_undo()
            .cloned()
    }

    pub fn peek_redo(&self) -> Option<EditOperation> {
        self.inner
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .peek_redo()
            .cloned()
    }

    pub fn clear(&self) {
        self.inner.lock().unwrap_or_else(|e| e.into_inner()).clear()
    }
}

/// Helper that integrates `EditHistory` with `SnapshotManager`.
///
/// Takes a shadow git snapshot before each edit as a safety net, then uses
/// the in-memory `EditHistory` for fast undo/redo. If in-memory undo fails
/// (e.g., content was evicted from the stack), the snapshot can be used
/// manually by the caller for full restore.
pub struct SnapshotBackedHistory {
    history: EditHistory,
    snapshot_manager: SnapshotManager,
}

impl SnapshotBackedHistory {
    pub fn new(workspace_root: PathBuf) -> Self {
        let snapshot_manager = SnapshotManager::new(workspace_root.clone());
        let history = EditHistory::new(workspace_root);
        Self {
            history,
            snapshot_manager,
        }
    }

    /// Record a file edit, capturing a shadow git snapshot first.
    ///
    /// The snapshot is taken before reading the file for in-memory undo,
    /// providing a safety net in case the in-memory undo stack is exhausted.
    pub fn record_edit(&mut self, relative_path: &Path) -> Result<()> {
        // Take snapshot for shadow git restore capability
        if let Err(e) = self.snapshot_manager.track() {
            tracing::debug!("Snapshot before edit failed (non-fatal): {}", e);
        }

        self.history.record_before_edit(relative_path)
    }

    /// Undo the most recent edit using in-memory content.
    ///
    /// Returns the reverted operation, or `None` if nothing to undo.
    pub fn undo(&mut self) -> Option<EditOperation> {
        self.history.undo()
    }

    /// Redo the most recently undone edit.
    pub fn redo(&mut self) -> Option<EditOperation> {
        self.history.redo()
    }

    pub fn can_undo(&self) -> bool {
        self.history.can_undo()
    }

    pub fn can_redo(&self) -> bool {
        self.history.can_redo()
    }

    pub fn undo_count(&self) -> usize {
        self.history.undo_count()
    }

    pub fn redo_count(&self) -> usize {
        self.history.redo_count()
    }

    pub fn clear(&mut self) {
        self.history.clear();
    }

    /// Access the underlying snapshot manager for full workspace restore.
    pub fn snapshot_manager(&self) -> &SnapshotManager {
        &self.snapshot_manager
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup_workspace() -> tempfile::TempDir {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path();

        // Init git repo (needed for canonicalization)
        std::process::Command::new("git")
            .args(["init"])
            .current_dir(path)
            .output()
            .unwrap();

        tmp
    }

    #[test]
    fn record_and_undo_existing_file() {
        let tmp = setup_workspace();
        let root = tmp.path();

        // Write initial file
        let file_path = Path::new("src/main.rs");
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(root.join(file_path), "fn main() {}").unwrap();

        let mut history = EditHistory::new(root.to_path_buf());

        // Record before edit
        history.record_before_edit(file_path).unwrap();

        // Simulate edit
        fs::write(root.join(file_path), "fn main() { println!(\"hello\"); }").unwrap();

        assert!(history.can_undo());
        assert_eq!(history.undo_count(), 1);

        // Undo
        let op = history.undo().unwrap();
        assert_eq!(op.path, file_path);
        assert_eq!(
            fs::read_to_string(root.join(file_path)).unwrap(),
            "fn main() {}"
        );
    }

    #[test]
    fn record_and_undo_new_file() {
        let tmp = setup_workspace();
        let root = tmp.path();

        let mut history = EditHistory::new(root.to_path_buf());

        let file_path = Path::new("new_file.txt");
        // File doesn't exist yet
        assert!(!root.join(file_path).exists());

        history.record_before_edit(file_path).unwrap();

        // Create the file
        fs::write(root.join(file_path), "new content").unwrap();

        // Undo should delete the file
        let op = history.undo().unwrap();
        assert!(op.before.is_none()); // Was a new file
        assert!(!root.join(file_path).exists());
    }

    #[test]
    fn redo_after_undo() {
        let tmp = setup_workspace();
        let root = tmp.path();

        let file_path = Path::new("test.txt");
        fs::write(root.join(file_path), "original").unwrap();

        let mut history = EditHistory::new(root.to_path_buf());

        // Record and edit
        history.record_before_edit(file_path).unwrap();
        fs::write(root.join(file_path), "modified").unwrap();

        // Undo
        history.undo().unwrap();
        assert_eq!(
            fs::read_to_string(root.join(file_path)).unwrap(),
            "original"
        );

        // Redo
        assert!(history.can_redo());
        let _op = history.redo().unwrap();
        assert_eq!(
            fs::read_to_string(root.join(file_path)).unwrap(),
            "modified"
        );
    }

    #[test]
    fn new_edit_clears_redo_stack() {
        let tmp = setup_workspace();
        let root = tmp.path();

        let file_a = Path::new("a.txt");
        let file_b = Path::new("b.txt");
        fs::write(root.join(file_a), "original A").unwrap();
        fs::write(root.join(file_b), "original B").unwrap();

        let mut history = EditHistory::new(root.to_path_buf());

        // Edit A and undo
        history.record_before_edit(file_a).unwrap();
        fs::write(root.join(file_a), "modified A").unwrap();
        history.undo().unwrap();

        assert!(history.can_redo());

        // New edit to B clears redo
        history.record_before_edit(file_b).unwrap();
        fs::write(root.join(file_b), "modified B").unwrap();

        assert!(!history.can_redo());
        assert_eq!(history.undo_count(), 1); // Only B remains in undo (A was popped during undo)
    }

    #[test]
    fn max_depth_evicts_oldest() {
        let tmp = setup_workspace();
        let root = tmp.path();

        let mut history = EditHistory::with_max_depth(root.to_path_buf(), 3);

        for i in 0..5 {
            let name = format!("file{}.txt", i);
            let path = Path::new(&name);
            fs::write(root.join(path), format!("content {}", i)).unwrap();
            history.record_before_edit(path).unwrap();
        }

        assert_eq!(history.undo_count(), 3);
        // Oldest 2 should be evicted
        let peek = history.peek_undo().unwrap();
        assert_eq!(peek.path, Path::new("file4.txt"));
    }

    #[test]
    fn multiple_undo_redo_cycle() {
        let tmp = setup_workspace();
        let root = tmp.path();

        let file_path = Path::new("cycle.txt");
        fs::write(root.join(file_path), "v0").unwrap();

        let mut history = EditHistory::new(root.to_path_buf());

        // Edit 1: v0 -> v1
        history.record_before_edit(file_path).unwrap();
        fs::write(root.join(file_path), "v1").unwrap();

        // Edit 2: v1 -> v2
        history.record_before_edit(file_path).unwrap();
        fs::write(root.join(file_path), "v2").unwrap();

        // Edit 3: v2 -> v3
        history.record_before_edit(file_path).unwrap();
        fs::write(root.join(file_path), "v3").unwrap();

        assert_eq!(history.undo_count(), 3);

        // Undo 3: v3 -> v2
        history.undo().unwrap();
        assert_eq!(fs::read_to_string(root.join(file_path)).unwrap(), "v2");

        // Undo 2: v2 -> v1
        history.undo().unwrap();
        assert_eq!(fs::read_to_string(root.join(file_path)).unwrap(), "v1");

        // Redo: v1 -> v2
        history.redo().unwrap();
        assert_eq!(fs::read_to_string(root.join(file_path)).unwrap(), "v2");

        // Redo: v2 -> v3
        history.redo().unwrap();
        assert_eq!(fs::read_to_string(root.join(file_path)).unwrap(), "v3");

        assert_eq!(history.undo_count(), 3);
        assert_eq!(history.redo_count(), 0);
    }

    #[test]
    fn clear_empties_both_stacks() {
        let tmp = setup_workspace();
        let root = tmp.path();

        let file_path = Path::new("clear_test.txt");
        fs::write(root.join(file_path), "content").unwrap();

        let mut history = EditHistory::new(root.to_path_buf());
        history.record_before_edit(file_path).unwrap();
        fs::write(root.join(file_path), "modified").unwrap();
        history.undo().unwrap();

        assert!(history.can_undo() || history.can_redo());
        history.clear();
        assert!(!history.can_undo());
        assert!(!history.can_redo());
    }

    #[test]
    fn shared_history_thread_safety() {
        use std::sync::Arc;
        use std::thread;

        let tmp = setup_workspace();
        let root = tmp.path();

        let file_path = Path::new("shared.txt");
        fs::write(root.join(file_path), "initial").unwrap();

        let history = Arc::new(SharedEditHistory::new(root.to_path_buf()));
        history.record_before_edit(file_path).unwrap();
        fs::write(root.join(file_path), "edited").unwrap();

        let h1 = Arc::clone(&history);
        let h2 = Arc::clone(&history);

        let t1 = thread::spawn(move || {
            assert!(h1.can_undo());
            assert_eq!(h1.undo_count(), 1);
        });

        let t2 = thread::spawn(move || {
            assert!(h2.can_undo());
        });

        t1.join().unwrap();
        t2.join().unwrap();
    }

    #[test]
    fn snapshot_backed_history_record_and_undo() {
        let tmp = setup_workspace();
        let root = tmp.path();

        // Configure git for snapshots
        std::process::Command::new("git")
            .args(["config", "user.email", "test@test.com"])
            .current_dir(root)
            .output()
            .unwrap();
        std::process::Command::new("git")
            .args(["config", "user.name", "test"])
            .current_dir(root)
            .output()
            .unwrap();

        let file_path = Path::new("snap_test.txt");
        fs::write(root.join(file_path), "original content").unwrap();

        let mut history = SnapshotBackedHistory::new(root.to_path_buf());
        history.record_edit(file_path).unwrap();

        // Simulate edit
        fs::write(root.join(file_path), "modified content").unwrap();

        assert!(history.can_undo());

        // Undo
        let op = history.undo().unwrap();
        assert_eq!(op.path, file_path);

        // File should be restored to original content
        let content = fs::read_to_string(root.join(file_path)).unwrap();
        assert_eq!(content, "original content");
    }

    #[test]
    fn peek_operations() {
        let tmp = setup_workspace();
        let root = tmp.path();

        let mut history = EditHistory::new(root.to_path_buf());

        let file_a = Path::new("a.txt");
        let file_b = Path::new("b.txt");

        fs::write(root.join(file_a), "a").unwrap();
        history.record_before_edit(file_a).unwrap();

        fs::write(root.join(file_b), "b").unwrap();
        history.record_before_edit(file_b).unwrap();

        let peek = history.peek_undo().unwrap();
        assert_eq!(peek.path, file_b);

        assert!(history.peek_redo().is_none());

        history.undo().unwrap();
        let peek_redo = history.peek_redo().unwrap();
        assert_eq!(peek_redo.path, file_b);
    }
}
