//! Git worktree isolation for parallel milestone execution.
//!
//! Manages git worktree lifecycle for isolated execution environments.
//! Matches orchestra-2's auto-worktree.ts pattern for safe autonomous development.

use crate::error::{OrchestrationError, Result};
use chrono::Utc;
use once_cell::sync::{Lazy, OnceCell};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;
use tokio::fs;
use tracing::{debug, info, warn};

/// Original project root before entering worktree
static ORIGINAL_BASE: OnceCell<Mutex<Option<PathBuf>>> = OnceCell::new();

/// Precompiled regex for checked checkboxes
// SAFETY: This regex pattern is a valid compile-time constant
#[allow(clippy::unwrap_used)]
static CHECKED_CHECKBOX_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"^- \[x\] (.+)$").unwrap());

/// Precompiled regex for unchecked checkboxes
// SAFETY: This regex pattern is a valid compile-time constant
#[allow(clippy::unwrap_used)]
static UNCHECKED_CHECKBOX_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"^- \[ \] (.+)$").unwrap());

/// Generate auto-worktree branch name for a milestone
///
/// Matches orchestra-2's `autoWorktreeBranch()` function.
/// Uses `milestone/<MID>` format to distinguish from manual worktrees.
pub fn auto_worktree_branch(milestone_id: &str) -> String {
    format!("milestone/{milestone_id}")
}

/// Get original project root (before entering worktree)
///
/// Returns None if not in a worktree.
pub fn original_base() -> Option<PathBuf> {
    ORIGINAL_BASE.get().and_then(|m| {
        m.lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    })
}

/// Check if currently in a worktree
pub fn in_worktree() -> bool {
    original_base().is_some()
}

/// Worktree manager
pub struct WorktreeManager {
    project_root: PathBuf,
    /// Worktrees directory
    worktrees_dir: PathBuf,
}

impl WorktreeManager {
    pub fn new(project_root: PathBuf) -> Self {
        let worktrees_dir = project_root.join(".orchestra").join("worktrees");

        Self {
            project_root,
            worktrees_dir,
        }
    }

