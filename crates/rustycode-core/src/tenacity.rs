//! Tenacity loop — self-referential execution that keeps going until done.
//!
//! Wraps the agent execution in a verify-and-retry loop that:
//! 1. Executes the agent with the original task
//! 2. Checks if actual file changes were produced (git diff)
//! 3. If no progress, retries with escalating urgency prompts
//! 4. Each retry uses a fresh prompt injection (not a new session)
//!
//! Only active in `--auto` mode. Max 4 attempts.

use std::path::Path;
use std::time::SystemTime;

/// Outcome of the Tenacity loop.
#[non_exhaustive]
#[derive(Debug)]
pub enum TenacityOutcome {
    /// Task completed — files were changed
    Completed { iterations: usize },
    /// Agent returned Ok but no file changes detected
    NoChanges { iterations: usize },
    /// Agent returned an error
    Failed { reason: String, iterations: usize },
}

const RETRY_PROMPTS: &[&str] = &[
    "The previous attempt did not produce any file changes — you only read/explored \
     without writing anything. IMPORTANT: The working directory already contains files \
     from your previous attempt — do NOT re-clone, re-download, or re-create anything. \
     Use list_dir to check what exists first. \
     You MUST immediately write or modify files. \
     Do NOT start by reading files again — you already know the structure. \
     Take action NOW: use write_file to create files, edit_file to modify code, \
     or bash to run build/install commands.\n\nOriginal task:",
    "Two previous attempts produced no file changes. The working directory already has files \
     from earlier attempts — do NOT clone or download anything. \
     STOP reading and START WRITING. Your first action must be write_file or edit_file. \
     Do NOT read any files first — you have already seen the code. \
     Create or modify the most important file IMMEDIATELY.\n\nOriginal task:",
    "This is your FINAL attempt. Three previous tries produced no file changes. \
     The working directory already has everything you need. \
     Do NOT explore, do NOT read files, do NOT list directories. \
     WRITE CODE NOW. Pick the most critical file and edit it IMMEDIATELY. \
     Your VERY FIRST action must be write_file, edit_file, or bash with a modifying command.\n\nOriginal task:",
];

/// Retry prompts for when changes were made but verification/progress wasn't sufficient.
#[allow(dead_code)] // kept for future retry-with-changes logic
const RETRY_PROMPTS_WITH_CHANGES: &[&str] = &[
    "The previous attempt made file changes but they may not be sufficient. \
     The working directory already contains your changes. \
     IMPORTANT: Do NOT undo or redo what already works. Instead: \
     1. Check what you changed (git diff or read files) \
     2. Identify what's still missing or broken \
     3. Fix ONLY the remaining issues \
     4. Run verification to confirm everything works \
     Your first action should be to CHECK the current state, not to start over.\n\nOriginal task:",
    "Two previous attempts made changes but verification still fails. \
     Do NOT start over — build on what exists. \
     Focus on the SPECIFIC failure: read error messages carefully and fix only what's broken. \
     If a test fails, read the test output and fix the exact error. \
     If a build fails, read the compiler error and fix the exact issue.\n\nOriginal task:",
    "FINAL attempt. Previous changes exist but verification keeps failing. \
     Try a completely different approach to the remaining issue. \
     If your current strategy isn't working, step back and think about what's fundamentally wrong. \
     Sometimes the fix is simpler than you think — re-read the task requirements carefully.\n\nOriginal task:",
];

/// Maximum number of tenacity iterations.
pub const MAX_ITERATIONS: usize = 4;

/// Get the current HEAD commit hash, or None if not a git repo.
pub fn git_head_rev(cwd: &Path) -> Option<String> {
    let output = std::process::Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(cwd)
        .output()
        .ok()?;
    if output.status.success() {
        Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        None
    }
}

/// Files created by RustyCode internally that should not count as task progress.
const INTERNAL_FILES: &[&str] = &[".rustycode_command_history", ".claude/"];

