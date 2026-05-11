//! Parallel worktree executor — runs multiple agents in isolated git worktrees.
//!
//! Takes a list of independent tasks, creates a worktree per task,
//! spawns an agent in each worktree, collects results, and merges changes back.
//!
//! # Architecture
//!
//! 1. **Conflict Detection**: Before execution, checks if any two tasks target
//!    overlapping files and reports warnings.
//! 2. **Worktree Isolation**: Each task gets its own git worktree so agents can
//!    work without interfering with each other.
//! 3. **Parallel Execution**: Uses [`tokio::task::JoinSet`] with a concurrency
//!    semaphore to respect `max_agents` limits.
//! 4. **Merge-Back**: After execution, results are merged back to the main
//!    branch using the configured [`MergeStrategy`].

use crate::error::ExecutionError;
use crate::git_worktree::{WorktreeConfig, WorktreeManager, WorktreeType};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::process::Command;
use tokio::sync::Semaphore;
use tracing::{debug, info, warn};

// Configuration

/// Configuration for parallel execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParallelConfig {
    /// Maximum number of agents running concurrently
    pub max_agents: usize,
    /// How to merge completed worktree branches back into main
    pub merge_strategy: MergeStrategy,
    /// Remove worktrees after a successful merge
    pub auto_cleanup: bool,
    /// Prefix used for worktree branch names (e.g. `parallel/task-1`)
    pub worktree_prefix: String,
}

impl Default for ParallelConfig {
    fn default() -> Self {
        Self {
            max_agents: 4,
            merge_strategy: MergeStrategy::Merge,
            auto_cleanup: true,
            worktree_prefix: "parallel".to_string(),
        }
    }
}

/// Strategy for merging a completed worktree branch back to main.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[non_exhaustive]
pub enum MergeStrategy {
    /// Cherry-pick individual commits from the worktree branch
    CherryPick,
    /// Merge the worktree branch with a merge commit
    Merge,
    /// Rebase the worktree branch onto main, then fast-forward
    Rebase,
}

// Task types

/// A task to be executed in isolation inside its own worktree.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IsolatedTask {
    /// Unique identifier for this task
    pub id: String,
    /// Human-readable description
    pub description: String,
    /// Files this task is expected to touch (used for conflict detection)
    pub target_files: Vec<String>,
    /// The prompt that would be sent to an agent
    pub prompt: String,
}

/// Status of a single parallel task after execution.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[non_exhaustive]
pub enum TaskExecutionStatus {
    /// Task completed successfully
    Completed,
    /// Task failed during execution
    Failed,
    /// A merge conflict was detected when merging back
    Conflict,
    /// The task exceeded its time budget
    Timeout,
}

/// Result of a single parallel task.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskResult {
    /// The task identifier
    pub task_id: String,
    /// Path to the worktree where the task ran
    pub worktree_path: PathBuf,
    /// Final execution status
    pub status: TaskExecutionStatus,
    /// The commit SHA produced by the agent (if any)
    pub commit_sha: Option<String>,
    /// Files that were changed in this worktree
    pub files_changed: Vec<String>,
    /// Error message if the task failed
    pub error: Option<String>,
}

// Conflict detection

/// Warning about file-level overlap between two tasks.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConflictWarning {
    /// ID of the first task
    pub task_a: String,
    /// ID of the second task
    pub task_b: String,
    /// Files that both tasks declare as targets
    pub overlapping_files: Vec<String>,
}

// Merge report

/// Summary of merging all parallel results back to main.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MergeReport {
    /// Number of tasks merged successfully
    pub successful: usize,
    /// Number of tasks that produced merge conflicts
    pub conflicts: usize,
    /// Number of tasks that failed (not merged)
    pub failed: usize,
    /// Per-task merge details
    pub details: Vec<TaskMergeDetail>,
}

/// Merge outcome for a single task.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskMergeDetail {
    /// Task identifier
    pub task_id: String,
    /// Whether the merge succeeded
    pub merged: bool,
    /// Conflict description if one occurred
    pub conflict_description: Option<String>,
}

// Executor

/// The main parallel worktree executor.
///
/// Coordinates git worktree creation, parallel task execution, and result
/// merging.
pub struct ParallelWorktreeExecutor {
    worktree_manager: WorktreeManager,
    config: ParallelConfig,
    repo_path: PathBuf,
}

impl std::fmt::Debug for ParallelWorktreeExecutor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ParallelWorktreeExecutor")
            .field("config", &self.config)
            .field("repo_path", &self.repo_path)
            .finish_non_exhaustive()
    }
}

impl ParallelWorktreeExecutor {
    pub fn new(repo_path: PathBuf, config: ParallelConfig) -> Result<Self, ExecutionError> {
        if !repo_path.exists() {
            return Err(ExecutionError::RepoNotFound(repo_path));
        }

        let worktree_config = WorktreeConfig {
            max_concurrent_worktrees: config.max_agents.max(1),
            ..WorktreeConfig::default()
        };

        let worktree_manager =
            WorktreeManager::new(repo_path.clone(), worktree_config).map_err(|e| {
                ExecutionError::Custom(format!("Failed to create WorktreeManager: {}", e))
            })?;

        Ok(Self {
            worktree_manager,
            config,
            repo_path,
        })
    }

    // -----------------------------------------------------------------------
    // Public API
    // -----------------------------------------------------------------------

