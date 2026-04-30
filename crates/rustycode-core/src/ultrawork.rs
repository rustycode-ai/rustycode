//! Ultrawork: lightweight progress tracking for agent turns.
//!
//! Provides utilities for detecting whether an agent turn made meaningful
//! progress (vs. looping, failing, or making only internal changes).

use std::path::Path;
use std::process::Command;

/// Maximum number of iteration retries.
/// 3 gives the agent: initial attempt + 2 retries. The model sometimes
/// ignores write-first prompts on the first retry, so the extra attempt
/// provides a safety net for "explore-but-don't-write" failures.
pub const MAX_ITERATIONS: usize = 3;

/// Internal files that should not count as task progress.
pub const INTERNAL_FILES: &[&str] = &[".rustycode_command_history", ".claude/", ".git/", "target/"];

/// Captures a point-in-time snapshot for progress detection.
#[derive(Debug, Clone)]
pub struct ProgressSnapshot {
    /// Git HEAD at snapshot time.
    pub git_head: Option<String>,
    /// Set of files that existed at snapshot time.
    pub files: std::collections::HashSet<String>,
}

impl ProgressSnapshot {
    /// Take a snapshot of the current working directory state.
    pub fn take(cwd: &Path) -> Self {
        let git_head = git_head_rev(cwd);
        let files = file_list(cwd);
        Self { git_head, files }
    }

    /// Check if there's been any meaningful progress since this snapshot.
    pub fn has_progress(&self, cwd: &Path) -> bool {
        // Check git changes
        if has_file_changes(cwd) {
            tracing::debug!("Progress detected via git status");
            return true;
        }
        // Check git HEAD changed
        if let Some(ref before) = self.git_head {
            if let Some(after) = git_head_rev(cwd) {
                if before != &after {
                    tracing::debug!("Progress detected via git HEAD change");
                    return true;
                }
            }
        }
        // Check file list changed
        let current_files = file_list(cwd);
        if current_files != self.files {
            let new_files: Vec<_> = current_files.difference(&self.files).collect();
            let removed_files: Vec<_> = self.files.difference(&current_files).collect();
            tracing::debug!(
                "Progress detected via file list: {} new, {} removed (sample: {:?})",
                new_files.len(),
                removed_files.len(),
                new_files.iter().take(5)
            );
            return true;
        }
        tracing::debug!(
            "No progress detected: git_status=false, files_unchanged (snapshot had {} files)",
            self.files.len()
        );
        false
    }
}

/// Get the current git HEAD revision (short SHA).
pub fn git_head_rev(cwd: &Path) -> Option<String> {
    let output = Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .current_dir(cwd)
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// Check if there's been any progress since a snapshot was taken.
pub fn has_progress(snapshot: &ProgressSnapshot, cwd: &Path) -> bool {
    snapshot.has_progress(cwd)
}

/// Check if there are uncommitted file changes.
pub fn has_file_changes(cwd: &Path) -> bool {
    let output = match Command::new("git")
        .args(["status", "--porcelain"])
        .current_dir(cwd)
        .output()
    {
        Ok(o) => Some(o),
        Err(e) => {
            tracing::debug!("Failed to run git status in {}: {}", cwd.display(), e);
            None
        }
    };

    match output {
        Some(o) if o.status.success() => {
            let status = String::from_utf8_lossy(&o.stdout);
            // Filter out only internal files
            for line in status.lines() {
                let path = line.trim_start_matches(|c: char| {
                    c.is_whitespace() || c == '?' || c == 'A' || c == 'M' || c == 'D'
                });
                if !INTERNAL_FILES.iter().any(|f| path.starts_with(f)) {
                    return true;
                }
            }
            false
        }
        _ => false,
    }
}

/// Get a list of files in the working directory (excluding internal ones).
/// Uses recursive walk to detect changes at any depth (e.g., agent creates
/// /app/project/src/utils/helper.py — shallow scan would miss this).
fn file_list(cwd: &Path) -> std::collections::HashSet<String> {
    let mut files = std::collections::HashSet::new();
    walk_dir(cwd, "", &mut files, 3);
    files
}

/// Recursively walk a directory, collecting relative paths up to `max_depth` levels.
fn walk_dir(
    dir: &Path,
    prefix: &str,
    files: &mut std::collections::HashSet<String>,
    max_depth: usize,
) {
    if max_depth == 0 {
        return;
    }
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            let rel_path = if prefix.is_empty() {
                name.clone()
            } else {
                format!("{}/{}", prefix, name)
            };
            if INTERNAL_FILES
                .iter()
                .any(|f| name.starts_with(f.trim_end_matches('/')))
            {
                continue;
            }
            files.insert(rel_path.clone());
            if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                walk_dir(&dir.join(&name), &rel_path, files, max_depth - 1);
            }
        }
    }
}

/// Build an iteration-specific prompt with retry context.
pub fn build_iteration_prompt(base_prompt: &str, iteration: usize) -> String {
    if iteration == 1 {
        base_prompt.to_string()
    } else {
        format!(
            "{}\n\n[Retry attempt {}/{}: Please continue from where you left off, building on the previous work.]",
            base_prompt,
            iteration,
            MAX_ITERATIONS
        )
    }
}
