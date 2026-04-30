//! Smart commit message generation from git diffs.
//!
//! Generates conventional-commit-style messages by analyzing staged changes.
//! Can operate in two modes:
//! - **Rule-based** (default): Fast, no LLM needed — analyzes diff stats and filenames.
//! - **LLM-enhanced**: Uses a language model for richer messages when a provider is available.

use std::path::Path;
use std::process::Command;

/// A generated commit message.
#[derive(Debug, Clone)]
pub struct CommitMessage {
    /// The commit type (feat, fix, refactor, docs, test, chore, perf, ci).
    pub commit_type: CommitType,
    /// The scope (e.g., crate name or module).
    pub scope: Option<String>,
    /// The description line.
    pub description: String,
    /// Optional body with additional details.
    pub body: Option<String>,
}

/// Conventional commit types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
pub enum CommitType {
    Feat,
    Fix,
    Refactor,
    Docs,
    Test,
    Chore,
    Perf,
    Ci,
}

impl std::fmt::Display for CommitType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CommitType::Feat => write!(f, "feat"),
            CommitType::Fix => write!(f, "fix"),
            CommitType::Refactor => write!(f, "refactor"),
            CommitType::Docs => write!(f, "docs"),
            CommitType::Test => write!(f, "test"),
            CommitType::Chore => write!(f, "chore"),
            CommitType::Perf => write!(f, "perf"),
            CommitType::Ci => write!(f, "ci"),
        }
    }
}

/// Diff summary for a set of changes.
#[derive(Debug, Clone)]
struct DiffSummary {
    files_added: Vec<String>,
    files_modified: Vec<String>,
    files_deleted: Vec<String>,
    lines_added: usize,
    lines_removed: usize,
    scopes: Vec<String>,
}

/// Generate a commit message from the staged git diff in the given directory.
///
/// Uses rule-based analysis (no LLM required).
pub fn generate_commit_message(repo_path: &Path) -> Option<CommitMessage> {
    let diff = get_staged_diff(repo_path)?;
    let summary = analyze_diff(&diff)?;
    build_message_from_summary(&summary)
}

/// Generate a commit message from a raw diff string.
pub fn generate_from_diff(diff: &str) -> Option<CommitMessage> {
    let summary = analyze_diff(diff)?;
    build_message_from_summary(&summary)
}

/// Get the staged diff from a git repository.
fn get_staged_diff(repo_path: &Path) -> Option<String> {
    let output = Command::new("git")
        .args(["diff", "--cached", "--stat", "--patch"])
        .current_dir(repo_path)
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let diff = String::from_utf8_lossy(&output.stdout).to_string();
    if diff.trim().is_empty() {
        return None;
    }

    Some(diff)
}

/// Analyze a diff to extract summary information.
fn analyze_diff(diff: &str) -> Option<DiffSummary> {
    let mut files_added = Vec::new();
    let mut files_modified = Vec::new();
    let mut files_deleted = Vec::new();
    let mut lines_added = 0usize;
    let mut lines_removed = 0usize;
    let mut scope_set = std::collections::HashSet::new();

    // First pass: count lines added/removed
    for line in diff.lines() {
        if line.starts_with("+") && !line.starts_with("+++") {
            lines_added += 1;
        } else if line.starts_with("-") && !line.starts_with("---") {
            lines_removed += 1;
        }
    }

    // Second pass: classify files using diff headers
    let mut is_new_file = false;
    let mut is_deleted_file = false;

    for line in diff.lines() {
        if line.starts_with("diff --git") {
            is_new_file = false;
            is_deleted_file = false;
        } else if line.starts_with("new file") {
            is_new_file = true;
        } else if line.starts_with("deleted file") {
            is_deleted_file = true;
        } else if line.starts_with("+++ b/") {
            let path = line.trim_start_matches("+++ b/").to_string();
            if is_new_file {
                files_added.push(path.clone());
            } else if !is_deleted_file {
                files_modified.push(path.clone());
            }
            extract_scope(&path, &mut scope_set);
        } else if is_deleted_file && line.starts_with("--- a/") {
            let path = line.trim_start_matches("--- a/").to_string();
            files_deleted.push(path.clone());
            extract_scope(&path, &mut scope_set);
        }
    }

    if files_added.is_empty() && files_modified.is_empty() && files_deleted.is_empty() {
        return None;
    }

    let scopes: Vec<String> = scope_set.into_iter().collect();

    Some(DiffSummary {
        files_added,
        files_modified,
        files_deleted,
        lines_added,
        lines_removed,
        scopes,
    })
}

