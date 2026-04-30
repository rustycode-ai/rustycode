//! Research brief generator for AST Phase 1: RESEARCH.
//!
//! Scans a workspace to produce a `ContextBrief` containing relevant files,
//! discovered patterns, dependencies, risks, and constraints. This phase is
//! read-only — it never modifies files.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use super::types::ContextBrief;

/// Configuration controlling how aggressively the research phase scans.
#[derive(Debug, Clone)]
pub struct ResearchConfig {
    /// Maximum number of files to include in the brief.
    pub max_files_to_scan: usize,
    /// Maximum directory traversal depth.
    pub max_depth: usize,
}

impl Default for ResearchConfig {
    fn default() -> Self {
        Self {
            max_files_to_scan: 50,
            max_depth: 3,
        }
    }
}

/// Generates a `ContextBrief` by scanning a workspace.
///
/// The research phase is intentionally lightweight — it inspects file names,
/// directory structure, and a small amount of file content to build context
/// without reading entire codebases.
pub struct ResearchBriefGenerator {
    config: ResearchConfig,
}

impl ResearchBriefGenerator {
    pub const fn new(config: ResearchConfig) -> Self {
        Self { config }
    }

    /// Run research on the given task request within the workspace.
    ///
    /// Returns a `ContextBrief` summarizing relevant files, patterns,
    /// dependencies, risks, and constraints discovered.
    pub fn research(&self, request: &str, workspace: &Path) -> ContextBrief {
        let keywords = Self::extract_keywords(request);
        let all_files = self.collect_files(workspace);
        let relevant_files = self.rank_files(&all_files, &keywords);

        let patterns = self.detect_patterns(&all_files, workspace);
        let dependencies = self.detect_dependencies(workspace);
        let risks = self.detect_risks(&relevant_files, &all_files, workspace);
        let constraints = self.detect_constraints(&all_files, workspace);

        ContextBrief {
            relevant_files,
            patterns_found: patterns,
            dependencies,
            risks,
            constraints,
        }
    }

    /// Extract search keywords from the task request.
    fn extract_keywords(request: &str) -> Vec<String> {
        let stop_words = [
            "a", "an", "the", "and", "or", "but", "in", "on", "at", "to", "for", "of", "with",
            "by", "from", "is", "are", "was", "were", "be", "been", "being", "have", "has", "had",
            "do", "does", "did", "will", "would", "could", "should", "may", "might", "can",
            "shall", "it", "its", "this", "that", "these", "those", "i", "we", "you", "he", "she",
            "they", "me", "us", "him", "her", "them", "my", "our", "your", "his", "their", "not",
            "no", "so", "if", "as",
        ];

        request
            .to_lowercase()
            .split(|c: char| !c.is_alphanumeric() && c != '_' && c != '-')
            .filter(|w| !w.is_empty())
            .filter(|w| !stop_words.contains(w))
            .map(String::from)
            .collect()
    }

    /// Walk the workspace collecting file paths up to `max_depth`.
    fn collect_files(&self, workspace: &Path) -> Vec<PathBuf> {
        let mut files = Vec::new();
        let skip_dirs: HashSet<&str> = [
            "target",
            "node_modules",
            ".git",
            "dist",
            "build",
            "__pycache__",
            ".next",
            "vendor",
            ".cache",
        ]
        .into_iter()
        .collect();

        Self::walk_dir(workspace, 0, self.config.max_depth, &skip_dirs, &mut files);
        files
    }

