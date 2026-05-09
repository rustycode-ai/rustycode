//! Auto-formatting on file modification.
//!
//! Detects project-level formatter configuration from build files and config files,
//! then runs the appropriate formatter after file-modifying tools (edit, write).
//!
//! # Detection Sources
//!
//! Build files (content-based):
//! - `package.json` → reads `devDependencies` and `scripts` for prettier/biome/eslint
//! - `pyproject.toml` → reads `[tool.ruff.format]`, `[tool.black]`, `[tool.isort]`
//! - `go.mod` → gofmt always available for Go projects
//! - `Cargo.toml` → rustfmt always available for Rust projects
//!
//! Dedicated config files (presence-based):
//! - `rustfmt.toml` / `.rustfmt.toml` → rustfmt
//! - `.prettierrc` / `.prettierrc.json` / `.prettierrc.js` → prettier
//! - `.clang-format` → clang-format
//! - `biome.json` → biome
//! - `stylua.toml` / `.stylua.toml` → stylua
//!
//! # Integration
//!
//! Called from `edit.rs` and `fs.rs` after file write. Returns a diff string
//! if the formatter changed the file, which is appended to the tool output.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

/// A detected formatter configuration.
#[derive(Debug, Clone)]
pub struct DetectedFormatter {
    /// Human-readable name (e.g., "rustfmt", "prettier")
    pub name: String,
    /// Command to execute (e.g., "rustfmt", "npx prettier")
    pub command: String,
    /// Arguments for single-file in-place formatting
    pub args: Vec<String>,
    /// File extensions this formatter handles (without dot, e.g., "rs", "ts")
    pub extensions: Vec<String>,
    /// Config file that was detected, if any
    pub config_file: Option<PathBuf>,
}

/// Global cache of detected formatters per project root.
/// Avoids re-scanning the filesystem on every file edit.
static FORMATTER_CACHE: std::sync::OnceLock<Arc<Mutex<HashMap<PathBuf, Vec<DetectedFormatter>>>>> =
    std::sync::OnceLock::new();

fn formatter_cache() -> &'static Arc<Mutex<HashMap<PathBuf, Vec<DetectedFormatter>>>> {
    FORMATTER_CACHE.get_or_init(|| Arc::new(Mutex::new(HashMap::new())))
}

/// Detect formatters for a project. Results are cached per project root.
///
/// Detection order (later entries override earlier ones):
/// 1. Build file content (Cargo.toml, package.json, pyproject.toml, go.mod)
/// 2. Dedicated config files (rustfmt.toml, .prettierrc, etc.)
pub fn detect_formatters(project_root: &Path) -> Vec<DetectedFormatter> {
    let root_key = project_root.to_path_buf();

    {
        let cache = formatter_cache()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(formatters) = cache.get(&root_key) {
            return formatters.clone();
        }
    }

    let mut formatters = Vec::new();

    detect_from_cargo(project_root, &mut formatters);
    detect_from_package_json(project_root, &mut formatters);
    detect_from_pyproject(project_root, &mut formatters);
    detect_from_go_mod(project_root, &mut formatters);
    detect_from_gemfile(project_root, &mut formatters);
    detect_from_composer(project_root, &mut formatters);

    detect_rustfmt_config(project_root, &mut formatters);
    detect_prettier_config(project_root, &mut formatters);
    detect_clang_format_config(project_root, &mut formatters);
    detect_biome_config(project_root, &mut formatters);
    detect_stylua_config(project_root, &mut formatters);
    detect_shfmt_config(project_root, &mut formatters);

    {
        let mut cache = formatter_cache()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        cache.insert(root_key, formatters.clone());
    }

    formatters
}

/// Clear the formatter cache (for testing or after config changes).
pub fn clear_cache() {
    let mut cache = formatter_cache()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    cache.clear();
}