/// Extract a scope (crate name, module name) from a file path.
fn extract_scope(path: &str, scopes: &mut std::collections::HashSet<String>) {
    // crates/foo-bar/src/... → foo-bar
    if let Some(crates_idx) = path.find("crates/") {
        let after_crates = &path[crates_idx + 7..];
        if let Some(slash_idx) = after_crates.find('/') {
            let crate_name = &after_crates[..slash_idx];
            scopes.insert(crate_name.to_string());
        }
    }

    // src/module.rs → module
    if let Some(src_idx) = path.find("src/") {
        let after_src = &path[src_idx + 4..];
        if let Some(slash_idx) = after_src.find('/') {
            let module = &after_src[..slash_idx];
            scopes.insert(module.to_string());
        } else if let Some(dot_idx) = after_src.rfind('.') {
            scopes.insert(after_src[..dot_idx].to_string());
        }
    }

    // pkg/module/... → module
    if let Some(pkg_idx) = path.find("pkg/") {
        let after_pkg = &path[pkg_idx + 4..];
        if let Some(slash_idx) = after_pkg.find('/') {
            scopes.insert(after_pkg[..slash_idx].to_string());
        }
    }
}

/// Build a commit message from the diff summary.
fn build_message_from_summary(summary: &DiffSummary) -> Option<CommitMessage> {
    let total_files =
        summary.files_added.len() + summary.files_modified.len() + summary.files_deleted.len();
    if total_files == 0 {
        return None;
    }

    let commit_type = infer_commit_type(summary);
    let scope = choose_scope(summary);
    let description = build_description(summary);
    let body = build_body(summary);

    Some(CommitMessage {
        commit_type,
        scope,
        description,
        body,
    })
}

/// Infer the commit type from the changes.
fn infer_commit_type(summary: &DiffSummary) -> CommitType {
    let all_test = summary
        .files_added
        .iter()
        .chain(summary.files_modified.iter())
        .all(|f| {
            f.contains("test")
                || f.ends_with("_test.rs")
                || f.ends_with("_test.go")
                || f.contains("tests/")
        });

    if all_test && !summary.files_added.is_empty() {
        return CommitType::Test;
    }

    let all_docs = summary
        .files_added
        .iter()
        .chain(summary.files_modified.iter())
        .all(|f| {
            f.ends_with(".md") || f.ends_with(".txt") || f.ends_with(".rst") || f.contains("doc")
        });

    if all_docs {
        return CommitType::Docs;
    }

    // New files with substantial code → feat
    if !summary.files_added.is_empty() && summary.lines_added > 20 {
        return CommitType::Feat;
    }

    // Mostly removals → refactor or fix
    if summary.lines_removed > summary.lines_added * 2 {
        return CommitType::Refactor;
    }

    // Mixed changes → default to chore
    CommitType::Chore
}

/// Choose the best scope for the commit.
fn choose_scope(summary: &DiffSummary) -> Option<String> {
    if summary.scopes.len() == 1 {
        return Some(summary.scopes[0].clone());
    }

    if summary.scopes.len() > 1 {
        // Pick the most common scope
        let mut counts = std::collections::HashMap::new();
        for file in summary
            .files_added
            .iter()
            .chain(summary.files_modified.iter())
        {
            for scope in &summary.scopes {
                if file.contains(scope) {
                    *counts.entry(scope.as_str()).or_insert(0) += 1;
                }
            }
        }
        return counts
            .into_iter()
            .max_by_key(|(_, c)| *c)
            .map(|(s, _)| s.to_string());
    }

    None
}

/// Build the description line.
fn build_description(summary: &DiffSummary) -> String {
    let total_files =
        summary.files_added.len() + summary.files_modified.len() + summary.files_deleted.len();

    if total_files == 1 {
        // Single file — describe what happened
        if let Some(f) = summary.files_added.first() {
            let name = file_name(f);
            return format!("add {}", name);
        }
        if let Some(f) = summary.files_modified.first() {
            let name = file_name(f);
            return format!("update {}", name);
        }
        if let Some(f) = summary.files_deleted.first() {
            let name = file_name(f);
            return format!("remove {}", name);
        }
    }

    // Multiple files
    let mut parts = Vec::new();
    if !summary.files_added.is_empty() {
        parts.push(format!(
            "{} new file{}",
            summary.files_added.len(),
            if summary.files_added.len() > 1 {
                "s"
            } else {
                ""
            }
        ));
    }
    if !summary.files_modified.is_empty() {
        parts.push(format!(
            "{} modification{}",
            summary.files_modified.len(),
            if summary.files_modified.len() > 1 {
                "s"
            } else {
                ""
            }
        ));
    }
    if !summary.files_deleted.is_empty() {
        parts.push(format!(
            "{} deletion{}",
            summary.files_deleted.len(),
            if summary.files_deleted.len() > 1 {
                "s"
            } else {
                ""
            }
        ));
    }

    parts.join(", ")
}

