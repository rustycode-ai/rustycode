//! Workspace context loading for project information
//!
//! This module provides functionality to load workspace information including
//! project files, directory structure, and git status. It's used to give the LLM
//! context about the current project when generating responses.
//!
//! # Features
//!
//! - **Project file detection**: Automatically finds and previews important files
//!   like README.md, CLAUDE.md, AGENTS.md, Cargo.toml, package.json, etc.
//! - **Directory structure scanning**: Lists directories and files with configurable limits
//! - **Git integration**: Shows current branch and working directory status
//! - **Ignore pattern support**: Respects .rustycodeignore files and default patterns
//! - **Progress tracking**: Optional progress callbacks for UI feedback during scanning
//!
//! # Usage
//!
//! ```rust,ignore
//! use std::path::PathBuf;
//!
//! // Simple usage
//! let context = load_workspace_context(&PathBuf::from("/project"), 10, 20);
//!
//! // With progress tracking
//! let progress_callback = Box::new(|scanned: usize, total: usize| {
//!     println!("Scanned {}/{} files", scanned, total);
//! });
//! let context = load_workspace_context_with_progress(
//!     &PathBuf::from("/project"),
//!     10,
//!     20,
//!     Some(progress_callback),
//! );
//! ```

use std::path::{Path, PathBuf};
use std::process::Command;

/// Progress callback for workspace scanning
///
/// Called periodically during workspace scanning to report progress.
/// The callback receives `(scanned, total)` where:
/// - `scanned`: Number of items processed so far
/// - `total`: Estimated total items to process
///
/// # Example
///
/// ```rust,ignore
/// let callback: ScanProgressCallback = Box::new(|scanned, total| {
///     let percent = (scanned as f64 / total as f64) * 100.0;
///     println!("Progress: {:.1}%", percent);
/// });
/// ```
pub type ScanProgressCallback = Box<dyn Fn(usize, usize) + Send>;

/// Default patterns to exclude from workspace scans
///
/// These patterns are always applied in addition to any patterns
/// found in the `.rustycodeignore` file.
const DEFAULT_IGNORE_PATTERNS: &[&str] = &[
    "node_modules",
    "target",
    ".git",
    "*.lock",
    ".DS_Store",
    "dist",
    "build",
    "*.pyc",
    "__pycache__",
    ".venv",
    "venv",
    ".env",
    "*.log",
];

/// Name of the ignore file
const IGNORE_FILE_NAME: &str = ".rustycodeignore";

/// Report progress every N items to avoid too many updates
///
/// This prevents the UI from being flooded with progress updates
/// during fast directory scans.
const PROGRESS_UPDATE_INTERVAL: usize = 10;

/// Maximum number of files to display in directory listing
///
/// Limits the output to prevent overwhelming the LLM context
/// with too many file entries.
const MAX_FILES_DISPLAY: usize = 30;

/// Division factor for directory scan progress weight
///
/// Directory scanning is weighted less than file reading in progress
/// calculations since it's generally faster.
const DIRECTORY_SCAN_WEIGHT: usize = 2;

/// Maximum number of directories to sample for file count estimation
///
/// Limits the sampling to prevent excessive I/O on large projects.
const MAX_DIR_SAMPLE_THRESHOLD: usize = 50;

/// Maximum number of subdirectory entries to sample for estimation
///
/// When estimating file counts, we sample at most this many entries
/// from each subdirectory to avoid counting huge directories.
const MAX_SUBDIR_SAMPLE: usize = 10;

/// Minimum file count estimate to ensure reasonable progress tracking
///
/// Even for empty directories, we report at least this many items
/// to ensure the progress bar behaves reasonably.
const MIN_FILE_COUNT_ESTIMATE: usize = 10;