    /// Execute multiple tasks in parallel, each in its own worktree.
    ///
    /// Respects the `max_agents` concurrency limit via a semaphore.
    /// Returns one [`TaskResult`] per task in the same order as the input.
    pub async fn execute_tasks(
        &self,
        tasks: Vec<IsolatedTask>,
    ) -> Result<Vec<TaskResult>, ExecutionError> {
        if tasks.is_empty() {
            return Ok(Vec::new());
        }

        let semaphore = std::sync::Arc::new(Semaphore::new(self.config.max_agents));
        let mut join_set = tokio::task::JoinSet::new();

        // Spawn one task per IsolatedTask
        for task in &tasks {
            let permit = semaphore.clone();
            let task = task.clone();
            let repo_path = self.repo_path.clone();
            let prefix = self.config.worktree_prefix.clone();

            join_set.spawn(async move {
                let _permit = if let Ok(p) = permit.acquire().await {
                    p
                } else {
                    warn!("Semaphore closed, cancelling task {}", task.id);
                    return TaskResult {
                        task_id: task.id.clone(),
                        worktree_path: PathBuf::new(),
                        status: TaskExecutionStatus::Failed,
                        commit_sha: None,
                        files_changed: vec![],
                        error: Some("semaphore closed".to_string()),
                    };
                };

                Self::run_task_in_worktree(&repo_path, &prefix, &task).await
            });
        }

        // Collect results preserving insertion order via a map
        let mut results_map: HashMap<String, TaskResult> = HashMap::new();
        while let Some(join_result) = join_set.join_next().await {
            match join_result {
                Ok(task_result) => {
                    let id = task_result.task_id.clone();
                    results_map.insert(id, task_result);
                }
                Err(join_err) => {
                    warn!("Parallel task panicked: {}", join_err);
                }
            }
        }

        // Re-order to match input
        let results: Vec<TaskResult> = tasks
            .iter()
            .filter_map(|t| results_map.remove(&t.id))
            .collect();

        info!(
            "Parallel execution complete: {} tasks, {} succeeded, {} failed",
            results.len(),
            results
                .iter()
                .filter(|r| r.status == TaskExecutionStatus::Completed)
                .count(),
            results
                .iter()
                .filter(|r| r.status != TaskExecutionStatus::Completed)
                .count(),
        );

        Ok(results)
    }

    /// Execute a single task in a new worktree.
    ///
    /// This is the per-task workhorse used by [`Self::execute_tasks`], but can also
    /// be called directly when you only need one isolated task.
    pub async fn execute_task(&self, task: &IsolatedTask) -> Result<TaskResult, ExecutionError> {
        Ok(Self::run_task_in_worktree(&self.repo_path, &self.config.worktree_prefix, task).await)
    }

    /// Merge completed task results back to the main branch.
    ///
    /// Only tasks with status [`TaskExecutionStatus::Completed`] are merged.
    /// The merge strategy is determined by [`ParallelConfig::merge_strategy`].
    pub async fn merge_results(
        &self,
        results: &[TaskResult],
    ) -> Result<MergeReport, ExecutionError> {
        let mut successful = 0usize;
        let mut conflicts = 0usize;
        let mut failed = 0usize;
        let mut details = Vec::new();

        for result in results {
            if result.status != TaskExecutionStatus::Completed {
                failed += 1;
                details.push(TaskMergeDetail {
                    task_id: result.task_id.clone(),
                    merged: false,
                    conflict_description: result.error.clone(),
                });
                continue;
            }

            let branch_name = format!("{}/{}", self.config.worktree_prefix, result.task_id);

            match self.merge_branch(&branch_name) {
                Ok(()) => {
                    successful += 1;
                    details.push(TaskMergeDetail {
                        task_id: result.task_id.clone(),
                        merged: true,
                        conflict_description: None,
                    });

                    if self.config.auto_cleanup {
                        if let Err(e) = self.worktree_manager.remove_worktree(&result.task_id).await
                        {
                            warn!(
                                "Failed to auto-cleanup worktree for {}: {}",
                                result.task_id, e
                            );
                        }
                    }
                }
                Err(merge_err) => {
                    conflicts += 1;
                    details.push(TaskMergeDetail {
                        task_id: result.task_id.clone(),
                        merged: false,
                        conflict_description: Some(merge_err.clone()),
                    });
                }
            }
        }

        Ok(MergeReport {
            successful,
            conflicts,
            failed,
            details,
        })
    }

    /// Detect file-level conflicts between tasks before execution.
    ///
    /// Returns a [`ConflictWarning`] for every pair of tasks that share at
    /// least one file in their `target_files` lists.
    pub fn detect_conflicts(&self, tasks: &[IsolatedTask]) -> Vec<ConflictWarning> {
        let mut warnings = Vec::new();

        for i in 0..tasks.len() {
            for j in (i + 1)..tasks.len() {
                let set_a: HashSet<&str> =
                    tasks[i].target_files.iter().map(String::as_str).collect();
                let overlapping: Vec<String> = tasks[j]
                    .target_files
                    .iter()
                    .filter(|f| set_a.contains(f.as_str()))
                    .cloned()
                    .collect();

                if !overlapping.is_empty() {
                    warnings.push(ConflictWarning {
                        task_a: tasks[i].id.clone(),
                        task_b: tasks[j].id.clone(),
                        overlapping_files: overlapping,
                    });
                }
            }
        }

        warnings
    }

    // -----------------------------------------------------------------------
    // Internal helpers
    // -----------------------------------------------------------------------