/// Build the optional body with file list.
fn build_body(summary: &DiffSummary) -> Option<String> {
    if summary.files_added.len() + summary.files_modified.len() + summary.files_deleted.len() <= 2 {
        return None;
    }

    let mut body = String::new();

    if !summary.files_added.is_empty() {
        body.push_str("New files:\n");
        for f in &summary.files_added {
            body.push_str(&format!("  - {}\n", f));
        }
    }

    if !summary.files_modified.is_empty() {
        body.push_str("Modified:\n");
        for f in summary.files_modified.iter().take(10) {
            body.push_str(&format!("  - {}\n", f));
        }
        if summary.files_modified.len() > 10 {
            body.push_str(&format!(
                "  ... and {} more\n",
                summary.files_modified.len() - 10
            ));
        }
    }

    if !summary.files_deleted.is_empty() {
        body.push_str("Deleted:\n");
        for f in &summary.files_deleted {
            body.push_str(&format!("  - {}\n", f));
        }
    }

    body.push_str(&format!(
        "\n{} lines added, {} lines removed",
        summary.lines_added, summary.lines_removed
    ));

    Some(body)
}

/// Format the full commit message string.
impl std::fmt::Display for CommitMessage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.scope {
            Some(scope) => write!(f, "{}({}): {}", self.commit_type, scope, self.description)?,
            None => write!(f, "{}: {}", self.commit_type, self.description)?,
        }

        if let Some(body) = &self.body {
            write!(f, "\n\n{}", body)?;
        }

        Ok(())
    }
}

