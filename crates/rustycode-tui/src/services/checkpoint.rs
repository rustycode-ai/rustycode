//! Checkpoint and task resumption system
//!
//! This module provides the ability to save checkpoints (snapshots of the current state)
//! and resume work from a previous checkpoint. This is useful for:
//! - Saving progress before risky operations
//! - Resuming work after a crash or interruption
//! - Branching to explore alternative approaches
//!
//! # Checkpoint Contents
//!
//! Each checkpoint contains:
//! - Conversation history
//! - Current tasks/todos state
//! - Working directory state
//! - Timestamp and metadata
//!
//! # Usage
//!
//! ```rust,ignore
//! // Create a checkpoint
//! checkpoint_manager.save("before-refactor").await?;
//!
//! // List available checkpoints
//! let checkpoints = checkpoint_manager.list().await?;
//!
//! // Restore from a checkpoint
//! checkpoint_manager.restore("checkpoint-id").await?;
//! ```

use crate::tasks::WorkspaceTasks;
use crate::ui::message::Message;
use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use tokio::fs;
use tokio::io::AsyncWriteExt;

/// Metadata about a checkpoint
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckpointMetadata {
    /// Unique identifier for this checkpoint
    pub id: String,
    /// Human-readable name/description
    pub name: String,
    /// When the checkpoint was created
    pub created_at: DateTime<Utc>,
    /// Number of messages in the conversation
    pub message_count: usize,
    /// Number of tasks
    pub task_count: usize,
    /// Number of incomplete todos
    pub todo_count: usize,
    /// Current working directory
    pub cwd: PathBuf,
    /// Git branch (if available)
    pub git_branch: Option<String>,
    /// Git commit (if available)
    pub git_commit: Option<String>,
    /// User-provided description
    pub description: Option<String>,
}

/// Complete checkpoint data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Checkpoint {
    /// Checkpoint metadata
    pub metadata: CheckpointMetadata,
    /// Conversation messages
    pub messages: Vec<Message>,
    /// Workspace tasks state
    pub tasks: WorkspaceTasks,
}

/// Manager for creating and restoring checkpoints
pub struct CheckpointManager {
    /// Directory where checkpoints are stored
    checkpoints_dir: PathBuf,
    /// Index of all checkpoints
    index: HashMap<String, CheckpointMetadata>,
}

impl CheckpointManager {
    /// Create a new checkpoint manager
    pub fn new(checkpoints_dir: PathBuf) -> Self {
        Self {
            checkpoints_dir,
            index: HashMap::new(),
        }
    }

    /// Initialize the checkpoint manager
    ///
    /// Creates the checkpoints directory if it doesn't exist
    /// and loads the existing checkpoint index.
    pub async fn initialize(&mut self) -> Result<()> {
        // Create checkpoints directory if it doesn't exist
        if !self.checkpoints_dir.exists() {
            fs::create_dir_all(&self.checkpoints_dir).await?;
        }

        // Load existing checkpoint index
        self.load_index().await?;

        tracing::info!(
            "CheckpointManager initialized with {} checkpoints",
            self.index.len()
        );

        Ok(())
    }

    /// Save a checkpoint with the given name
    pub async fn save(
        &mut self,
        name: impl AsRef<str>,
        messages: Vec<Message>,
        tasks: &WorkspaceTasks,
        cwd: PathBuf,
    ) -> Result<String> {
        let name = name.as_ref();
        let timestamp = Utc::now();

        // Generate checkpoint ID from timestamp and name
        let id = format!(
            "{}-{}",
            timestamp.format("%Y%m%d-%H%M%S"),
            name.to_lowercase()
                .replace(' ', "-")
                .chars()
                .take(20)
                .collect::<String>()
        );

        // Get git info if available
        let (git_branch, git_commit) = self.get_git_info(&cwd).await;

        // Count incomplete todos
        let todo_count = tasks.todos.iter().filter(|t| !t.done).count();

        let metadata = CheckpointMetadata {
            id: id.clone(),
            name: name.to_string(),
            created_at: timestamp,
            message_count: messages.len(),
            task_count: tasks.tasks.len(),
            todo_count,
            cwd: cwd.clone(),
            git_branch,
            git_commit,
            description: None,
        };

        let checkpoint = Checkpoint {
            metadata: metadata.clone(),
            messages,
            tasks: tasks.clone(),
        };

        // Save checkpoint to file
        let checkpoint_path = self.checkpoints_dir.join(format!("{}.json", id));
        let data = serde_json::to_string_pretty(&checkpoint)?;

        let mut file = fs::File::create(&checkpoint_path).await?;
        file.write_all(data.as_bytes()).await?;
        file.flush().await?;

        // Update index
        self.index.insert(id.clone(), metadata);

        // Save updated index
        self.save_index().await?;

        tracing::info!("Checkpoint saved: {}", id);

        Ok(id)
    }

