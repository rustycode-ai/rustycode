//! Hints Loading System
//!
//! Loads project-specific hints from `.rustycodehints` and `CLAUDE.md` files.
//! Automatically discovers subdirectory hints when tools access new directories.
//!
//! Ported from goose's `hints/load_hints.rs` with adaptations:
//! - Uses `file_reference::expand_references` for `@file` expansion
//! - Gitignore integration for filtering referenced files
//! - Git-root-aware import boundaries
//! - `SubdirectoryHintTracker` for lazy loading on tool access
//!
//! # Security
//!
//! - Import boundary is the git root (or cwd if no git repo)
//! - File references are expanded with path traversal protection
//! - Gitignored files are excluded from reference expansion

use crate::file_reference::expand_references;
use ignore::gitignore::{Gitignore, GitignoreBuilder};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// Default hint filenames to look for
pub const DEFAULT_HINTS_FILENAME: &str = ".rustycodehints";
pub const CLAUDE_MD_FILENAME: &str = "CLAUDE.md";

/// Get the default list of hint filenames to search for.
pub fn default_hints_filenames() -> Vec<String> {
    vec![
        DEFAULT_HINTS_FILENAME.to_string(),
        CLAUDE_MD_FILENAME.to_string(),
    ]
}

/// Find the git root directory by walking up from `start_dir`.
///
/// Returns `None` if no `.git` directory is found.
pub fn find_git_root(start_dir: &Path) -> Option<PathBuf> {
    let mut dir = start_dir;
    loop {
        if dir.join(".git").exists() {
            return Some(dir.to_path_buf());
        }
        dir = dir.parent()?;
    }
}

/// Build a `Gitignore` that includes `.gitignore` files from the git root
/// down to `cwd`, matching git's hierarchical ignore semantics.
///
/// When there is no git root, only `cwd/.gitignore` is loaded.
pub fn build_gitignore(cwd: &Path) -> Gitignore {
    let git_root = find_git_root(cwd);
    let directories = get_local_directories(git_root.as_deref(), cwd);

    let mut builder = GitignoreBuilder::new(cwd);
    for dir in &directories {
        let gitignore_path = dir.join(".gitignore");
        if gitignore_path.is_file() {
            builder.add(&gitignore_path);
        }
    }
    builder.build().unwrap_or_else(|_| {
        GitignoreBuilder::new(cwd)
            .build()
            .expect("Failed to build default gitignore")
    })
}

/// Get the list of directories from git root down to cwd.
fn get_local_directories(git_root: Option<&Path>, cwd: &Path) -> Vec<PathBuf> {
    match git_root {
        Some(root) => {
            let mut dirs = Vec::new();
            let mut current = cwd;
            loop {
                dirs.push(current.to_path_buf());
                if current == root {
                    break;
                }
                current = match current.parent() {
                    Some(p) => p,
                    None => break,
                };
            }
            dirs.reverse();
            dirs
        }
        None => vec![cwd.to_path_buf()],
    }
}

/// Check if a file is ignored by gitignore rules.
fn is_gitignored(path: &Path, gitignore: &Gitignore) -> bool {
    // Use relative path for matching
    gitignore.matched(path, false).is_ignore()
}

/// Expand file references in hint content, respecting gitignore rules.
///
/// Reads the file, expands `@file` references while skipping gitignored files.
fn expand_hints_content(
    hints_path: &Path,
    import_boundary: &Path,
    gitignore: &Gitignore,
) -> String {
    let content = match std::fs::read_to_string(hints_path) {
        Ok(c) => c,
        Err(e) => {
            log::warn!("Could not read hints file {hints_path:?}: {e}");
            return String::new();
        }
    };

    // Parse file references and filter out gitignored ones
    let refs = crate::file_reference::parse_file_references(&content);
    let hints_dir = hints_path.parent().unwrap_or(hints_path);

    let mut result = content;
    for reference in refs {
        let resolved = if reference.is_absolute() {
            reference.clone()
        } else {
            hints_dir.join(&reference)
        };

        // Skip gitignored files
        if is_gitignored(&resolved, gitignore) {
            log::debug!("Skipping gitignored reference: {reference:?}");
            continue;
        }

        // Expand the reference
        if resolved.is_file() {
            let expanded = expand_references(&resolved, import_boundary);
            if !expanded.is_empty() {
                let pattern = format!("@{}", reference.to_string_lossy());
                let replacement = format!(
                    "--- Content from {} ---\n{}\n--- End of {} ---",
                    reference.display(),
                    expanded,
                    reference.display()
                );
                result = result.replace(&pattern, &replacement);
            }
        }
    }

    result
}