/// Load ignore patterns from .rustycodeignore file and defaults
///
/// Combines the default ignore patterns with any patterns found in
/// the `.rustycodeignore` file in the project root.
///
/// # Arguments
/// * `cwd` - Current working directory (project root)
///
/// # Returns
/// * Vector of ignore patterns as strings
///
/// # Pattern Syntax
///
/// - Simple names match any directory or file with that name (e.g., "node_modules")
/// - Wildcards match file extensions (e.g., "*.pyc")
/// - Lines starting with `#` are treated as comments
/// - Empty lines are ignored
fn load_ignore_patterns(cwd: &Path) -> Vec<String> {
    let mut patterns: Vec<String> = DEFAULT_IGNORE_PATTERNS
        .iter()
        .map(|s| s.to_string())
        .collect();

    // Load from .rustycodeignore if exists
    let ignore_file_path = cwd.join(IGNORE_FILE_NAME);
    if let Ok(content) = std::fs::read_to_string(&ignore_file_path) {
        for line in content.lines() {
            let line = line.trim();
            // Skip empty lines and comments
            if !line.is_empty() && !line.starts_with('#') {
                patterns.push(line.to_string());
            }
        }
    }

    patterns
}

/// Find a project guidance file at the workspace root.
///
/// RustyCode looks for task-specific instructions first, then project-level
/// agent guidance, then a README fallback. Returns the filename and file
/// contents when one exists and is non-empty.
pub fn find_project_instruction_file(cwd: &Path) -> Option<(String, String)> {
    for filename in ["CLAUDE.md", "AGENTS.md", "README.md"] {
        let path = cwd.join(filename);
        if let Ok(content) = std::fs::read_to_string(&path) {
            if !content.trim().is_empty() {
                return Some((filename.to_string(), content));
            }
        }
    }

    None
}

/// Check if a path matches any ignore pattern
///
/// Patterns can be:
/// - Simple names (e.g., "node_modules") - matches any directory with that name
/// - Wildcards (e.g., "*.pyc") - matches files with that extension
/// - Path components - matches any component in the path
///
/// # Arguments
/// * `path` - The path to check
/// * `patterns` - List of ignore patterns
///
/// # Returns
/// * `true` if the path should be ignored
/// * `false` otherwise
///
/// # Examples
///
/// ```rust,ignore
/// let patterns = vec!["node_modules".to_string(), "*.pyc".to_string()];
///
/// assert!(matches_ignore_pattern(Path::new("node_modules"), &patterns));
/// assert!(matches_ignore_pattern(Path::new("src/node_modules"), &patterns));
/// assert!(matches_ignore_pattern(Path::new("test.pyc"), &patterns));
/// assert!(!matches_ignore_pattern(Path::new("src/main.py"), &patterns));
/// ```
fn matches_ignore_pattern(path: &Path, patterns: &[String]) -> bool {
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");

    patterns.iter().any(|pattern| {
        if let Some(ext) = pattern.strip_prefix('*') {
            // Wildcard pattern (e.g., "*.pyc") - check file extension
            name.ends_with(ext)
        } else {
            // For directory patterns, check if any component matches
            // This handles cases like src/node_modules
            name == pattern
                || path
                    .components()
                    .any(|c| c.as_os_str().to_str() == Some(pattern))
        }
    })
}

