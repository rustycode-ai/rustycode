//! Environment bootstrapping -- automatic project profiling on first run.
//!
//! Inspired by the Meta-Harness insight that "the harness itself is the
//! optimization target." This module analyses a project's file structure to
//! determine language, build system, test runner, linter, and formatter so
//! that downstream agent teams can self-configure without manual setup.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tracing::info;

// ---------------------------------------------------------------------------
// ProjectProfile
// ---------------------------------------------------------------------------

/// A fully-detected project profile describing how to build, test, lint, and
/// format a codebase.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProjectProfile {
    /// Primary language (Rust, Go, TypeScript, Python, etc.).
    pub language: String,
    /// Framework if detected (Actix, Axum, React, Django, etc.).
    pub framework: Option<String>,
    /// Build system command (cargo, go, npm, pip, etc.).
    pub build_system: String,
    /// Command to run tests.
    pub test_runner: String,
    /// Command to run the linter.
    pub lint_command: String,
    /// Command to run the formatter.
    pub format_command: String,
    /// Absolute path to the project root.
    pub project_root: PathBuf,
    /// Source directories relative to project root.
    pub source_dirs: Vec<String>,
    /// Test directories relative to project root.
    pub test_dirs: Vec<String>,
    /// Configuration files present in the project root.
    pub config_files: Vec<String>,
    /// Suggested maximum agent turns based on project complexity.
    pub max_turns_hint: u32,
}

// ---------------------------------------------------------------------------
// ProjectProfiler
// ---------------------------------------------------------------------------

/// Detects project characteristics from the filesystem.
pub struct ProjectProfiler;

impl ProjectProfiler {
    /// Analyse the project rooted at `root` and return a full profile.
    ///
    /// Detection is purely filesystem-based (no LLM calls). The profiler
    /// looks for well-known config files and directory layouts.
    pub fn profile(root: &Path) -> ProjectProfile {
        info!(path = %root.display(), "profiling project environment");

        let config_files = Self::detect_config_files(root);
        let source_dirs = Self::detect_source_dirs(root);
        let test_dirs = Self::detect_test_dirs(root);

        let (language, framework, build_system, test_runner, lint_command, format_command) =
            Self::detect_language_stack(root, &config_files);

        let max_turns_hint = Self::compute_complexity_hint(root, &language, &source_dirs);

        info!(
            language = %language,
            framework = ?framework,
            build = %build_system,
            "environment profile complete"
        );

        ProjectProfile {
            language,
            framework,
            build_system,
            test_runner,
            lint_command,
            format_command,
            project_root: root.to_path_buf(),
            source_dirs,
            test_dirs,
            config_files,
            max_turns_hint,
        }
    }

    // -- Language / toolchain detection ------------------------------------

    fn detect_language_stack(
        root: &Path,
        config_files: &[String],
    ) -> (String, Option<String>, String, String, String, String) {
        // The order matters: more specific checks first.
        if Self::has_file(root, "Cargo.toml") {
            let framework = Self::detect_rust_framework(root);
            return (
                "Rust".into(),
                framework,
                "cargo build".into(),
                "cargo test".into(),
                "cargo clippy".into(),
                "cargo fmt".into(),
            );
        }

        if Self::has_file(root, "go.mod") {
            return (
                "Go".into(),
                None,
                "go build ./...".into(),
                "go test ./...".into(),
                "golangci-lint run".into(),
                "gofmt -w .".into(),
            );
        }

        if Self::has_file(root, "package.json") {
            let framework = Self::detect_js_framework(config_files);
            let test_runner = Self::detect_js_test_runner(root);
            let lint = Self::detect_js_linter(root);
            let fmt = Self::detect_js_formatter(root);
            let language = if config_files.iter().any(|f| f == "tsconfig.json") {
                "TypeScript"
            } else {
                "JavaScript"
            };
            return (
                language.into(),
                framework,
                "npm run build".into(),
                test_runner,
                lint,
                fmt,
            );
        }

        if Self::has_file(root, "pyproject.toml")
            || Self::has_file(root, "requirements.txt")
            || Self::has_file(root, "setup.py")
        {
            let framework = Self::detect_python_framework(config_files);
            return (
                "Python".into(),
                framework,
                "pip install -e .".into(),
                "pytest".into(),
                "ruff check .".into(),
                "ruff format .".into(),
            );
        }

        // Fallback: generic project
        (
            "Unknown".into(),
            None,
            "make".into(),
            "make test".into(),
            "make lint".into(),
            "make fmt".into(),
        )
    }