    /// Create an auto-worktree for a milestone (orchestra-2 pattern)
    ///
    /// Creates a worktree with `milestone/<MID>` branch naming,
    /// syncs initial state, and stores original base path.
    pub async fn create_auto_worktree(&self, milestone_id: &str) -> Result<Worktree> {
        let branch_name = auto_worktree_branch(milestone_id);
        let worktree_id = format!("auto-{milestone_id}");
        let worktree_path = self.worktrees_dir.join(&worktree_id);

        info!("🌳 Creating auto-worktree for milestone {}", milestone_id);
        debug!("  Branch: {}", branch_name);
        debug!("  Path: {}", worktree_path.display());

        // Check if worktree already exists
        if worktree_path.exists() {
            // Re-attach to existing worktree
            warn!("Worktree already exists, re-attaching");

            // Reconcile plan checkboxes from project root
            self.reconcile_plan_checkboxes(&worktree_path, milestone_id)
                .await?;

            // Store original base if not already stored
            let cell = ORIGINAL_BASE.get_or_init(|| Mutex::new(None));
            let mut guard = cell
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if guard.is_none() {
                *guard = Some(self.project_root.clone());
                debug!("Stored original base: {}", self.project_root.display());
            }
            drop(guard);

            return Ok(Worktree {
                id: worktree_id,
                branch: branch_name,
                path: worktree_path,
                created_at: Utc::now(),
            });
        }

        // Create worktrees directory if it doesn't exist
        fs::create_dir_all(&self.worktrees_dir).await?;

        // Check if branch already exists
        let branch_exists = self.branch_exists(&branch_name)?;

        if branch_exists {
            // Branch exists, create worktree from existing branch
            debug!(
                "Branch {} exists, creating worktree from branch",
                branch_name
            );

            let output = Command::new("git")
                .arg("worktree")
                .arg("add")
                .arg(&worktree_path)
                .arg(&branch_name)
                .current_dir(&self.project_root)
                .output()
                .map_err(|e| {
                    OrchestrationError::Worktree(format!("Failed to create worktree: {e}"))
                })?;

            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                return Err(OrchestrationError::Worktree(format!(
                    "Git worktree add failed: {stderr}"
                )));
            }
        } else {
            // Create new worktree with new branch
            debug!("Creating new worktree with branch {}", branch_name);

            let output = Command::new("git")
                .arg("worktree")
                .arg("add")
                .arg(&worktree_path)
                .arg("-b")
                .arg(&branch_name)
                .current_dir(&self.project_root)
                .output()
                .map_err(|e| {
                    OrchestrationError::Worktree(format!("Failed to create worktree: {e}"))
                })?;

            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                return Err(OrchestrationError::Worktree(format!(
                    "Git worktree add failed: {stderr}"
                )));
            }
        }

        // Sync state from project root to worktree
        self.sync_project_root_to_worktree(&worktree_path, milestone_id)
            .await?;

        // Create lock file
        let lock_path = self.worktrees_dir.join(format!("{worktree_id}.lock"));
        let lock_content = format!(
            "worktree_id: {}\nbranch: {}\npath: {}\ncreated_at: {}\nmilestone_id: {}\n",
            worktree_id,
            branch_name,
            worktree_path.display(),
            Utc::now().to_rfc3339(),
            milestone_id
        );
        fs::write(&lock_path, lock_content).await?;

        // Store original base path
        let cell = ORIGINAL_BASE.get_or_init(|| Mutex::new(None));
        *cell
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(self.project_root.clone());
        debug!("Stored original base: {}", self.project_root.display());

        info!("✅ Auto-worktree created successfully");

        Ok(Worktree {
            id: worktree_id,
            branch: branch_name,
            path: worktree_path,
            created_at: Utc::now(),
        })
    }

    /// Create a new worktree (legacy method for backward compatibility)
    pub async fn create_worktree(&self, worktree_id: &str, branch_name: &str) -> Result<Worktree> {
        let worktree_path = self.worktrees_dir.join(worktree_id);

        // Create worktrees directory if it doesn't exist
        fs::create_dir_all(&self.worktrees_dir).await?;

        // Create git worktree
        let output = Command::new("git")
            .arg("worktree")
            .arg("add")
            .arg(&worktree_path)
            .arg("-b")
            .arg(branch_name)
            .current_dir(&self.project_root)
            .output()
            .map_err(|e| OrchestrationError::Worktree(format!("Failed to create worktree: {e}")))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(OrchestrationError::Worktree(format!(
                "Git worktree add failed: {stderr}"
            )));
        }

        // Create lock file
        let lock_path = self.worktrees_dir.join(format!("{worktree_id}.lock"));
        let lock_content = format!(
            "worktree_id: {}\nbranch: {}\npath: {}\ncreated_at: {}\n",
            worktree_id,
            branch_name,
            worktree_path.display(),
            Utc::now().to_rfc3339()
        );
        fs::write(&lock_path, lock_content).await?;

        Ok(Worktree {
            id: worktree_id.to_string(),
            branch: branch_name.to_string(),
            path: worktree_path,
            created_at: Utc::now(),
        })
    }

    /// Enter a worktree (change directory to it)
    #[allow(clippy::unused_async)]
    pub async fn enter_worktree(&self, worktree: &Worktree) -> Result<PathBuf> {
        if !worktree.path.exists() {
            return Err(OrchestrationError::Worktree(format!(
                "Worktree path does not exist: {}",
                worktree.path.display()
            )));
        }

        debug!("📍 Entering worktree: {}", worktree.path.display());
        Ok(worktree.path.clone())
    }

    /// Commit changes in worktree
    #[allow(clippy::unused_async)]
    pub async fn commit_worktree(&self, worktree: &Worktree, message: &str) -> Result<String> {
        debug!("💾 Committing in worktree: {}", message);

        // Stage all changes
        let output = Command::new("git")
            .arg("add")
            .arg(".")
            .current_dir(&worktree.path)
            .output()
            .map_err(|e| OrchestrationError::Worktree(format!("Failed to stage changes: {e}")))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(OrchestrationError::Worktree(format!(
                "Git add failed: {stderr}"
            )));
        }

        // Commit changes
        let output = Command::new("git")
            .arg("commit")
            .arg("-m")
            .arg(message)
            .current_dir(&worktree.path)
            .output()
            .map_err(|e| OrchestrationError::Worktree(format!("Failed to commit: {e}")))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(OrchestrationError::Worktree(format!(
                "Git commit failed: {stderr}"
            )));
        }

        // Get commit hash
        let output = Command::new("git")
            .arg("rev-parse")
            .arg("HEAD")
            .current_dir(&worktree.path)
            .output()
            .map_err(|e| OrchestrationError::Worktree(format!("Failed to get commit hash: {e}")))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(OrchestrationError::Worktree(format!(
                "Failed to get commit hash: {stderr}"
            )));
        }

        let commit_hash = String::from_utf8(output.stdout)
            .map_err(|e| OrchestrationError::Worktree(format!("Failed to parse commit hash: {e}")))?
            .trim()
            .to_string();

        debug!("✅ Committed: {}", commit_hash);
        Ok(commit_hash)
    }

    /// Sync worktree state back to project root (orchestra-2 pattern)
    ///
    /// Copies .orchestra state files from worktree back to project root
    /// after each task completion.
    pub async fn sync_worktree_to_project_root(
        &self,
        worktree: &Worktree,
        milestone_id: &str,
    ) -> Result<()> {
        let worktree_milestone = worktree
            .path
            .join(".orchestra")
            .join("milestones")
            .join(milestone_id);
        let project_milestone = self
            .project_root
            .join(".orchestra")
            .join("milestones")
            .join(milestone_id);

        if !worktree_milestone.exists() {
            return Ok(());
        }

        debug!("🔄 Syncing worktree state to project root");

        // Copy milestone directory from worktree to project root
        if project_milestone.exists() {
            fs::remove_dir_all(&project_milestone).await?;
        }
        std::fs::create_dir_all(&project_milestone)?;

        copy_directory_recursive(&worktree_milestone, &project_milestone)?;

        debug!("✅ State synced to project root");
        Ok(())
    }

    /// Sync project root state to worktree (orchestra-2 pattern)
    ///
    /// Copies .orchestra state files from project root to worktree
    /// on worktree creation.
    #[allow(clippy::unused_async)]
    async fn sync_project_root_to_worktree(
        &self,
        worktree_path: &Path,
        milestone_id: &str,
    ) -> Result<()> {
        let project_milestone = self
            .project_root
            .join(".orchestra")
            .join("milestones")
            .join(milestone_id);
        let worktree_milestone = worktree_path
            .join(".orchestra")
            .join("milestones")
            .join(milestone_id);

        if !project_milestone.exists() {
            return Ok(());
        }

        debug!("🔄 Syncing project root state to worktree");

        // Create target directory
        std::fs::create_dir_all(&worktree_milestone)?;

        // Copy milestone directory from project root to worktree
        copy_directory_recursive(&project_milestone, &worktree_milestone)?;

        debug!("✅ State synced to worktree");
        Ok(())
    }

    /// Reconcile plan checkboxes from project root to worktree (orchestra-2 pattern)
    ///
    /// Forward-merge checkbox state: if project root has [x] and worktree has [ ],
    /// update worktree to [x]. Never downgrades [x] → [ ].
    ///
    /// This fixes crash recovery where worktree branch HEAD is behind filesystem
    /// state at project root.
    async fn reconcile_plan_checkboxes(
        &self,
        worktree_path: &Path,
        milestone_id: &str,
    ) -> Result<()> {
        let src_milestone = self
            .project_root
            .join(".orchestra")
            .join("milestones")
            .join(milestone_id);
        let dst_milestone = worktree_path
            .join(".orchestra")
            .join("milestones")
            .join(milestone_id);

        if !src_milestone.exists() || !dst_milestone.exists() {
            return Ok(());
        }

        debug!("🔀 Reconciling plan checkboxes");

        // Walk all markdown files in the milestone directory
        let src_files = walk_markdown_files(&src_milestone);

        for src_file in src_files {
            let rel_path = src_file.strip_prefix(&src_milestone).map_err(|e| {
                OrchestrationError::Worktree(format!("Failed to get relative path: {e}"))
            })?;

            let dst_file = dst_milestone.join(rel_path);

            if !dst_file.exists() {
                continue;
            }

            // Read both files
            let src_content = fs::read_to_string(&src_file).await?;
            let dst_content = fs::read_to_string(&dst_file).await?;

            // Forward-merge checkbox states
            let merged = merge_checkbox_states(&dst_content, &src_content);

            // Write back if changed
            if merged != dst_content {
                fs::write(&dst_file, merged).await?;
                debug!("  Updated checkboxes: {:?}", rel_path);
            }
        }

        debug!("✅ Checkboxes reconciled");
        Ok(())
    }

    /// Teardown worktree after completion
    pub async fn teardown_worktree(&self, worktree: &Worktree) -> Result<()> {
        info!("🗑️  Tearing down worktree: {}", worktree.id);

        // Remove worktree
        let output = Command::new("git")
            .arg("worktree")
            .arg("remove")
            .arg(&worktree.path)
            .current_dir(&self.project_root)
            .output()
            .map_err(|e| OrchestrationError::Worktree(format!("Failed to remove worktree: {e}")))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(OrchestrationError::Worktree(format!(
                "Failed to remove worktree: {stderr}"
            )));
        }

        // Remove lock file
        let lock_path = self.worktrees_dir.join(format!("{}.lock", worktree.id));
        if lock_path.exists() {
            fs::remove_file(&lock_path).await?;
        }

        // Clear original base
        if let Some(cell) = ORIGINAL_BASE.get() {
            *cell
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner) = None;
        }

        info!("✅ Worktree torn down");
        Ok(())
    }

    /// Merge worktree back to main branch
    #[allow(clippy::unused_async)]
    pub async fn merge_to_main(&self, worktree: &Worktree, main_branch: &str) -> Result<()> {
        info!(
            "🔀 Merging worktree branch {} to {}",
            worktree.branch, main_branch
        );

        // Switch to main branch in project root
        let output = Command::new("git")
            .arg("checkout")
            .arg(main_branch)
            .current_dir(&self.project_root)
            .output()
            .map_err(|e| OrchestrationError::Worktree(format!("Failed to checkout main: {e}")))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(OrchestrationError::Worktree(format!(
                "Failed to checkout main branch: {stderr}"
            )));
        }

        // Merge worktree branch into main
        let output = Command::new("git")
            .arg("merge")
            .arg("--squash")
            .arg(&worktree.branch)
            .current_dir(&self.project_root)
            .output()
            .map_err(|e| OrchestrationError::Worktree(format!("Failed to merge: {e}")))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(OrchestrationError::Worktree(format!(
                "Git merge failed: {stderr}"
            )));
        }

        info!("✅ Merged successfully");
        Ok(())
    }

    /// Check if a worktree lock exists
    #[allow(clippy::unused_async)]
    pub async fn has_active_worktree(&self, worktree_id: &str) -> bool {
        let lock_path = self.worktrees_dir.join(format!("{worktree_id}.lock"));
        lock_path.exists()
    }

    /// Get all active worktrees
    pub async fn list_active_worktrees(&self) -> Result<Vec<String>> {
        let mut worktrees = Vec::new();

        let mut entries = fs::read_dir(&self.worktrees_dir).await?;
        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("lock") {
                let worktree_id = path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("")
                    .to_string();
                worktrees.push(worktree_id);
            }
        }

        Ok(worktrees)
    }

    /// Check if a branch exists
    fn branch_exists(&self, branch_name: &str) -> Result<bool> {
        let output = Command::new("git")
            .arg("branch")
            .arg("--list")
            .arg(branch_name)
            .current_dir(&self.project_root)
            .output()
            .map_err(|e| OrchestrationError::Worktree(format!("Failed to list branches: {e}")))?;

        Ok(output.status.success() && !String::from_utf8_lossy(&output.stdout).trim().is_empty())
    }
}