/// Extract just the filename from a path.
fn file_name(path: &str) -> String {
    std::path::Path::new(path)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| path.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_commit_type_display() {
        assert_eq!(CommitType::Feat.to_string(), "feat");
        assert_eq!(CommitType::Fix.to_string(), "fix");
        assert_eq!(CommitType::Refactor.to_string(), "refactor");
        assert_eq!(CommitType::Chore.to_string(), "chore");
    }

    #[test]
    fn test_analyze_diff_single_file() {
        let diff = "\
diff --git a/src/main.rs b/src/main.rs
index abc..def 100644
--- a/src/main.rs
+++ b/src/main.rs
@@ -1,3 +1,4 @@
 fn main() {
+    println!(\"hello\");
 }
 ";
        let summary = analyze_diff(diff).unwrap();
        assert_eq!(summary.files_modified.len(), 1);
        assert_eq!(summary.lines_added, 1);
        assert_eq!(summary.lines_removed, 0);
    }

    #[test]
    fn test_analyze_diff_new_file() {
        let diff = "\
diff --git a/src/new_module.rs b/src/new_module.rs
new file mode 100644
--- /dev/null
+++ b/src/new_module.rs
@@ -0,0 +1,5 @@
+pub fn hello() -> &'static str {
+    \"hello\"
+}
 ";
        let summary = analyze_diff(diff).unwrap();
        assert_eq!(summary.files_added.len(), 1);
        assert!(summary.files_modified.is_empty());
        assert_eq!(summary.lines_added, 3);
    }

    #[test]
    fn test_analyze_diff_deleted_file() {
        let diff = "\
diff --git a/src/old.rs b/src/old.rs
deleted file mode 100644
--- a/src/old.rs
+++ /dev/null
@@ -1,2 +0,0 @@
-pub fn old() {}
 ";
        let summary = analyze_diff(diff).unwrap();
        assert!(summary.files_deleted.iter().any(|f| f.contains("old.rs")));
        assert_eq!(summary.lines_removed, 1);
    }

    #[test]
    fn test_extract_scope_crate() {
        let mut scopes = std::collections::HashSet::new();
        extract_scope("crates/rustycode-tools/src/lib.rs", &mut scopes);
        assert!(scopes.contains("rustycode-tools"));
    }

    #[test]
    fn test_extract_scope_src_module() {
        let mut scopes = std::collections::HashSet::new();
        extract_scope("src/auth/login.rs", &mut scopes);
        assert!(scopes.contains("auth"));
    }

    #[test]
    fn test_infer_commit_type_test_files() {
        let summary = DiffSummary {
            files_added: vec!["tests/integration_test.rs".to_string()],
            files_modified: vec![],
            files_deleted: vec![],
            lines_added: 50,
            lines_removed: 0,
            scopes: vec![],
        };
        assert_eq!(infer_commit_type(&summary), CommitType::Test);
    }

    #[test]
    fn test_infer_commit_type_docs() {
        let summary = DiffSummary {
            files_added: vec![],
            files_modified: vec!["README.md".to_string()],
            files_deleted: vec![],
            lines_added: 10,
            lines_removed: 5,
            scopes: vec![],
        };
        assert_eq!(infer_commit_type(&summary), CommitType::Docs);
    }

    #[test]
    fn test_infer_commit_type_new_feature() {
        let summary = DiffSummary {
            files_added: vec!["src/cache.rs".to_string()],
            files_modified: vec!["src/lib.rs".to_string()],
            files_deleted: vec![],
            lines_added: 150,
            lines_removed: 0,
            scopes: vec![],
        };
        assert_eq!(infer_commit_type(&summary), CommitType::Feat);
    }

    #[test]
    fn test_build_description_single_file() {
        let summary = DiffSummary {
            files_added: vec!["src/new.rs".to_string()],
            files_modified: vec![],
            files_deleted: vec![],
            lines_added: 10,
            lines_removed: 0,
            scopes: vec![],
        };
        let desc = build_description(&summary);
        assert!(desc.contains("new.rs"));
    }

    #[test]
    fn test_build_description_multiple_files() {
        let summary = DiffSummary {
            files_added: vec!["a.rs".to_string()],
            files_modified: vec!["b.rs".to_string(), "c.rs".to_string()],
            files_deleted: vec![],
            lines_added: 10,
            lines_removed: 5,
            scopes: vec![],
        };
        let desc = build_description(&summary);
        assert!(desc.contains("1 new file"));
        assert!(desc.contains("2 modifications"));
    }

    #[test]
    fn test_commit_message_format_no_scope() {
        let msg = CommitMessage {
            commit_type: CommitType::Feat,
            scope: None,
            description: "add caching layer".to_string(),
            body: None,
        };
        assert_eq!(msg.to_string(), "feat: add caching layer");
    }

    #[test]
    fn test_commit_message_format_with_scope() {
        let msg = CommitMessage {
            commit_type: CommitType::Fix,
            scope: Some("auth".to_string()),
            description: "fix null check in login".to_string(),
            body: None,
        };
        assert_eq!(msg.to_string(), "fix(auth): fix null check in login");
    }

    #[test]
    fn test_commit_message_format_with_body() {
        let msg = CommitMessage {
            commit_type: CommitType::Chore,
            scope: None,
            description: "update dependencies".to_string(),
            body: Some("Updated serde to 1.0.190\nUpdated tokio to 1.35".to_string()),
        };
        let formatted = msg.to_string();
        assert!(formatted.starts_with("chore: update dependencies\n\n"));
        assert!(formatted.contains("serde"));
    }

    #[test]
    fn test_generate_from_diff_empty() {
        assert!(generate_from_diff("").is_none());
    }

    #[test]
    fn test_generate_from_diff_valid() {
        // 25 lines to cross the 20-line Feat threshold
        let mut diff_lines = vec![
            "diff --git a/crates/tools/src/cache.rs b/crates/tools/src/cache.rs".to_string(),
            "new file mode 100644".to_string(),
            "--- /dev/null".to_string(),
            "+++ b/crates/tools/src/cache.rs".to_string(),
            "@@ -0,0 +1,30 @@".to_string(),
        ];
        for i in 0..25 {
            diff_lines.push(format!("+// line {}", i));
        }
        let diff = diff_lines.join("\n");
        let msg = generate_from_diff(&diff).unwrap();
        assert_eq!(msg.commit_type, CommitType::Feat);
        assert!(
            msg.scope.is_some(),
            "should detect a scope from crates/tools/src/cache.rs"
        );
    }

    #[test]
    fn test_analyze_empty_diff() {
        assert!(analyze_diff("").is_none());
    }

    #[test]
    fn test_analyze_diff_no_changes() {
        let diff = "diff --git a/file.rs b/file.rs\nindex abc..def 100644\n";
        // No actual file changes detected (no +++ b/ lines with content)
        // The diff header exists but no actual modifications
        let result = analyze_diff(diff);
        // Should return None because no files are categorized
        assert!(result.is_none());
    }
}