    /// Restore from a checkpoint
    pub async fn restore(&self, id: &str) -> Result<Checkpoint> {
        let checkpoint_path = self.checkpoints_dir.join(format!("{}.json", id));

        if !checkpoint_path.exists() {
            anyhow::bail!("Checkpoint not found: {}", id);
        }

        let data = fs::read_to_string(&checkpoint_path).await?;
        let checkpoint: Checkpoint = serde_json::from_str(&data)?;

        // If this checkpoint captured a git commit, rewind the workspace to that commit.
        if let Some(commit) = &checkpoint.metadata.git_commit {
            let cwd = checkpoint.metadata.cwd.clone();
            // Check repo cleanliness before destructive rewind
            let cwd_clone = cwd.clone();
            let dirty = tokio::task::spawn_blocking(move || {
                rustycode_storage::repo_has_uncommitted_changes(&cwd_clone)
            })
            .await
            .map_err(|e| anyhow::anyhow!(e))??;
            if dirty {
                tracing::warn!("Repository {} has uncommitted changes. Awaiting user confirmation in UI to proceed.", cwd.display());

                // Register a confirmation request and wait for the user's decision.
                let request_id = format!("checkpoint-{}-{}", id, Utc::now().timestamp_micros());
                let rx = crate::app::confirmation::register(request_id.clone());

                // Wait for decision in a blocking thread to avoid blocking the async runtime.
                let decision = tokio::task::spawn_blocking(move || -> Result<bool, String> {
                    #[allow(clippy::duration_suboptimal_units)]
                    let timeout = std::time::Duration::from_secs(300);
                    rx.recv_timeout(timeout).map_err(|e| {
                        format!("Checkpoint confirmation timed out or channel error: {}", e)
                    })
                })
                .await
                .map_err(|e| anyhow::anyhow!(e))?
                .map_err(|e| anyhow::anyhow!(e))?;

                if !decision {
                    anyhow::bail!("Restore cancelled by user");
                }
            }

            let commit_clone = commit.clone();
            let snapshot = rustycode_storage::GitRewindSnapshot {
                git_hash: commit_clone,
                files: vec![],
            };

            tokio::task::spawn_blocking(move || rustycode_storage::rewind_repo(&cwd, &snapshot))
                .await
                .map_err(|e| anyhow::anyhow!(e))??;

            // Clean untracked files
            let clean_path = checkpoint.metadata.cwd.clone();
            let clean_output = tokio::task::spawn_blocking(move || {
                std::process::Command::new("git")
                    .args(["-C", &clean_path.to_string_lossy(), "clean", "-fd"])
                    .output()
            })
            .await
            .map_err(|e| anyhow::anyhow!(e))??;

            if !clean_output.status.success() {
                tracing::warn!(
                    "git clean failed during checkpoint restore: {}",
                    String::from_utf8_lossy(&clean_output.stderr)
                );
            } else {
                tracing::info!(
                    "Workspace files rewound to commit {}",
                    &checkpoint
                        .metadata
                        .git_commit
                        .as_deref()
                        .unwrap_or("(unknown)")
                );
            }
        }

        tracing::info!("Checkpoint restored: {}", id);

        Ok(checkpoint)
    }