/// Walk all markdown files in a directory recursively
fn walk_markdown_files(dir: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();

    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                files.extend(walk_markdown_files(&path));
            } else if path.extension().and_then(|s| s.to_str()) == Some("md") {
                files.push(path);
            }
        }
    }

    files
}

/// Copy directory recursively (uses `std::fs` for synchronous copy)
fn copy_directory_recursive(src: &Path, dst: &Path) -> Result<()> {
    if !src.exists() {
        return Ok(());
    }

    if src.is_dir() {
        std::fs::create_dir_all(dst)?;

        if let Ok(entries) = std::fs::read_dir(src) {
            for entry in entries.flatten() {
                let src_path = entry.path();
                let dst_path = dst.join(entry.file_name());
                copy_directory_recursive(&src_path, &dst_path)?;
            }
        }
    } else {
        std::fs::copy(src, dst)?;
    }

    Ok(())
}

/// Merge checkbox states from source into destination
///
/// Forward-only: if source has [x] and destination has [ ], update to [x].
/// Never downgrades [x] → [ ].
fn merge_checkbox_states(destination: &str, source: &str) -> String {
    let mut result = String::new();

    let dest_lines: Vec<&str> = destination.lines().collect();
    let src_lines: Vec<&str> = source.lines().collect();

    // Track checkbox states from source
    let mut src_checkboxes: HashSet<String> = HashSet::new();

    for line in &src_lines {
        if let Some(cap) = CHECKED_CHECKBOX_RE.captures(line) {
            if let Some(text) = cap.get(1) {
                src_checkboxes.insert(text.as_str().to_string());
            }
        }
    }

    // Update destination if source has [x]
    for line in dest_lines {
        let updated_line = UNCHECKED_CHECKBOX_RE.captures(line).map_or_else(
            || line.to_string(),
            |cap| {
                cap.get(1).map_or_else(
                    || line.to_string(),
                    |text| {
                        if src_checkboxes.contains(text.as_str()) {
                            line.replace("- [ ]", "- [x]")
                        } else {
                            line.to_string()
                        }
                    },
                )
            },
        );

        result.push_str(&updated_line);
        result.push('\n');
    }

    result
}

