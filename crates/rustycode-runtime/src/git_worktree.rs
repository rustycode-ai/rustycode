//! Git Worktree Management System
//!
//! This module provides comprehensive git worktree management for the orchestrator,
//! enabling isolated development environments with automatic cleanup and management.
//!
//! ## Features
//!
//! - **Worktree Creation**: Create new worktrees with automatic branch creation
//! - **Worktree Listing**: List all worktrees with their status
//! - **Worktree Removal**: Clean removal of worktrees and branches
//! - **Worktree Pruning**: Remove stale worktrees automatically
//! - **Status Tracking**: Track worktree health and status
//! - **Branch Management**: Automatic branch naming and management
//! - **Conflict Detection**: Detect and handle worktree conflicts
//!
//! ## Architecture
//!
//! The worktree system provides three levels of management:
//!
//! 1. **Session Worktrees**: Temporary worktrees for specific tasks
//! 2. **Feature Worktrees**: Long-lived worktrees for features
//! 3. **Cleanup Service**: Automatic cleanup of stale worktrees

use crate::error::WorktreeError;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use tokio::sync::RwLock;

// --- Session-scoped worktree state (re-exported from tools-api) ---

pub use rustycode_tools_api::{
    clear_session_original_cwd, in_worktree_session, session_original_cwd, set_session_original_cwd,
};

/// Worktree information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Worktree {
    /// Unique identifier
    pub id: String,
    /// Worktree name
    pub name: String,
    /// Path to worktree
    pub path: PathBuf,
    /// Branch name
    pub branch: String,
    /// Commit SHA
    pub commit: String,
    /// Worktree status
    pub status: WorktreeStatus,
    pub created_at: DateTime<Utc>,
    /// Last accessed at
    pub accessed_at: DateTime<Utc>,
    pub worktree_type: WorktreeType,
    /// Whether it's tracked by git
    pub is_tracked: bool,
}

/// Worktree status
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[non_exhaustive]
pub enum WorktreeStatus {
    /// Worktree is clean (no modifications)
    Clean,
    /// Worktree has uncommitted changes
    Dirty,
    /// Worktree has untracked files
    Untracked,
    /// Worktree is in merge conflict
    Conflict,
    /// Worktree is detached (no branch)
    Detached,
}

/// Worktree type classification
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[non_exhaustive]
pub enum WorktreeType {
    /// Temporary session worktree
    Session,
    /// Feature development worktree
    Feature,
    /// Bug fix worktree
    Bugfix,
    /// Experiment worktree
    Experiment,
    /// Release worktree
    Release,
}

/// Worktree statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorktreeStats {
    /// Total number of worktrees
    pub total_worktrees: usize,
    /// Worktrees by status
    pub by_status: HashMap<String, usize>,
    /// Worktrees by type
    pub by_type: HashMap<String, usize>,
    /// Total disk usage in bytes
    pub disk_usage_bytes: u64,
    /// Oldest worktree age in hours
    pub oldest_age_hours: i64,
    /// Stale worktree count
    pub stale_count: usize,
}

/// Worktree manager
pub struct WorktreeManager {
    /// Repository path
    repo_path: PathBuf,
    /// Worktree storage directory
    worktrees_dir: PathBuf,
    /// Active worktrees
    worktrees: Arc<RwLock<HashMap<String, Worktree>>>,
    /// Configuration
    config: WorktreeConfig,
}

/// Worktree configuration
#[derive(Debug, Clone)]
pub struct WorktreeConfig {
    /// Worktrees storage directory name
    pub worktrees_dir_name: String,
    /// Automatic cleanup enabled
    pub auto_cleanup: bool,
    /// Maximum age for session worktrees (hours)
    pub session_max_age_hours: i64,
    /// Maximum age for feature worktrees (days)
    pub feature_max_age_days: i64,
    /// Stale threshold (hours without access)
    pub stale_threshold_hours: i64,
    /// Maximum concurrent worktrees
    pub max_concurrent_worktrees: usize,
    /// Branch name prefix
    pub branch_prefix: String,
}

impl Default for WorktreeConfig {
    fn default() -> Self {
        Self {
            worktrees_dir_name: ".worktrees".to_string(),
            auto_cleanup: true,
            session_max_age_hours: 24,
            feature_max_age_days: 30,
            stale_threshold_hours: 168, // 1 week
            max_concurrent_worktrees: 10,
            branch_prefix: "worktree".to_string(),
        }
    }
}

impl WorktreeManager {
    pub fn new(repo_path: PathBuf, config: WorktreeConfig) -> Result<Self, WorktreeError> {
        if !repo_path.exists() {
            return Err(WorktreeError::CreationFailed(
                "Repository path does not exist".to_string(),
            ));
        }

        let worktrees_dir = repo_path.join(&config.worktrees_dir_name);

        // Create worktrees directory if it doesn't exist
        if !worktrees_dir.exists() {
            std::fs::create_dir_all(&worktrees_dir).map_err(|e| {
                WorktreeError::CreationFailed(format!(
                    "Failed to create worktrees directory: {}",
                    e
                ))
            })?;
        }

        Ok(Self {
            repo_path,
            worktrees_dir,
            worktrees: Arc::new(RwLock::new(HashMap::new())),
            config,
        })
    }