    /// List all available checkpoints
    pub fn list(&self) -> Vec<&CheckpointMetadata> {
        let mut checkpoints: Vec<_> = self.index.values().collect();
        checkpoints.sort_by_key(|a| std::cmp::Reverse(a.created_at));
        checkpoints
    }

    /// Get a specific checkpoint by ID
    pub fn get(&self, id: &str) -> Option<&CheckpointMetadata> {
        self.index.get(id)
    }

    /// Delete a checkpoint
    pub async fn delete(&mut self, id: &str) -> Result<()> {
        if !self.index.contains_key(id) {
            anyhow::bail!("Checkpoint not found: {}", id);
        }

        // Delete checkpoint file
        let checkpoint_path = self.checkpoints_dir.join(format!("{}.json", id));
        if checkpoint_path.exists() {
            fs::remove_file(&checkpoint_path).await?;
        }

        // Remove from index
        self.index.remove(id);

        // Save updated index
        self.save_index().await?;

        tracing::info!("Checkpoint deleted: {}", id);

        Ok(())
    }

    /// Add a description to a checkpoint
    pub async fn set_description(&mut self, id: &str, description: String) -> Result<()> {
        if let Some(metadata) = self.index.get_mut(id) {
            metadata.description = Some(description);
            self.save_index().await?;
            Ok(())
        } else {
            anyhow::bail!("Checkpoint not found: {}", id);
        }
    }

    /// Load the checkpoint index from disk
    async fn load_index(&mut self) -> Result<()> {
        let index_path = self.checkpoints_dir.join("index.json");

        if !index_path.exists() {
            return Ok(());
        }

        let data = fs::read_to_string(&index_path).await?;
        let saved_index: HashMap<String, CheckpointMetadata> = serde_json::from_str(&data)?;

        self.index = saved_index;

        Ok(())
    }

    /// Save the checkpoint index to disk
    async fn save_index(&self) -> Result<()> {
        let index_path = self.checkpoints_dir.join("index.json");
        let data = serde_json::to_string_pretty(&self.index)?;

        let mut file = fs::File::create(&index_path).await?;
        file.write_all(data.as_bytes()).await?;
        file.flush().await?;

        Ok(())
    }

    /// Get git branch and commit for the current directory
    async fn get_git_info(&self, cwd: &PathBuf) -> (Option<String>, Option<String>) {
        let mut branch = None;
        let mut commit = None;

        // Try to get current branch
        if let Ok(output) = tokio::process::Command::new("git")
            .args(["rev-parse", "--abbrev-ref", "HEAD"])
            .current_dir(cwd)
            .output()
            .await
        {
            if output.status.success() {
                branch = String::from_utf8(output.stdout)
                    .ok()
                    .map(|s| s.trim().to_string());
            }
        }

        // Try to get current commit
        if let Ok(output) = tokio::process::Command::new("git")
            .args(["rev-parse", "--short", "HEAD"])
            .current_dir(cwd)
            .output()
            .await
        {
            if output.status.success() {
                commit = String::from_utf8(output.stdout)
                    .ok()
                    .map(|s| s.trim().to_string());
            }
        }

        (branch, commit)
    }

    /// Clean up old checkpoints (keep last N)
    pub async fn cleanup_old(&mut self, keep_count: usize) -> Result<Vec<String>> {
        if self.index.len() <= keep_count {
            return Ok(vec![]);
        }

        let mut checkpoints: Vec<_> = self.index.keys().cloned().collect();

        // Sort by creation time (oldest first)
        checkpoints.sort_by(|a, b| {
            let a_meta = self.index.get(a);
            let b_meta = self.index.get(b);
            match (a_meta, b_meta) {
                (Some(am), Some(bm)) => am.created_at.cmp(&bm.created_at),
                (Some(_), None) => std::cmp::Ordering::Less,
                (None, Some(_)) => std::cmp::Ordering::Greater,
                (None, None) => std::cmp::Ordering::Equal,
            }
        });

        // Remove oldest checkpoints beyond keep_count
        let mut removed = Vec::new();
        let to_remove = checkpoints.len() - keep_count;

        for id in checkpoints.into_iter().take(to_remove) {
            if let Err(e) = self.delete(&id).await {
                tracing::warn!("Failed to delete checkpoint {}: {}", id, e);
            } else {
                removed.push(id);
            }
        }

        tracing::info!("Cleaned up {} old checkpoints", removed.len());

        Ok(removed)
    }

