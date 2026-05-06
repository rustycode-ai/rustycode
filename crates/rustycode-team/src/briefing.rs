//! BriefingBuilder -- reconstructs a structured Briefing from disk every turn.
//!
//! This is the "fresh mind" mechanism: instead of carrying raw conversation
//! history forward, each turn starts with a clean slate plus a curated briefing
//! that contains only structured, verified information.
//!
//! Key principle: state lives on disk, not in context. The briefing is
//! reconstructed from the current state of the world (files, tests, attempts),
//! not from what the agent remembers about previous turns.
//!
//! # Usage
//!
//! ```ignore
//! use rustycode_core::team::briefing::BriefingBuilder;
//!
//! let builder = BriefingBuilder::new("/path/to/project");
//! let briefing = builder.build(
//!     "Fix the auth token validation bug",
//!     &["src/auth.rs".to_string()],
//!     &attempt_log,
//!     &insights,
//!     Some(verification_state),
//! ).await?;
//! ```

use anyhow::{Context, Result};
use rustycode_protocol::team::*;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use tokio::fs;

/// Default maximum number of lines included per file snippet.
const DEFAULT_MAX_SNIPPET_LINES: usize = 100;

/// Default maximum number of attempt summaries retained.
const DEFAULT_MAX_ATTEMPTS: usize = 5;

/// Directory within the project where session data is stored.
const RUSTYCODE_DIR: &str = ".rustycode";

/// Reconstructs a Briefing from disk every turn.
///
/// Each call to `build` reads files fresh from disk, compresses the attempt
/// log, deduplicates insights, and detects project constraints. Nothing is
/// cached between calls -- every turn gets a completely fresh view.
pub struct BriefingBuilder {
    /// Root of the project being worked on.
    project_root: PathBuf,
    /// Maximum number of lines per file snippet (default 100).
    max_snippet_lines: usize,
    /// Maximum number of attempt summaries to keep (default 5).
    max_attempts: usize,
}

impl BriefingBuilder {
    pub fn new(project_root: impl Into<PathBuf>) -> Self {
        Self {
            project_root: project_root.into(),
            max_snippet_lines: DEFAULT_MAX_SNIPPET_LINES,
            max_attempts: DEFAULT_MAX_ATTEMPTS,
        }
    }

    /// Set a custom maximum snippet line count.
    pub fn with_max_snippet_lines(mut self, lines: usize) -> Self {
        self.max_snippet_lines = lines;
        self
    }

    /// Set a custom maximum number of retained attempts.
    pub fn with_max_attempts(mut self, max: usize) -> Self {
        self.max_attempts = max;
        self
    }

    /// Build a fresh briefing from the current state of the world.
    ///
    /// Reads files from disk (not from memory), compresses the attempt history,
    /// and curates insights. Every call is a complete reconstruction.
    pub async fn build(
        &self,
        task: &str,
        dirty_files: &[String],
        attempt_log: &[AttemptSummary],
        insights: &[String],
        verification: Option<VerificationState>,
    ) -> Result<Briefing> {
        let relevant_code = self.read_relevant_files(dirty_files).await;
        let compressed_attempts = self.compress_attempts(attempt_log);
        let curated_insights = self.curate_insights(insights);
        let constraints = self.detect_constraints().await;
        let current_approach = self.derive_current_approach(&compressed_attempts);
        let learnings = self.load_project_learnings(task).await;
        let few_shot_examples = self.load_few_shot_examples(task).await;

        Ok(Briefing {
            task: task.to_string(),
            relevant_code,
            attempts: compressed_attempts,
            insights: curated_insights,
            current_approach,
            constraints,
            verification_state: verification,
            structural_declaration: None,
            learnings,
            few_shot_examples,
        })
    }