    // -- Rust specifics ----------------------------------------------------

    fn detect_rust_framework(root: &Path) -> Option<String> {
        let cargo_toml = root.join("Cargo.toml");
        let contents = fs::read_to_string(&cargo_toml).ok()?;

        // Check dependencies for known web frameworks.
        let frameworks = [
            ("actix-web", "Actix Web"),
            ("axum", "Axum"),
            ("rocket", "Rocket"),
            ("warp", "Warp"),
            ("poem", "Poem"),
            ("salvo", "Salvo"),
            ("tide", "Tide"),
            ("thruster", "Thruster"),
            ("gotham", "Gotham"),
            ("actix", "Actix"),
        ];

        for (dep, name) in &frameworks {
            if contents.contains(dep) {
                return Some((*name).into());
            }
        }
        None
    }

    // -- JavaScript / TypeScript specifics ---------------------------------

    fn detect_js_framework(config_files: &[String]) -> Option<String> {
        // Look for framework-specific config files or infer from package.json.
        if config_files
            .iter()
            .any(|f| f == "next.config.js" || f == "next.config.mjs")
        {
            return Some("Next.js".into());
        }
        if config_files
            .iter()
            .any(|f| f == "nuxt.config.js" || f == "nuxt.config.ts")
        {
            return Some("Nuxt".into());
        }
        // Heuristic: check for framework directories (not perfect, but cheap).
        None
    }

    fn detect_js_test_runner(root: &Path) -> String {
        let pkg = fs::read_to_string(root.join("package.json")).unwrap_or_default();
        if pkg.contains("\"vitest\"") {
            return "npx vitest run".into();
        }
        if pkg.contains("\"jest\"") {
            return "npx jest".into();
        }
        if pkg.contains("\"mocha\"") {
            return "npx mocha".into();
        }
        // Check for a "test" script as the generic fallback.
        if pkg.contains("\"test\"") {
            return "npm test".into();
        }
        "npm test".into()
    }

    fn detect_js_linter(root: &Path) -> String {
        let pkg = fs::read_to_string(root.join("package.json")).unwrap_or_default();
        if pkg.contains("\"eslint\"")
            || root.join(".eslintrc.json").exists()
            || root.join(".eslintrc.js").exists()
        {
            return "npx eslint .".into();
        }
        if pkg.contains("\"biome\"") || root.join("biome.json").exists() {
            return "npx biome check .".into();
        }
        "npx eslint .".into()
    }

    fn detect_js_formatter(root: &Path) -> String {
        let pkg = fs::read_to_string(root.join("package.json")).unwrap_or_default();
        if pkg.contains("\"prettier\"")
            || root.join(".prettierrc").exists()
            || root.join(".prettierrc.json").exists()
        {
            return "npx prettier --write .".into();
        }
        if pkg.contains("\"biome\"") || root.join("biome.json").exists() {
            return "npx biome format --write .".into();
        }
        "npx prettier --write .".into()
    }

    // -- Python specifics --------------------------------------------------

    fn detect_python_framework(config_files: &[String]) -> Option<String> {
        let pyproject = config_files
            .iter()
            .find(|f| *f == "pyproject.toml")
            .and_then(|_| fs::read_to_string("pyproject.toml").ok())
            .unwrap_or_default();

        if pyproject.contains("django") {
            return Some("Django".into());
        }
        if pyproject.contains("flask") {
            return Some("Flask".into());
        }
        if pyproject.contains("fastapi") {
            return Some("FastAPI".into());
        }
        None
    }

    // -- Directory detection ------------------------------------------------

    fn detect_source_dirs(root: &Path) -> Vec<String> {
        let candidates = ["src", "lib", "cmd", "internal", "pkg", "app"];
        Self::existing_dirs(root, &candidates)
    }