    /// Run a single task inside a freshly created worktree.
    ///
    /// This creates the worktree, runs a placeholder agent, collects the
    /// resulting diff, and returns a [`TaskResult`].
    async fn run_task_in_worktree(
        repo_path: &Path,
        prefix: &str,
        task: &IsolatedTask,
    ) -> TaskResult {
        let worktree_name = format!("parallel-{}", task.id);
        let branch_name = format!("{}/{}", prefix, task.id);

        // Create a worktree via the manager
        let worktree_config = WorktreeConfig {
            max_concurrent_worktrees: 50, // generous for parallel
            ..WorktreeConfig::default()
        };

        let manager = match WorktreeManager::new(repo_path.to_path_buf(), worktree_config) {
            Ok(m) => m,
            Err(e) => {
                return TaskResult {
                    task_id: task.id.clone(),
                    worktree_path: PathBuf::new(),
                    status: TaskExecutionStatus::Failed,
                    commit_sha: None,
                    files_changed: Vec::new(),
                    error: Some(format!("Failed to create WorktreeManager: {}", e)),
                };
            }
        };

        let worktree = match manager
            .create_worktree(
                worktree_name.clone(),
                branch_name.clone(),
                WorktreeType::Session,
            )
            .await
        {
            Ok(w) => w,
            Err(e) => {
                return TaskResult {
                    task_id: task.id.clone(),
                    worktree_path: PathBuf::new(),
                    status: TaskExecutionStatus::Failed,
                    commit_sha: None,
                    files_changed: Vec::new(),
                    error: Some(format!("Failed to create worktree: {}", e)),
                };
            }
        };

        debug!(
            "Created worktree at {} for task {}",
            worktree.path.display(),
            task.id
        );

        // Placeholder agent execution.
        //
        // In a full implementation this would:
        //   1. Build an agent with the worktree.path as cwd
        //   2. Send task.prompt to the LLM
        //   3. Let the agent run tool calls (file edits, bash, etc.)
        //   4. Collect the final state
        //
        // For now we simulate by creating a marker file and committing it,
        // which exercises the full worktree lifecycle.
        let agent_result = Self::simulate_agent(&worktree.path, task).await;

        // Gather the commit SHA and changed files from the worktree
        let commit_sha = Self::get_head_commit(&worktree.path);
        let files_changed = Self::get_changed_files(&worktree.path);

        let (status, error) = match agent_result {
            Ok(()) => (TaskExecutionStatus::Completed, None),
            Err(e) => (TaskExecutionStatus::Failed, Some(e)),
        };

        TaskResult {
            task_id: task.id.clone(),
            worktree_path: worktree.path,
            status,
            commit_sha,
            files_changed,
            error,
        }
    }

    /// Placeholder agent: writes a marker file and commits.
    ///
    /// This exercises the git operations so tests can verify real commits,
    /// but does not invoke an LLM.
    async fn simulate_agent(
        worktree_path: &PathBuf,
        task: &IsolatedTask,
    ) -> Result<(), ExecutionError> {
        let marker_path = worktree_path.join(".parallel-task");
        tokio::fs::write(&marker_path, &task.prompt)
            .await
            .map_err(|e| ExecutionError::Custom(format!("Failed to write marker: {}", e)))?;

        // git add + commit in the worktree
        let add_output = Command::new("git")
            .arg("add")
            .arg(".parallel-task")
            .current_dir(worktree_path)
            .output()
            .map_err(|e| ExecutionError::GitAddFailed(format!("git add failed: {}", e)))?;

        if !add_output.status.success() {
            return Err(ExecutionError::GitAddFailed(
                String::from_utf8_lossy(&add_output.stderr).to_string(),
            ));
        }

        let commit_output = Command::new("git")
            .arg("commit")
            .arg("-m")
            .arg(format!("parallel task: {}", task.id))
            .current_dir(worktree_path)
            .output()
            .map_err(|e| ExecutionError::GitCommitFailed(format!("git commit failed: {}", e)))?;

        if !commit_output.status.success() {
            // Not a failure if there is nothing to commit
            let stderr = String::from_utf8_lossy(&commit_output.stderr);
            if stderr.contains("nothing to commit") {
                return Ok(());
            }
            return Err(ExecutionError::GitCommitFailed(stderr.to_string()));
        }

        Ok(())
    }