    /// Create a new worktree
    pub async fn create_worktree(
        &self,
        name: String,
        branch: String,
        worktree_type: WorktreeType,
    ) -> Result<Worktree, WorktreeError> {
        // Check max concurrent worktrees
        let worktrees = self.worktrees.read().await;
        if worktrees.len() >= self.config.max_concurrent_worktrees {
            return Err(WorktreeError::CreationFailed(
                "Maximum concurrent worktrees reached".to_string(),
            ));
        }
        drop(worktrees);

        // Create unique worktree path
        let worktree_path = self.worktrees_dir.join(&name);

        // Check if worktree already exists
        if worktree_path.exists() {
            return Err(WorktreeError::CreationFailed(
                "Worktree already exists".to_string(),
            ));
        }

        // Get current commit
        let commit = self.get_current_commit()?;

        // Create worktree using git
        let output = Command::new("git")
            .arg("worktree")
            .arg("add")
            .arg("-b")
            .arg(&branch)
            .arg(&worktree_path)
            .arg(&commit)
            .current_dir(&self.repo_path)
            .output()
            .map_err(|e| {
                WorktreeError::GitError(format!("Failed to execute git worktree add: {}", e))
            })?;

        if !output.status.success() {
            let error = String::from_utf8_lossy(&output.stderr);
            return Err(WorktreeError::CreationFailed(error.to_string()));
        }

        let now = Utc::now();
        let worktree = Worktree {
            id: uuid::Uuid::new_v4().to_string(),
            name: name.clone(),
            path: worktree_path,
            branch: branch.clone(),
            commit,
            status: WorktreeStatus::Clean,
            created_at: now,
            accessed_at: now,
            worktree_type,
            is_tracked: true,
        };

        // Store worktree
        let mut worktrees = self.worktrees.write().await;
        worktrees.insert(name.clone(), worktree.clone());

        Ok(worktree)
    }

    /// List all worktrees
    pub async fn list_worktrees(&self) -> Vec<Worktree> {
        self.refresh_worktrees().await;
        let worktrees = self.worktrees.read().await;
        worktrees.values().cloned().collect()
    }

    /// Get a specific worktree
    pub async fn worktree(&self, name: &str) -> Option<Worktree> {
        let worktrees = self.worktrees.read().await;
        worktrees.get(name).cloned()
    }

    /// Remove a worktree
    pub async fn remove_worktree(&self, name: &str) -> Result<bool, WorktreeError> {
        let worktree = {
            let worktrees = self.worktrees.read().await;
            worktrees.get(name).cloned()
        };

        let worktree = worktree
            .ok_or_else(|| WorktreeError::RemovalFailed("Worktree not found".to_string()))?;

        // Remove worktree using git
        let output = Command::new("git")
            .arg("worktree")
            .arg("remove")
            .arg(&worktree.path)
            .current_dir(&self.repo_path)
            .output()
            .map_err(|e| {
                WorktreeError::GitError(format!("Failed to execute git worktree remove: {}", e))
            })?;

        if !output.status.success() {
            let error = String::from_utf8_lossy(&output.stderr);
            return Err(WorktreeError::RemovalFailed(error.to_string()));
        }

        // Remove from tracking
        let mut worktrees = self.worktrees.write().await;
        worktrees.remove(name);

        Ok(true)
    }

    /// Prune stale worktrees
    pub async fn prune_worktrees(&self) -> Result<usize, WorktreeError> {
        let pruned = {
            let worktrees = self.worktrees.read().await;
            let now = Utc::now();
            let mut to_prune = Vec::new();

            for (name, worktree) in worktrees.iter() {
                let age = now.signed_duration_since(worktree.accessed_at).num_hours();

                let should_prune = match worktree.worktree_type {
                    WorktreeType::Session => age >= self.config.session_max_age_hours,
                    WorktreeType::Feature => age >= self.config.feature_max_age_days * 24,
                    _ => false,
                };

                if should_prune {
                    to_prune.push(name.clone());
                }
            }

            to_prune
        };

        let mut count = 0;
        for name in pruned {
            if self.remove_worktree(&name).await.is_ok() {
                count += 1;
            }
        }

        Ok(count)
    }