/// Format a single file using the detected project formatter.
///
/// Returns `Some(diff_string)` if a formatter ran and changed the file.
/// Returns `None` if no formatter is configured for this file type,
/// the formatter is not installed, or formatting produced no changes.
///
/// Formatting failures are logged but do not propagate as errors.
pub fn format_file(file_path: &Path, project_root: &Path) -> Option<String> {
    let extension = file_path.extension()?.to_str()?.to_string();
    let formatters = detect_formatters(project_root);

    let formatter = formatters
        .iter()
        .find(|f| f.extensions.contains(&extension))?;

    let before = std::fs::read_to_string(file_path).ok()?;

    // Shell handles multi-word commands like "npx prettier"
    let mut cmd_parts = vec![formatter.command.clone()];
    cmd_parts.extend(formatter.args.iter().cloned());
    cmd_parts.push(shell_escape(file_path));

    let full_cmd = cmd_parts.join(" ");

    let output = match std::process::Command::new(crate::subprocess::SHELL_INFO.binary)
        .arg(crate::subprocess::SHELL_INFO.exec_flag)
        .arg(&full_cmd)
        .current_dir(project_root)
        .output()
    {
        Ok(o) => o,
        Err(e) => {
            tracing::debug!("Formatter '{}' not available: {}", formatter.name, e);
            return None;
        }
    };

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        tracing::debug!(
            "Formatter '{}' failed for {}: {}",
            formatter.name,
            file_path.display(),
            stderr.trim()
        );
        return None;
    }

    let after = match std::fs::read_to_string(file_path) {
        Ok(content) => content,
        Err(e) => {
            tracing::debug!("Failed to re-read file after formatting: {}", e);
            return None;
        }
    };

    if before == after {
        return None; // Already formatted
    }

    let diff =
        crate::line_endings::generate_diff(&before, &after, &file_path.display().to_string(), 50);

    Some(format!(
        "\n\n[Auto-formatted with {}]\n{}",
        formatter.name, diff
    ))
}

/// Shell-escape a file path for use in shell commands.
fn shell_escape(path: &Path) -> String {
    let s = path.display().to_string();
    if s.contains(' ') || s.contains('\'') || s.contains('"') || s.contains('$') {
        format!("'{}'", s.replace('\'', "'\\''"))
    } else {
        s
    }
}

fn detect_from_cargo(root: &Path, formatters: &mut Vec<DetectedFormatter>) {
    if !root.join("Cargo.toml").exists() {
        return;
    }

    formatters.push(DetectedFormatter {
        name: "rustfmt".to_string(),
        command: "rustfmt".to_string(),
        args: vec![],
        extensions: vec!["rs".to_string()],
        config_file: None,
    });
}

fn detect_from_package_json(root: &Path, formatters: &mut Vec<DetectedFormatter>) {
    let pkg_path = root.join("package.json");
    if !pkg_path.exists() {
        return;
    }

    let content = match std::fs::read_to_string(&pkg_path) {
        Ok(c) => c,
        Err(_) => return,
    };

    let pkg: serde_json::Value = match serde_json::from_str(&content) {
        Ok(v) => v,
        Err(_) => return,
    };

    // Check for custom format script
    let has_format_script = pkg
        .get("scripts")
        .and_then(|s| s.as_object())
        .map(|s| s.contains_key("format"))
        .unwrap_or(false);

    if has_format_script {
        // "format" script takes priority — it's the project's explicit formatter choice
        let pkg_manager = detect_package_manager(root);
        formatters.push(DetectedFormatter {
            name: "format script".to_string(),
            command: format!("{pkg_manager} run format"),
            args: vec![],
            extensions: vec![
                "js".to_string(),
                "jsx".to_string(),
                "ts".to_string(),
                "tsx".to_string(),
                "css".to_string(),
                "scss".to_string(),
                "json".to_string(),
                "md".to_string(),
                "yaml".to_string(),
                "yml".to_string(),
                "html".to_string(),
            ],
            config_file: Some(pkg_path),
        });
        return;
    }

    // Check devDependencies for known formatters
    let deps = pkg.get("devDependencies").and_then(|d| d.as_object());
    if let Some(deps) = deps {
        if deps.contains_key("prettier") {
            formatters.push(DetectedFormatter {
                name: "prettier".to_string(),
                command: "npx prettier".to_string(),
                args: vec!["--write".to_string()],
                extensions: vec![
                    "js".to_string(),
                    "jsx".to_string(),
                    "ts".to_string(),
                    "tsx".to_string(),
                    "css".to_string(),
                    "scss".to_string(),
                    "json".to_string(),
                    "md".to_string(),
                    "yaml".to_string(),
                    "yml".to_string(),
                    "html".to_string(),
                ],
                config_file: Some(pkg_path.clone()),
            });
        }

        if deps.contains_key("biome") || deps.contains_key("@biomejs/biome") {
            formatters.push(DetectedFormatter {
                name: "biome".to_string(),
                command: "npx biome".to_string(),
                args: vec!["format".to_string(), "--write".to_string()],
                extensions: vec![
                    "js".to_string(),
                    "jsx".to_string(),
                    "ts".to_string(),
                    "tsx".to_string(),
                    "css".to_string(),
                    "json".to_string(),
                ],
                config_file: Some(pkg_path),
            });
        }
    }
}