/// Load hint files from the project directory hierarchy.
///
/// Searches from the git root down to `cwd` for hint files, expanding
/// `@file` references with gitignore filtering. Returns a structured
/// string with global and project hints sections.
///
pub fn load_hint_files(
    cwd: &Path,
    config_dir: &Path,
    hints_filenames: &[String],
    gitignore: &Gitignore,
) -> String {
    let mut global_hints = Vec::with_capacity(hints_filenames.len());
    let mut project_hints = Vec::with_capacity(hints_filenames.len());

    // Load global hints from config directory
    for filename in hints_filenames {
        let global_path = config_dir.join(filename);
        if global_path.is_file() {
            let hints_dir = global_path.parent().unwrap_or(config_dir);
            let expanded = expand_hints_content(&global_path, hints_dir, gitignore);
            if !expanded.is_empty() {
                global_hints.push(expanded);
            }
        }
    }

    // Load local hints from project directory hierarchy
    let git_root = find_git_root(cwd);
    let local_dirs = get_local_directories(git_root.as_deref(), cwd);
    let import_boundary = git_root.unwrap_or_else(|| cwd.to_path_buf());

    for dir in &local_dirs {
        for filename in hints_filenames {
            let hints_path = dir.join(filename);
            if hints_path.is_file() {
                let expanded = expand_hints_content(&hints_path, &import_boundary, gitignore);
                if !expanded.is_empty() {
                    project_hints.push(expanded);
                }
            }
        }
    }

    // Build the result string
    let mut result = String::new();

    if !global_hints.is_empty() {
        result.push_str("\n### Global Hints\nThese are your global rustycode hints.\n");
        result.push_str(&global_hints.join("\n"));
    }

    if !project_hints.is_empty() {
        if !result.is_empty() {
            result.push_str("\n\n");
        }
        result.push_str(
            "### Project Hints\nThese are hints for working on the project in this directory.\n",
        );
        result.push_str(&project_hints.join("\n"));
    }

    result
}

// ── Subdirectory Hint Tracker ─────────────────────────────────────────────────
//
// Tracks directories accessed by tool invocations and loads hints from
// newly discovered subdirectories. This enables lazy loading of project
// context as the agent explores the codebase.

/// Tracks tool-accessed directories and loads hints from new subdirectories.
///
/// When tools (`read_file`, bash, grep, etc.) access files in directories
/// that haven't been seen before, this tracker loads any hint files found
/// in those directories. Hints are loaded once per directory (deduplication).
///
/// # Example
///
/// ```ignore
/// use rustycode_tools::hints_loader::SubdirectoryHintTracker;
///
/// let mut tracker = SubdirectoryHintTracker::new();
///
/// // Record tool arguments (extracts path from "path" and "command" fields)
/// tracker.record_tool_arguments(&Some(args), &working_dir);
///
/// // Load any new hints from discovered directories
/// let new_hints = tracker.load_new_hints(&working_dir);
/// for (key, content) in new_hints {
///     println!("{}: {}", key, content);
/// }
/// ```
#[derive(Default)]
pub struct SubdirectoryHintTracker {
    loaded_dirs: HashSet<PathBuf>,
    pending_dirs: Vec<PathBuf>,
    hints_filenames: Vec<String>,
}

impl SubdirectoryHintTracker {
    pub fn new() -> Self {
        Self {
            loaded_dirs: HashSet::new(),
            pending_dirs: Vec::new(),
            hints_filenames: default_hints_filenames(),
        }
    }