/// Check if any files changed (staged, unstaged, or untracked).
///
/// Returns true if at least one file was created, modified, or deleted,
/// ignoring RustyCode's own internal files.
pub fn has_file_changes(cwd: &Path) -> bool {
    let output = std::process::Command::new("git")
        .args(["status", "--porcelain"])
        .current_dir(cwd)
        .output();

    match output {
        Ok(out) => {
            let status = String::from_utf8_lossy(&out.stdout);
            status.lines().any(|line| {
                let path = line.trim_start_matches(|c: char| {
                    c.is_whitespace() || c == '?' || c == 'A' || c == 'M' || c == 'D'
                });
                !INTERNAL_FILES
                    .iter()
                    .any(|internal| path.starts_with(internal))
            })
        }
        Err(_) => false,
    }
}

/// Check if progress was made during a session.
///
/// Progress means at least one of:
/// - Uncommitted file changes exist (edits, new files, deletions)
/// - New commits were created since `head_before` (e.g. git merge)
pub fn has_progress(cwd: &Path, head_before: &Option<String>) -> bool {
    // Check for uncommitted changes first
    if has_file_changes(cwd) {
        return true;
    }

    // Check if new commits were created
    if let Some(before) = head_before {
        if let Some(after) = git_head_rev(cwd) {
            return before != &after;
        }
    }

    false
}

/// Snapshot of file modification times in a directory.
/// Used as a fallback for non-git directories.
#[derive(Debug)]
pub struct FileMtimeSnapshot {
    mtimes: Vec<(String, SystemTime)>,
}

impl FileMtimeSnapshot {
    /// Take a snapshot of file mtimes in a directory (recursive, depth-limited).
    /// Tracks regular files up to 3 levels deep so that `git clone` creating
    /// subdirectories is detected as progress.
    pub fn take(cwd: &Path) -> Self {
        let mut mtimes = Vec::new();
        Self::collect_recursive(cwd, cwd, 0, &mut mtimes);
        Self { mtimes }
    }

    /// Recursively collect file mtimes up to `MAX_SNAPSHOT_DEPTH` levels.
    fn collect_recursive(
        base: &Path,
        dir: &Path,
        depth: usize,
        mtimes: &mut Vec<(String, SystemTime)>,
    ) {
        const MAX_SNAPSHOT_DEPTH: usize = 3;
        const MAX_FILES_PER_SNAPSHOT: usize = 500;

        if depth > MAX_SNAPSHOT_DEPTH || mtimes.len() >= MAX_FILES_PER_SNAPSHOT {
            return;
        }

        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                if mtimes.len() >= MAX_FILES_PER_SNAPSHOT {
                    break;
                }
                let name = entry.file_name().to_string_lossy().to_string();
                if name.starts_with('.') || INTERNAL_FILES.iter().any(|&f| name.starts_with(f)) {
                    continue;
                }
                if let Ok(metadata) = entry.metadata() {
                    if metadata.is_dir() {
                        // Recurse into subdirectories
                        Self::collect_recursive(base, &entry.path(), depth + 1, mtimes);
                    } else if let Ok(mtime) = metadata.modified() {
                        // Store path relative to base
                        let entry_path = entry.path();
                        let rel = entry_path
                            .strip_prefix(base)
                            .unwrap_or(&entry_path)
                            .to_string_lossy()
                            .to_string();
                        mtimes.push((rel, mtime));
                    }
                }
            }
        }
    }

    /// Check if any files changed since this snapshot.
    /// Compares current state against the snapshot: new files, deleted files,
    /// or modified files all count as changes.
    pub fn has_changes_since(&self, cwd: &Path) -> bool {
        let current = Self::take(cwd);

        // If the number of tracked files changed significantly, that's progress
        // (e.g., git clone added many files)
        let old_count = self.mtimes.len();
        let new_count = current.mtimes.len();
        if new_count > old_count.saturating_add(5) {
            // More than 5 new files → definite progress (e.g., git clone)
            return true;
        }

        for (name, mtime) in &current.mtimes {
            if let Some((_, old_mtime)) = self.mtimes.iter().find(|(n, _)| n == name) {
                if mtime != old_mtime {
                    return true;
                }
            } else {
                // New file that wasn't in the snapshot
                return true;
            }
        }

        for (name, _) in &self.mtimes {
            if !current.mtimes.iter().any(|(n, _)| n == name) {
                return true; // file deleted
            }
        }

        false
    }
}