    /// Read the current state of relevant files from disk.
    ///
    /// Only reads files whose paths are provided (dirty files or files in scope).
    /// Each file is truncated to `max_snippet_lines` and annotated with its line range.
    /// Files that cannot be read are silently skipped -- the briefing should still
    /// be usable even if some files are missing or deleted.
    pub async fn read_relevant_files(&self, paths: &[String]) -> Vec<FileSnippet> {
        let mut snippets = Vec::with_capacity(paths.len());

        for path_str in paths {
            let full_path = self.project_root.join(path_str);
            match fs::read_to_string(&full_path).await {
                Ok(content) => {
                    let total_lines = content.lines().count();
                    let (truncated, line_range) = if total_lines > self.max_snippet_lines {
                        let kept: String = content
                            .lines()
                            .take(self.max_snippet_lines)
                            .collect::<Vec<&str>>()
                            .join("\n");
                        (kept, Some((1, self.max_snippet_lines)))
                    } else {
                        (content, Some((1, total_lines)))
                    };

                    snippets.push(FileSnippet {
                        path: path_str.clone(),
                        content: truncated,
                        line_range,
                    });
                }
                Err(_) => {
                    // File may have been deleted or is inaccessible.
                    // Skip silently -- the briefing must still be buildable.
                }
            }
        }

        snippets
    }

    /// Compress attempt history to structured summaries.
    ///
    /// Retains at most `max_attempts` entries (default 5), dropping the oldest
    /// when the limit is exceeded. Each AttemptSummary already contains
    /// structured data, so no further compression is needed.
    pub fn compress_attempts(&self, attempts: &[AttemptSummary]) -> Vec<AttemptSummary> {
        if attempts.len() <= self.max_attempts {
            return attempts.to_vec();
        }

        // Keep the most recent entries.
        let start = attempts.len() - self.max_attempts;
        attempts[start..].to_vec()
    }

    /// Deduplicate insights by exact string match.
    ///
    /// Preserves insertion order and keeps the first occurrence of each insight.
    /// This is intentionally simple -- fuzzy deduplication can be added later
    /// if needed.
    pub fn curate_insights(&self, raw_insights: &[String]) -> Vec<String> {
        let mut seen = HashSet::new();
        let mut curated = Vec::with_capacity(raw_insights.len());

        for insight in raw_insights {
            let normalized = insight.trim().to_lowercase();
            if !normalized.is_empty() && seen.insert(normalized) {
                curated.push(insight.clone());
            }
        }

        curated
    }

    /// Detect project constraints from the filesystem.
    ///
    /// Examines:
    /// - Cargo.toml for Rust edition and MSRV
    /// - Presence of test files (implies "tests must pass")
    /// - CI configuration (.github/workflows/) (implies "CI must pass")
    pub async fn detect_constraints(&self) -> Vec<String> {
        let mut constraints = Vec::new();

        // Read Cargo.toml for Rust edition and MSRV.
        if let Some(cargo_constraints) = self.detect_rust_constraints().await {
            constraints.extend(cargo_constraints);
        }

        // Check for existing tests.
        if self.has_tests().await {
            constraints.push("existing tests must continue to pass".to_string());
        }

        // Check for CI configuration.
        if self.has_ci_config().await {
            constraints.push("CI pipeline must pass".to_string());
        }

        constraints
    }

    /// Derive the current approach from the latest attempt summary.
    ///
    /// Returns the approach string from the most recent attempt, or an empty
    /// string if no attempts have been recorded yet.
    fn derive_current_approach(&self, attempts: &[AttemptSummary]) -> String {
        attempts
            .last()
            .map(|a| a.approach.clone())
            .unwrap_or_default()
    }

    // -----------------------------------------------------------------------
    // Persistence
    // -----------------------------------------------------------------------