    /// Create a tracker with custom hint filenames.
    pub fn with_filenames(filenames: Vec<String>) -> Self {
        Self {
            hints_filenames: filenames,
            ..Default::default()
        }
    }

    /// Record tool arguments to discover new directories.
    ///
    /// Extracts paths from `path` and `command` fields in the arguments.
    /// For `command` fields, parses shell words and skips flags.
    pub fn record_tool_arguments(
        &mut self,
        arguments: &Option<serde_json::Map<String, serde_json::Value>>,
        working_dir: &Path,
    ) {
        let args = match arguments.as_ref() {
            Some(a) => a,
            None => return,
        };

        // Extract directory from "path" argument
        if let Some(path_str) = args.get("path").and_then(|v| v.as_str()) {
            if let Some(dir) = resolve_to_parent_dir(path_str, working_dir) {
                self.pending_dirs.push(dir);
            }
        }

        // Extract directories from "command" argument
        if let Some(cmd) = args.get("command").and_then(|v| v.as_str()) {
            if let Ok(tokens) = shell_words::split(cmd) {
                for token in tokens {
                    if token.starts_with('-') {
                        continue;
                    }
                    if token.contains(std::path::MAIN_SEPARATOR) || token.contains('.') {
                        if let Some(dir) = resolve_to_parent_dir(&token, working_dir) {
                            self.pending_dirs.push(dir);
                        }
                    }
                }
            }
        }
    }

    /// Load hints from newly discovered subdirectories.
    ///
    /// Returns a list of `(key, content)` pairs for each new directory
    /// that contains hint files. Only directories within the working directory
    /// (but not the working directory itself) are checked.
    pub fn load_new_hints(&mut self, working_dir: &Path) -> Vec<(String, String)> {
        let pending = std::mem::take(&mut self.pending_dirs);
        if pending.is_empty() {
            return Vec::new();
        }

        let mut results = Vec::new();
        for dir in pending {
            // Only load from subdirectories within the working directory
            if !dir.starts_with(working_dir) || dir == working_dir {
                continue;
            }
            if self.loaded_dirs.contains(&dir) {
                continue;
            }
            if let Some(content) =
                load_hints_from_directory(&dir, working_dir, &self.hints_filenames)
            {
                let key = format!("subdir_hints:{}", dir.display());
                results.push((key, content));
            }
            self.loaded_dirs.insert(dir);
        }
        results
    }

    /// Get the number of loaded directories.
    pub fn loaded_count(&self) -> usize {
        self.loaded_dirs.len()
    }

    /// Check if a directory has been loaded.
    pub fn is_loaded(&self, dir: &Path) -> bool {
        self.loaded_dirs.contains(dir)
    }

    /// Reset the tracker state.
    pub fn reset(&mut self) {
        self.loaded_dirs.clear();
        self.pending_dirs.clear();
    }
}

/// Resolve a path string to its parent directory.
fn resolve_to_parent_dir(token: &str, working_dir: &Path) -> Option<PathBuf> {
    let path = Path::new(token);
    let resolved = if path.is_absolute() {
        path.to_path_buf()
    } else {
        working_dir.join(path)
    };
    resolved.parent().map(Path::to_path_buf)
}

fn is_strict_descendant(path: &Path, root: &Path) -> bool {
    path.starts_with(root) && path != root
}