fn detect_from_pyproject(root: &Path, formatters: &mut Vec<DetectedFormatter>) {
    let pyproject_path = root.join("pyproject.toml");
    if !pyproject_path.exists() {
        return;
    }

    let content = match std::fs::read_to_string(&pyproject_path) {
        Ok(c) => c,
        Err(_) => return,
    };

    // String search avoids toml parser dependency

    if content.contains("[tool.ruff") && content.contains("format") {
        formatters.push(DetectedFormatter {
            name: "ruff format".to_string(),
            command: "ruff".to_string(),
            args: vec!["format".to_string()],
            extensions: vec!["py".to_string()],
            config_file: Some(pyproject_path.clone()),
        });
    } else if content.contains("[tool.black]") {
        formatters.push(DetectedFormatter {
            name: "black".to_string(),
            command: "black".to_string(),
            args: vec![],
            extensions: vec!["py".to_string()],
            config_file: Some(pyproject_path.clone()),
        });
    }

    if content.contains("[tool.isort]") {
        formatters.push(DetectedFormatter {
            name: "isort".to_string(),
            command: "isort".to_string(),
            args: vec![],
            extensions: vec!["py".to_string()],
            config_file: Some(pyproject_path.clone()),
        });
    }
}

fn detect_from_go_mod(root: &Path, formatters: &mut Vec<DetectedFormatter>) {
    if !root.join("go.mod").exists() {
        return;
    }

    formatters.push(DetectedFormatter {
        name: "gofmt".to_string(),
        command: "gofmt".to_string(),
        args: vec!["-w".to_string()],
        extensions: vec!["go".to_string()],
        config_file: None,
    });
}

fn detect_from_gemfile(root: &Path, formatters: &mut Vec<DetectedFormatter>) {
    if !root.join("Gemfile").exists() {
        return;
    }

    // Check for .rubocop.yml to confirm rubocop usage
    let config_file = if root.join(".rubocop.yml").exists() {
        Some(root.join(".rubocop.yml"))
    } else {
        None
    };

    formatters.push(DetectedFormatter {
        name: "rubocop".to_string(),
        command: "bundle exec rubocop".to_string(),
        args: vec!["-A".to_string()],
        extensions: vec!["rb".to_string()],
        config_file,
    });
}

fn detect_from_composer(root: &Path, formatters: &mut Vec<DetectedFormatter>) {
    if !root.join("composer.json").exists() {
        return;
    }

    formatters.push(DetectedFormatter {
        name: "php-cs-fixer".to_string(),
        command: "php-cs-fixer".to_string(),
        args: vec!["fix".to_string()],
        extensions: vec!["php".to_string()],
        config_file: None,
    });
}

fn detect_rustfmt_config(root: &Path, formatters: &mut [DetectedFormatter]) {
    let config_names = ["rustfmt.toml", ".rustfmt.toml"];
    for name in &config_names {
        let path = root.join(name);
        if path.exists() {
            if let Some(existing) = formatters.iter_mut().find(|f| f.name == "rustfmt") {
                existing.config_file = Some(path);
            }
            return;
        }
    }
}

fn detect_prettier_config(root: &Path, formatters: &mut Vec<DetectedFormatter>) {
    let config_names = [
        ".prettierrc",
        ".prettierrc.json",
        ".prettierrc.js",
        ".prettierrc.yml",
        ".prettierrc.yaml",
        "prettier.config.js",
        "prettier.config.mjs",
    ];

    for name in &config_names {
        let path = root.join(name);
        if path.exists() {
            if let Some(existing) = formatters.iter_mut().find(|f| f.name == "prettier") {
                existing.config_file = Some(path);
                return;
            }

            formatters.push(DetectedFormatter {
                name: "prettier".to_string(),
                command: "npx prettier".to_string(),
                args: vec!["--write".to_string()],
                extensions: vec![
                    "js".to_string(),
                    "jsx".to_string(),
                    "ts".to_string(),
                    "tsx".to_string(),
                    "css".to_string(),
                    "scss".to_string(),
                    "json".to_string(),
                    "md".to_string(),
                    "yaml".to_string(),
                    "yml".to_string(),
                    "html".to_string(),
                ],
                config_file: Some(path),
            });
            return;
        }
    }
}