    /// Get total size of all checkpoints in bytes
    pub async fn total_size(&self) -> Result<u64> {
        let mut total = 0u64;

        let mut entries = fs::read_dir(&self.checkpoints_dir).await?;

        while let Some(entry) = entries.next_entry().await? {
            let metadata = entry.metadata().await?;
            if metadata.is_file() {
                total += metadata.len();
            }
        }

        Ok(total)
    }
}

/// Format checkpoint metadata for display
pub fn format_checkpoint(metadata: &CheckpointMetadata) -> String {
    format!(
        "{} [{}]\n  Created: {}\n  Messages: {} | Tasks: {} | Todos: {}\n  Branch: {}",
        metadata.name,
        metadata.id,
        metadata.created_at.format("%Y-%m-%d %H:%M"),
        metadata.message_count,
        metadata.task_count,
        metadata.todo_count,
        metadata.git_branch.as_deref().unwrap_or("(none)")
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_checkpoint_manager_init() {
        let temp_dir = TempDir::new().unwrap();
        let mut manager = CheckpointManager::new(temp_dir.path().to_path_buf());

        manager.initialize().await.unwrap();

        assert!(temp_dir.path().exists());
    }

    #[tokio::test]
    async fn test_save_and_list_checkpoint() {
        let temp_dir = TempDir::new().unwrap();
        let mut manager = CheckpointManager::new(temp_dir.path().to_path_buf());

        manager.initialize().await.unwrap();

        let id = manager
            .save(
                "test-checkpoint",
                vec![],
                &WorkspaceTasks {
                    tasks: vec![],
                    todos: vec![],
                    active_agents: vec![],
                },
                PathBuf::from("/tmp"),
            )
            .await
            .unwrap();

        let checkpoints = manager.list();
        assert_eq!(checkpoints.len(), 1);
        assert_eq!(checkpoints[0].id, id);
        assert_eq!(checkpoints[0].name, "test-checkpoint");
    }

    #[tokio::test]
    async fn test_restore_checkpoint() {
        let temp_dir = TempDir::new().unwrap();
        let mut manager = CheckpointManager::new(temp_dir.path().to_path_buf());

        manager.initialize().await.unwrap();

        let messages = vec![
            Message::user("Hello".to_string()),
            Message::assistant("Hi there".to_string()),
        ];

        let id = manager
            .save(
                "test-checkpoint",
                messages.clone(),
                &WorkspaceTasks {
                    tasks: vec![],
                    todos: vec![],
                    active_agents: vec![],
                },
                PathBuf::from("/tmp"),
            )
            .await
            .unwrap();

        let checkpoint = manager.restore(&id).await.unwrap();

        assert_eq!(checkpoint.messages.len(), 2);
        assert_eq!(checkpoint.messages[0].content, "Hello");
        assert_eq!(checkpoint.messages[1].content, "Hi there");
    }

    #[tokio::test]
    async fn test_delete_checkpoint() {
        let temp_dir = TempDir::new().unwrap();
        let mut manager = CheckpointManager::new(temp_dir.path().to_path_buf());

        manager.initialize().await.unwrap();

        let id = manager
            .save(
                "test-checkpoint",
                vec![],
                &WorkspaceTasks {
                    tasks: vec![],
                    todos: vec![],
                    active_agents: vec![],
                },
                PathBuf::from("/tmp"),
            )
            .await
            .unwrap();

        assert!(manager.get(&id).is_some());

        manager.delete(&id).await.unwrap();

        assert!(manager.get(&id).is_none());
        assert_eq!(manager.list().len(), 0);
    }
}