/// Load hints from a specific directory.
///
/// Walks from the directory up to (but not including) the working directory,
/// loading hint files from each level. This ensures we pick up hints from
/// intermediate directories.
fn load_hints_from_directory(
    directory: &Path,
    working_dir: &Path,
    hints_filenames: &[String],
) -> Option<String> {
    if !directory.is_dir() || !directory.is_absolute() {
        return None;
    }
    if !is_strict_descendant(directory, working_dir) {
        return None;
    }

    let git_root = find_git_root(working_dir);
    let import_boundary = git_root.unwrap_or_else(|| working_dir.to_path_buf());
    let gitignore = build_gitignore(working_dir);

    // Collect directories from working_dir down to the target directory
    let mut directories: Vec<PathBuf> = directory
        .ancestors()
        .take_while(|d| is_strict_descendant(d, working_dir))
        .map(Path::to_path_buf)
        .collect();
    directories.reverse();

    let mut contents = Vec::new();
    for dir in &directories {
        for hints_filename in hints_filenames {
            let hints_path = dir.join(hints_filename);
            if hints_path.is_file() {
                let expanded = expand_hints_content(&hints_path, &import_boundary, &gitignore);
                if !expanded.is_empty() {
                    contents.push(expanded);
                }
            }
        }
    }

    if contents.is_empty() {
        None
    } else {
        Some(format!(
            "### Subdirectory Hints ({})\n{}",
            directory.display(),
            contents.join("\n")
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn create_dummy_gitignore() -> Gitignore {
        let temp_dir = tempfile::tempdir().expect("failed to create tempdir");
        let builder = GitignoreBuilder::new(temp_dir.path());
        builder.build().expect("failed to build gitignore")
    }

    fn create_file(dir: &Path, name: &str, content: &str) -> PathBuf {
        let path = dir.join(name);
        fs::write(&path, content).unwrap();
        path
    }

    #[test]
    fn test_find_git_root() {
        let temp = TempDir::new().unwrap();
        let root = temp.path();
        fs::create_dir(root.join(".git")).unwrap();

        let subdir = root.join("src").join("module");
        fs::create_dir_all(&subdir).unwrap();

        assert_eq!(find_git_root(&subdir), Some(root.to_path_buf()));
        assert_eq!(find_git_root(root), Some(root.to_path_buf()));
    }

    #[test]
    fn test_find_git_root_not_found() {
        let temp = TempDir::new().unwrap();
        // No .git directory
        assert_eq!(find_git_root(temp.path()), None);
    }

    #[test]
    fn test_build_gitignore_basic() {
        let temp = TempDir::new().unwrap();
        let root = temp.path();
        fs::create_dir(root.join(".git")).unwrap();
        fs::write(root.join(".gitignore"), "*.log\n").unwrap();

        let gitignore = build_gitignore(root);
        assert!(gitignore.matched(Path::new("debug.log"), false).is_ignore());
        assert!(!gitignore.matched(Path::new("main.rs"), false).is_ignore());
    }

    #[test]
    fn test_build_gitignore_nested() {
        let temp = TempDir::new().unwrap();
        let root = temp.path();
        fs::create_dir(root.join(".git")).unwrap();
        fs::write(root.join(".gitignore"), "*.log\n").unwrap();

        let subdir = root.join("src");
        fs::create_dir(&subdir).unwrap();
        fs::write(subdir.join(".gitignore"), "*.tmp\n").unwrap();

        let gitignore = build_gitignore(&subdir);
        assert!(gitignore.matched(Path::new("debug.log"), false).is_ignore());
        assert!(gitignore.matched(Path::new("cache.tmp"), false).is_ignore());
    }

    #[test]
    fn test_load_hint_files_basic() {
        let temp = TempDir::new().unwrap();
        let root = temp.path();

        create_file(root, DEFAULT_HINTS_FILENAME, "Test hint content");
        let gitignore = create_dummy_gitignore();
        let config_dir = temp.path().join("config");
        fs::create_dir(&config_dir).unwrap();

        let hints = load_hint_files(root, &config_dir, &default_hints_filenames(), &gitignore);
        assert!(hints.contains("Test hint content"));
        assert!(hints.contains("Project Hints"));
    }

    #[test]
    fn test_load_hint_files_multiple_filenames() {
        let temp = TempDir::new().unwrap();
        let root = temp.path();

        create_file(root, "CLAUDE.md", "Claude hints content");
        create_file(root, DEFAULT_HINTS_FILENAME, "RustyCode hints content");
        let gitignore = create_dummy_gitignore();
        let config_dir = temp.path().join("config");
        fs::create_dir(&config_dir).unwrap();

        let hints = load_hint_files(root, &config_dir, &default_hints_filenames(), &gitignore);
        assert!(hints.contains("Claude hints content"));
        assert!(hints.contains("RustyCode hints content"));
    }

    #[test]
    fn test_load_hint_files_nested_with_git() {
        let temp = TempDir::new().unwrap();
        let root = temp.path();
        fs::create_dir(root.join(".git")).unwrap();

        create_file(root, DEFAULT_HINTS_FILENAME, "Root hints");
        let subdir = root.join("subdir");
        fs::create_dir(&subdir).unwrap();
        create_file(&subdir, DEFAULT_HINTS_FILENAME, "Subdir hints");

        let current = subdir.join("work");
        fs::create_dir(&current).unwrap();
        create_file(&current, DEFAULT_HINTS_FILENAME, "Current hints");

        let gitignore = create_dummy_gitignore();
        let config_dir = temp.path().join("config");
        fs::create_dir(&config_dir).unwrap();

        let hints = load_hint_files(
            &current,
            &config_dir,
            &default_hints_filenames(),
            &gitignore,
        );
        assert!(hints.contains("Root hints"));
        assert!(hints.contains("Subdir hints"));
        assert!(hints.contains("Current hints"));
    }

    #[test]
    fn test_load_hint_files_without_git() {
        let temp = TempDir::new().unwrap();
        let base = temp.path();

        let current = base.join("current");
        fs::create_dir(&current).unwrap();
        create_file(&current, DEFAULT_HINTS_FILENAME, "Current hints");

        let parent = base.join("parent.md");
        fs::write(&parent, "Parent content").unwrap();

        let gitignore = create_dummy_gitignore();
        let config_dir = temp.path().join("config");
        fs::create_dir(&config_dir).unwrap();

        let hints = load_hint_files(
            &current,
            &config_dir,
            &default_hints_filenames(),
            &gitignore,
        );
        assert!(hints.contains("Current hints"));
        // Without git, should only load from current directory
        assert!(!hints.contains("Parent content"));
    }

    #[test]
    fn test_load_hint_files_global() {
        let temp = TempDir::new().unwrap();
        let root = temp.path();
        let config_dir = temp.path().join("config");
        fs::create_dir(&config_dir).unwrap();

        create_file(&config_dir, DEFAULT_HINTS_FILENAME, "Global hint content");
        let gitignore = create_dummy_gitignore();

        let hints = load_hint_files(root, &config_dir, &default_hints_filenames(), &gitignore);
        assert!(hints.contains("Global hint content"));
        assert!(hints.contains("Global Hints"));
    }

    #[test]
    fn test_load_hint_files_empty() {
        let temp = TempDir::new().unwrap();
        let root = temp.path();
        let config_dir = temp.path().join("config");
        fs::create_dir(&config_dir).unwrap();

        let gitignore = create_dummy_gitignore();
        let hints = load_hint_files(root, &config_dir, &default_hints_filenames(), &gitignore);
        assert!(hints.is_empty());
    }

    #[test]
    fn test_resolve_to_parent_dir_relative() {
        let wd = Path::new("/home/user/project");
        assert_eq!(
            resolve_to_parent_dir("src/main.rs", wd),
            Some(PathBuf::from("/home/user/project/src"))
        );
    }

    #[test]
    fn test_resolve_to_parent_dir_absolute() {
        let wd = Path::new("/home/user/project");
        assert_eq!(
            resolve_to_parent_dir("/tmp/foo.rs", wd),
            Some(PathBuf::from("/tmp"))
        );
    }

    #[test]
    fn test_resolve_to_parent_dir_no_parent() {
        let wd = Path::new("/home/user/project");
        // Root path has no parent
        assert_eq!(resolve_to_parent_dir("/", wd), None);
    }

    // ── SubdirectoryHintTracker Tests ──────────────────────────────────────

    #[test]
    fn test_tracker_records_path_argument() {
        let wd = PathBuf::from("/home/user/project");
        let mut tracker = SubdirectoryHintTracker::new();
        let args: serde_json::Map<String, serde_json::Value> =
            serde_json::from_str(r#"{"path": "src/main.rs"}"#).unwrap();
        tracker.record_tool_arguments(&Some(args), &wd);
        let hints = tracker.load_new_hints(&wd);
        assert!(hints.is_empty()); // No actual files
        assert!(tracker.is_loaded(&PathBuf::from("/home/user/project/src")));
    }

    #[test]
    fn test_tracker_records_command_argument() {
        let wd = PathBuf::from("/home/user/project");
        let mut tracker = SubdirectoryHintTracker::new();
        let args: serde_json::Map<String, serde_json::Value> =
            serde_json::from_str(r#"{"command": "cat nested/doc.md"}"#).unwrap();
        tracker.record_tool_arguments(&Some(args), &wd);
        let _ = tracker.load_new_hints(&wd);
        assert!(tracker.is_loaded(&PathBuf::from("/home/user/project/nested")));
    }

    #[test]
    fn test_tracker_skips_flags_in_command() {
        let wd = PathBuf::from("/home/user/project");
        let mut tracker = SubdirectoryHintTracker::new();
        let args: serde_json::Map<String, serde_json::Value> =
            serde_json::from_str(r#"{"command": "grep -rn pattern src/lib.rs"}"#).unwrap();
        tracker.record_tool_arguments(&Some(args), &wd);
        let _ = tracker.load_new_hints(&wd);
        assert!(tracker.is_loaded(&PathBuf::from("/home/user/project/src")));
        assert_eq!(tracker.loaded_count(), 1);
    }

    #[test]
    fn test_tracker_loads_subdirectory_hints() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().to_path_buf();
        let subdir = root.join("nested");
        fs::create_dir_all(&subdir).unwrap();
        create_file(&subdir, DEFAULT_HINTS_FILENAME, "nested subdirectory hints");

        let mut tracker = SubdirectoryHintTracker::new();
        let args: serde_json::Map<String, serde_json::Value> =
            serde_json::from_str(r#"{"path": "nested/foo.rs"}"#).unwrap();
        tracker.record_tool_arguments(&Some(args), &root);
        let hints = tracker.load_new_hints(&root);
        assert_eq!(hints.len(), 1);
        assert!(hints[0].0.contains("nested"));
        assert!(hints[0].1.contains("nested subdirectory hints"));
    }

    #[test]
    fn test_tracker_deduplicates_directories() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().to_path_buf();
        let subdir = root.join("nested");
        fs::create_dir_all(&subdir).unwrap();
        create_file(&subdir, DEFAULT_HINTS_FILENAME, "nested hints");

        let mut tracker = SubdirectoryHintTracker::new();
        let args: serde_json::Map<String, serde_json::Value> =
            serde_json::from_str(r#"{"path": "nested/foo.rs"}"#).unwrap();
        tracker.record_tool_arguments(&Some(args.clone()), &root);
        let hints = tracker.load_new_hints(&root);
        assert_eq!(hints.len(), 1);

        // Second access should not produce hints (already loaded)
        tracker.record_tool_arguments(&Some(args), &root);
        let hints = tracker.load_new_hints(&root);
        assert!(hints.is_empty());
    }

    #[test]
    fn test_tracker_no_arguments() {
        let wd = PathBuf::from("/home/user/project");
        let mut tracker = SubdirectoryHintTracker::new();
        tracker.record_tool_arguments(&None, &wd);
        let hints = tracker.load_new_hints(&wd);
        assert!(hints.is_empty());
    }

    #[test]
    fn test_tracker_reset() {
        let wd = PathBuf::from("/home/user/project");
        let mut tracker = SubdirectoryHintTracker::new();
        let args: serde_json::Map<String, serde_json::Value> =
            serde_json::from_str(r#"{"path": "src/main.rs"}"#).unwrap();
        tracker.record_tool_arguments(&Some(args), &wd);
        let _ = tracker.load_new_hints(&wd);
        assert_eq!(tracker.loaded_count(), 1);

        tracker.reset();
        assert_eq!(tracker.loaded_count(), 0);
    }

    #[test]
    fn test_tracker_ignores_working_dir() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().to_path_buf();
        create_file(&root, DEFAULT_HINTS_FILENAME, "root hints");

        let mut tracker = SubdirectoryHintTracker::new();
        let args: serde_json::Map<String, serde_json::Value> =
            serde_json::from_str(r#"{"path": "file.rs"}"#).unwrap();
        tracker.record_tool_arguments(&Some(args), &root);
        let hints = tracker.load_new_hints(&root);
        // Should not load hints from the working dir itself
        assert!(hints.is_empty());
    }

    // ── Gitignore Integration Tests ──────────────────────────────────────

    #[test]
    fn test_gitignore_filters_referenced_files() {
        let temp = TempDir::new().unwrap();
        let root = temp.path();
        fs::create_dir(root.join(".git")).unwrap();

        create_file(root, "allowed.md", "Allowed content");
        create_file(root, "secret.env", "SECRET_KEY=abc123");
        fs::write(root.join(".gitignore"), "*.env\n").unwrap();

        let hints_content = "Project hints\n@allowed.md\n@secret.env\nEnd of hints";
        create_file(root, DEFAULT_HINTS_FILENAME, hints_content);

        let gitignore = build_gitignore(root);
        let config_dir = temp.path().join("config");
        fs::create_dir(&config_dir).unwrap();

        let hints = load_hint_files(root, &config_dir, &default_hints_filenames(), &gitignore);
        assert!(hints.contains("Allowed content"));
        assert!(!hints.contains("SECRET_KEY=abc123"));
        // The reference should be left as-is (not expanded)
        assert!(hints.contains("@secret.env"));
    }

    #[test]
    fn test_gitignore_merged_nested() {
        let temp = TempDir::new().unwrap();
        let root = temp.path();
        fs::create_dir(root.join(".git")).unwrap();
        fs::write(root.join(".gitignore"), "*.log\n").unwrap();

        let subdir = root.join("subdir");
        fs::create_dir(&subdir).unwrap();
        fs::write(subdir.join(".gitignore"), "*.tmp\n").unwrap();

        create_file(root, "debug.log", "debug log");
        create_file(&subdir, "cache.tmp", "temp data");
        create_file(&subdir, "readme.md", "Readme content");

        let hints_content = "Hints\n@../debug.log\n@cache.tmp\n@readme.md\nEnd";
        create_file(&subdir, DEFAULT_HINTS_FILENAME, hints_content);

        let gitignore = build_gitignore(&subdir);
        let config_dir = temp.path().join("config");
        fs::create_dir(&config_dir).unwrap();

        let hints = load_hint_files(&subdir, &config_dir, &default_hints_filenames(), &gitignore);
        assert!(hints.contains("Readme content"));
        assert!(!hints.contains("debug log"));
        assert!(!hints.contains("temp data"));
    }

    #[test]
    fn test_import_boundary_with_git_root() {
        let temp = TempDir::new().unwrap();
        let root = temp.path();
        fs::create_dir(root.join(".git")).unwrap();

        create_file(root, "root_file.md", "Root file content");
        let subdir = root.join("subdir");
        fs::create_dir(&subdir).unwrap();
        create_file(&subdir, "local_file.md", "Local content");

        let hints_content = "Subdir hints\n@local_file.md\n@../root_file.md\nEnd";
        create_file(&subdir, DEFAULT_HINTS_FILENAME, hints_content);

        let gitignore = create_dummy_gitignore();
        let config_dir = temp.path().join("config");
        fs::create_dir(&config_dir).unwrap();

        let hints = load_hint_files(&subdir, &config_dir, &default_hints_filenames(), &gitignore);
        // Both should be accessible within the git root boundary
        assert!(hints.contains("Local content"));
        assert!(hints.contains("Root file content"));
    }

    #[test]
    fn test_default_hints_filenames() {
        let names = default_hints_filenames();
        assert!(names.contains(&DEFAULT_HINTS_FILENAME.to_string()));
        assert!(names.contains(&CLAUDE_MD_FILENAME.to_string()));
    }

    #[test]
    fn test_strict_descendant_rejects_prefix_siblings() {
        let root = Path::new("/tmp/work");
        assert!(!is_strict_descendant(Path::new("/tmp/worktree"), root));
        assert!(is_strict_descendant(Path::new("/tmp/work/sub"), root));
    }
}
