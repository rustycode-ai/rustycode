//! Checkpoint and rewind functionality for recovery and state management
//!
//! This module provides checkpoint creation and restoration, allowing sessions to
//! save and restore the state of the git repository at specific points in time.
//!
//! # Example
//!
//! ```rust,no_run
//! use rustycode_core::recovery::CheckpointRecovery;
//!
//! # #[tokio::main]
//! # async fn main() -> anyhow::Result<()> {
//! let recovery = CheckpointRecovery::new("/path/to/repo")?;
//!
//! // Create a checkpoint at current state
//! let checkpoint = recovery.create()?;
//! println!("Checkpoint created: {}", checkpoint.id);
//!
//! // ... make changes ...
//!
//! // Rewind to the checkpoint
//! recovery.rewind(&checkpoint).await?;
//! # Ok(())
//! # }
//! ```

use anyhow::{Context, Result};
use rustycode_git::GitClient;
use rustycode_protocol::session::{SessionSnapshot, SessionState};

/// Orchestrates checkpoint creation and restoration operations
pub struct CheckpointRecovery {
    git: GitClient,
}

impl std::fmt::Debug for CheckpointRecovery {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CheckpointRecovery")
            .field("git", &"GitClient")
            .finish()
    }
}

impl CheckpointRecovery {
    /// Create a new CheckpointRecovery instance
    ///
    /// # Arguments
    ///
    /// * `repo_path` - Path to the git repository
    ///
    /// # Errors
    ///
    /// Returns an error if the repository cannot be opened
    pub fn new(repo_path: &str) -> Result<Self> {
        let path = std::path::Path::new(repo_path);
        let git = GitClient::new(path).context("Failed to open git repository")?;
        Ok(Self { git })
    }

    /// Create a checkpoint at the current state
    ///
    /// # Returns
    ///
    /// A SessionSnapshot containing the current git hash and modified files
    ///
    /// # Errors
    ///
    /// Returns an error if git operations fail
    pub fn create(&self) -> Result<SessionSnapshot> {
        let git_hash = self
            .git
            .current_commit()
            .context("Failed to get current commit hash")?;
        let modified_files = self
            .git
            .modified_files()
            .context("Failed to get modified files")?;

        Ok(SessionSnapshot {
            id: uuid::Uuid::new_v4().to_string(),
            created_at: chrono::Utc::now().to_rfc3339(),
            last_step: 0,
            checkpoint_git_hash: Some(git_hash),
            checkpoint_modified_files: modified_files,
            checkpoint_created_at: Some(chrono::Utc::now().to_rfc3339()),
            state: SessionState::Active,
            context: String::new(),
        })
    }

    /// Rewind the repository to a checkpoint
    ///
    /// This performs a hard reset to the checkpoint's git commit, restoring
    /// all files to their state at checkpoint time. Any changes after the
    /// checkpoint are discarded, including untracked files that were created
    /// after the checkpoint.
    ///
    /// # Arguments
    ///
    /// * `checkpoint` - The checkpoint to rewind to
    ///
    /// # Errors
    ///
    /// Returns an error if the git reset operation fails
    pub async fn rewind(&self, checkpoint: &SessionSnapshot) -> Result<()> {
        let git_hash = checkpoint
            .checkpoint_git_hash
            .as_ref()
            .context("Checkpoint does not contain a git hash")?;

        // Perform the rewind in a blocking thread to avoid blocking async runtime.
        let repo_root = self.git.repository_root().to_path_buf();
        let git_hash_clone = git_hash.clone();

        tokio::task::spawn_blocking(move || {
            // Check for uncommitted changes before destructive rewind
            if rustycode_storage::repo_has_uncommitted_changes(&repo_root)? {
                anyhow::bail!(
                    "Cannot rewind: repository has uncommitted changes at {}. Commit or stash changes first.",
                    repo_root.display()
                );
            }

            // Use the storage-level helper which runs git checkout
            let snapshot = rustycode_storage::GitRewindSnapshot {
                git_hash: git_hash_clone.clone(),
                files: vec![],
            };
            rustycode_storage::rewind_repo(&repo_root, &snapshot)
                .with_context(|| format!("Failed to rewind to checkpoint {}", git_hash_clone))?;

            // Clean up untracked files that were created after the checkpoint
            let output = std::process::Command::new("git")
                .args(["clean", "-fd"])
                .current_dir(&repo_root)
                .output()
                .context("Failed to clean untracked files")?;
            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                return Err(anyhow::anyhow!("git clean failed: {}", stderr.trim()));
            }

            Ok(()) as Result<()>
        })
        .await
        .map_err(|e| anyhow::anyhow!(e))??;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn create_test_repo() -> Result<(TempDir, String)> {
        let temp = TempDir::new()?;
        let repo_path = temp.path().to_string_lossy().to_string();

        // Initialize git repo
        std::process::Command::new("git")
            .args(["init"])
            .current_dir(&repo_path)
            .output()?;

        // Configure git user for commits
        std::process::Command::new("git")
            .args(["config", "user.email", "test@example.com"])
            .current_dir(&repo_path)
            .output()?;

        std::process::Command::new("git")
            .args(["config", "user.name", "Test User"])
            .current_dir(&repo_path)
            .output()?;

        Ok((temp, repo_path))
    }