    /// Merge a branch back into the current HEAD of the main repo.
    fn merge_branch(&self, branch_name: &str) -> Result<(), ExecutionError> {
        let result = match self.config.merge_strategy {
            MergeStrategy::CherryPick => {
                // Find the commits unique to this branch
                let log_output = Command::new("git")
                    .arg("log")
                    .arg("--format=%H")
                    .arg(format!("HEAD..{}", branch_name))
                    .current_dir(&self.repo_path)
                    .output()
                    .map_err(|e| ExecutionError::GitLogFailed(format!("git log failed: {}", e)))?;

                if !log_output.status.success() {
                    return Err(ExecutionError::GitLogFailed(
                        String::from_utf8_lossy(&log_output.stderr).to_string(),
                    ));
                }

                let log_stdout = String::from_utf8_lossy(&log_output.stdout).into_owned();
                let commits: Vec<&str> = log_stdout
                    .lines()
                    .filter(|l| !l.trim().is_empty())
                    .collect();

                for commit in commits {
                    let cp_output = Command::new("git")
                        .arg("cherry-pick")
                        .arg(commit.trim())
                        .current_dir(&self.repo_path)
                        .output()
                        .map_err(|e| ExecutionError::CherryPickConflict {
                            branch: commit.trim().to_string(),
                            details: format!("git cherry-pick failed: {}", e),
                        })?;

                    if !cp_output.status.success() {
                        // Abort the cherry-pick to leave the tree clean
                        let _ = Command::new("git")
                            .arg("cherry-pick")
                            .arg("--abort")
                            .current_dir(&self.repo_path)
                            .output();
                        return Err(ExecutionError::CherryPickConflict {
                            branch: commit.trim().to_string(),
                            details: String::from_utf8_lossy(&cp_output.stderr).to_string(),
                        });
                    }
                }
                Ok(())
            }
            MergeStrategy::Merge => {
                let output = Command::new("git")
                    .arg("merge")
                    .arg("--no-ff")
                    .arg(branch_name)
                    .arg("-m")
                    .arg(format!("Merge parallel branch {}", branch_name))
                    .current_dir(&self.repo_path)
                    .output()
                    .map_err(|e| {
                        ExecutionError::MergeConflict(format!("git merge failed: {}", e))
                    })?;

                if output.status.success() {
                    Ok(())
                } else {
                    // Abort to leave the tree clean
                    let _ = Command::new("git")
                        .arg("merge")
                        .arg("--abort")
                        .current_dir(&self.repo_path)
                        .output();
                    Err(ExecutionError::MergeConflict(
                        String::from_utf8_lossy(&output.stderr).to_string(),
                    ))
                }
            }
            MergeStrategy::Rebase => {
                // Rebase the worktree branch onto the current branch, then
                // fast-forward the original branch to the rebased tip.
                //
                // We cannot `git rebase <upstream> <branch_name>` directly
                // because branch_name is already checked out in a worktree,
                // and git refuses to check it out twice.  Instead, we create
                // a temporary branch, rebase that, then fast-forward.

                // 1. Capture the current branch name
                let current_branch_output = Command::new("git")
                    .args(["rev-parse", "--abbrev-ref", "HEAD"])
                    .current_dir(&self.repo_path)
                    .output()
                    .map_err(|e| ExecutionError::Custom(format!("git rev-parse failed: {}", e)))?;
                let current_branch = String::from_utf8_lossy(&current_branch_output.stdout)
                    .trim()
                    .to_string();

                // 2. Create a temporary branch from the worktree branch
                let temp_branch = format!("{}-rebase-temp", branch_name);
                let _ = Command::new("git")
                    .arg("branch")
                    .arg("-D")
                    .arg(&temp_branch)
                    .current_dir(&self.repo_path)
                    .output(); // ignore error (may not exist)

                let branch_output = Command::new("git")
                    .arg("branch")
                    .arg(&temp_branch)
                    .arg(branch_name)
                    .current_dir(&self.repo_path)
                    .output()
                    .map_err(|e| {
                        ExecutionError::TempBranchFailed(format!("git branch create failed: {}", e))
                    })?;

                if !branch_output.status.success() {
                    return Err(ExecutionError::TempBranchFailed(
                        String::from_utf8_lossy(&branch_output.stderr).to_string(),
                    ));
                }

                // 3. Rebase temp_branch onto the current branch
                let output = Command::new("git")
                    .arg("rebase")
                    .arg(&current_branch)
                    .arg(&temp_branch)
                    .current_dir(&self.repo_path)
                    .output()
                    .map_err(|e| {
                        ExecutionError::RebaseConflict(format!("git rebase failed: {}", e))
                    })?;

                if output.status.success() {
                    // 4. Switch back to the original branch
                    let checkout_output = Command::new("git")
                        .arg("checkout")
                        .arg(&current_branch)
                        .current_dir(&self.repo_path)
                        .output()
                        .map_err(|e| ExecutionError::CheckoutFailed {
                            branch: current_branch.clone(),
                            details: format!("git checkout failed: {}", e),
                        })?;

                    if !checkout_output.status.success() {
                        let branch_cleanup = Command::new("git")
                            .arg("branch")
                            .arg("-D")
                            .arg(&temp_branch)
                            .current_dir(&self.repo_path)
                            .output();
                        if let Err(e) = branch_cleanup {
                            warn!(
                                "Failed to clean up temp branch after checkout failure: {}",
                                e
                            );
                        }
                        return Err(ExecutionError::CheckoutFailed {
                            branch: current_branch,
                            details: String::from_utf8_lossy(&checkout_output.stderr).to_string(),
                        });
                    }

                    // 5. Fast-forward the original branch to the rebased tip
                    let ff_output = Command::new("git")
                        .arg("merge")
                        .arg("--ff-only")
                        .arg(&temp_branch)
                        .current_dir(&self.repo_path)
                        .output()
                        .map_err(|e| {
                            ExecutionError::FastForwardFailed(format!("git ff merge failed: {}", e))
                        })?;

                    // 6. Clean up the temporary branch
                    let _ = Command::new("git")
                        .arg("branch")
                        .arg("-d")
                        .arg(&temp_branch)
                        .current_dir(&self.repo_path)
                        .output();

                    if ff_output.status.success() {
                        Ok(())
                    } else {
                        Err(ExecutionError::FastForwardFailed(
                            String::from_utf8_lossy(&ff_output.stderr).to_string(),
                        ))
                    }
                } else {
                    let _ = Command::new("git")
                        .arg("rebase")
                        .arg("--abort")
                        .current_dir(&self.repo_path)
                        .output();
                    // Clean up temp branch on failure
                    let _ = Command::new("git")
                        .arg("branch")
                        .arg("-D")
                        .arg(&temp_branch)
                        .current_dir(&self.repo_path)
                        .output();
                    Err(ExecutionError::RebaseConflict(
                        String::from_utf8_lossy(&output.stderr).to_string(),
                    ))
                }
            }
        };

        result
    }

    /// Get the HEAD commit SHA from a directory.
    fn get_head_commit(dir: &PathBuf) -> Option<String> {
        let output = Command::new("git")
            .arg("rev-parse")
            .arg("HEAD")
            .current_dir(dir)
            .output()
            .ok()?;

        if output.status.success() {
            Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
        } else {
            None
        }
    }

    /// Get the list of files changed in the worktree relative to its parent.
    fn get_changed_files(dir: &PathBuf) -> Vec<String> {
        let output = match Command::new("git")
            .arg("diff")
            .arg("--name-only")
            .arg("HEAD~1")
            .current_dir(dir)
            .output()
        {
            Ok(o) => o,
            Err(_) => return Vec::new(),
        };

        if output.status.success() {
            String::from_utf8_lossy(&output.stdout)
                .lines()
                .filter(|l| !l.trim().is_empty())
                .map(ToString::to_string)
                .collect()
        } else {
            Vec::new()
        }
    }
}