/// Load workspace context for a given directory
///
/// This is the main entry point for loading workspace context without
/// progress tracking. It scans the project directory and returns a
/// formatted string containing project information.
///
/// # Arguments
/// * `cwd` - Current working directory to scan
/// * `file_preview_max_lines` - Maximum lines for file previews
/// * `status_max_lines` - Maximum lines for git status
///
/// # Returns
/// * Formatted string containing workspace information
///
/// # Content Structure
///
/// The returned string includes:
/// 1. Workspace path
/// 2. Project files found (with previews for markdown/config files)
/// 3. Directory structure (limited to MAX_FILES_DISPLAY entries)
/// 4. Git status and current branch
///
/// # Example
///
/// ```rust,ignore
/// let context = load_workspace_context(&PathBuf::from("/project"), 10, 20);
/// println!("{}", context);
/// // Output:
/// // Workspace: /project
/// //
/// // ## Project Files Found:
/// //   ✓ README.md
/// //     --- Preview ---
/// //     # My Project
/// //     ...
/// //
/// // ## Directory Structure:
/// //   📁 src/
/// //   📄 Cargo.toml
/// //
/// // ## Git Status:
/// //   Working directory clean
/// //   Branch: main
/// ```
///
/// Load workspace context with progress tracking
///
/// Extended version of `load_workspace_context` that supports progress
/// callbacks for UI feedback during scanning.
///
/// # Arguments
/// * `cwd` - Current working directory
/// * `file_preview_max_lines` - Maximum lines for file previews
/// * `status_max_lines` - Maximum lines for git status
/// * `progress_callback` - Optional callback for progress updates (scanned, total)
///
/// # Progress Tracking
///
/// The progress callback is called with `(scanned, total)` where:
/// - `scanned` increases as files/directories are processed
/// - `total` is an estimate based on initial directory sampling
/// - Final call will have `scanned == total` indicating completion
///
/// # Example
///
/// ```rust,ignore
/// let progress_callback: ScanProgressCallback = Box::new(|scanned, total| {
///     let percent = if total > 0 {
///         (scanned as f64 / total as f64) * 100.0
///     } else {
///         0.0
///     };
///     update_progress_bar(percent);
/// });
///
/// let context = load_workspace_context_with_progress(
///     &PathBuf::from("/project"),
///     10,
///     20,
///     Some(progress_callback),
/// );
/// ```
pub fn load_workspace_context_with_progress(
    cwd: &PathBuf,
    file_preview_max_lines: usize,
    status_max_lines: usize,
    progress_callback: Option<ScanProgressCallback>,
) -> String {
    let mut context = String::new();
    context.push_str(&format!("Workspace: {}\n\n", cwd.display()));

    // Load ignore patterns
    let ignore_patterns = load_ignore_patterns(cwd);

    // First pass: estimate total files by counting directories
    let estimated_total = estimate_file_count(cwd, &ignore_patterns);
    let mut scanned = 0usize;

    // Helper to report progress
    let report_progress = |scanned: usize, total: usize| {
        if let Some(ref callback) = progress_callback {
            callback(scanned, total);
        }
    };

    // List of important files to look for and preview
    let important_files = vec![
        "README.md",
        "README.txt",
        "instruction.md",
        "CLAUDE.md",
        "AGENTS.md",
        "CONTRIBUTING.md",
        "package.json",
        "Cargo.toml",
        "pyproject.toml",
        "go.mod",
    ];

    context.push_str("## Project Files Found:\n");
    for filename in important_files {
        let file_path = cwd.join(filename);
        if file_path.exists() {
            context.push_str(&format!("  ✓ {}\n", filename));
            scanned += 1;
            report_progress(scanned, estimated_total);
            // Preview markdown and config files
            if filename.ends_with(".md") || filename == "Cargo.toml" || filename == "package.json" {
                if let Ok(contents) = std::fs::read_to_string(&file_path) {
                    scanned += 1;
                    report_progress(scanned, estimated_total);
                    let preview: String = contents
                        .lines()
                        .take(file_preview_max_lines)
                        .collect::<Vec<_>>()
                        .join("\n");
                    context.push_str(&format!(
                        "    --- Preview ---\n{}\n    --- End Preview ---\n",
                        preview
                    ));
                }
            }
        }
    }

    context.push_str("\n## Directory Structure:\n");
    if let Ok(entries) = std::fs::read_dir(cwd) {
        let mut dirs = Vec::new();
        let mut files = Vec::new();

        for entry in entries.flatten() {
            let path = entry.path();

            // Skip ignored patterns
            if matches_ignore_pattern(&path, &ignore_patterns) {
                continue;
            }

            let name = entry.file_name().to_string_lossy().to_string();

            if entry.path().is_dir() {
                dirs.push(name);
            } else {
                files.push(name);
            }
            scanned += 1;
            // Report progress every N items to avoid too many updates
            if scanned.is_multiple_of(PROGRESS_UPDATE_INTERVAL) {
                report_progress(scanned, estimated_total);
            }
        }

        dirs.sort();
        files.sort();

        for dir in dirs {
            context.push_str(&format!("  📁 {}\n", dir));
        }
        for file in files.into_iter().take(MAX_FILES_DISPLAY) {
            context.push_str(&format!("  📄 {}\n", file));
        }

        // Final progress update for directory scan
        scanned = scanned.max(estimated_total / DIRECTORY_SCAN_WEIGHT);
        report_progress(scanned, estimated_total);
    }

    context.push_str("\n## Git Status:\n");
    if let Ok(output) = Command::new("git")
        .args(["-C", &cwd.to_string_lossy(), "status", "--short"])
        .output()
    {
        if output.status.success() {
            let status = String::from_utf8_lossy(&output.stdout);
            if status.trim().is_empty() {
                context.push_str("  Working directory clean\n");
            } else {
                for line in status.lines().take(status_max_lines) {
                    context.push_str(&format!("  {}\n", line));
                }
            }
        }
    }

    if let Ok(output) = Command::new("git")
        .args(["-C", &cwd.to_string_lossy(), "branch", "--show-current"])
        .output()
    {
        if output.status.success() {
            let branch = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !branch.is_empty() {
                context.push_str(&format!("  Branch: {}\n", branch));
            }
        }
    }

    // Append repo map (tree-sitter structural summary) if available
    if let Ok(repo_map) = rustycode_tools::RepoMap::build(cwd, 2000) {
        let map_str = repo_map.to_map_string();
        if !map_str.is_empty() {
            context.push_str("\n## Code Structure Map:\n");
            context.push_str(map_str);
        }
    }

    // Final progress update - mark as complete
    report_progress(estimated_total, estimated_total);

    context
}