fn detect_clang_format_config(root: &Path, formatters: &mut Vec<DetectedFormatter>) {
    let config_names = [".clang-format", "_clang-format", ".clang-format"];
    for name in &config_names {
        let path = root.join(name);
        if path.exists() {
            formatters.push(DetectedFormatter {
                name: "clang-format".to_string(),
                command: "clang-format".to_string(),
                args: vec!["-i".to_string()],
                extensions: vec![
                    "c".to_string(),
                    "cpp".to_string(),
                    "h".to_string(),
                    "hpp".to_string(),
                    "cs".to_string(),
                    "java".to_string(),
                    "proto".to_string(),
                ],
                config_file: Some(path),
            });
            return;
        }
    }
}

fn detect_biome_config(root: &Path, formatters: &mut Vec<DetectedFormatter>) {
    let config_names = ["biome.json", "biome.jsonc"];
    for name in &config_names {
        let path = root.join(name);
        if path.exists() {
            if let Some(existing) = formatters.iter_mut().find(|f| f.name == "biome") {
                existing.config_file = Some(path);
                return;
            }

            formatters.push(DetectedFormatter {
                name: "biome".to_string(),
                command: "npx biome".to_string(),
                args: vec!["format".to_string(), "--write".to_string()],
                extensions: vec![
                    "js".to_string(),
                    "jsx".to_string(),
                    "ts".to_string(),
                    "tsx".to_string(),
                    "css".to_string(),
                    "json".to_string(),
                ],
                config_file: Some(path),
            });
            return;
        }
    }
}

fn detect_stylua_config(root: &Path, formatters: &mut Vec<DetectedFormatter>) {
    let config_names = ["stylua.toml", ".stylua.toml"];
    for name in &config_names {
        let path = root.join(name);
        if path.exists() {
            formatters.push(DetectedFormatter {
                name: "stylua".to_string(),
                command: "stylua".to_string(),
                args: vec![],
                extensions: vec!["lua".to_string()],
                config_file: Some(path),
            });
            return;
        }
    }
}

fn detect_shfmt_config(root: &Path, formatters: &mut Vec<DetectedFormatter>) {
    let editorconfig = root.join(".editorconfig");
    if !editorconfig.exists() {
        return;
    }

    // Require explicit .shfmt marker to avoid false positives on every .editorconfig
    if !root.join(".shfmt").exists() {
        return;
    }

    formatters.push(DetectedFormatter {
        name: "shfmt".to_string(),
        command: "shfmt".to_string(),
        args: vec!["-w".to_string()],
        extensions: vec!["sh".to_string(), "Bash".to_string()],
        config_file: Some(editorconfig),
    });
}