// Tests

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    // -----------------------------------------------------------------------
    // Helpers
    // -----------------------------------------------------------------------

    /// Create a temporary git repo with an initial commit.
    fn create_test_repo() -> (TempDir, PathBuf) {
        let temp_dir = TempDir::new().unwrap();
        let repo_path = temp_dir.path().to_path_buf();

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

        std::fs::write(repo_path.join("README.md"), "# Test").unwrap();
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

        (temp_dir, repo_path)
    }

    /// Build a default `ParallelConfig` for tests.
    fn test_config() -> ParallelConfig {
        ParallelConfig {
            max_agents: 4,
            merge_strategy: MergeStrategy::Merge,
            auto_cleanup: true,
            worktree_prefix: "par-test".to_string(),
        }
    }

    fn make_task(id: &str, files: &[&str]) -> IsolatedTask {
        IsolatedTask {
            id: id.to_string(),
            description: format!("Task {}", id),
            target_files: files.iter().map(|s| s.to_string()).collect(),
            prompt: format!("Do task {}", id),
        }
    }

    // -----------------------------------------------------------------------
    // Struct / enum tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_parallel_config_default() {
        let config = ParallelConfig::default();
        assert_eq!(config.max_agents, 4);
        assert_eq!(config.merge_strategy, MergeStrategy::Merge);
        assert!(config.auto_cleanup);
        assert_eq!(config.worktree_prefix, "parallel");
    }

    #[test]
    fn test_merge_strategy_variants() {
        assert_ne!(MergeStrategy::CherryPick, MergeStrategy::Merge);
        assert_ne!(MergeStrategy::Merge, MergeStrategy::Rebase);
        assert_ne!(MergeStrategy::CherryPick, MergeStrategy::Rebase);
    }

    #[test]
    fn test_isolated_task_creation() {
        let task = make_task("t1", &["src/a.rs", "src/b.rs"]);
        assert_eq!(task.id, "t1");
        assert_eq!(task.target_files.len(), 2);
        assert!(task.prompt.contains("t1"));
    }

    #[test]
    fn test_task_execution_status_equality() {
        assert_eq!(
            TaskExecutionStatus::Completed,
            TaskExecutionStatus::Completed
        );
        assert_ne!(TaskExecutionStatus::Completed, TaskExecutionStatus::Failed);
        assert_ne!(TaskExecutionStatus::Conflict, TaskExecutionStatus::Timeout);
    }

    #[test]
    fn test_task_result_fields() {
        let result = TaskResult {
            task_id: "r1".to_string(),
            worktree_path: PathBuf::from("/tmp/wt"),
            status: TaskExecutionStatus::Completed,
            commit_sha: Some("abc123".to_string()),
            files_changed: vec!["src/a.rs".to_string()],
            error: None,
        };
        assert_eq!(result.task_id, "r1");
        assert!(result.commit_sha.is_some());
        assert!(result.error.is_none());
    }

    // -----------------------------------------------------------------------
    // Conflict detection tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_detect_no_conflicts() {
        let (_temp, repo) = create_test_repo();
        let executor = ParallelWorktreeExecutor::new(repo, test_config()).unwrap();

        let tasks = vec![
            make_task("t1", &["src/a.rs"]),
            make_task("t2", &["src/b.rs"]),
            make_task("t3", &["src/c.rs"]),
        ];

        let warnings = executor.detect_conflicts(&tasks);
        assert!(warnings.is_empty());
    }

    #[test]
    fn test_detect_single_conflict() {
        let (_temp, repo) = create_test_repo();
        let executor = ParallelWorktreeExecutor::new(repo, test_config()).unwrap();

        let tasks = vec![
            make_task("t1", &["src/a.rs", "src/shared.rs"]),
            make_task("t2", &["src/b.rs", "src/shared.rs"]),
        ];

        let warnings = executor.detect_conflicts(&tasks);
        assert_eq!(warnings.len(), 1);
        assert_eq!(warnings[0].task_a, "t1");
        assert_eq!(warnings[0].task_b, "t2");
        assert_eq!(warnings[0].overlapping_files, vec!["src/shared.rs"]);
    }

    #[test]
    fn test_detect_multiple_conflicts() {
        let (_temp, repo) = create_test_repo();
        let executor = ParallelWorktreeExecutor::new(repo, test_config()).unwrap();

        let tasks = vec![
            make_task("t1", &["src/a.rs", "src/common.rs"]),
            make_task("t2", &["src/common.rs"]),
            make_task("t3", &["src/a.rs", "src/common.rs"]),
        ];

        let warnings = executor.detect_conflicts(&tasks);
        // t1-t2 overlap on common.rs, t1-t3 overlap on a.rs + common.rs,
        // t2-t3 overlap on common.rs
        assert_eq!(warnings.len(), 3);
    }

    #[test]
    fn test_detect_conflicts_empty_tasks() {
        let (_temp, repo) = create_test_repo();
        let executor = ParallelWorktreeExecutor::new(repo, test_config()).unwrap();

        let warnings = executor.detect_conflicts(&[]);
        assert!(warnings.is_empty());
    }

    #[test]
    fn test_detect_conflicts_single_task() {
        let (_temp, repo) = create_test_repo();
        let executor = ParallelWorktreeExecutor::new(repo, test_config()).unwrap();

        let warnings = executor.detect_conflicts(&[make_task("t1", &["a.rs"])]);
        assert!(warnings.is_empty());
    }

    // -----------------------------------------------------------------------
    // Executor creation tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_executor_creation_valid_repo() {
        let (_temp, repo) = create_test_repo();
        let executor = ParallelWorktreeExecutor::new(repo, test_config());
        assert!(executor.is_ok());
    }

    #[test]
    fn test_executor_creation_invalid_path() {
        let executor =
            ParallelWorktreeExecutor::new(PathBuf::from("/nonexistent/path"), test_config());
        assert!(executor.is_err());
        assert!(executor.unwrap_err().contains("does not exist"));
    }

    // -----------------------------------------------------------------------
    // Worktree per task tests (with real git)
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_execute_single_task() {
        let (_temp, repo) = create_test_repo();
        let executor = ParallelWorktreeExecutor::new(repo.clone(), test_config()).unwrap();

        let task = make_task("single-1", &["src/lib.rs"]);
        let result = executor.execute_task(&task).await.unwrap();

        assert_eq!(result.task_id, "single-1");
        assert_eq!(result.status, TaskExecutionStatus::Completed);
        assert!(result.worktree_path.exists());
        assert!(result.commit_sha.is_some());
        assert!(result.error.is_none());
    }

    #[tokio::test]
    async fn test_execute_multiple_tasks_parallel() {
        let (_temp, repo) = create_test_repo();
        let executor = ParallelWorktreeExecutor::new(repo.clone(), test_config()).unwrap();

        let tasks = vec![
            make_task("p1", &["src/a.rs"]),
            make_task("p2", &["src/b.rs"]),
            make_task("p3", &["src/c.rs"]),
        ];

        let results = executor.execute_tasks(tasks).await.unwrap();
        assert_eq!(results.len(), 3);

        for result in &results {
            assert_eq!(result.status, TaskExecutionStatus::Completed);
            assert!(result.commit_sha.is_some());
        }
    }

    #[tokio::test]
    async fn test_execute_empty_tasks() {
        let (_temp, repo) = create_test_repo();
        let executor = ParallelWorktreeExecutor::new(repo, test_config()).unwrap();

        let results = executor.execute_tasks(vec![]).await.unwrap();
        assert!(results.is_empty());
    }

    #[tokio::test]
    async fn test_execute_tasks_respects_max_agents() {
        let (_temp, repo) = create_test_repo();
        let config = ParallelConfig {
            max_agents: 2,
            ..test_config()
        };
        let executor = ParallelWorktreeExecutor::new(repo.clone(), config).unwrap();

        // Submit 5 tasks but only 2 should run concurrently
        let tasks: Vec<IsolatedTask> = (0..5)
            .map(|i| {
                let file = format!("src/f{}.rs", i);
                make_task(&format!("lim-{}", i), &[file.as_str()])
            })
            .collect();

        let results = executor.execute_tasks(tasks).await.unwrap();
        assert_eq!(results.len(), 5);

        for result in &results {
            assert_eq!(result.status, TaskExecutionStatus::Completed);
        }
    }

    // -----------------------------------------------------------------------
    // Merge report tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_merge_report_construction() {
        let report = MergeReport {
            successful: 3,
            conflicts: 1,
            failed: 0,
            details: vec![
                TaskMergeDetail {
                    task_id: "t1".into(),
                    merged: true,
                    conflict_description: None,
                },
                TaskMergeDetail {
                    task_id: "t2".into(),
                    merged: false,
                    conflict_description: Some("merge conflict in src/a.rs".into()),
                },
            ],
        };
        assert_eq!(report.successful, 3);
        assert_eq!(report.conflicts, 1);
        assert_eq!(report.details.len(), 2);
    }

    // -----------------------------------------------------------------------
    // Merge integration tests (with real git)
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_merge_results_with_merge_strategy() {
        let (_temp, repo) = create_test_repo();
        let config = ParallelConfig {
            merge_strategy: MergeStrategy::Merge,
            auto_cleanup: false,
            ..test_config()
        };
        let executor = ParallelWorktreeExecutor::new(repo.clone(), config).unwrap();

        let tasks = vec![make_task("merge-1", &["src/x.rs"])];
        let results = executor.execute_tasks(tasks).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].status, TaskExecutionStatus::Completed);

        let report = executor.merge_results(&results).await.unwrap();
        assert_eq!(report.successful, 1);
        assert_eq!(report.conflicts, 0);
        assert!(report.details[0].merged);
    }

    #[tokio::test]
    async fn test_merge_results_skips_failed_tasks() {
        let (_temp, repo) = create_test_repo();
        let executor = ParallelWorktreeExecutor::new(repo.clone(), test_config()).unwrap();

        let failed_result = TaskResult {
            task_id: "fail-1".into(),
            worktree_path: PathBuf::new(),
            status: TaskExecutionStatus::Failed,
            commit_sha: None,
            files_changed: vec![],
            error: Some("something went wrong".into()),
        };

        let report = executor.merge_results(&[failed_result]).await.unwrap();
        assert_eq!(report.successful, 0);
        assert_eq!(report.failed, 1);
        assert!(!report.details[0].merged);
    }

    #[tokio::test]
    async fn test_merge_with_cherry_pick_strategy() {
        let (_temp, repo) = create_test_repo();
        let config = ParallelConfig {
            merge_strategy: MergeStrategy::CherryPick,
            auto_cleanup: false,
            ..test_config()
        };
        let executor = ParallelWorktreeExecutor::new(repo.clone(), config).unwrap();

        let tasks = vec![make_task("cp-1", &["src/y.rs"])];
        let results = executor.execute_tasks(tasks).await.unwrap();

        let report = executor.merge_results(&results).await.unwrap();
        assert_eq!(report.successful, 1);
    }

    #[tokio::test]
    async fn test_merge_with_rebase_strategy() {
        let (_temp, repo) = create_test_repo();
        let config = ParallelConfig {
            merge_strategy: MergeStrategy::Rebase,
            auto_cleanup: false,
            ..test_config()
        };
        let executor = ParallelWorktreeExecutor::new(repo.clone(), config).unwrap();

        let tasks = vec![make_task("rebase-1", &["src/z.rs"])];
        let results = executor.execute_tasks(tasks).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].status, TaskExecutionStatus::Completed);

        // Capture HEAD before merge to verify it advances
        let head_before = ParallelWorktreeExecutor::get_head_commit(&repo).unwrap();

        let report = executor.merge_results(&results).await.unwrap();
        if report.successful != 1 {
            for detail in &report.details {
                eprintln!(
                    "task={} merged={} conflict={:?}",
                    detail.task_id, detail.merged, detail.conflict_description
                );
            }
        }
        assert_eq!(report.successful, 1);
        assert_eq!(report.conflicts, 0);
        assert!(report.details[0].merged);

        // Verify HEAD advanced after the rebase + ff merge
        let head_after = ParallelWorktreeExecutor::get_head_commit(&repo).unwrap();
        assert_ne!(
            head_before, head_after,
            "HEAD should have advanced after rebase merge"
        );
    }

    #[tokio::test]
    async fn test_merge_with_rebase_strategy_preserves_branch() {
        // Verify the rebase strategy returns to the original branch after merging
        let (_temp, repo) = create_test_repo();
        let config = ParallelConfig {
            merge_strategy: MergeStrategy::Rebase,
            auto_cleanup: false,
            worktree_prefix: "rebase-test".to_string(),
            ..test_config()
        };
        let executor = ParallelWorktreeExecutor::new(repo.clone(), config).unwrap();

        // Get the current branch name before merging
        let branch_before = Command::new("git")
            .args(["rev-parse", "--abbrev-ref", "HEAD"])
            .current_dir(&repo)
            .output()
            .unwrap();
        let branch_name = String::from_utf8_lossy(&branch_before.stdout)
            .trim()
            .to_string();

        let tasks = vec![make_task("rb-branch-1", &["src/br.rs"])];
        let results = executor.execute_tasks(tasks).await.unwrap();
        let _ = executor.merge_results(&results).await.unwrap();

        // Verify we're back on the original branch (not detached, not the worktree branch)
        let branch_after = Command::new("git")
            .args(["rev-parse", "--abbrev-ref", "HEAD"])
            .current_dir(&repo)
            .output()
            .unwrap();
        let branch_after_str = String::from_utf8_lossy(&branch_after.stdout)
            .trim()
            .to_string();
        assert_eq!(
            branch_name, branch_after_str,
            "Should be back on original branch after rebase merge, got '{}'",
            branch_after_str
        );
    }

    // -----------------------------------------------------------------------
    // Serialization tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_parallel_config_serialization_roundtrip() {
        let config = test_config();
        let json = serde_json::to_string(&config).unwrap();
        let deserialized: ParallelConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(config.max_agents, deserialized.max_agents);
        assert_eq!(config.merge_strategy, deserialized.merge_strategy);
    }

    #[test]
    fn test_isolated_task_serialization() {
        let task = make_task("s1", &["a.rs", "b.rs"]);
        let json = serde_json::to_string(&task).unwrap();
        let back: IsolatedTask = serde_json::from_str(&json).unwrap();
        assert_eq!(task.id, back.id);
        assert_eq!(task.target_files, back.target_files);
    }

    #[test]
    fn test_task_result_serialization() {
        let result = TaskResult {
            task_id: "tr1".into(),
            worktree_path: PathBuf::from("/tmp/wt"),
            status: TaskExecutionStatus::Completed,
            commit_sha: Some("deadbeef".into()),
            files_changed: vec!["a.rs".into()],
            error: None,
        };
        let json = serde_json::to_string(&result).unwrap();
        let back: TaskResult = serde_json::from_str(&json).unwrap();
        assert_eq!(result.task_id, back.task_id);
        assert_eq!(result.status, back.status);
    }

    #[test]
    fn test_conflict_warning_serialization() {
        let warning = ConflictWarning {
            task_a: "a".into(),
            task_b: "b".into(),
            overlapping_files: vec!["shared.rs".into()],
        };
        let json = serde_json::to_string(&warning).unwrap();
        let back: ConflictWarning = serde_json::from_str(&json).unwrap();
        assert_eq!(warning.overlapping_files, back.overlapping_files);
    }

    #[test]
    fn test_merge_report_serialization() {
        let report = MergeReport {
            successful: 2,
            conflicts: 0,
            failed: 1,
            details: vec![],
        };
        let json = serde_json::to_string(&report).unwrap();
        let back: MergeReport = serde_json::from_str(&json).unwrap();
        assert_eq!(report.successful, back.successful);
    }

    // =========================================================================
    // Terminal-bench: 15 additional tests for parallel_executor
    // =========================================================================

    // 1. MergeStrategy serde roundtrip all variants
    #[test]
    fn merge_strategy_serde_roundtrip() {
        let strategies = [
            MergeStrategy::CherryPick,
            MergeStrategy::Merge,
            MergeStrategy::Rebase,
        ];
        for s in &strategies {
            let json = serde_json::to_string(s).unwrap();
            let decoded: MergeStrategy = serde_json::from_str(&json).unwrap();
            assert_eq!(*s, decoded);
        }
    }

    // 2. TaskExecutionStatus serde roundtrip all variants
    #[test]
    fn task_execution_status_serde_roundtrip() {
        let statuses = [
            TaskExecutionStatus::Completed,
            TaskExecutionStatus::Failed,
            TaskExecutionStatus::Conflict,
            TaskExecutionStatus::Timeout,
        ];
        for s in &statuses {
            let json = serde_json::to_string(s).unwrap();
            let decoded: TaskExecutionStatus = serde_json::from_str(&json).unwrap();
            assert_eq!(*s, decoded);
        }
    }

    // 3. ParallelConfig clone equal
    #[test]
    fn parallel_config_clone_equal() {
        let config = test_config();
        let cloned = config.clone();
        assert_eq!(cloned.max_agents, config.max_agents);
        assert_eq!(cloned.merge_strategy, config.merge_strategy);
        assert_eq!(cloned.auto_cleanup, config.auto_cleanup);
        assert_eq!(cloned.worktree_prefix, config.worktree_prefix);
    }

    // 4. ParallelConfig custom serde roundtrip
    #[test]
    fn parallel_config_custom_serde_roundtrip() {
        let config = ParallelConfig {
            max_agents: 8,
            merge_strategy: MergeStrategy::Rebase,
            auto_cleanup: false,
            worktree_prefix: "wt-custom".into(),
        };
        let json = serde_json::to_string(&config).unwrap();
        let decoded: ParallelConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.max_agents, 8);
        assert_eq!(decoded.merge_strategy, MergeStrategy::Rebase);
        assert!(!decoded.auto_cleanup);
        assert_eq!(decoded.worktree_prefix, "wt-custom");
    }

    // 5. IsolatedTask clone equal
    #[test]
    fn isolated_task_clone_equal() {
        let task = make_task("clone-1", &["a.rs", "b.rs"]);
        let cloned = task.clone();
        assert_eq!(cloned.id, task.id);
        assert_eq!(cloned.description, task.description);
        assert_eq!(cloned.target_files, task.target_files);
        assert_eq!(cloned.prompt, task.prompt);
    }

    // 6. IsolatedTask with empty target files
    #[test]
    fn isolated_task_empty_target_files() {
        let task = IsolatedTask {
            id: "empty".into(),
            description: "No files".into(),
            target_files: vec![],
            prompt: "Do nothing".into(),
        };
        let json = serde_json::to_string(&task).unwrap();
        let decoded: IsolatedTask = serde_json::from_str(&json).unwrap();
        assert!(decoded.target_files.is_empty());
    }

    // 7. TaskResult clone equal
    #[test]
    fn task_result_clone_equal() {
        let result = TaskResult {
            task_id: "tr_clone".into(),
            worktree_path: PathBuf::from("/tmp/wt"),
            status: TaskExecutionStatus::Completed,
            commit_sha: Some("abc123".into()),
            files_changed: vec!["a.rs".into()],
            error: None,
        };
        let cloned = result.clone();
        assert_eq!(cloned.task_id, result.task_id);
        assert_eq!(cloned.status, result.status);
        assert_eq!(cloned.commit_sha, result.commit_sha);
    }

    // 8. TaskResult with Failed status serde roundtrip
    #[test]
    fn task_result_failed_serde_roundtrip() {
        let result = TaskResult {
            task_id: "tr_fail".into(),
            worktree_path: PathBuf::from("/tmp/none"),
            status: TaskExecutionStatus::Failed,
            commit_sha: None,
            files_changed: vec![],
            error: Some("something went wrong".into()),
        };
        let json = serde_json::to_string(&result).unwrap();
        let decoded: TaskResult = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.status, TaskExecutionStatus::Failed);
        assert_eq!(decoded.error, Some("something went wrong".into()));
    }

    // 9. ConflictWarning clone equal
    #[test]
    fn conflict_warning_clone_equal() {
        let warning = ConflictWarning {
            task_a: "t1".into(),
            task_b: "t2".into(),
            overlapping_files: vec!["shared.rs".into(), "common.rs".into()],
        };
        let cloned = warning.clone();
        assert_eq!(cloned.task_a, warning.task_a);
        assert_eq!(cloned.task_b, warning.task_b);
        assert_eq!(cloned.overlapping_files, warning.overlapping_files);
    }

    // 10. MergeReport with all zeros serde roundtrip
    #[test]
    fn merge_report_zero_values_serde() {
        let report = MergeReport {
            successful: 0,
            conflicts: 0,
            failed: 0,
            details: vec![],
        };
        let json = serde_json::to_string(&report).unwrap();
        let decoded: MergeReport = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.successful, 0);
        assert!(decoded.details.is_empty());
    }

    // 11. TaskMergeDetail serde roundtrip
    #[test]
    fn task_merge_detail_serde_roundtrip() {
        let detail = TaskMergeDetail {
            task_id: "merge-1".into(),
            merged: true,
            conflict_description: None,
        };
        let json = serde_json::to_string(&detail).unwrap();
        let decoded: TaskMergeDetail = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.task_id, "merge-1");
        assert!(decoded.merged);
        assert!(decoded.conflict_description.is_none());
    }

    // 12. TaskMergeDetail with conflict serde roundtrip
    #[test]
    fn task_merge_detail_conflict_serde() {
        let detail = TaskMergeDetail {
            task_id: "merge-2".into(),
            merged: false,
            conflict_description: Some("conflict in src/main.rs".into()),
        };
        let json = serde_json::to_string(&detail).unwrap();
        let decoded: TaskMergeDetail = serde_json::from_str(&json).unwrap();
        assert!(!decoded.merged);
        assert_eq!(
            decoded.conflict_description,
            Some("conflict in src/main.rs".into())
        );
    }

    // 13. ParallelConfig debug format
    #[test]
    fn parallel_config_debug_format() {
        let config = test_config();
        let debug = format!("{:?}", config);
        assert!(debug.contains("max_agents"));
        assert!(debug.contains("merge_strategy"));
    }

    // 14. MergeReport debug format
    #[test]
    fn merge_report_debug_format() {
        let report = MergeReport {
            successful: 1,
            conflicts: 0,
            failed: 0,
            details: vec![TaskMergeDetail {
                task_id: "t1".into(),
                merged: true,
                conflict_description: None,
            }],
        };
        let debug = format!("{:?}", report);
        assert!(debug.contains("successful"));
        assert!(debug.contains("details"));
    }

    // 15. Conflict warning with multiple overlapping files
    #[test]
    fn conflict_warning_multiple_files_serde() {
        let warning = ConflictWarning {
            task_a: "alpha".into(),
            task_b: "beta".into(),
            overlapping_files: vec!["a.rs".into(), "b.rs".into(), "c.rs".into()],
        };
        let json = serde_json::to_string(&warning).unwrap();
        let decoded: ConflictWarning = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.overlapping_files.len(), 3);
        assert_eq!(decoded.task_a, "alpha");
    }
}