    fn create_commit(repo_path: &str, filename: &str, content: &str) -> Result<()> {
        let file_path = format!("{}/{}", repo_path, filename);
        fs::write(&file_path, content)?;

        std::process::Command::new("git")
            .args(["add", filename])
            .current_dir(repo_path)
            .output()?;

        std::process::Command::new("git")
            .args(["commit", "-m", &format!("Add {}", filename)])
            .current_dir(repo_path)
            .output()?;

        Ok(())
    }

    #[test]
    fn test_checkpoint_create() -> Result<()> {
        let (_temp, repo_path) = create_test_repo()?;
        create_commit(&repo_path, "test.txt", "initial content")?;

        let recovery = CheckpointRecovery::new(&repo_path)?;
        let checkpoint = recovery.create()?;

        assert!(!checkpoint.id.is_empty());
        assert!(checkpoint.checkpoint_git_hash.is_some());
        assert!(checkpoint.checkpoint_created_at.is_some());

        Ok(())
    }

    #[tokio::test]
    async fn test_checkpoint_create_and_restore() -> Result<()> {
        let (_temp, repo_path) = create_test_repo()?;
        create_commit(&repo_path, "test.txt", "initial content")?;

        // Create checkpoint
        let recovery = CheckpointRecovery::new(&repo_path)?;
        let checkpoint = recovery.create()?;

        let file_path = format!("{}/test.txt", repo_path);

        // Verify initial content
        assert_eq!(fs::read_to_string(&file_path)?, "initial content");

        // Modify and commit file (checkpoint rewind requires clean working tree)
        fs::write(&file_path, "modified content")?;
        assert_eq!(fs::read_to_string(&file_path)?, "modified content");
        create_commit(&repo_path, "test.txt", "modified content")?;

        // Rewind to checkpoint
        recovery.rewind(&checkpoint).await?;

        // Verify file is restored
        assert_eq!(fs::read_to_string(&file_path)?, "initial content");

        Ok(())
    }

    #[tokio::test]
    async fn test_rewind_with_new_files() -> Result<()> {
        let (_temp, repo_path) = create_test_repo()?;
        create_commit(&repo_path, "file1.txt", "content1")?;

        // Create checkpoint
        let recovery = CheckpointRecovery::new(&repo_path)?;
        let checkpoint = recovery.create()?;

        let file1 = format!("{}/file1.txt", repo_path);
        let file2 = format!("{}/file2.txt", repo_path);

        // Verify initial state
        assert!(std::path::Path::new(&file1).exists());
        assert!(!std::path::Path::new(&file2).exists());

        // Create and commit new file after checkpoint
        create_commit(&repo_path, "file2.txt", "content2")?;
        assert!(std::path::Path::new(&file2).exists());

        // Rewind (committed new file should be removed)
        recovery.rewind(&checkpoint).await?;

        // Verify new file is gone
        assert!(!std::path::Path::new(&file2).exists());
        assert_eq!(fs::read_to_string(&file1)?, "content1");

        Ok(())
    }

    #[tokio::test]
    async fn test_rewind_with_modified_and_new_files() -> Result<()> {
        let (_temp, repo_path) = create_test_repo()?;
        create_commit(&repo_path, "file1.txt", "original content")?;

        let recovery = CheckpointRecovery::new(&repo_path)?;
        let checkpoint = recovery.create()?;

        let file1 = format!("{}/file1.txt", repo_path);
        let file2 = format!("{}/file2.txt", repo_path);
        let file3 = format!("{}/file3.txt", repo_path);

        // Modify existing file and commit
        fs::write(&file1, "modified content")?;
        create_commit(&repo_path, "file1.txt", "modified content")?;

        // Create two new files and commit
        create_commit(&repo_path, "file2.txt", "new file 2")?;
        create_commit(&repo_path, "file3.txt", "new file 3")?;

        // Rewind (requires clean working tree)
        recovery.rewind(&checkpoint).await?;

        // Verify state
        assert_eq!(fs::read_to_string(&file1)?, "original content");
        assert!(!std::path::Path::new(&file2).exists());
        assert!(!std::path::Path::new(&file3).exists());

        Ok(())
    }
}