    fn detect_test_dirs(root: &Path) -> Vec<String> {
        let mut dirs = Vec::new();

        // Standard test directories.
        let candidates = ["tests", "test", "__tests__", "spec"];
        dirs.extend(Self::existing_dirs(root, &candidates));

        // Language-specific test patterns.
        // Rust: tests/ is already covered; src/ contains #[cfg(test)] modules.
        if Self::has_file(root, "Cargo.toml") && !dirs.iter().any(|d| d == "src") {
            dirs.push("src".into());
        }
        // Go: *_test.go files live alongside source.
        // Python: test_*.py files live alongside source or in tests/.
        if Self::has_file(root, "go.mod") || Self::has_file(root, "pyproject.toml") {
            // Source dirs double as test dirs for these languages.
            for src in &["src", "lib", "pkg", "internal"] {
                if root.join(src).is_dir() && !dirs.iter().any(|d| d == *src) {
                    dirs.push((*src).into());
                }
            }
        }

        dirs
    }

    fn detect_config_files(root: &Path) -> Vec<String> {
        let candidates = [
            "Cargo.toml",
            "Cargo.lock",
            "go.mod",
            "go.sum",
            "package.json",
            "package-lock.json",
            "yarn.lock",
            "pnpm-lock.yaml",
            "tsconfig.json",
            "jsconfig.json",
            "pyproject.toml",
            "requirements.txt",
            "setup.py",
            "setup.cfg",
            "Pipfile",
            ".eslintrc.json",
            ".eslintrc.js",
            ".eslintrc.yml",
            ".prettierrc",
            ".prettierrc.json",
            "biome.json",
            "next.config.js",
            "next.config.mjs",
            "nuxt.config.js",
            "nuxt.config.ts",
            "Makefile",
            "Dockerfile",
            "docker-compose.yml",
        ];

        candidates
            .iter()
            .filter(|name| root.join(name).exists())
            .map(|s| s.to_string())
            .collect()
    }

    // -- Complexity ---------------------------------------------------------

    fn compute_complexity_hint(root: &Path, language: &str, source_dirs: &[String]) -> u32 {
        let file_count = Self::count_source_files(root, source_dirs);
        let is_monorepo = Self::is_monorepo(root, language);

        let mut hint: u32 = 20; // baseline

        // Scale with file count (use saturating to prevent overflow).
        hint = hint.saturating_add((file_count / 20) as u32);

        // Monorepo bump.
        if is_monorepo {
            hint += 15;
        }

        // For Rust workspaces, add per-crate budget.
        if language == "Rust" {
            let crate_count = Self::count_workspace_crates(root);
            hint += crate_count.saturating_sub(1) * 5;
        }

        // Cap at a reasonable maximum.
        hint.min(200)
    }