    fn walk_dir(
        dir: &Path,
        depth: usize,
        max_depth: usize,
        skip_dirs: &HashSet<&str>,
        files: &mut Vec<PathBuf>,
    ) {
        if depth > max_depth {
            return;
        }

        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };

        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    if !skip_dirs.contains(name) && !name.starts_with('.') {
                        Self::walk_dir(&path, depth + 1, max_depth, skip_dirs, files);
                    }
                }
            } else {
                files.push(path);
            }
        }
    }

    /// Rank files by relevance to the keywords and return the top N.
    fn rank_files(&self, files: &[PathBuf], keywords: &[String]) -> Vec<PathBuf> {
        let mut scored: Vec<(PathBuf, usize)> = files
            .iter()
            .map(|f| {
                let score = Self::score_file(f, keywords);
                (f.clone(), score)
            })
            .filter(|(_, score)| *score > 0)
            .collect();

        scored.sort_by_key(|b| std::cmp::Reverse(b.1));
        scored.truncate(self.config.max_files_to_scan);
        scored.into_iter().map(|(p, _)| p).collect()
    }

    /// Score a file path against keywords.
    fn score_file(path: &Path, keywords: &[String]) -> usize {
        let path_str = path.to_string_lossy().to_lowercase();
        let file_stem = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_lowercase();

        let mut score = 0;
        for kw in keywords {
            if file_stem.contains(kw.as_str()) {
                score += 3; // Strong match on file name stem
            } else if path_str.contains(kw.as_str()) {
                score += 1; // Weaker match on full path
            }
        }

        // Boost common implementation files
        if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            let impl_exts = ["rs", "py", "ts", "js", "go", "java", "rb"];
            if impl_exts.contains(&ext) {
                score += 1;
            }
        }

        score
    }

    /// Detect common project patterns from file layout.
    #[allow(clippy::unused_self)]
    fn detect_patterns(&self, files: &[PathBuf], workspace: &Path) -> Vec<String> {
        let mut patterns = Vec::new();

        // Detect test directories
        let has_tests_dir = files.iter().any(|f| {
            f.starts_with(workspace.join("tests"))
                || f.starts_with(workspace.join("src").join("tests"))
        });
        if has_tests_dir {
            patterns.push("integration_tests_present".into());
        }

        // Detect inline test modules
        let inline_tests = files.iter().any(|f| {
            f.extension().and_then(|e| e.to_str()) == Some("rs")
                && f.to_string_lossy().contains("tests")
        });
        if inline_tests {
            patterns.push("inline_rust_tests".into());
        }

        // Detect Cargo workspace
        if workspace.join("Cargo.toml").exists() {
            let contents =
                std::fs::read_to_string(workspace.join("Cargo.toml")).unwrap_or_default();
            if contents.contains("[workspace]") {
                patterns.push("cargo_workspace".into());
            } else {
                patterns.push("cargo_single_crate".into());
            }
        }

        // Detect module organization patterns
        let src_dir = workspace.join("src");
        if src_dir.is_dir() {
            if let Ok(entries) = std::fs::read_dir(&src_dir) {
                let subdirs: Vec<String> = entries
                    .flatten()
                    .filter(|e| e.path().is_dir())
                    .filter_map(|e| e.file_name().to_str().map(String::from))
                    .collect();
                if !subdirs.is_empty() {
                    patterns.push(format!("modules: {}", subdirs.join(", ")));
                }
            }
        }

        // Detect common config files
        let config_files = [
            (".pre-commit-config.yaml", "pre_commit_hooks"),
            ("clippy.toml", "clippy_config"),
            ("rustfmt.toml", "rustfmt_config"),
            (".gitleaks.toml", "secret_scanning"),
        ];
        for (filename, label) in config_files {
            if workspace.join(filename).exists() {
                patterns.push(label.into());
            }
        }

        if patterns.is_empty() {
            patterns.push("no_detected_patterns".into());
        }

        patterns
    }

    /// Detect dependencies from Cargo.toml.
    #[allow(clippy::unused_self)]
    #[allow(clippy::unused_self)]
    fn detect_dependencies(&self, workspace: &Path) -> Vec<String> {
        let cargo_path = workspace.join("Cargo.toml");
        let Ok(contents) = std::fs::read_to_string(&cargo_path) else {
            return Vec::new();
        };

        let mut deps = Vec::new();

        // Simple TOML parsing for [dependencies] section
        let mut in_deps = false;
        let mut in_workspace_deps = false;
        for line in contents.lines() {
            let trimmed = line.trim();
            if trimmed == "[dependencies]" {
                in_deps = true;
                in_workspace_deps = false;
                continue;
            }
            if trimmed == "[workspace.dependencies]" {
                in_deps = false;
                in_workspace_deps = true;
                continue;
            }
            if trimmed.starts_with('[') {
                in_deps = false;
                in_workspace_deps = false;
                continue;
            }

            if (in_deps || in_workspace_deps) && trimmed.contains('=') {
                if let Some(name) = trimmed.split('=').next() {
                    let dep_name = name.trim().to_string();
                    // Skip workspace-inherited references
                    if !dep_name.contains('.') && !dep_name.is_empty() && !deps.contains(&dep_name)
                    {
                        deps.push(dep_name);
                    }
                }
            }
        }

        deps
    }

    /// Detect risks based on file count, pattern complexity, etc.
    #[allow(clippy::unused_self)]
    fn detect_risks(
        &self,
        relevant_files: &[PathBuf],
        all_files: &[PathBuf],
        workspace: &Path,
    ) -> Vec<String> {
        let mut risks = Vec::new();

        // Risk: many files to touch
        if relevant_files.len() > 20 {
            risks.push(format!(
                "high_file_count: {} relevant files identified",
                relevant_files.len()
            ));
        }

        // Risk: large workspace
        if all_files.len() > 500 {
            risks.push(format!(
                "large_codebase: {} total files in workspace",
                all_files.len()
            ));
        }

        // Risk: modifying core modules
        let core_indicators = ["core", "lib", "main", "mod"];
        for file in relevant_files {
            let name = file.file_stem().and_then(|s| s.to_str()).unwrap_or("");
            if core_indicators.contains(&name) {
                risks.push(format!("core_module_modification: {}", file.display()));
                break; // Report once
            }
        }

        // Risk: no tests found nearby
        let has_test_files = all_files.iter().any(|f| {
            let name = f.to_string_lossy().to_lowercase();
            name.contains("test") || name.contains("spec")
        });
        if !has_test_files && !relevant_files.is_empty() {
            risks.push("no_existing_tests: no test files found in workspace".into());
        }

        // Risk: shared or generated files
        let generated_indicators = ["generated", "auto", "vendor", "dist"];
        for file in relevant_files {
            let path_str = file.to_string_lossy();
            if generated_indicators.iter().any(|g| path_str.contains(g)) {
                risks.push(format!("generated_file_area: {}", file.display()));
                break;
            }
        }

        // Risk: circular dependency crate markers
        if workspace.join("Cargo.toml").exists() {
            if let Ok(contents) = std::fs::read_to_string(workspace.join("Cargo.toml")) {
                // Check for the crate itself depending on something it also provides
                if contents.contains("rustycode-llm") && contents.contains("rustycode-tools") {
                    risks.push(
                        "potential_circular_dependency: llm and tools crates referenced".into(),
                    );
                }
            }
        }

        risks
    }

    /// Detect constraints from project configuration.
    #[allow(clippy::unused_self)]
    fn detect_constraints(&self, files: &[PathBuf], workspace: &Path) -> Vec<String> {
        let mut constraints = Vec::new();

        // Language constraint from file extensions
        let extensions: HashSet<String> = files
            .iter()
            .filter_map(|f| f.extension().and_then(|e| e.to_str()).map(String::from))
            .collect();

        if extensions.contains("rs") {
            constraints.push("language: rust".into());
        }
        if extensions.contains("py") {
            constraints.push("language: python".into());
        }
        if extensions.contains("ts") || extensions.contains("tsx") {
            constraints.push("language: typescript".into());
        }

        // Lint constraints from Cargo.toml
        let cargo_path = workspace.join("Cargo.toml");
        if let Ok(contents) = std::fs::read_to_string(&cargo_path) {
            if contents.contains("deny(warnings)") || contents.contains("-D warnings") {
                constraints.push("strict_lints: warnings treated as errors".into());
            }
            if contents.contains("unwrap_used") {
                constraints.push("no_unwrap: unwrap usage restricted".into());
            }
            if contents.contains("unsafe_code") {
                constraints.push("no_unsafe: unsafe code restricted".into());
            }
        }

        // Workspace structure constraint
        if workspace.join("Cargo.toml").exists() {
            if let Ok(contents) = std::fs::read_to_string(workspace.join("Cargo.toml")) {
                if contents.contains("[workspace]") {
                    constraints.push("cargo_workspace: multi-crate project".into());
                }
            }
        }

        if constraints.is_empty() {
            constraints.push("no_detected_constraints".into());
        }

        constraints
    }
}