/// Pre-session state for progress detection. Works with or without git.
pub struct ProgressSnapshot {
    head_before: Option<String>,
    mtime_snapshot: FileMtimeSnapshot,
    /// Initial git status output, used to avoid false positives when the repo
    /// already has uncommitted changes before the agent runs.
    initial_git_status: Option<String>,
}

impl ProgressSnapshot {
    /// Take a snapshot before the agent runs.
    pub fn take(cwd: &Path) -> Self {
        let initial_git_status = std::process::Command::new("git")
            .args(["status", "--porcelain"])
            .current_dir(cwd)
            .output()
            .ok()
            .map(|out| String::from_utf8_lossy(&out.stdout).to_string());
        Self {
            head_before: git_head_rev(cwd),
            mtime_snapshot: FileMtimeSnapshot::take(cwd),
            initial_git_status,
        }
    }

    /// Check if progress was made since the snapshot.
    pub fn has_progress(&self, cwd: &Path) -> bool {
        // Check if git status changed (new files modified/added/removed).
        // Compare against the initial status to avoid false positives from
        // pre-existing uncommitted changes.
        if self.git_status_changed(cwd) {
            return true;
        }
        // Check if new commits were created
        if let Some(ref before) = self.head_before {
            if let Some(after) = git_head_rev(cwd) {
                if before != &after {
                    return true;
                }
            }
        }
        // Check if regular files changed (non-git directories)
        if self.mtime_snapshot.has_changes_since(cwd) {
            return true;
        }
        // Check for subdirectory git repos (e.g., agent cloned into /app/repo).
        // The parent directory's git status won't show changes inside nested repos,
        // so we need to check them explicitly.
        if let Ok(entries) = std::fs::read_dir(cwd) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if name.starts_with('.') {
                    continue;
                }
                if entry.metadata().map(|m| m.is_dir()).unwrap_or(false) {
                    let subdir = entry.path();
                    if subdir.join(".git").exists() && has_file_changes(&subdir) {
                        return true;
                    }
                }
            }
        }
        false
    }

    /// Check if git status changed since the snapshot was taken.
    /// Returns false if git status is the same (even if there are uncommitted changes).
    fn git_status_changed(&self, cwd: &Path) -> bool {
        let current_status = match std::process::Command::new("git")
            .args(["status", "--porcelain"])
            .current_dir(cwd)
            .output()
        {
            Ok(out) => String::from_utf8_lossy(&out.stdout).to_string(),
            Err(_) => return false,
        };

        match &self.initial_git_status {
            Some(initial) => current_status != *initial,
            None => !current_status.trim().is_empty(),
        }
    }
}
///
/// - Iteration 1: original task as-is
/// - Iteration 2: retry prompt + original task
/// - Iteration 3: final attempt prompt + original task
pub fn build_iteration_prompt(original_task: &str, iteration: usize) -> String {
    if iteration <= 1 {
        return original_task.to_string();
    }

    let retry_index = (iteration - 2).min(RETRY_PROMPTS.len() - 1);
    format!("{}\n{}", RETRY_PROMPTS[retry_index], original_task)
}