/// Estimate the total number of files to scan
///
/// Provides a rough estimate by counting entries in the root directory
/// and sampling subdirectories. The estimate may be wrong for large projects,
/// but that's okay - the progress bar can handle exceeding 100%.
///
/// # Arguments
/// * `cwd` - Current working directory to estimate
/// * `ignore_patterns` - Patterns to exclude from counting
///
/// # Returns
/// * Estimated file count (minimum MIN_FILE_COUNT_ESTIMATE)
///
/// # Estimation Algorithm
///
/// 1. Count non-ignored entries in root directory
/// 2. For each directory (up to MAX_DIR_SAMPLE_THRESHOLD), sample subdirectory entries
/// 3. Add up to MAX_SUBDIR_SAMPLE entries from each subdirectory
/// 4. Return max of actual count and MIN_FILE_COUNT_ESTIMATE
fn estimate_file_count(cwd: &PathBuf, ignore_patterns: &[String]) -> usize {
    let mut count = 0usize;

    // Count entries in root directory
    if let Ok(entries) = std::fs::read_dir(cwd) {
        for entry in entries.flatten() {
            let path = entry.path();

            // Skip ignored patterns
            if matches_ignore_pattern(&path, ignore_patterns) {
                continue;
            }

            count += 1;

            // For directories, sample a few entries to estimate depth
            if entry.path().is_dir() && count < MAX_DIR_SAMPLE_THRESHOLD {
                if let Ok(sub_entries) = std::fs::read_dir(entry.path()) {
                    let sub_count = sub_entries.flatten().count();
                    // Add a fraction of subdirectory contents to estimate
                    count += sub_count.min(MAX_SUBDIR_SAMPLE);
                }
            }
        }
    }

    // Ensure we have a reasonable minimum
    count.max(MIN_FILE_COUNT_ESTIMATE)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_load_workspace_context() {
        let temp_dir = std::env::temp_dir();
        let context = load_workspace_context_with_progress(&temp_dir, 5, 10, None);

        // Should contain workspace info
        assert!(context.contains("Workspace:"));
        assert!(context.contains("## Project Files Found:"));
        assert!(context.contains("## Directory Structure:"));
        assert!(context.contains("## Git Status:"));
    }

    #[test]
    fn test_load_workspace_context_includes_agent_docs() {
        let temp_dir = TempDir::new().unwrap();
        fs::write(temp_dir.path().join("README.md"), "# Test").unwrap();
        fs::write(temp_dir.path().join("CLAUDE.md"), "Claude guidance").unwrap();
        fs::write(temp_dir.path().join("AGENTS.md"), "Agent guidance").unwrap();

        let context =
            load_workspace_context_with_progress(&temp_dir.path().to_path_buf(), 5, 10, None);

        assert!(context.contains("✓ README.md"));
        assert!(context.contains("✓ CLAUDE.md"));
        assert!(context.contains("✓ AGENTS.md"));
        assert!(context.contains("Agent guidance"));
    }

    #[test]
    fn test_default_ignore_patterns() {
        let temp_dir = TempDir::new().unwrap();
        let patterns = load_ignore_patterns(temp_dir.path());

        // Should contain default patterns
        assert!(patterns.contains(&"node_modules".to_string()));
        assert!(patterns.contains(&"target".to_string()));
        assert!(patterns.contains(&".git".to_string()));
        assert!(patterns.contains(&"*.lock".to_string()));
        assert!(patterns.contains(&".DS_Store".to_string()));
    }

    #[test]
    fn test_load_ignore_patterns_from_file() {
        let temp_dir = TempDir::new().unwrap();
        let ignore_file = temp_dir.path().join(".rustycodeignore");

        // Create .rustycodeignore file
        fs::write(
            &ignore_file,
            "custom_dir\n*.tmp\n# This is a comment\n\nignored_file\n",
        )
        .unwrap();

        let patterns = load_ignore_patterns(temp_dir.path());

        // Should contain default patterns
        assert!(patterns.contains(&"node_modules".to_string()));
        assert!(patterns.contains(&"target".to_string()));

        // Should contain custom patterns from file
        assert!(patterns.contains(&"custom_dir".to_string()));
        assert!(patterns.contains(&"*.tmp".to_string()));
        assert!(patterns.contains(&"ignored_file".to_string()));

        // Should NOT contain comments or empty lines
        assert!(!patterns.contains(&"# This is a comment".to_string()));
        assert!(!patterns.contains(&"".to_string()));
    }

    #[test]
    fn test_matches_ignore_pattern_exact() {
        let patterns = vec!["node_modules".to_string(), ".git".to_string()];

        assert!(matches_ignore_pattern(Path::new("node_modules"), &patterns));
        assert!(matches_ignore_pattern(Path::new(".git"), &patterns));
        assert!(!matches_ignore_pattern(Path::new("src"), &patterns));
        assert!(!matches_ignore_pattern(
            Path::new("node_modules_extra"),
            &patterns
        ));
    }

    #[test]
    fn test_matches_ignore_pattern_wildcard() {
        let patterns = vec![
            "*.lock".to_string(),
            "*.pyc".to_string(),
            "*.json".to_string(),
        ];

        assert!(matches_ignore_pattern(Path::new("Cargo.lock"), &patterns));
        assert!(matches_ignore_pattern(Path::new("yarn.lock"), &patterns));
        assert!(matches_ignore_pattern(Path::new("test.pyc"), &patterns));
        assert!(matches_ignore_pattern(Path::new("package.json"), &patterns));
        assert!(!matches_ignore_pattern(Path::new("lockfile"), &patterns));
        assert!(!matches_ignore_pattern(Path::new("pycache"), &patterns));
    }

    #[test]
    fn test_matches_ignore_pattern_subdirectory() {
        let patterns = vec!["node_modules".to_string()];

        // Should match node_modules in subdirectories
        assert!(matches_ignore_pattern(
            Path::new("src/node_modules"),
            &patterns
        ));
        assert!(matches_ignore_pattern(
            Path::new("packages/app/node_modules"),
            &patterns
        ));
        // Should still match at root level
        assert!(matches_ignore_pattern(Path::new("node_modules"), &patterns));
    }

    #[test]
    fn test_node_modules_excluded_from_scan() {
        let temp_dir = TempDir::new().unwrap();

        // Create some directories
        fs::create_dir(temp_dir.path().join("src")).unwrap();
        fs::create_dir(temp_dir.path().join("node_modules")).unwrap();
        fs::create_dir(temp_dir.path().join(".git")).unwrap();
        fs::create_dir(temp_dir.path().join("target")).unwrap();

        let context =
            load_workspace_context_with_progress(&temp_dir.path().to_path_buf(), 5, 10, None);

        // Should contain src (directory listing uses "📁 {name}" format)
        assert!(
            context.contains("📁 src"),
            "Context should contain 📁 src, got:\n{}",
            context
        );

        // Should NOT contain ignored directories
        assert!(!context.contains("📁 node_modules"));
        assert!(!context.contains("📁 .git"));
        assert!(!context.contains("📁 target"));
    }

    #[test]
    fn test_custom_ignore_patterns_work() {
        let temp_dir = TempDir::new().unwrap();

        // Create .rustycodeignore with custom patterns
        fs::write(
            temp_dir.path().join(".rustycodeignore"),
            "my_custom_dir\n*.custom\n",
        )
        .unwrap();

        // Create directories
        fs::create_dir(temp_dir.path().join("src")).unwrap();
        fs::create_dir(temp_dir.path().join("my_custom_dir")).unwrap();
        fs::write(temp_dir.path().join("test.custom"), "content").unwrap();
        fs::write(temp_dir.path().join("normal.txt"), "content").unwrap();

        let context =
            load_workspace_context_with_progress(&temp_dir.path().to_path_buf(), 5, 10, None);

        // Should contain src (directory listing uses "📁 {name}" format)
        assert!(context.contains("📁 src"));

        // Should NOT contain custom ignored items
        assert!(!context.contains("📁 my_custom_dir"));
        assert!(!context.contains("📄 test.custom"));

        // Should contain normal files
        assert!(context.contains("📄 normal.txt"));
    }

    #[test]
    fn test_load_workspace_context_with_progress() {
        let temp_dir = TempDir::new().unwrap();

        // Create some files and directories
        fs::create_dir(temp_dir.path().join("src")).unwrap();
        fs::create_dir(temp_dir.path().join("docs")).unwrap();
        fs::write(temp_dir.path().join("README.md"), "# Test").unwrap();
        fs::write(temp_dir.path().join("Cargo.toml"), "[package]").unwrap();

        // Track progress updates
        let progress_updates = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let progress_updates_clone = progress_updates.clone();

        let progress_callback: ScanProgressCallback = Box::new(move |scanned, total| {
            progress_updates_clone
                .lock()
                .unwrap()
                .push((scanned, total));
        });

        let context = load_workspace_context_with_progress(
            &temp_dir.path().to_path_buf(),
            5,
            10,
            Some(progress_callback),
        );

        // Should still produce valid context
        assert!(context.contains("Workspace:"));
        assert!(context.contains("## Project Files Found:"));
        assert!(context.contains("## Directory Structure:"));

        // Should have received progress updates
        let updates = progress_updates.lock().unwrap_or_else(|e| e.into_inner());
        assert!(!updates.is_empty(), "Should have received progress updates");

        // Last update should show completion (scanned == total)
        let last = updates.last().unwrap();
        assert_eq!(last.0, last.1, "Final progress should show completion");
    }

    #[test]
    fn test_estimate_file_count() {
        let temp_dir = TempDir::new().unwrap();

        // Create some files and directories
        fs::create_dir(temp_dir.path().join("src")).unwrap();
        fs::create_dir(temp_dir.path().join("docs")).unwrap();
        fs::write(temp_dir.path().join("file1.txt"), "content").unwrap();
        fs::write(temp_dir.path().join("file2.txt"), "content").unwrap();

        let patterns = load_ignore_patterns(temp_dir.path());
        let estimate = estimate_file_count(&temp_dir.path().to_path_buf(), &patterns);

        // Should have a reasonable estimate (at least the files we created)
        assert!(
            estimate >= 4,
            "Estimate should be at least 4 (2 dirs + 2 files)"
        );
        assert!(estimate >= 10, "Estimate should have minimum of 10");
    }

    #[test]
    fn test_estimate_file_count_with_ignores() {
        let temp_dir = TempDir::new().unwrap();

        // Create directories including ignored ones
        fs::create_dir(temp_dir.path().join("src")).unwrap();
        fs::create_dir(temp_dir.path().join("node_modules")).unwrap();
        fs::create_dir(temp_dir.path().join(".git")).unwrap();
        fs::write(temp_dir.path().join("file1.txt"), "content").unwrap();

        let patterns = load_ignore_patterns(temp_dir.path());
        let estimate = estimate_file_count(&temp_dir.path().to_path_buf(), &patterns);

        // Should only count non-ignored entries (src + file1.txt = 2, but min is 10)
        assert!(estimate >= 10, "Estimate should respect minimum");
    }
}