    /// Save briefing to disk for crash recovery.
    ///
    /// Writes to `.rustycode/session/{session_id}/briefing.json`.
    /// Creates parent directories as needed.
    pub async fn save(&self, briefing: &Briefing, session_id: &str) -> Result<()> {
        let dir = self.session_dir(session_id);
        fs::create_dir_all(&dir)
            .await
            .with_context(|| format!("failed to create session directory: {}", dir.display()))?;

        let path = dir.join("briefing.json");
        let json =
            serde_json::to_string_pretty(briefing).context("failed to serialize briefing")?;

        let tmp_path = dir.join("briefing.json.tmp");
        fs::write(&tmp_path, &json)
            .await
            .with_context(|| format!("failed to write briefing to {}", tmp_path.display()))?;
        fs::rename(&tmp_path, &path)
            .await
            .with_context(|| format!("failed to rename briefing to {}", path.display()))?;

        Ok(())
    }

    /// Load the last saved briefing from disk.
    ///
    /// Returns `Ok(None)` if no saved briefing exists (not an error -- first
    /// turn of a new session). Returns an error only if the file exists but
    /// cannot be parsed.
    pub async fn load(&self, session_id: &str) -> Result<Option<Briefing>> {
        let path = self.session_dir(session_id).join("briefing.json");

        match fs::read_to_string(&path).await {
            Ok(json) => {
                let briefing: Briefing = serde_json::from_str(&json)
                    .with_context(|| format!("failed to parse briefing from {}", path.display()))?;
                Ok(Some(briefing))
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => {
                Err(e).with_context(|| format!("failed to read briefing from {}", path.display()))
            }
        }
    }

    // -----------------------------------------------------------------------
    // Private helpers
    // -----------------------------------------------------------------------

    /// Load project learnings using semantic search.
    ///
    /// Searches vector memory for learnings relevant to the current task.
    /// Falls back to markdown-based learnings if vector memory is unavailable.
    #[cfg(feature = "vector-memory")]
    async fn load_project_learnings(&self, task: &str) -> String {
        use crate::team_learnings::TeamLearnings;
        use rustycode_vector_memory::{MemoryType, VectorMemory};

        // Try vector memory semantic search first
        let mut memory = VectorMemory::new(&self.project_root);
        if memory.init().is_ok() {
            // Search for relevant learnings semantically
            let results = memory.search(task, MemoryType::Learnings, 5);

            if !results.is_empty() {
                // Format semantic search results
                let mut formatted = String::from("# Relevant Learnings\n\n");
                for (i, result) in results.iter().enumerate() {
                    let similarity_pct = (result.similarity * 100.0) as i32;
                    formatted.push_str(&format!(
                        "{}. {} (relevance: {}%)\n",
                        i + 1,
                        result.entry.content,
                        similarity_pct
                    ));
                }
                formatted.push_str("\n---\n");

                // Also include markdown learnings for full context
                if let Ok(learnings) = TeamLearnings::load(&self.project_root) {
                    formatted.push_str(&learnings.all());
                }

                return formatted;
            }
        }

        // Fallback to markdown-only learnings
        match TeamLearnings::load(&self.project_root) {
            Ok(learnings) => learnings.all(),
            Err(_) => String::new(),
        }
    }

    /// Load project learnings (markdown-only fallback when vector memory is disabled).
    #[cfg(not(feature = "vector-memory"))]
    async fn load_project_learnings(&self, _task: &str) -> String {
        use crate::team_learnings::TeamLearnings;

        match TeamLearnings::load(&self.project_root) {
            Ok(learnings) => learnings.all(),
            Err(_) => String::new(),
        }
    }

    /// Load few-shot examples from similar past tasks.
    ///
    /// Searches vector memory for TaskTraces similar to the current task.
    /// Returns formatted examples showing what approach was taken and whether it succeeded.
    #[cfg(feature = "vector-memory")]
    async fn load_few_shot_examples(&self, task: &str) -> String {
        use rustycode_vector_memory::{MemoryType, VectorMemory};

        let mut memory = VectorMemory::new(&self.project_root);
        if memory.init().is_ok() {
            // Search for similar past tasks
            let results = memory.search(task, MemoryType::TaskTraces, 3);

            if !results.is_empty() {
                let mut formatted = String::from("## Similar Past Tasks\n\n");

                for (i, result) in results.iter().enumerate() {
                    let similarity_pct = (result.similarity * 100.0) as i32;
                    formatted.push_str(&format!(
                        "### Task {} ({}% similar)\n{}\n\n",
                        i + 1,
                        similarity_pct,
                        result.entry.content
                    ));
                }

                formatted.push_str("---\n\n");
                return formatted;
            }
        }

        // No similar tasks found
        String::new()
    }

    /// Load few-shot examples (stub when vector memory is disabled).
    #[cfg(not(feature = "vector-memory"))]
    async fn load_few_shot_examples(&self, _task: &str) -> String {
        String::new()
    }

    /// Compute the session directory path for a given session ID.
    fn session_dir(&self, session_id: &str) -> PathBuf {
        self.project_root
            .join(RUSTYCODE_DIR)
            .join("session")
            .join(session_id)
    }

    /// Detect Rust-specific constraints from Cargo.toml.
    async fn detect_rust_constraints(&self) -> Option<Vec<String>> {
        let cargo_path = self.project_root.join("Cargo.toml");
        let content = fs::read_to_string(&cargo_path).await.ok()?;

        let mut constraints = Vec::new();

        // Extract edition.
        if let Some(edition) = extract_toml_value(&content, "edition") {
            constraints.push(format!("Rust edition {}", edition));
        }

        // Extract MSRV (rust-version).
        if let Some(msrv) = extract_toml_value(&content, "rust-version") {
            constraints.push(format!("minimum Rust version {}", msrv));
        }

        if constraints.is_empty() {
            None
        } else {
            Some(constraints)
        }
    }

    /// Check whether the project contains any test files.
    ///
    /// Looks for the conventional `tests/` directory and any `*_test.rs` or
    /// `test_*.rs` files in the source tree. Returns true at the first match.
    async fn has_tests(&self) -> bool {
        // Check for a top-level tests/ directory.
        let tests_dir = self.project_root.join("tests");
        if tests_dir.is_dir() {
            return true;
        }

        // Check for test files in the source tree.
        // Use a bounded walk to avoid scanning huge projects.
        self.has_test_files_recursive(&self.project_root, 3).await
    }

    /// Recursively check for test files up to a given depth.
    fn has_test_files_recursive<'a>(
        &'a self,
        dir: &'a Path,
        max_depth: usize,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = bool> + 'a>> {
        Box::pin(async move {
            if max_depth == 0 {
                return false;
            }

            let Ok(mut entries) = fs::read_dir(dir).await else {
                return false;
            };

            while let Ok(Some(entry)) = entries.next_entry().await {
                let path = entry.path();
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    // Skip hidden directories and common non-source directories.
                    if name.starts_with('.') || name == "target" || name == "node_modules" {
                        continue;
                    }

                    if path.is_dir() {
                        if self.has_test_files_recursive(&path, max_depth - 1).await {
                            return true;
                        }
                    } else if name.ends_with("_test.rs")
                        || name.ends_with("_tests.rs")
                        || (name.starts_with("test_") && name.ends_with(".rs"))
                    {
                        return true;
                    }
                }
            }

            false
        })
    }

    /// Check whether the project has CI configuration files.
    async fn has_ci_config(&self) -> bool {
        // GitHub Actions.
        let gh_workflows = self.project_root.join(".github/workflows");
        if gh_workflows.is_dir() {
            return true;
        }

        // GitLab CI.
        if self.project_root.join(".gitlab-ci.yml").exists() {
            return true;
        }

        // CircleCI.
        let circle = self.project_root.join(".circleci/config.yml");
        if circle.exists() {
            return true;
        }

        false
    }
}