/// Run the Tenacity loop for `run --auto` mode.
///
/// This is a thin coordination layer that:
/// 1. Calls the provided executor function
/// 2. Checks for file changes after each attempt
/// 3. Returns the outcome
///
/// The actual LLM execution is delegated to the caller via the executor closure.
pub async fn run_tenacity_loop<F, Fut>(
    cwd: &Path,
    original_task: &str,
    mut executor: F,
) -> TenacityOutcome
where
    F: FnMut(String) -> Fut,
    Fut: std::future::Future<Output = Result<(), anyhow::Error>>,
{
    // Capture HEAD before the first iteration so we can detect new commits
    let head_before = git_head_rev(cwd);

    for iteration in 1..=MAX_ITERATIONS {
        let prompt = build_iteration_prompt(original_task, iteration);

        if iteration > 1 {
            tracing::info!(
                iteration,
                max = MAX_ITERATIONS,
                "Tenacity retrying with adjusted approach"
            );
        }

        match executor(prompt).await {
            Ok(()) => {
                if has_progress(cwd, &head_before) {
                    if iteration > 1 {
                        tracing::info!(iteration, max = MAX_ITERATIONS, "Task completed on retry");
                    }
                    return TenacityOutcome::Completed {
                        iterations: iteration,
                    };
                } else {
                    // No changes — check if we should retry
                    if iteration < MAX_ITERATIONS {
                        tracing::warn!(iteration, "No progress detected, will retry");
                        continue;
                    } else {
                        tracing::warn!(
                            max = MAX_ITERATIONS,
                            "No progress detected after all iterations"
                        );
                        return TenacityOutcome::NoChanges {
                            iterations: MAX_ITERATIONS,
                        };
                    }
                }
            }
            Err(e) => {
                if iteration < MAX_ITERATIONS {
                    tracing::warn!(iteration, error = %e, "Iteration failed, will retry");
                    continue;
                } else {
                    return TenacityOutcome::Failed {
                        reason: e.to_string(),
                        iterations: iteration,
                    };
                }
            }
        }
    }

    // Should not reach here, but just in case
    TenacityOutcome::NoChanges {
        iterations: MAX_ITERATIONS,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_first_iteration_uses_original_task() {
        let prompt = build_iteration_prompt("fix the bug", 1);
        assert_eq!(prompt, "fix the bug");
    }

    #[test]
    fn test_second_iteration_adds_retry_prompt() {
        let prompt = build_iteration_prompt("fix the bug", 2);
        assert!(prompt.contains("previous attempt did not produce"));
        assert!(prompt.contains("fix the bug"));
    }

    #[test]
    fn test_third_iteration_adds_escalation_prompt() {
        let prompt = build_iteration_prompt("fix the bug", 3);
        assert!(prompt.contains("STOP reading and START WRITING"));
        assert!(prompt.contains("fix the bug"));
    }

    #[test]
    fn test_fourth_iteration_adds_final_prompt() {
        let prompt = build_iteration_prompt("fix the bug", 4);
        assert!(prompt.contains("FINAL attempt"));
        assert!(prompt.contains("fix the bug"));
    }

    #[test]
    fn test_out_of_range_iteration_clamps() {
        let prompt = build_iteration_prompt("fix the bug", 10);
        assert!(prompt.contains("FINAL attempt"));
        assert!(prompt.contains("fix the bug"));
    }

    #[test]
    fn test_has_file_changes_detects_changes() {
        // This test runs in the project directory which has changes
        let cwd = std::env::current_dir().unwrap();
        // In CI this might be clean, so just verify it doesn't panic
        let _ = has_file_changes(&cwd);
    }

    #[test]
    fn test_git_head_rev_returns_some_in_repo() {
        let cwd = std::env::current_dir().unwrap();
        let rev = git_head_rev(&cwd);
        assert!(rev.is_some());
        // A commit hash is 40 hex chars
        assert_eq!(rev.unwrap().len(), 40);
    }

    #[test]
    fn test_git_head_rev_returns_none_outside_repo() {
        let tmp = tempfile::tempdir().unwrap();
        let rev = git_head_rev(tmp.path());
        assert!(rev.is_none());
    }

    #[test]
    fn test_has_progress_detects_uncommitted_changes() {
        let cwd = std::env::current_dir().unwrap();
        // With None as head_before, has_progress only checks file changes
        // In this project there are usually uncommitted changes
        let result = has_progress(&cwd, &None);
        // Just verify it doesn't panic — result depends on working tree state
        let _ = result;
    }

    #[test]
    fn test_has_progress_detects_new_commits() {
        let tmp = tempfile::tempdir().unwrap();
        init_git_repo(tmp.path());

        // Create initial commit
        std::fs::write(tmp.path().join("a.txt"), "initial").unwrap();
        std::process::Command::new("git")
            .args(["add", "."])
            .current_dir(tmp.path())
            .output()
            .unwrap();
        std::process::Command::new("git")
            .args(["commit", "-m", "initial"])
            .current_dir(tmp.path())
            .output()
            .unwrap();

        let head_before = git_head_rev(tmp.path());

        // Create second commit
        std::fs::write(tmp.path().join("b.txt"), "second").unwrap();
        std::process::Command::new("git")
            .args(["add", "."])
            .current_dir(tmp.path())
            .output()
            .unwrap();
        std::process::Command::new("git")
            .args(["commit", "-m", "second"])
            .current_dir(tmp.path())
            .output()
            .unwrap();

        // Should detect progress via new commit
        assert!(has_progress(tmp.path(), &head_before));
    }

    #[test]
    fn test_has_progress_no_changes_same_commit() {
        let tmp = tempfile::tempdir().unwrap();
        init_git_repo(tmp.path());

        std::fs::write(tmp.path().join("a.txt"), "content").unwrap();
        std::process::Command::new("git")
            .args(["add", "."])
            .current_dir(tmp.path())
            .output()
            .unwrap();
        std::process::Command::new("git")
            .args(["commit", "-m", "initial"])
            .current_dir(tmp.path())
            .output()
            .unwrap();

        let head_before = git_head_rev(tmp.path());
        // No changes since head_before
        assert!(!has_progress(tmp.path(), &head_before));
    }

    fn init_git_repo(path: &Path) {
        std::process::Command::new("git")
            .args(["init"])
            .current_dir(path)
            .output()
            .unwrap();
        std::process::Command::new("git")
            .args(["config", "user.email", "test@test.com"])
            .current_dir(path)
            .output()
            .unwrap();
        std::process::Command::new("git")
            .args(["config", "user.name", "test"])
            .current_dir(path)
            .output()
            .unwrap();
    }

    #[tokio::test]
    async fn test_tenacity_completes_on_first_success() {
        let cwd = std::env::current_dir().unwrap();
        let outcome = run_tenacity_loop(&cwd, "test task", |_prompt| async {
            // Simulate success — in real use this calls run_agent_with_mode
            Ok(())
        })
        .await;

        // Since the test project has git changes, this should detect them
        match outcome {
            TenacityOutcome::Completed { iterations } => assert_eq!(iterations, 1),
            TenacityOutcome::NoChanges { .. } => {} // possible in clean state
            TenacityOutcome::Failed { .. } => panic!("Should not fail"),
        }
    }

    #[tokio::test]
    async fn test_tenacity_retries_on_no_changes() {
        let tmp = tempfile::tempdir().unwrap();
        // Initialize a git repo so has_file_changes can work
        std::process::Command::new("git")
            .args(["init"])
            .current_dir(tmp.path())
            .output()
            .unwrap();
        std::process::Command::new("git")
            .args(["config", "user.email", "test@test.com"])
            .current_dir(tmp.path())
            .output()
            .unwrap();
        std::process::Command::new("git")
            .args(["config", "user.name", "test"])
            .current_dir(tmp.path())
            .output()
            .unwrap();

        let mut call_count = 0;
        let outcome = run_tenacity_loop(tmp.path(), "test task", |_prompt| {
            call_count += 1;
            async move { Ok(()) }
        })
        .await;

        // Should have retried since no changes in clean git repo
        match outcome {
            TenacityOutcome::NoChanges { iterations } => {
                assert_eq!(iterations, MAX_ITERATIONS);
            }
            other => panic!("Expected NoChanges, got {:?}", other),
        }
    }
}