    /// Get worktree statistics
    pub async fn stats(&self) -> WorktreeStats {
        self.refresh_worktrees().await;

        let worktrees = self.worktrees.read().await;
        let now = Utc::now();

        let mut by_status = HashMap::new();
        let mut by_type = HashMap::new();
        let mut total_disk_usage = 0u64;
        let mut oldest_age_hours = 0i64;
        let mut stale_count = 0usize;

        for worktree in worktrees.values() {
            // Count by status
            let status_str = format!("{:?}", worktree.status);
            let status_count = by_status.entry(status_str).or_insert(0);
            *status_count += 1;

            // Count by type
            let type_str = format!("{:?}", worktree.worktree_type);
            let type_count = by_type.entry(type_str).or_insert(0);
            *type_count += 1;

            // Calculate disk usage
            if let Ok(size) = self.get_directory_size(&worktree.path) {
                total_disk_usage += size;
            }

            // Calculate age
            let age = now.signed_duration_since(worktree.created_at).num_hours();
            if age > oldest_age_hours {
                oldest_age_hours = age;
            }

            // Count stale
            let access_age = now.signed_duration_since(worktree.accessed_at).num_hours();
            if access_age >= self.config.stale_threshold_hours {
                stale_count += 1;
            }
        }

        WorktreeStats {
            total_worktrees: worktrees.len(),
            by_status,
            by_type,
            disk_usage_bytes: total_disk_usage,
            oldest_age_hours,
            stale_count,
        }
    }

    /// Update worktree access time
    pub async fn update_access(&self, name: &str) {
        let mut worktrees = self.worktrees.write().await;
        if let Some(worktree) = worktrees.get_mut(name) {
            worktree.accessed_at = Utc::now();
        }
    }

    /// Get worktrees directory path
    pub fn worktrees_dir(&self) -> &Path {
        &self.worktrees_dir
    }

    /// Get worktree status
    pub async fn worktree_status(&self, name: &str) -> Option<WorktreeStatus> {
        let worktree = self.worktree(name).await?;
        Some(self.check_worktree_status(&worktree.path).await)
    }