    fn count_source_files(root: &Path, source_dirs: &[String]) -> usize {
        let mut count = 0;
        for dir_name in source_dirs {
            let dir_path = root.join(dir_name);
            if let Ok(entries) = fs::read_dir(&dir_path) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_file() {
                        if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                            if matches!(ext, "rs" | "go" | "ts" | "tsx" | "js" | "jsx" | "py") {
                                count += 1;
                            }
                        }
                    } else if path.is_dir() {
                        // Shallow recursion for nested dirs (1 level).
                        if let Ok(sub) = fs::read_dir(&path) {
                            for sub_entry in sub.flatten() {
                                if sub_entry.path().is_file() {
                                    if let Some(ext) =
                                        sub_entry.path().extension().and_then(|e| e.to_str())
                                    {
                                        if matches!(
                                            ext,
                                            "rs" | "go" | "ts" | "tsx" | "js" | "jsx" | "py"
                                        ) {
                                            count += 1;
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        count
    }

    fn is_monorepo(root: &Path, language: &str) -> bool {
        match language {
            "Rust" => {
                let cargo_toml = root.join("Cargo.toml");
                fs::read_to_string(&cargo_toml)
                    .map(|c| c.contains("[workspace]"))
                    .unwrap_or(false)
            }
            "Go" => root.join("go.work").exists(),
            "TypeScript" | "JavaScript" => {
                root.join("lerna.json").exists()
                    || root.join("pnpm-workspace.yaml").exists()
                    || root.join("nx.json").exists()
                    || root.join("turbo.json").exists()
            }
            _ => false,
        }
    }

    fn count_workspace_crates(root: &Path) -> u32 {
        let cargo_toml = root.join("Cargo.toml");
        let contents = match fs::read_to_string(&cargo_toml) {
            Ok(c) => c,
            Err(_) => return 1,
        };

        if !contents.contains("[workspace]") {
            return 1;
        }

        // Count "members" entries or count Cargo.toml files in subdirectories.
        let crates_dir = root.join("crates");
        if crates_dir.is_dir() {
            if let Ok(entries) = fs::read_dir(&crates_dir) {
                return entries
                    .flatten()
                    .filter(|e| e.path().join("Cargo.toml").exists())
                    .count()
                    .try_into()
                    .unwrap_or(u32::MAX);
            }
        }

        // Fallback: look for any Cargo.toml in direct subdirectories.
        if let Ok(entries) = fs::read_dir(root) {
            return entries
                .flatten()
                .filter(|e| e.path().join("Cargo.toml").exists())
                .count()
                .try_into()
                .unwrap_or(u32::MAX);
        }

        1
    }

    // -- Helpers ------------------------------------------------------------

    fn has_file(root: &Path, name: &str) -> bool {
        root.join(name).is_file()
    }

    fn existing_dirs(root: &Path, candidates: &[&str]) -> Vec<String> {
        candidates
            .iter()
            .filter(|name| root.join(name).is_dir())
            .map(|s| s.to_string())
            .collect()
    }
}

impl Default for ProjectProfiler {
    fn default() -> Self {
        Self
    }
}

// ---------------------------------------------------------------------------
// ProfileCache
// ---------------------------------------------------------------------------

const CACHE_DIR: &str = ".rustycode";
const CACHE_FILE: &str = "profile.json";

/// Persists and loads [`ProjectProfile`] from the `.rustycode/profile.json`
/// file inside the project root.
pub struct ProfileCache;

impl ProfileCache {
    /// Load a previously saved profile. Returns `None` if no cached profile
    /// exists or if deserialisation fails.
    pub fn load(root: &Path) -> Option<ProjectProfile> {
        let path = Self::cache_path(root);
        if !path.exists() {
            return None;
        }
        let data = fs::read_to_string(&path).ok()?;
        match serde_json::from_str(&data) {
            Ok(profile) => {
                info!("loaded cached environment profile from {}", path.display());
                Some(profile)
            }
            Err(e) => {
                info!(
                    "ignoring corrupt profile cache at {}: {}",
                    path.display(),
                    e
                );
                None
            }
        }
    }

    /// Save a profile to disk. Creates the `.rustycode/` directory if needed.
    pub fn save(root: &Path, profile: &ProjectProfile) -> anyhow::Result<()> {
        let dir = root.join(CACHE_DIR);
        fs::create_dir_all(&dir)?;

        let path = dir.join(CACHE_FILE);
        let data = serde_json::to_string_pretty(profile)?;
        fs::write(&path, data)?;

        info!(path = %path.display(), "saved environment profile cache");
        Ok(())
    }

    /// Returns the full path to the cache file.
    pub fn cache_path(root: &Path) -> PathBuf {
        root.join(CACHE_DIR).join(CACHE_FILE)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn write_file(dir: &Path, name: &str, contents: &str) {
        fs::write(dir.join(name), contents).unwrap();
    }

    fn create_dir(dir: &Path, name: &str) -> PathBuf {
        let p = dir.join(name);
        fs::create_dir_all(&p).unwrap();
        p
    }

    // -- Rust detection -----------------------------------------------------

    #[test]
    fn detect_rust_project() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();

        write_file(
            root,
            "Cargo.toml",
            r#"[package]
name = "test-project"
version = "0.1.0"
edition = "2021"

[dependencies]
axum = "0.7"
"#,
        );
        create_dir(root, "src");
        create_dir(root, "tests");

        let profile = ProjectProfiler::profile(root);

        assert_eq!(profile.language, "Rust");
        assert_eq!(profile.framework.as_deref(), Some("Axum"));
        assert_eq!(profile.build_system, "cargo build");
        assert_eq!(profile.test_runner, "cargo test");
        assert_eq!(profile.lint_command, "cargo clippy");
        assert_eq!(profile.format_command, "cargo fmt");
        assert!(profile.source_dirs.contains(&"src".to_string()));
        assert!(profile.test_dirs.contains(&"tests".to_string()));
        assert!(profile.config_files.contains(&"Cargo.toml".to_string()));
    }

    // -- Go detection -------------------------------------------------------

    #[test]
    fn detect_go_project() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();

        write_file(root, "go.mod", "module example.com/test\n\ngo 1.22\n");
        create_dir(root, "cmd");
        create_dir(root, "internal");

        let profile = ProjectProfiler::profile(root);

        assert_eq!(profile.language, "Go");
        assert!(profile.framework.is_none());
        assert_eq!(profile.build_system, "go build ./...");
        assert_eq!(profile.test_runner, "go test ./...");
        assert!(profile.source_dirs.contains(&"cmd".to_string()));
        assert!(profile.source_dirs.contains(&"internal".to_string()));
    }

    // -- TypeScript detection -----------------------------------------------

    #[test]
    fn detect_typescript_project() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();

        write_file(
            root,
            "package.json",
            r#"{
                "name": "test-ts",
                "scripts": { "test": "jest" },
                "devDependencies": { "jest": "^29.0.0" }
            }"#,
        );
        write_file(root, "tsconfig.json", "{}");
        create_dir(root, "src");
        create_dir(root, "__tests__");

        let profile = ProjectProfiler::profile(root);

        assert_eq!(profile.language, "TypeScript");
        assert_eq!(profile.test_runner, "npx jest");
        assert!(profile.source_dirs.contains(&"src".to_string()));
        assert!(profile.test_dirs.contains(&"__tests__".to_string()));
        assert!(profile.config_files.contains(&"tsconfig.json".to_string()));
    }

    // -- Python detection ---------------------------------------------------

    #[test]
    fn detect_python_project() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();

        write_file(
            root,
            "pyproject.toml",
            r#"[project]
name = "test-py"
dependencies = ["fastapi"]
"#,
        );
        create_dir(root, "src");
        create_dir(root, "tests");

        let profile = ProjectProfiler::profile(root);

        assert_eq!(profile.language, "Python");
        assert_eq!(profile.test_runner, "pytest");
        assert!(profile.source_dirs.contains(&"src".to_string()));
        assert!(profile.test_dirs.contains(&"tests".to_string()));
    }

    // -- Complexity hint ----------------------------------------------------

    #[test]
    fn max_turns_hint_scales_with_project_size() {
        let small = TempDir::new().unwrap();
        let small_root = small.path();
        write_file(small_root, "Cargo.toml", "[package]\nname = \"small\"\n");
        create_dir(small_root, "src");
        // 1 source file.
        write_file(&small_root.join("src"), "main.rs", "fn main() {}");

        let big = TempDir::new().unwrap();
        let big_root = big.path();
        write_file(
            big_root,
            "Cargo.toml",
            "[workspace]\nmembers = [\"crates/*\"]\n",
        );
        let crates = create_dir(big_root, "crates");
        for i in 0..5 {
            let crate_dir = create_dir(&crates, &format!("crate-{i}"));
            write_file(
                &crate_dir,
                "Cargo.toml",
                &format!("[package]\nname = \"crate-{i}\"\n"),
            );
            create_dir(&crate_dir, "src");
        }
        create_dir(big_root, "src");

        let small_profile = ProjectProfiler::profile(small_root);
        let big_profile = ProjectProfiler::profile(big_root);

        assert!(
            big_profile.max_turns_hint > small_profile.max_turns_hint,
            "big ({}) should have higher hint than small ({})",
            big_profile.max_turns_hint,
            small_profile.max_turns_hint,
        );
    }

    // -- ProfileCache round-trip -------------------------------------------

    #[test]
    fn profile_cache_roundtrip() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();

        let original = ProjectProfile {
            language: "Rust".into(),
            framework: Some("Axum".into()),
            build_system: "cargo build".into(),
            test_runner: "cargo test".into(),
            lint_command: "cargo clippy".into(),
            format_command: "cargo fmt".into(),
            project_root: root.to_path_buf(),
            source_dirs: vec!["src".into()],
            test_dirs: vec!["tests".into()],
            config_files: vec!["Cargo.toml".into()],
            max_turns_hint: 42,
        };

        ProfileCache::save(root, &original).unwrap();

        // Verify the file exists.
        assert!(root.join(".rustycode/profile.json").exists());

        let loaded = ProfileCache::load(root).expect("profile should load from cache");
        assert_eq!(loaded, original);
    }

    #[test]
    fn profile_cache_returns_none_when_missing() {
        let tmp = TempDir::new().unwrap();
        assert!(ProfileCache::load(tmp.path()).is_none());
    }
}
