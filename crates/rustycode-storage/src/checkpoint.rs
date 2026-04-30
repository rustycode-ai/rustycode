// Copyright 2025 The RustyCode Authors. All rights reserved.
// Use of this source code is governed by an MIT-style license.

//! Checkpoint storage for session recovery and rewind operations.
//!
//! This module provides storage abstractions for git-based checkpoints,
//! enabling session recovery and rewind capabilities.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

/// A checkpoint representing a saved session state with git reference.
///
/// Checkpoints are created at key points in session execution and include
/// git commit hash information for recovery operations.
///
/// # Example
///
/// ```rust,no_run
/// use rustycode_storage::Checkpoint;
///
/// let checkpoint = Checkpoint {
///     git_hash: "abc123def456".to_string(),
///     modified_files: vec![
///         "src/main.rs".to_string(),
///         "src/lib.rs".to_string(),
///     ],
///     created_at: "2025-01-01T00:00:00Z".to_string(),
/// };
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Checkpoint {
    /// Git commit hash representing the checkpoint
    pub git_hash: String,
    /// List of files modified since the previous checkpoint
    pub modified_files: Vec<String>,
    /// ISO 8601 timestamp when checkpoint was created
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitRewindSnapshot {
    /// Target git hash to rewind to
    pub git_hash: String,
    /// Optional list of files to restore (unused by basic implementation)
    pub files: Vec<String>,
}

/// Trait for checkpoint storage implementations.
///
/// Different storage backends (git, database, filesystem) can implement
/// this trait to provide checkpoint persistence.
pub trait CheckpointStorage: Send + Sync {
    /// Save a checkpoint to storage.
    ///
    /// # Arguments
    ///
    /// * `checkpoint` - The checkpoint to save
    ///
    /// # Errors
    ///
    /// Returns an error if the checkpoint cannot be saved.
    fn save_checkpoint(&self, checkpoint: &Checkpoint) -> Result<()>;

    /// Load a checkpoint from storage.
    ///
    /// # Arguments
    ///
    /// * `id` - The identifier for the checkpoint to load
    ///
    /// # Returns
    ///
    /// Returns `Ok(Some(checkpoint))` if found, `Ok(None)` if not found,
    /// or an error if the load operation fails.
    fn load_checkpoint(&self, id: &str) -> Result<Option<Checkpoint>>;

    /// Delete a checkpoint from storage.
    ///
    /// # Arguments
    ///
    /// * `id` - The identifier for the checkpoint to delete
    ///
    /// # Errors
    ///
    /// Returns an error if the checkpoint cannot be deleted.
    fn delete_checkpoint(&self, id: &str) -> Result<()>;

    /// Rewind the repository to a given snapshot.
    ///
    /// Implementations should attempt to restore file state for the provided
    /// `GitRewindSnapshot`. For backends that cannot perform a rewind, return an
    /// error.
    fn rewind_to_checkpoint(&self, snapshot: &GitRewindSnapshot) -> Result<()>;
}

/// Git-based checkpoint storage implementation.
///
/// This implementation stores checkpoint metadata related to git commits,
/// using the repository path for reference.
///
/// # Example
///
/// ```rust,no_run
/// use rustycode_storage::{GitCheckpointStorage, CheckpointStorage, Checkpoint};
///
/// let storage = GitCheckpointStorage::new("/path/to/repo");
/// let checkpoint = Checkpoint {
///     git_hash: "abc123def456".to_string(),
///     modified_files: vec!["src/main.rs".to_string()],
///     created_at: "2025-01-01T00:00:00Z".to_string(),
/// };
///
/// // storage.save_checkpoint(&checkpoint)?;
/// # Ok::<_, anyhow::Error>(())
/// ```
pub struct GitCheckpointStorage {
    /// Path to the git repository
    repo_path: String,
    /// Directory for storing checkpoint metadata
    checkpoint_dir: PathBuf,
}

impl GitCheckpointStorage {
    /// Create a new git checkpoint storage instance.
    ///
    /// # Arguments
    ///
    /// * `repo_path` - Path to the git repository
    pub fn new(repo_path: impl Into<String>) -> Self {
        let repo_path = repo_path.into();
        let checkpoint_dir = PathBuf::from(&repo_path)
            .join(".git")
            .join("rustycode_checkpoints");
        Self {
            repo_path,
            checkpoint_dir,
        }
    }

    /// Create a new git checkpoint storage instance from a `PathBuf`.
    pub fn from_path(repo_path: PathBuf) -> Self {
        let checkpoint_dir = repo_path.join(".git").join("rustycode_checkpoints");
        Self {
            repo_path: repo_path.to_string_lossy().to_string(),
            checkpoint_dir,
        }
    }

    /// Get the repository path.
    pub fn repo_path(&self) -> &str {
        &self.repo_path
    }

    fn checkpoint_file(&self, id: &str) -> PathBuf {
        self.checkpoint_dir.join(format!("{id}.json"))
    }

    fn load_index(&self) -> Result<HashMap<String, Checkpoint>> {
        if !self.checkpoint_dir.exists() {
            return Ok(HashMap::new());
        }
        let mut index = HashMap::new();
        for entry in fs::read_dir(&self.checkpoint_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("json") {
                if let Ok(content) = fs::read_to_string(&path) {
                    if let Ok(checkpoint) = serde_json::from_str::<Checkpoint>(&content) {
                        index.insert(checkpoint.git_hash.clone(), checkpoint);
                    } else {
                        tracing::warn!("Skipping malformed checkpoint file: {}", path.display());
                    }
                }
            }
        }
        Ok(index)
    }

    fn save_index(&self, index: &HashMap<String, Checkpoint>) -> Result<()> {
        fs::create_dir_all(&self.checkpoint_dir)
            .with_context(|| "Failed to create checkpoint directory")?;

        // Remove stale checkpoint files not in the index
        let valid_ids: std::collections::HashSet<String> = index.keys().cloned().collect();
        if self.checkpoint_dir.exists() {
            for entry in fs::read_dir(&self.checkpoint_dir)
                .with_context(|| "Failed to read checkpoint directory")?
            {
                let entry = entry?;
                let path = entry.path();
                if path.extension().and_then(|s| s.to_str()) == Some("json") {
                    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
                    if !valid_ids.contains(stem) {
                        let _ = fs::remove_file(&path);
                    }
                }
            }
        }

        for checkpoint in index.values() {
            let path = self.checkpoint_file(&checkpoint.git_hash);
            let content = serde_json::to_string_pretty(checkpoint)
                .with_context(|| "Failed to serialize checkpoint")?;
            fs::write(&path, content)
                .with_context(|| format!("Failed to write checkpoint to {}", path.display()))?;
        }
        Ok(())
    }
}

/// Return true if repository has uncommitted changes (staged, unstaged, or untracked).
pub fn repo_has_uncommitted_changes(repo_path: &std::path::Path) -> Result<bool> {
    // Use `git status --porcelain` which prints any changed/untracked files
    let output = std::process::Command::new("git")
        .args(["status", "--porcelain"])
        .current_dir(repo_path)
        .output()
        .with_context(|| "failed to run git status --porcelain")?;
    if !output.status.success() {
        anyhow::bail!(
            "git status failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    Ok(!stdout.trim().is_empty())
}

impl GitCheckpointStorage {
    /// Preview what a rewind would change without performing destructive operations.
    /// Returns a human-readable summary of files that would be modified or removed.
    pub fn preview_rewind(&self, snapshot: &GitRewindSnapshot) -> Result<String> {
        let mut parts: Vec<String> = Vec::new();

        if snapshot.files.is_empty() {
            // Full reset — list files that differ between target and HEAD
            let diff_output = std::process::Command::new("git")
                .args(["diff", "--name-only", &snapshot.git_hash, "HEAD"])
                .current_dir(&self.repo_path)
                .output()
                .with_context(|| "failed to run git diff --name-only")?;
            if !diff_output.status.success() {
                anyhow::bail!(
                    "git diff failed: {}",
                    String::from_utf8_lossy(&diff_output.stderr)
                );
            }
            let modified = String::from_utf8_lossy(&diff_output.stdout);
            if modified.trim().is_empty() {
                parts.push(format!(
                    "No tracked file changes between {} and HEAD.",
                    snapshot.git_hash
                ));
            } else {
                parts.push(format!(
                    "Files differing from {}..HEAD:\n{}",
                    snapshot.git_hash, modified
                ));
            }

            // List untracked files that would be removed by `git clean -fd` using -n (dry-run)
            let clean_output = std::process::Command::new("git")
                .args(["clean", "-nd"]) // -n: dry-run, -d: include directories
                .current_dir(&self.repo_path)
                .output()
                .with_context(|| "failed to run git clean -nd")?;
            if clean_output.status.success() {
                let clean_out = String::from_utf8_lossy(&clean_output.stdout);
                if clean_out.trim().is_empty() {
                    parts.push("No untracked files would be removed by git clean -fd.".to_string());
                } else {
                    parts.push(format!(
                        "Untracked files that would be removed by git clean -fd:\n{clean_out}"
                    ));
                }
            } else {
                // git clean -n may exit success with no stdout; treat non-success as warning
                parts.push(format!(
                    "Warning: git clean -nd failed: {}",
                    String::from_utf8_lossy(&clean_output.stderr)
                ));
            }
        } else {
            // Preview checkout of specific files — simply list them
            parts.push(format!(
                "Files that would be checked out from {}:\n{}",
                snapshot.git_hash,
                snapshot.files.join("\n")
            ));
        }

        Ok(parts.join("\n\n"))
    }
}

impl CheckpointStorage for GitCheckpointStorage {
    fn save_checkpoint(&self, checkpoint: &Checkpoint) -> Result<()> {
        let mut index = self.load_index()?;
        index.insert(checkpoint.git_hash.clone(), checkpoint.clone());
        self.save_index(&index)?;
        Ok(())
    }

    fn load_checkpoint(&self, id: &str) -> Result<Option<Checkpoint>> {
        let path = self.checkpoint_file(id);
        if !path.exists() {
            return Ok(None);
        }
        let content = fs::read_to_string(&path)
            .with_context(|| format!("Failed to read checkpoint from {}", path.display()))?;
        let checkpoint: Checkpoint =
            serde_json::from_str(&content).with_context(|| "Failed to parse checkpoint")?;
        Ok(Some(checkpoint))
    }

    fn delete_checkpoint(&self, id: &str) -> Result<()> {
        let path = self.checkpoint_file(id);
        if path.exists() {
            fs::remove_file(&path)
                .with_context(|| format!("Failed to delete checkpoint {}", path.display()))?;
        }
        Ok(())
    }

    fn rewind_to_checkpoint(&self, snapshot: &GitRewindSnapshot) -> Result<()> {
        if snapshot.files.is_empty() {
            // Full reset to commit
            let output = std::process::Command::new("git")
                .args(["reset", "--hard", &snapshot.git_hash])
                .current_dir(&self.repo_path)
                .output()
                .with_context(|| "Failed to run git reset --hard")?;
            if !output.status.success() {
                anyhow::bail!(
                    "git reset failed: {}",
                    String::from_utf8_lossy(&output.stderr)
                );
            }
        } else {
            // Checkout specific files from commit
            let mut args = vec![
                "checkout".to_string(),
                snapshot.git_hash.clone(),
                "--".to_string(),
            ];
            for f in &snapshot.files {
                args.push(f.clone());
            }
            let output = std::process::Command::new("git")
                .args(&args)
                .current_dir(&self.repo_path)
                .output()
                .with_context(|| "Failed to run git checkout for specific files")?;
            if !output.status.success() {
                anyhow::bail!(
                    "git checkout failed: {}",
                    String::from_utf8_lossy(&output.stderr)
                );
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_checkpoint_creation() {
        let checkpoint = Checkpoint {
            git_hash: "abc123def456".to_string(),
            modified_files: vec!["src/main.rs".to_string(), "src/lib.rs".to_string()],
            created_at: "2025-01-01T00:00:00Z".to_string(),
        };

        assert_eq!(checkpoint.git_hash, "abc123def456");
        assert_eq!(checkpoint.modified_files.len(), 2);
        assert_eq!(checkpoint.created_at, "2025-01-01T00:00:00Z");
    }

    #[test]
    fn test_git_checkpoint_storage_creation() {
        let temp_dir = TempDir::new().unwrap();
        let storage = GitCheckpointStorage::from_path(temp_dir.path().to_path_buf());
        assert_eq!(storage.repo_path(), temp_dir.path().to_str().unwrap());
    }

    #[test]
    fn test_git_checkpoint_storage_save() {
        let temp_dir = TempDir::new().unwrap();
        let storage = GitCheckpointStorage::from_path(temp_dir.path().to_path_buf());
        let checkpoint = Checkpoint {
            git_hash: "abc123def456".to_string(),
            modified_files: vec!["src/main.rs".to_string()],
            created_at: "2025-01-01T00:00:00Z".to_string(),
        };

        let result = storage.save_checkpoint(&checkpoint);
        assert!(result.is_ok());

        let loaded = storage.load_checkpoint("abc123def456").unwrap();
        assert!(loaded.is_some());
        assert_eq!(loaded.unwrap().git_hash, "abc123def456");
    }

    #[test]
    fn test_git_checkpoint_storage_load() {
        let temp_dir = TempDir::new().unwrap();
        let storage = GitCheckpointStorage::from_path(temp_dir.path().to_path_buf());

        let result = storage.load_checkpoint("test_id");
        assert!(result.is_ok());
        assert!(result.unwrap().is_none());
    }

    #[test]
    fn test_git_checkpoint_storage_delete() {
        let temp_dir = TempDir::new().unwrap();
        let storage = GitCheckpointStorage::from_path(temp_dir.path().to_path_buf());

        let checkpoint = Checkpoint {
            git_hash: "to_delete".to_string(),
            modified_files: vec![],
            created_at: "2025-01-01T00:00:00Z".to_string(),
        };
        storage.save_checkpoint(&checkpoint).unwrap();

        let result = storage.delete_checkpoint("to_delete");
        assert!(result.is_ok());

        let loaded = storage.load_checkpoint("to_delete").unwrap();
        assert!(loaded.is_none());
    }
}