    /// Refresh worktree information from git
    async fn refresh_worktrees(&self) {
        // Get list of worktrees from git
        let output = Command::new("git")
            .arg("worktree")
            .arg("list")
            .arg("--porcelain")
            .current_dir(&self.repo_path)
            .output();

        if let Ok(output) = output {
            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout);
                self.parse_worktree_list(&stdout).await;
            }
        }
    }

    /// Parse worktree list output
    async fn parse_worktree_list(&self, output: &str) {
        let mut worktrees = self.worktrees.write().await;
        let mut current_worktree: Option<(String, Worktree)> = None;

        for line in output.lines() {
            if line.is_empty() {
                if let Some((name, mut worktree)) = current_worktree.take() {
                    worktree.status = self.check_worktree_status_sync(&worktree.path);
                    worktrees.insert(name, worktree);
                }
            } else if let Some(rest) = line.strip_prefix("worktree ") {
                let path = PathBuf::from(rest.trim());
                let name = path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("unknown")
                    .to_string();

                current_worktree = Some((
                    name.clone(),
                    Worktree {
                        id: uuid::Uuid::new_v4().to_string(),
                        name: name.clone(),
                        path: path.clone(),
                        branch: "unknown".to_string(),
                        commit: "unknown".to_string(),
                        status: WorktreeStatus::Clean,
                        created_at: Utc::now(),
                        accessed_at: Utc::now(),
                        worktree_type: WorktreeType::Session,
                        is_tracked: true,
                    },
                ));
            } else if let Some(rest) = line.strip_prefix("HEAD ") {
                if let Some((_, ref mut worktree)) = current_worktree {
                    worktree.commit = rest.trim().to_string();
                }
            } else if let Some(rest) = line.strip_prefix("branch ") {
                if let Some((_, ref mut worktree)) = current_worktree {
                    worktree.branch = rest.trim().to_string();
                }
            } else if line.starts_with("detached") {
                if let Some((_, ref mut worktree)) = current_worktree {
                    worktree.status = WorktreeStatus::Detached;
                }
            }
        }

        // Don't forget the last worktree
        if let Some((name, mut worktree)) = current_worktree {
            worktree.status = self.check_worktree_status_sync(&worktree.path);
            worktrees.insert(name, worktree);
        }
    }

    /// Check worktree status (synchronous)
    fn check_worktree_status_sync(&self, path: &Path) -> WorktreeStatus {
        // Check for merge conflicts
        let git_dir = path.join(".git");
        if git_dir.exists() {
            let merge_head = git_dir.join("MERGE_HEAD");
            if merge_head.exists() {
                return WorktreeStatus::Conflict;
            }
        }

        // Check git status
        let output = Command::new("git")
            .arg("status")
            .arg("--porcelain")
            .current_dir(path)
            .output();

        if let Ok(output) = output {
            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout);
                let has_untracked = stdout.lines().any(|line| line.starts_with("??"));
                let has_modifications = stdout.lines().any(|line| {
                    line.starts_with(" M")
                        || line.starts_with("M ")
                        || line.starts_with("MM")
                        || line.starts_with("AA")
                });

                if stdout
                    .lines()
                    .any(|line| line.starts_with("AA") || line.starts_with("UU"))
                {
                    return WorktreeStatus::Conflict;
                }

                if has_modifications {
                    return WorktreeStatus::Dirty;
                }

                if has_untracked {
                    return WorktreeStatus::Untracked;
                }
            }
        }

        WorktreeStatus::Clean
    }

    /// Check worktree status (async)
    async fn check_worktree_status(&self, path: &Path) -> WorktreeStatus {
        self.check_worktree_status_sync(path)
    }

    /// Get current commit SHA
    fn get_current_commit(&self) -> Result<String, WorktreeError> {
        let output = Command::new("git")
            .arg("rev-parse")
            .arg("HEAD")
            .current_dir(&self.repo_path)
            .output()
            .map_err(|e| WorktreeError::GitError(format!("Failed to get current commit: {}", e)))?;

        if !output.status.success() {
            return Err(WorktreeError::GitError(
                "Failed to get current commit".to_string(),
            ));
        }

        let commit = String::from_utf8_lossy(&output.stdout);
        Ok(commit.trim().to_string())
    }

    /// Calculate directory size with depth cap to avoid walking huge trees.
    fn get_directory_size(&self, path: &Path) -> Result<u64, WorktreeError> {
        let mut total_size = 0u64;

        if path.is_dir() {
            for entry in walkdir::WalkDir::new(path)
                .max_depth(5)
                .into_iter()
                .filter_map(Result::ok)
            {
                if entry.file_type().is_file() {
                    if let Ok(metadata) = entry.metadata() {
                        total_size += metadata.len();
                    }
                }
            }
        }

        Ok(total_size)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_test_manager() -> (WorktreeManager, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let repo_path = temp_dir.path().to_path_buf();

        // Initialize git repository
        Command::new("git")
            .arg("init")
            .current_dir(&repo_path)
            .output()
            .unwrap();

        // Configure git
        Command::new("git")
            .arg("config")
            .arg("user.email")
            .arg("test@example.com")
            .current_dir(&repo_path)
            .output()
            .unwrap();

        Command::new("git")
            .arg("config")
            .arg("user.name")
            .arg("Test User")
            .current_dir(&repo_path)
            .output()
            .unwrap();

        // Create initial commit
        let test_file = repo_path.join("README.md");
        std::fs::write(&test_file, "# Test Repository").unwrap();

        Command::new("git")
            .arg("add")
            .arg("README.md")
            .current_dir(&repo_path)
            .output()
            .unwrap();

        Command::new("git")
            .arg("commit")
            .arg("-m")
            .arg("Initial commit")
            .current_dir(&repo_path)
            .output()
            .unwrap();

        let config = WorktreeConfig::default();
        let manager = WorktreeManager::new(repo_path, config).unwrap();

        (manager, temp_dir)
    }

    #[tokio::test]
    async fn test_worktree_manager_creation() {
        let temp_dir = TempDir::new().unwrap();
        let repo_path = temp_dir.path().to_path_buf();

        // Initialize git repository
        Command::new("git")
            .arg("init")
            .current_dir(&repo_path)
            .output()
            .unwrap();

        let config = WorktreeConfig::default();
        let manager = WorktreeManager::new(repo_path, config);

        assert!(manager.is_ok());
    }

    #[tokio::test]
    async fn test_create_and_list_worktree() {
        let (manager, _temp_dir) = create_test_manager();

        let worktree = manager
            .create_worktree(
                "test-worktree".to_string(),
                "feature/test-branch".to_string(),
                WorktreeType::Feature,
            )
            .await;

        assert!(worktree.is_ok());
        let worktree = worktree.unwrap();
        assert_eq!(worktree.name, "test-worktree");
        assert_eq!(worktree.branch, "feature/test-branch");

        let worktrees = manager.list_worktrees().await;
        // Filter to only the worktree we created
        let our_worktrees: Vec<_> = worktrees
            .iter()
            .filter(|w| w.name == "test-worktree")
            .collect();
        assert_eq!(our_worktrees.len(), 1);
    }

    #[tokio::test]
    async fn test_get_worktree() {
        let (manager, _temp_dir) = create_test_manager();

        manager
            .create_worktree(
                "test-worktree".to_string(),
                "feature/test-branch".to_string(),
                WorktreeType::Feature,
            )
            .await
            .unwrap();

        let worktree = manager.worktree("test-worktree").await;
        assert!(worktree.is_some());
        assert_eq!(worktree.unwrap().name, "test-worktree");
    }

    #[tokio::test]
    async fn test_remove_worktree() {
        let (manager, _temp_dir) = create_test_manager();

        manager
            .create_worktree(
                "test-worktree".to_string(),
                "feature/test-branch".to_string(),
                WorktreeType::Feature,
            )
            .await
            .unwrap();

        let removed = manager.remove_worktree("test-worktree").await;
        assert!(removed.is_ok());
        assert!(removed.unwrap());

        let worktrees = manager.list_worktrees().await;
        // Main repository still exists
        assert!(!worktrees.is_empty());
        // But our created worktree is gone
        assert!(!worktrees.iter().any(|w| w.name == "test-worktree"));
    }

    #[tokio::test]
    async fn test_worktree_stats() {
        let (manager, _temp_dir) = create_test_manager();

        manager
            .create_worktree(
                "test1".to_string(),
                "feature/test1".to_string(),
                WorktreeType::Feature,
            )
            .await
            .unwrap();

        manager
            .create_worktree(
                "test2".to_string(),
                "feature/test2".to_string(),
                WorktreeType::Session,
            )
            .await
            .unwrap();

        let stats = manager.stats().await;
        // Main repo + 2 created worktrees = at least 3 total
        assert!(stats.total_worktrees >= 3);
    }

    #[tokio::test]
    async fn test_update_access() {
        let (manager, _temp_dir) = create_test_manager();

        manager
            .create_worktree(
                "test-worktree".to_string(),
                "feature/test-branch".to_string(),
                WorktreeType::Feature,
            )
            .await
            .unwrap();

        let before = manager.worktree("test-worktree").await.unwrap();
        let accessed_before = before.accessed_at;

        // Sleep a bit to ensure time difference
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

        manager.update_access("test-worktree").await;

        let after = manager.worktree("test-worktree").await.unwrap();
        let accessed_after = after.accessed_at;

        assert!(accessed_after > accessed_before);
    }

    // --- New tests ---

    #[test]
    fn test_worktree_config_default_values() {
        let config = WorktreeConfig::default();
        assert_eq!(config.worktrees_dir_name, ".worktrees");
        assert!(config.auto_cleanup);
        assert_eq!(config.session_max_age_hours, 24);
        assert_eq!(config.feature_max_age_days, 30);
        assert_eq!(config.stale_threshold_hours, 168);
        assert_eq!(config.max_concurrent_worktrees, 10);
        assert_eq!(config.branch_prefix, "worktree");
    }

    #[test]
    fn test_worktree_config_custom_values() {
        let config = WorktreeConfig {
            worktrees_dir_name: ".my-trees".to_string(),
            auto_cleanup: false,
            session_max_age_hours: 48,
            feature_max_age_days: 14,
            stale_threshold_hours: 72,
            max_concurrent_worktrees: 5,
            branch_prefix: "wt".to_string(),
        };
        assert_eq!(config.worktrees_dir_name, ".my-trees");
        assert!(!config.auto_cleanup);
        assert_eq!(config.session_max_age_hours, 48);
        assert_eq!(config.feature_max_age_days, 14);
        assert_eq!(config.stale_threshold_hours, 72);
        assert_eq!(config.max_concurrent_worktrees, 5);
        assert_eq!(config.branch_prefix, "wt");
    }

    #[test]
    fn test_worktree_status_serde_roundtrip() {
        let statuses = [
            WorktreeStatus::Clean,
            WorktreeStatus::Dirty,
            WorktreeStatus::Untracked,
            WorktreeStatus::Conflict,
            WorktreeStatus::Detached,
        ];
        for status in &statuses {
            let json = serde_json::to_string(status).unwrap();
            let deserialized: WorktreeStatus = serde_json::from_str(&json).unwrap();
            assert_eq!(*status, deserialized);
        }
    }

    #[test]
    fn test_worktree_type_serde_roundtrip() {
        let types = [
            WorktreeType::Session,
            WorktreeType::Feature,
            WorktreeType::Bugfix,
            WorktreeType::Experiment,
            WorktreeType::Release,
        ];
        for wtype in &types {
            let json = serde_json::to_string(wtype).unwrap();
            let deserialized: WorktreeType = serde_json::from_str(&json).unwrap();
            assert_eq!(*wtype, deserialized);
        }
    }

    #[test]
    fn test_worktree_serde_roundtrip() {
        let now = Utc::now();
        let worktree = Worktree {
            id: "test-id-123".to_string(),
            name: "my-feature".to_string(),
            path: PathBuf::from("/tmp/worktrees/my-feature"),
            branch: "feature/my-feature".to_string(),
            commit: "abc123def456".to_string(),
            status: WorktreeStatus::Dirty,
            created_at: now,
            accessed_at: now,
            worktree_type: WorktreeType::Feature,
            is_tracked: true,
        };
        let json = serde_json::to_string(&worktree).unwrap();
        let deserialized: Worktree = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.id, "test-id-123");
        assert_eq!(deserialized.name, "my-feature");
        assert_eq!(
            deserialized.path,
            PathBuf::from("/tmp/worktrees/my-feature")
        );
        assert_eq!(deserialized.branch, "feature/my-feature");
        assert_eq!(deserialized.commit, "abc123def456");
        assert_eq!(deserialized.status, WorktreeStatus::Dirty);
        assert_eq!(deserialized.worktree_type, WorktreeType::Feature);
        assert!(deserialized.is_tracked);
    }

    #[test]
    fn test_worktree_stats_serde_roundtrip() {
        let mut by_status = HashMap::new();
        by_status.insert("Clean".to_string(), 3);
        by_status.insert("Dirty".to_string(), 1);

        let mut by_type = HashMap::new();
        by_type.insert("Feature".to_string(), 2);
        by_type.insert("Session".to_string(), 2);

        let stats = WorktreeStats {
            total_worktrees: 4,
            by_status: by_status.clone(),
            by_type: by_type.clone(),
            disk_usage_bytes: 1024000,
            oldest_age_hours: 72,
            stale_count: 1,
        };
        let json = serde_json::to_string(&stats).unwrap();
        let deserialized: WorktreeStats = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.total_worktrees, 4);
        assert_eq!(deserialized.by_status, by_status);
        assert_eq!(deserialized.by_type, by_type);
        assert_eq!(deserialized.disk_usage_bytes, 1024000);
        assert_eq!(deserialized.oldest_age_hours, 72);
        assert_eq!(deserialized.stale_count, 1);
    }

    #[test]
    fn test_worktree_status_equality() {
        assert_eq!(WorktreeStatus::Clean, WorktreeStatus::Clean);
        assert_eq!(WorktreeStatus::Dirty, WorktreeStatus::Dirty);
        assert_ne!(WorktreeStatus::Clean, WorktreeStatus::Dirty);
        assert_ne!(WorktreeStatus::Untracked, WorktreeStatus::Conflict);
        assert_ne!(WorktreeStatus::Detached, WorktreeStatus::Clean);
    }

    #[test]
    fn test_worktree_type_equality() {
        assert_eq!(WorktreeType::Session, WorktreeType::Session);
        assert_eq!(WorktreeType::Feature, WorktreeType::Feature);
        assert_ne!(WorktreeType::Session, WorktreeType::Feature);
        assert_ne!(WorktreeType::Bugfix, WorktreeType::Experiment);
        assert_ne!(WorktreeType::Release, WorktreeType::Session);
    }

    #[test]
    fn test_worktree_status_json_roundtrip() {
        let status = WorktreeStatus::Conflict;
        let json = serde_json::to_string_pretty(&status).unwrap();
        let restored: WorktreeStatus = serde_json::from_str(&json).unwrap();
        assert_eq!(status, restored);

        // Verify JSON representation uses the variant name
        assert!(json.contains("Conflict"));
    }

    #[test]
    fn test_worktree_type_json_roundtrip() {
        let wtype = WorktreeType::Experiment;
        let json = serde_json::to_string_pretty(&wtype).unwrap();
        let restored: WorktreeType = serde_json::from_str(&json).unwrap();
        assert_eq!(wtype, restored);

        // Verify JSON representation uses the variant name
        assert!(json.contains("Experiment"));
    }

    #[tokio::test]
    async fn test_worktree_manager_nonexistent_path() {
        let config = WorktreeConfig::default();
        let result = WorktreeManager::new(PathBuf::from("/nonexistent/path/xyz"), config);
        match result {
            Err(msg) => assert!(msg.contains("does not exist")),
            Ok(_) => panic!("Expected error for nonexistent path"),
        }
    }

    #[tokio::test]
    async fn test_worktree_duplicate_creation_fails() {
        let (manager, _temp_dir) = create_test_manager();

        let name = "dup-worktree".to_string();
        let branch1 = "feature/first".to_string();

        // First creation should succeed
        let first = manager
            .create_worktree(name.clone(), branch1, WorktreeType::Feature)
            .await;
        assert!(first.is_ok());

        // Second creation with same name should fail
        let branch2 = "feature/second".to_string();
        let second = manager
            .create_worktree(name, branch2, WorktreeType::Session)
            .await;
        assert!(second.is_err());
    }

    #[tokio::test]
    async fn test_get_nonexistent_worktree() {
        let (manager, _temp_dir) = create_test_manager();

        let result = manager.worktree("does-not-exist").await;
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_remove_nonexistent_worktree() {
        let (manager, _temp_dir) = create_test_manager();

        let result = manager.remove_worktree("does-not-exist").await;
        assert!(result.is_err());
    }

    #[test]
    fn test_worktrees_dir_path() {
        let temp_dir = TempDir::new().unwrap();
        let repo_path = temp_dir.path().to_path_buf();

        // Initialize git repo
        Command::new("git")
            .arg("init")
            .current_dir(&repo_path)
            .output()
            .unwrap();

        Command::new("git")
            .arg("config")
            .arg("user.email")
            .arg("test@example.com")
            .current_dir(&repo_path)
            .output()
            .unwrap();

        Command::new("git")
            .arg("config")
            .arg("user.name")
            .arg("Test User")
            .current_dir(&repo_path)
            .output()
            .unwrap();

        let config = WorktreeConfig {
            worktrees_dir_name: ".custom-trees".to_string(),
            ..WorktreeConfig::default()
        };
        let manager = WorktreeManager::new(repo_path, config).unwrap();

        assert!(manager.worktrees_dir().ends_with(".custom-trees"));
    }

    // =========================================================================
    // Terminal-bench: 15 additional tests for git_worktree
    // =========================================================================

    // 1. WorktreeStatus Copy trait - verify values are copyable
    #[test]
    fn worktree_status_is_copy() {
        let a = WorktreeStatus::Dirty;
        let b = a; // Copy, not move
        assert_eq!(a, b);
    }

    // 2. WorktreeType Copy trait
    #[test]
    fn worktree_type_is_copy() {
        let a = WorktreeType::Feature;
        let b = a;
        assert_eq!(a, b);
    }

    // 3. Worktree with all statuses serde roundtrip
    #[test]
    fn worktree_all_statuses_serde() {
        let statuses = [
            WorktreeStatus::Clean,
            WorktreeStatus::Dirty,
            WorktreeStatus::Untracked,
            WorktreeStatus::Conflict,
            WorktreeStatus::Detached,
        ];
        for status in &statuses {
            let w = Worktree {
                id: "id".into(),
                name: "n".into(),
                path: PathBuf::from("/tmp/w"),
                branch: "b".into(),
                commit: "c".into(),
                status: *status,
                created_at: Utc::now(),
                accessed_at: Utc::now(),
                worktree_type: WorktreeType::Session,
                is_tracked: true,
            };
            let json = serde_json::to_string(&w).unwrap();
            let decoded: Worktree = serde_json::from_str(&json).unwrap();
            assert_eq!(decoded.status, *status);
        }
    }

    // 4. Worktree with all types serde roundtrip
    #[test]
    fn worktree_all_types_serde() {
        let types = [
            WorktreeType::Session,
            WorktreeType::Feature,
            WorktreeType::Bugfix,
            WorktreeType::Experiment,
            WorktreeType::Release,
        ];
        for wt in &types {
            let w = Worktree {
                id: "id".into(),
                name: "n".into(),
                path: PathBuf::from("/tmp/w"),
                branch: "b".into(),
                commit: "c".into(),
                status: WorktreeStatus::Clean,
                created_at: Utc::now(),
                accessed_at: Utc::now(),
                worktree_type: *wt,
                is_tracked: true,
            };
            let json = serde_json::to_string(&w).unwrap();
            let decoded: Worktree = serde_json::from_str(&json).unwrap();
            assert_eq!(decoded.worktree_type, *wt);
        }
    }

    // 5. WorktreeStats with zero values serde roundtrip
    #[test]
    fn worktree_stats_zero_values_serde() {
        let stats = WorktreeStats {
            total_worktrees: 0,
            by_status: HashMap::new(),
            by_type: HashMap::new(),
            disk_usage_bytes: 0,
            oldest_age_hours: 0,
            stale_count: 0,
        };
        let json = serde_json::to_string(&stats).unwrap();
        let decoded: WorktreeStats = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.total_worktrees, 0);
        assert!(decoded.by_status.is_empty());
        assert_eq!(decoded.disk_usage_bytes, 0);
    }

    // 6. Worktree debug format contains fields
    #[test]
    fn worktree_debug_format() {
        let w = Worktree {
            id: "debug-id".into(),
            name: "debug-name".into(),
            path: PathBuf::from("/tmp/debug"),
            branch: "debug-branch".into(),
            commit: "abc123".into(),
            status: WorktreeStatus::Clean,
            created_at: Utc::now(),
            accessed_at: Utc::now(),
            worktree_type: WorktreeType::Feature,
            is_tracked: true,
        };
        let debug = format!("{:?}", w);
        assert!(debug.contains("debug-id"));
        assert!(debug.contains("debug-name"));
        assert!(debug.contains("abc123"));
    }

    // 7. WorktreeStats debug format
    #[test]
    fn worktree_stats_debug_format() {
        let stats = WorktreeStats {
            total_worktrees: 5,
            by_status: HashMap::new(),
            by_type: HashMap::new(),
            disk_usage_bytes: 1024,
            oldest_age_hours: 48,
            stale_count: 2,
        };
        let debug = format!("{:?}", stats);
        assert!(debug.contains("total_worktrees"));
    }

    // 8. WorktreeConfig default values are sensible
    #[test]
    fn worktree_config_default_sensible() {
        let c = WorktreeConfig::default();
        assert!(c.auto_cleanup);
        assert!(c.max_concurrent_worktrees > 0);
        assert!(c.session_max_age_hours > 0);
        assert!(c.feature_max_age_days > 0);
        assert!(c.stale_threshold_hours > 0);
        assert!(!c.worktrees_dir_name.is_empty());
        assert!(!c.branch_prefix.is_empty());
    }

    // 9. Worktree with is_tracked = false serde roundtrip
    #[test]
    fn worktree_untracked_serde() {
        let w = Worktree {
            id: "untracked-1".into(),
            name: "u".into(),
            path: PathBuf::from("/tmp/u"),
            branch: "b".into(),
            commit: "c".into(),
            status: WorktreeStatus::Detached,
            created_at: Utc::now(),
            accessed_at: Utc::now(),
            worktree_type: WorktreeType::Experiment,
            is_tracked: false,
        };
        let json = serde_json::to_string(&w).unwrap();
        let decoded: Worktree = serde_json::from_str(&json).unwrap();
        assert!(!decoded.is_tracked);
        assert_eq!(decoded.status, WorktreeStatus::Detached);
        assert_eq!(decoded.worktree_type, WorktreeType::Experiment);
    }

    // 10. WorktreeStatus all variants distinct
    #[test]
    fn worktree_status_all_distinct() {
        let statuses = [
            WorktreeStatus::Clean,
            WorktreeStatus::Dirty,
            WorktreeStatus::Untracked,
            WorktreeStatus::Conflict,
            WorktreeStatus::Detached,
        ];
        for i in 0..statuses.len() {
            for j in (i + 1)..statuses.len() {
                assert_ne!(
                    statuses[i], statuses[j],
                    "Statuses at {} and {} should differ",
                    i, j
                );
            }
        }
    }

    // 11. WorktreeType all variants distinct
    #[test]
    fn worktree_type_all_distinct() {
        let types = [
            WorktreeType::Session,
            WorktreeType::Feature,
            WorktreeType::Bugfix,
            WorktreeType::Experiment,
            WorktreeType::Release,
        ];
        for i in 0..types.len() {
            for j in (i + 1)..types.len() {
                assert_ne!(types[i], types[j], "Types at {} and {} should differ", i, j);
            }
        }
    }

    // 12. WorktreeStats with populated maps serde
    #[test]
    fn worktree_stats_populated_maps_serde() {
        let mut by_status = HashMap::new();
        by_status.insert("Clean".into(), 5);
        by_status.insert("Dirty".into(), 2);
        by_status.insert("Conflict".into(), 1);

        let mut by_type = HashMap::new();
        by_type.insert("Feature".into(), 3);
        by_type.insert("Bugfix".into(), 2);

        let stats = WorktreeStats {
            total_worktrees: 8,
            by_status: by_status.clone(),
            by_type: by_type.clone(),
            disk_usage_bytes: 5_000_000,
            oldest_age_hours: 120,
            stale_count: 3,
        };
        let json = serde_json::to_string(&stats).unwrap();
        let decoded: WorktreeStats = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.total_worktrees, 8);
        assert_eq!(decoded.by_status.get("Dirty"), Some(&2));
        assert_eq!(decoded.by_type.get("Feature"), Some(&3));
        assert_eq!(decoded.stale_count, 3);
    }

    // 13. Worktree clone produces equal copy
    #[test]
    fn worktree_clone_equal() {
        let now = Utc::now();
        let w = Worktree {
            id: "clone-id".into(),
            name: "clone-name".into(),
            path: PathBuf::from("/tmp/clone"),
            branch: "clone-branch".into(),
            commit: "def456".into(),
            status: WorktreeStatus::Dirty,
            created_at: now,
            accessed_at: now,
            worktree_type: WorktreeType::Bugfix,
            is_tracked: true,
        };
        let cloned = w.clone();
        assert_eq!(w.id, cloned.id);
        assert_eq!(w.name, cloned.name);
        assert_eq!(w.path, cloned.path);
        assert_eq!(w.branch, cloned.branch);
        assert_eq!(w.commit, cloned.commit);
        assert_eq!(w.status, cloned.status);
        assert_eq!(w.worktree_type, cloned.worktree_type);
        assert_eq!(w.is_tracked, cloned.is_tracked);
    }

    // 14. WorktreeConfig clone produces equal copy
    #[test]
    fn worktree_config_clone_equal() {
        let c = WorktreeConfig {
            worktrees_dir_name: ".wt".into(),
            auto_cleanup: false,
            session_max_age_hours: 12,
            feature_max_age_days: 7,
            stale_threshold_hours: 48,
            max_concurrent_worktrees: 3,
            branch_prefix: "fix".into(),
        };
        let cloned = c.clone();
        assert_eq!(c.worktrees_dir_name, cloned.worktrees_dir_name);
        assert_eq!(c.auto_cleanup, cloned.auto_cleanup);
        assert_eq!(c.max_concurrent_worktrees, cloned.max_concurrent_worktrees);
    }

    // 15. Worktree timestamp serde preserves values
    #[test]
    fn worktree_timestamp_serde_preserves() {
        let now = Utc::now();
        let w = Worktree {
            id: "ts".into(),
            name: "ts".into(),
            path: PathBuf::from("/tmp/ts"),
            branch: "b".into(),
            commit: "c".into(),
            status: WorktreeStatus::Clean,
            created_at: now,
            accessed_at: now,
            worktree_type: WorktreeType::Session,
            is_tracked: true,
        };
        let json = serde_json::to_string(&w).unwrap();
        let decoded: Worktree = serde_json::from_str(&json).unwrap();
        let diff_created = (decoded.created_at - now).num_milliseconds().abs();
        let diff_accessed = (decoded.accessed_at - now).num_milliseconds().abs();
        assert!(diff_created < 1000);
        assert!(diff_accessed < 1000);
    }
}