/// Extract a simple string value from TOML content by key name.
///
/// This is a minimal parser that handles `key = "value"` patterns in the
/// [package] section. It does not need a full TOML parser -- Cargo.toml fields
/// like `edition` and `rust-version` are always simple strings.
fn extract_toml_value(content: &str, key: &str) -> Option<String> {
    // Find [package] section, then look for the key within it.
    let mut in_package = false;

    for line in content.lines() {
        let trimmed = line.trim();

        if trimmed == "[package]" {
            in_package = true;
            continue;
        }

        // A new section header ends the [package] section.
        if trimmed.starts_with('[') && trimmed != "[package]" {
            in_package = false;
            continue;
        }

        if in_package && trimmed.starts_with(key) {
            // Parse: key = "value"  or  key = 'value'
            if let Some(rest) = trimmed.strip_prefix(key) {
                let rest = rest.trim();
                if let Some(value_part) = rest.strip_prefix('=') {
                    return extract_quoted_string(value_part.trim());
                }
            }
        }
    }

    None
}

/// Extract a quoted string value from a TOML value expression.
///
/// Handles both `"value"` and `'value'` quoting styles.
fn extract_quoted_string(s: &str) -> Option<String> {
    let s = s.trim();
    if (s.starts_with('"') && s.ends_with('"') && s.len() >= 2)
        || (s.starts_with('\'') && s.ends_with('\'') && s.len() >= 2)
    {
        Some(s[1..s.len() - 1].to_string())
    } else {
        None
    }
}