/// Git worktree
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Worktree {
    /// Worktree ID
    pub id: String,
    /// Branch name
    pub branch: String,
    /// Path to worktree
    pub path: PathBuf,
    /// When worktree was created
    pub created_at: chrono::DateTime<Utc>,
}

/// Worktree lock file
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorktreeLock {
    pub worktree_id: String,
    /// Branch name
    pub branch: String,
    /// Path to worktree
    pub path: PathBuf,
    /// When lock was created
    pub created_at: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_auto_worktree_branch() {
        assert_eq!(auto_worktree_branch("M01"), "milestone/M01");
        assert_eq!(auto_worktree_branch("M02"), "milestone/M02");
    }

    #[test]
    fn test_merge_checkbox_states() {
        let dest = "- [ ] Task 1\n- [x] Task 2\n- [ ] Task 3";
        let src = "- [x] Task 1\n- [x] Task 2\n- [ ] Task 4";

        let merged = merge_checkbox_states(dest, src);

        // Task 1 should be updated to [x]
        assert!(merged.contains("- [x] Task 1"));
        // Task 2 should remain [x]
        assert!(merged.contains("- [x] Task 2"));
        // Task 3 should remain [ ]
        assert!(merged.contains("- [ ] Task 3"));
    }

    #[test]
    fn test_worktree_manager_creation() {
        let manager = WorktreeManager::new(PathBuf::from("/tmp/test"));
        assert_eq!(
            manager.worktrees_dir,
            PathBuf::from("/tmp/test/.orchestra/worktrees")
        );
    }
}