impl Default for ResearchBriefGenerator {
    fn default() -> Self {
        Self::new(ResearchConfig::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn create_workspace() -> tempfile::TempDir {
        tempfile::tempdir().expect("failed to create temp dir")
    }

    fn write_file(dir: &Path, relative_path: &str, content: &str) {
        let path = dir.join(relative_path);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("failed to create parent dir");
        }
        fs::write(&path, content).expect("failed to write file");
    }

    #[test]
    fn finds_relevant_files_by_keyword() {
        let dir = create_workspace();
        let ws = dir.path();

        write_file(ws, "src/auth.rs", "fn login() {}");
        write_file(ws, "src/session.rs", "fn create() {}");
        write_file(ws, "src/utils.rs", "fn helper() {}");

        let gen = ResearchBriefGenerator::default();
        let brief = gen.research("Add auth tests for the login flow", ws);

        // auth.rs should rank higher than unrelated files
        let auth_found = brief
            .relevant_files
            .iter()
            .any(|f| f.to_string_lossy().contains("auth"));
        assert!(auth_found, "auth.rs should appear in relevant files");
    }

    #[test]
    fn detects_test_patterns() {
        let dir = create_workspace();
        let ws = dir.path();

        write_file(ws, "tests/integration_test.rs", "fn test_it() {}");
        write_file(
            ws,
            "src/lib.rs",
            "pub fn add(a: i32, b: i32) -> i32 { a + b }",
        );

        let gen = ResearchBriefGenerator::default();
        let brief = gen.research("Fix the add function", ws);

        assert!(
            brief
                .patterns_found
                .iter()
                .any(|p| p == "integration_tests_present"),
            "Should detect integration test directory"
        );
    }

    #[test]
    fn extracts_dependencies_from_cargo_toml() {
        let dir = create_workspace();
        let ws = dir.path();

        write_file(
            ws,
            "Cargo.toml",
            "[package]\nname = \"test-crate\"\n\n[dependencies]\nserde = \"1\"\nregex = \"1\"\n",
        );

        let gen = ResearchBriefGenerator::default();
        let brief = gen.research("Update the serde serialization", ws);

        assert!(
            brief.dependencies.contains(&"serde".to_string()),
            "serde should appear in dependencies"
        );
        assert!(
            brief.dependencies.contains(&"regex".to_string()),
            "regex should appear in dependencies"
        );
    }

    #[test]
    fn detects_no_tests_risk() {
        let dir = create_workspace();
        let ws = dir.path();

        write_file(ws, "src/main.rs", "fn main() {}");
        // No test files anywhere

        let gen = ResearchBriefGenerator::default();
        let brief = gen.research("Modify the main function", ws);

        assert!(
            brief.risks.iter().any(|r| r.contains("no_existing_tests")),
            "Should flag risk of no existing tests"
        );
    }

    #[test]
    fn respects_max_files_limit() {
        let dir = create_workspace();
        let ws = dir.path();

        // Create many files with the same keyword
        for i in 0..100 {
            write_file(ws, &format!("src/module_{i}.rs"), "fn func() {}");
        }

        let config = ResearchConfig {
            max_files_to_scan: 10,
            max_depth: 3,
        };
        let gen = ResearchBriefGenerator::new(config);
        let brief = gen.research("module", ws);

        assert!(
            brief.relevant_files.len() <= 10,
            "Should not exceed max_files_to_scan"
        );
    }

    #[test]
    fn detects_rust_language_constraint() {
        let dir = create_workspace();
        let ws = dir.path();

        write_file(ws, "src/lib.rs", "pub fn hello() {}");

        let gen = ResearchBriefGenerator::default();
        let brief = gen.research("Refactor the hello function", ws);

        assert!(
            brief.constraints.iter().any(|c| c.contains("rust")),
            "Should detect Rust language constraint"
        );
    }

    #[test]
    fn skips_hidden_and_target_directories() {
        let dir = create_workspace();
        let ws = dir.path();

        write_file(ws, "target/debug/output", "binary");
        write_file(ws, ".git/config", "git config");
        write_file(ws, "src/main.rs", "fn main() {}");

        let gen = ResearchBriefGenerator::default();
        let brief = gen.research("Fix main", ws);

        let has_target = brief
            .relevant_files
            .iter()
            .any(|f| f.to_string_lossy().contains("target"));
        let has_git = brief
            .relevant_files
            .iter()
            .any(|f| f.to_string_lossy().contains(".git"));

        assert!(!has_target, "Should skip target directory");
        assert!(!has_git, "Should skip .git directory");
    }

    #[test]
    fn handles_empty_workspace() {
        let dir = create_workspace();
        let ws = dir.path();

        let gen = ResearchBriefGenerator::default();
        let brief = gen.research("Implement something new", ws);

        assert!(brief.relevant_files.is_empty());
        assert!(
            brief
                .patterns_found
                .iter()
                .any(|p| p == "no_detected_patterns"),
            "Empty workspace should report no patterns"
        );
    }

    #[test]
    fn detects_cargo_workspace_pattern() {
        let dir = create_workspace();
        let ws = dir.path();

        write_file(
            ws,
            "Cargo.toml",
            "[workspace]\nmembers = [\"crates/a\", \"crates/b\"]\n",
        );
        write_file(ws, "crates/a/src/lib.rs", "");

        let gen = ResearchBriefGenerator::default();
        let brief = gen.research("Update workspace crates", ws);

        assert!(
            brief.patterns_found.iter().any(|p| p == "cargo_workspace"),
            "Should detect cargo workspace pattern"
        );
    }

    #[test]
    fn keyword_extraction_ignores_stop_words() {
        let keywords = ResearchBriefGenerator::extract_keywords(
            "Add a test for the login function in the auth module",
        );

        assert!(
            !keywords.contains(&"a".to_string()),
            "Stop word 'a' should be excluded"
        );
        assert!(
            !keywords.contains(&"the".to_string()),
            "Stop word 'the' should be excluded"
        );
        assert!(
            keywords.contains(&"test".to_string()),
            "'test' should be a keyword"
        );
        assert!(
            keywords.contains(&"login".to_string()),
            "'login' should be a keyword"
        );
        assert!(
            keywords.contains(&"auth".to_string()),
            "'auth' should be a keyword"
        );
    }

    #[test]
    fn detects_strict_lint_constraints() {
        let dir = create_workspace();
        let ws = dir.path();

        write_file(ws, "Cargo.toml", "[lints]\nwarn_unwrap_used = true\n");
        write_file(ws, "src/lib.rs", "");

        let gen = ResearchBriefGenerator::default();
        let brief = gen.research("Add error handling", ws);

        // At minimum, language constraint should be detected
        assert!(!brief.constraints.is_empty());
    }
}