/// Detect which package manager to use based on lock files.
fn detect_package_manager(root: &Path) -> &'static str {
    if root.join("pnpm-lock.yaml").exists() {
        "pnpm"
    } else if root.join("yarn.lock").exists() {
        "yarn"
    } else {
        "npm"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn temp_project() -> tempfile::TempDir {
        tempfile::tempdir().unwrap()
    }

    #[test]
    fn detect_rustfmt_from_cargo() {
        clear_cache();
        let dir = temp_project();
        fs::write(dir.path().join("Cargo.toml"), "[package]\nname = \"test\"").unwrap();

        let formatters = detect_formatters(dir.path());
        assert_eq!(formatters.len(), 1);
        assert_eq!(formatters[0].name, "rustfmt");
        assert!(formatters[0].extensions.contains(&"rs".to_string()));
        assert_eq!(formatters[0].command, "rustfmt");
    }

    #[test]
    fn detect_rustfmt_with_config_file() {
        clear_cache();
        let dir = temp_project();
        fs::write(dir.path().join("Cargo.toml"), "[package]\nname = \"test\"").unwrap();
        fs::write(dir.path().join("rustfmt.toml"), "max_width = 100\n").unwrap();

        let formatters = detect_formatters(dir.path());
        assert_eq!(formatters.len(), 1);
        assert_eq!(formatters[0].name, "rustfmt");
        assert!(formatters[0].config_file.is_some());
        assert!(formatters[0]
            .config_file
            .as_ref()
            .unwrap()
            .ends_with("rustfmt.toml"));
    }

    #[test]
    fn detect_prettier_from_package_json() {
        clear_cache();
        let dir = temp_project();
        fs::write(
            dir.path().join("package.json"),
            r#"{"devDependencies": {"prettier": "^3.0.0"}}"#,
        )
        .unwrap();

        let formatters = detect_formatters(dir.path());
        assert_eq!(formatters.len(), 1);
        assert_eq!(formatters[0].name, "prettier");
        assert!(formatters[0].extensions.contains(&"ts".to_string()));
        assert!(formatters[0].args.contains(&"--write".to_string()));
    }

    #[test]
    fn detect_format_script_from_package_json() {
        clear_cache();
        let dir = temp_project();
        fs::write(
            dir.path().join("package.json"),
            r#"{"scripts": {"format": "prettier --write ."}, "devDependencies": {"prettier": "^3.0.0"}}"#,
        )
        .unwrap();

        let formatters = detect_formatters(dir.path());
        assert_eq!(formatters.len(), 1);
        assert_eq!(formatters[0].name, "format script");
        assert!(formatters[0].command.contains("run format"));
    }

    #[test]
    fn detect_biome_from_package_json() {
        clear_cache();
        let dir = temp_project();
        fs::write(
            dir.path().join("package.json"),
            r#"{"devDependencies": {"@biomejs/biome": "^1.0.0"}}"#,
        )
        .unwrap();

        let formatters = detect_formatters(dir.path());
        assert_eq!(formatters.len(), 1);
        assert_eq!(formatters[0].name, "biome");
    }

    #[test]
    fn detect_prettier_from_config_file_only() {
        clear_cache();
        let dir = temp_project();
        fs::write(dir.path().join(".prettierrc"), "{}").unwrap();

        let formatters = detect_formatters(dir.path());
        assert_eq!(formatters.len(), 1);
        assert_eq!(formatters[0].name, "prettier");
        assert!(formatters[0].config_file.is_some());
    }

    #[test]
    fn detect_prettier_config_refines_package_json() {
        clear_cache();
        let dir = temp_project();
        fs::write(
            dir.path().join("package.json"),
            r#"{"devDependencies": {"prettier": "^3.0.0"}}"#,
        )
        .unwrap();
        fs::write(dir.path().join(".prettierrc.json"), "{\"tabWidth\": 4}").unwrap();

        let formatters = detect_formatters(dir.path());
        assert_eq!(formatters.len(), 1);
        assert_eq!(formatters[0].name, "prettier");
        assert!(formatters[0]
            .config_file
            .as_ref()
            .unwrap()
            .ends_with(".prettierrc.json"));
    }

    #[test]
    fn detect_ruff_from_pyproject() {
        clear_cache();
        let dir = temp_project();
        fs::write(
            dir.path().join("pyproject.toml"),
            "[tool.ruff]\nline-length = 88\n\n[tool.ruff.format]\nindent-style = \"tab\"\n",
        )
        .unwrap();

        let formatters = detect_formatters(dir.path());
        assert_eq!(formatters.len(), 1);
        assert_eq!(formatters[0].name, "ruff format");
        assert!(formatters[0].extensions.contains(&"py".to_string()));
    }

    #[test]
    fn detect_black_from_pyproject() {
        clear_cache();
        let dir = temp_project();
        fs::write(
            dir.path().join("pyproject.toml"),
            "[tool.black]\nline-length = 88\n",
        )
        .unwrap();

        let formatters = detect_formatters(dir.path());
        assert_eq!(formatters.len(), 1);
        assert_eq!(formatters[0].name, "black");
    }

    #[test]
    fn detect_gofmt_from_go_mod() {
        clear_cache();
        let dir = temp_project();
        fs::write(
            dir.path().join("go.mod"),
            "module example.com/m\n\ngo 1.21\n",
        )
        .unwrap();

        let formatters = detect_formatters(dir.path());
        assert_eq!(formatters.len(), 1);
        assert_eq!(formatters[0].name, "gofmt");
        assert_eq!(formatters[0].args, vec!["-w"]);
    }

    #[test]
    fn detect_clang_format_from_config() {
        clear_cache();
        let dir = temp_project();
        fs::write(dir.path().join(".clang-format"), "BasedOnStyle: LLVM\n").unwrap();

        let formatters = detect_formatters(dir.path());
        assert_eq!(formatters.len(), 1);
        assert_eq!(formatters[0].name, "clang-format");
        assert!(formatters[0].extensions.contains(&"cpp".to_string()));
    }

    #[test]
    fn detect_biome_from_config_file() {
        clear_cache();
        let dir = temp_project();
        fs::write(
            dir.path().join("biome.json"),
            r#"{"formatter": {"enabled": true}}"#,
        )
        .unwrap();

        let formatters = detect_formatters(dir.path());
        assert_eq!(formatters.len(), 1);
        assert_eq!(formatters[0].name, "biome");
    }

    #[test]
    fn detect_stylua_from_config() {
        clear_cache();
        let dir = temp_project();
        fs::write(dir.path().join("stylua.toml"), "column_width = 120\n").unwrap();

        let formatters = detect_formatters(dir.path());
        assert_eq!(formatters.len(), 1);
        assert_eq!(formatters[0].name, "stylua");
        assert!(formatters[0].extensions.contains(&"lua".to_string()));
    }

    #[test]
    fn detect_no_formatters() {
        clear_cache();
        let dir = temp_project();
        let formatters = detect_formatters(dir.path());
        assert!(formatters.is_empty());
    }

    #[test]
    fn detect_multiple_formatters_from_mixed_project() {
        clear_cache();
        let dir = temp_project();
        fs::write(dir.path().join("Cargo.toml"), "[package]\nname = \"test\"").unwrap();
        fs::write(
            dir.path().join("package.json"),
            r#"{"devDependencies": {"prettier": "^3.0.0"}}"#,
        )
        .unwrap();

        let formatters = detect_formatters(dir.path());
        assert!(formatters.len() >= 2);

        let names: Vec<&str> = formatters.iter().map(|f| f.name.as_str()).collect();
        assert!(names.contains(&"rustfmt"));
        assert!(names.contains(&"prettier"));
    }

    #[test]
    fn formatter_cache_hit() {
        clear_cache();
        let dir = temp_project();
        fs::write(dir.path().join("Cargo.toml"), "[package]\nname = \"test\"").unwrap();

        let first = detect_formatters(dir.path());
        let second = detect_formatters(dir.path());
        assert_eq!(first.len(), second.len());
    }

    #[test]
    fn shell_escape_simple_path() {
        let path = Path::new("/tmp/test.rs");
        assert_eq!(shell_escape(path), "/tmp/test.rs");
    }

    #[test]
    fn shell_escape_path_with_spaces() {
        let path = Path::new("/tmp/my project/test.rs");
        assert_eq!(shell_escape(path), "'/tmp/my project/test.rs'");
    }

    #[test]
    fn detect_npm_default() {
        let dir = temp_project();
        fs::write(dir.path().join("package.json"), "{}").unwrap();
        assert_eq!(detect_package_manager(dir.path()), "npm");
    }

    #[test]
    fn detect_pnpm() {
        let dir = temp_project();
        fs::write(dir.path().join("pnpm-lock.yaml"), "").unwrap();
        assert_eq!(detect_package_manager(dir.path()), "pnpm");
    }

    #[test]
    fn detect_yarn() {
        let dir = temp_project();
        fs::write(dir.path().join("yarn.lock"), "").unwrap();
        assert_eq!(detect_package_manager(dir.path()), "yarn");
    }

    #[test]
    fn format_file_no_formatter_configured() {
        clear_cache();
        let dir = temp_project();
        let test_file = dir.path().join("test.txt");
        fs::write(&test_file, "hello").unwrap();

        let result = format_file(&test_file, dir.path());
        assert!(result.is_none());
    }

    #[test]
    fn format_file_no_extension() {
        clear_cache();
        let dir = temp_project();
        let test_file = dir.path().join("Makefile");
        fs::write(&test_file, "all: build\n").unwrap();

        let result = format_file(&test_file, dir.path());
        assert!(result.is_none());
    }

    #[test]
    fn format_file_nonexistent_path() {
        clear_cache();
        let dir = temp_project();
        let result = format_file(&dir.path().join("nonexistent.rs"), dir.path());
        assert!(result.is_none());
    }

    #[test]
    fn format_file_with_rustfmt() {
        clear_cache();
        let dir = temp_project();
        fs::write(dir.path().join("Cargo.toml"), "[package]\nname = \"test\"").unwrap();

        // Create a poorly formatted Rust file
        let test_file = dir.path().join("src/main.rs");
        fs::create_dir_all(dir.path().join("src")).unwrap();
        fs::write(&test_file, "fn main ( ) {  let x  = 1  ; }").unwrap();

        // Run formatter — will only succeed if rustfmt is installed
        let result = format_file(&test_file, dir.path());

        // If rustfmt is installed, it should format the file
        if let Some(diff) = result {
            assert!(diff.contains("Auto-formatted with rustfmt"));
        }
        // If rustfmt is not installed, that's fine — result is None
    }
}