// Tests

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    /// Helper: create a test BriefingBuilder pointing at a temp directory.
    fn test_builder(dir: &TempDir) -> BriefingBuilder {
        BriefingBuilder::new(dir.path())
    }

    /// Helper: create a test AttemptSummary.
    fn make_attempt(index: usize) -> AttemptSummary {
        AttemptSummary {
            approach: format!("approach {}", index),
            files_changed: vec![format!("file_{}.rs", index)],
            outcome: AttemptOutcome::TestFailure,
            root_cause: format!("root cause {}", index),
            builder_generation: 0,
        }
    }

    // -----------------------------------------------------------------------
    // test_build_minimal_briefing
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_build_minimal_briefing() {
        let dir = TempDir::new().unwrap();
        let builder = test_builder(&dir);

        let briefing = builder
            .build("fix the login bug", &[], &[], &[], None)
            .await
            .unwrap();

        assert_eq!(briefing.task, "fix the login bug");
        assert!(briefing.relevant_code.is_empty());
        assert!(briefing.attempts.is_empty());
        assert!(briefing.insights.is_empty());
        assert!(briefing.current_approach.is_empty());
        assert!(briefing.verification_state.is_none());
    }

    // -----------------------------------------------------------------------
    // test_build_with_dirty_files
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_build_with_dirty_files() {
        let dir = TempDir::new().unwrap();

        // Create a file on disk.
        let file_path = dir.path().join("src/main.rs");
        fs::create_dir_all(dir.path().join("src")).await.unwrap();
        fs::write(&file_path, "fn main() {\n    println!(\"hello\");\n}\n")
            .await
            .unwrap();

        let builder = test_builder(&dir);
        let briefing = builder
            .build(
                "update the greeting",
                &["src/main.rs".to_string()],
                &[],
                &[],
                None,
            )
            .await
            .unwrap();

        assert_eq!(briefing.relevant_code.len(), 1);
        assert_eq!(briefing.relevant_code[0].path, "src/main.rs");
        assert!(briefing.relevant_code[0].content.contains("hello"));
        assert_eq!(briefing.relevant_code[0].line_range, Some((1, 3)));
    }

    // -----------------------------------------------------------------------
    // test_attempt_compression_limits_to_5
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_attempt_compression_limits_to_5() {
        let dir = TempDir::new().unwrap();
        let builder = test_builder(&dir);

        // Create 8 attempts.
        let attempts: Vec<AttemptSummary> = (0..8).map(make_attempt).collect();

        let briefing = builder
            .build("task", &[], &attempts, &[], None)
            .await
            .unwrap();

        assert_eq!(briefing.attempts.len(), 5);
        // Should keep the most recent 5 (indices 3-7).
        assert_eq!(briefing.attempts[0].approach, "approach 3");
        assert_eq!(briefing.attempts[4].approach, "approach 7");
    }

    #[test]
    fn test_attempt_compression_keeps_all_when_under_limit() {
        let dir = TempDir::new().unwrap();
        let builder = test_builder(&dir);

        let attempts: Vec<AttemptSummary> = (0..3).map(make_attempt).collect();
        let compressed = builder.compress_attempts(&attempts);

        assert_eq!(compressed.len(), 3);
    }

    // -----------------------------------------------------------------------
    // test_insight_deduplication
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_insight_deduplication() {
        let dir = TempDir::new().unwrap();
        let builder = test_builder(&dir);

        let insights = vec![
            "The auth module uses bcrypt".to_string(),
            "The AUTH MODULE USES BCRYPT".to_string(),
            "the auth module uses bcrypt ".to_string(),
            "Tests are in a separate crate".to_string(),
            "".to_string(),
            "   ".to_string(),
            "Tests are in a separate crate".to_string(),
        ];

        let briefing = builder
            .build("task", &[], &[], &insights, None)
            .await
            .unwrap();

        // Should deduplicate: 3 variants of the same insight (case/whitespace),
        // plus the test insight (duplicated), plus 2 empties (skipped).
        assert_eq!(briefing.insights.len(), 2);
        // First occurrence is preserved.
        assert_eq!(briefing.insights[0], "The auth module uses bcrypt");
        assert_eq!(briefing.insights[1], "Tests are in a separate crate");
    }

    #[test]
    fn test_curate_insights_empty_and_whitespace() {
        let dir = TempDir::new().unwrap();
        let builder = test_builder(&dir);

        let insights = vec!["".to_string(), "   ".to_string()];
        let result = builder.curate_insights(&insights);
        assert!(result.is_empty());
    }

    // -----------------------------------------------------------------------
    // test_constraint_detection
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_constraint_detection_rust_project() {
        let dir = TempDir::new().unwrap();

        // Write a Cargo.toml with edition and rust-version.
        let cargo_toml = r#"
[package]
name = "test-project"
version = "0.1.0"
edition = "2021"
rust-version = "1.70"

[dependencies]
"#;
        fs::write(dir.path().join("Cargo.toml"), cargo_toml)
            .await
            .unwrap();

        // Create a tests/ directory.
        fs::create_dir(dir.path().join("tests")).await.unwrap();

        // Create GitHub Actions config.
        fs::create_dir_all(dir.path().join(".github/workflows"))
            .await
            .unwrap();
        fs::write(dir.path().join(".github/workflows/ci.yml"), "name: CI\n")
            .await
            .unwrap();

        let builder = BriefingBuilder::new(dir.path());
        let constraints = builder.detect_constraints().await;

        assert!(constraints.contains(&"Rust edition 2021".to_string()));
        assert!(constraints.contains(&"minimum Rust version 1.70".to_string()));
        assert!(constraints
            .iter()
            .any(|c| c.contains("tests must continue to pass")));
        assert!(constraints
            .iter()
            .any(|c| c.contains("CI pipeline must pass")));
    }

    #[tokio::test]
    async fn test_constraint_detection_empty_project() {
        let dir = TempDir::new().unwrap();
        let builder = test_builder(&dir);

        let constraints = builder.detect_constraints().await;
        assert!(constraints.is_empty());
    }

    #[tokio::test]
    async fn test_constraint_detection_test_files_in_source() {
        let dir = TempDir::new().unwrap();

        // Write a Cargo.toml so it looks like a Rust project.
        fs::write(
            dir.path().join("Cargo.toml"),
            "[package]\nname = \"x\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        )
        .await
        .unwrap();

        // Create a test file in the source tree (no tests/ directory).
        fs::create_dir_all(dir.path().join("src")).await.unwrap();
        fs::write(dir.path().join("src/auth_test.rs"), "")
            .await
            .unwrap();

        let builder = BriefingBuilder::new(dir.path());
        let constraints = builder.detect_constraints().await;

        assert!(constraints
            .iter()
            .any(|c| c.contains("tests must continue to pass")));
    }

    // -----------------------------------------------------------------------
    // test_save_and_load_briefing
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_save_and_load_briefing() {
        let dir = TempDir::new().unwrap();
        let builder = test_builder(&dir);

        let briefing = Briefing {
            task: "fix the bug".to_string(),
            relevant_code: vec![FileSnippet {
                path: "src/main.rs".to_string(),
                content: "fn main() {}".to_string(),
                line_range: Some((1, 1)),
            }],
            attempts: vec![AttemptSummary {
                approach: "edit main.rs".to_string(),
                files_changed: vec!["src/main.rs".to_string()],
                outcome: AttemptOutcome::CompilationError,
                root_cause: "missing semicolon".to_string(),
                builder_generation: 1,
            }],
            insights: vec!["the project uses Rust 2021".to_string()],
            current_approach: "edit main.rs".to_string(),
            constraints: vec!["Rust edition 2021".to_string()],
            verification_state: Some(VerificationState {
                compiles: false,
                tests: TestSummary {
                    total: 5,
                    passed: 3,
                    failed: 2,
                    failed_names: vec!["test_a".to_string(), "test_b".to_string()],
                },
                dirty_files: vec!["src/main.rs".to_string()],
            }),
            structural_declaration: None,
            learnings: String::new(),
            few_shot_examples: String::new(),
        };

        // Save.
        builder.save(&briefing, "test-session-42").await.unwrap();

        // Load.
        let loaded = builder.load("test-session-42").await.unwrap();

        assert!(loaded.is_some());
        let loaded = loaded.unwrap();

        assert_eq!(loaded.task, briefing.task);
        assert_eq!(loaded.relevant_code.len(), briefing.relevant_code.len());
        assert_eq!(loaded.relevant_code[0].path, "src/main.rs");
        assert_eq!(loaded.attempts.len(), 1);
        assert_eq!(loaded.attempts[0].approach, "edit main.rs");
        assert_eq!(loaded.insights, briefing.insights);
        assert_eq!(loaded.current_approach, "edit main.rs");
        assert_eq!(loaded.constraints, briefing.constraints);

        let vs = loaded.verification_state.unwrap();
        assert!(!vs.compiles);
        assert_eq!(vs.tests.total, 5);
        assert_eq!(vs.tests.failed, 2);
        assert_eq!(vs.tests.failed_names, vec!["test_a", "test_b"]);
    }

    #[tokio::test]
    async fn test_load_nonexistent_session() {
        let dir = TempDir::new().unwrap();
        let builder = test_builder(&dir);

        let result = builder.load("nonexistent-session").await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_load_corrupted_briefing() {
        let dir = TempDir::new().unwrap();
        let builder = test_builder(&dir);

        // Write invalid JSON.
        let session_dir = dir.path().join(".rustycode/session/corrupt-session");
        fs::create_dir_all(&session_dir).await.unwrap();
        fs::write(session_dir.join("briefing.json"), "not valid json{{{")
            .await
            .unwrap();

        let result = builder.load("corrupt-session").await;
        assert!(result.is_err());
    }

    // -----------------------------------------------------------------------
    // test_briefing_freshness
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_briefing_freshness() {
        let dir = TempDir::new().unwrap();

        // Create a file with initial content.
        let file_path = dir.path().join("src/lib.rs");
        fs::create_dir_all(dir.path().join("src")).await.unwrap();
        fs::write(&file_path, "initial content").await.unwrap();

        let builder = test_builder(&dir);

        // First build reads the initial content.
        let b1 = builder
            .build("task", &["src/lib.rs".to_string()], &[], &[], None)
            .await
            .unwrap();
        assert_eq!(b1.relevant_code[0].content, "initial content");

        // Modify the file on disk.
        fs::write(&file_path, "modified content").await.unwrap();

        // Second build must reflect the new content, not the old.
        let b2 = builder
            .build("task", &["src/lib.rs".to_string()], &[], &[], None)
            .await
            .unwrap();
        assert_eq!(b2.relevant_code[0].content, "modified content");
    }

    // -----------------------------------------------------------------------
    // Additional edge case tests
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_read_relevant_files_skips_missing() {
        let dir = TempDir::new().unwrap();
        let builder = test_builder(&dir);

        let snippets = builder
            .read_relevant_files(&["nonexistent.rs".to_string()])
            .await;

        assert!(snippets.is_empty());
    }

    #[tokio::test]
    async fn test_read_relevant_files_truncates_long_files() {
        let dir = TempDir::new().unwrap();

        // Create a 200-line file.
        let file_path = dir.path().join("big_file.rs");
        let content: String = (0..200)
            .map(|i| format!("line {}", i))
            .collect::<Vec<_>>()
            .join("\n");
        fs::write(&file_path, &content).await.unwrap();

        let builder = BriefingBuilder::new(dir.path()).with_max_snippet_lines(50);
        let snippets = builder
            .read_relevant_files(&["big_file.rs".to_string()])
            .await;

        assert_eq!(snippets.len(), 1);
        assert_eq!(snippets[0].line_range, Some((1, 50)));
        // The truncated content should have 50 lines.
        assert_eq!(snippets[0].content.lines().count(), 50);
    }

    #[tokio::test]
    async fn test_current_approach_from_latest_attempt() {
        let dir = TempDir::new().unwrap();
        let builder = test_builder(&dir);

        let attempts = vec![
            AttemptSummary {
                approach: "first try".to_string(),
                files_changed: vec![],
                outcome: AttemptOutcome::TestFailure,
                root_cause: "wrong".to_string(),
                builder_generation: 0,
            },
            AttemptSummary {
                approach: "second try".to_string(),
                files_changed: vec![],
                outcome: AttemptOutcome::CompilationError,
                root_cause: "still wrong".to_string(),
                builder_generation: 1,
            },
        ];

        let briefing = builder
            .build("task", &[], &attempts, &[], None)
            .await
            .unwrap();

        assert_eq!(briefing.current_approach, "second try");
    }

    #[test]
    fn test_extract_toml_value() {
        let cargo = r#"
[package]
name = "my-crate"
version = "0.1.0"
edition = "2021"
rust-version = "1.75"

[dependencies]
serde = "1"
"#;
        assert_eq!(
            extract_toml_value(cargo, "edition"),
            Some("2021".to_string())
        );
        assert_eq!(
            extract_toml_value(cargo, "rust-version"),
            Some("1.75".to_string())
        );
        assert_eq!(
            extract_toml_value(cargo, "name"),
            Some("my-crate".to_string())
        );
        // "serde" is in [dependencies], not [package].
        assert_eq!(extract_toml_value(cargo, "serde"), None);
    }

    #[test]
    fn test_extract_quoted_string() {
        assert_eq!(
            extract_quoted_string("\"hello\""),
            Some("hello".to_string())
        );
        assert_eq!(extract_quoted_string("'hello'"), Some("hello".to_string()));
        assert_eq!(extract_quoted_string("unquoted"), None);
        assert_eq!(extract_quoted_string("\""), None);
    }

    #[tokio::test]
    async fn test_save_creates_directories() {
        let dir = TempDir::new().unwrap();
        let builder = test_builder(&dir);

        let briefing = Briefing::new("test task");
        builder
            .save(&briefing, "deep/nested/session")
            .await
            .unwrap();

        let loaded = builder.load("deep/nested/session").await.unwrap();
        assert!(loaded.is_some());
        assert_eq!(loaded.unwrap().task, "test task");
    }
}
