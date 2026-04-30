// ── .rustycodeignore Support ─────────────────────────────────────────────────
//
// Loads and evaluates ignore patterns from .rustycodeignore and .gitignore files.
// Patterns follow .gitignore syntax: comments (#), glob patterns, directory
// trailing-slash patterns, and negation (!).

use std::path::{Path, PathBuf};
use tracing::{debug, warn};

/// Compiled ignore pattern with metadata.
#[derive(Debug, Clone)]
struct IgnorePattern {
    /// The raw pattern string (for debugging).
    raw: String,
    /// Whether this is a negation pattern (prefixed with `!`).
    negated: bool,
    /// Whether the pattern contains a path separator (or is root-anchored).
    has_separator: bool,
    /// Regex compiled from the glob pattern.
    regex: regex::Regex,
}

/// Ignore pattern loader and matcher.
///
/// Loads patterns from `.rustycodeignore` and `.gitignore` in the project root,
/// merged with built-in default patterns. Supports standard .gitignore syntax:
/// - `# comment` lines are ignored
/// - Blank lines are ignored
/// - `pattern/` matches directories only
/// - `*.ext` matches file extensions
/// - `!pattern` negates a previous ignore
/// - `dir/` matches any directory named `dir` at any depth
///
/// # Example
///
/// ```no_run
/// use std::path::Path;
/// use rustycode_core::context::ignore::RustyCodeIgnore;
///
/// let project_root = Path::new("/my/project");
/// let ignorer = RustyCodeIgnore::load(project_root);
///
/// assert!(ignorer.should_ignore(Path::new("target/debug/main.o")));
/// assert!(!ignorer.should_ignore(Path::new("src/main.rs")));
/// ```
#[derive(Debug, Clone)]
pub struct RustyCodeIgnore {
    /// Compiled ignore patterns (order matters for negation).
    patterns: Vec<IgnorePattern>,
    /// Project root (for relative path computation).
    project_root: PathBuf,
}

/// Binary file extensions that should always be excluded from LLM context.
const BINARY_EXTENSIONS: &[&str] = &[
    "so", "dylib", "dll", "exe", "wasm", "pdb", "o", "obj", "a", "lib", "class", "pyc", "pyo",
    "png", "jpg", "jpeg", "gif", "bmp", "ico", "webp", "mp3", "mp4", "wav", "avi", "mov", "mkv",
    "flac", "ogg", "zip", "tar", "gz", "bz2", "xz", "7z", "rar", "pdf", "doc", "docx", "xls",
    "xlsx", "ppt", "pptx", "woff", "woff2", "ttf", "eot", "otf", "sqlite", "db",
];

/// File name suffixes that indicate generated/minified content.
const GENERATED_SUFFIXES: &[&str] = &[".min.js", ".min.css", ".bundle.js", ".bundle.css"];

impl RustyCodeIgnore {
    /// Load ignore patterns from the project root.
    ///
    /// Loads `.rustycodeignore` first (if present), then `.gitignore` as fallback.
    /// Both are merged with built-in default patterns.
    pub fn load(project_root: &Path) -> Self {
        let mut patterns = Vec::new();

        // Load built-in defaults first (lowest priority).
        Self::add_default_patterns(&mut patterns);

        // Load .gitignore as fallback.
        let gitignore_path = project_root.join(".gitignore");
        if gitignore_path.exists() {
            Self::load_file(&gitignore_path, &mut patterns, ".gitignore");
        }

        // Load .rustycodeignore (highest priority, overrides everything).
        let rustycodeignore_path = project_root.join(".rustycodeignore");
        if rustycodeignore_path.exists() {
            Self::load_file(&rustycodeignore_path, &mut patterns, ".rustycodeignore");
        }

        debug!(
            "Loaded {} ignore patterns for project {:?}",
            patterns.len(),
            project_root
        );

        Self {
            patterns,
            project_root: project_root.to_path_buf(),
        }
    }

    /// Create an ignore instance with only default patterns (no file loading).
    ///
    /// Useful for testing or when there is no project root.
    pub fn defaults_only() -> Self {
        let mut patterns = Vec::new();
        Self::add_default_patterns(&mut patterns);
        Self {
            patterns,
            project_root: PathBuf::new(),
        }
    }

    /// Check if a path should be ignored entirely (excluded from workspace scans).
    ///
    /// The path can be absolute (under project_root) or relative to the project root.
    pub fn should_ignore(&self, path: &Path) -> bool {
        let relative = self.make_relative(path);
        let relative_str = relative.to_string_lossy();
        let path_str = relative_str.as_ref();

        // Always check binary extensions first (fast path).
        if self.is_binary_by_extension(path) {
            return true;
        }

        let mut ignored = false;

        for pattern in &self.patterns {
            let matched = self.match_pattern(pattern, path_str);

            if matched {
                if pattern.negated {
                    ignored = false;
                    debug!(
                        "Negation pattern '{}' un-ignored: {}",
                        pattern.raw, path_str
                    );
                } else {
                    ignored = true;
                }
            }
        }

        if ignored {
            debug!("Ignoring path: {}", path_str);
        }

        ignored
    }

    /// Check if file content should be excluded from LLM context.
    ///
    /// This is stricter than `should_ignore` -- it also excludes generated
    /// and minified files that may exist in source-controlled directories.
    pub fn should_ignore_content(&self, path: &Path) -> bool {
        // Everything ignored by should_ignore is also content-ignored.
        if self.should_ignore(path) {
            return true;
        }

        let file_name = path
            .file_name()
            .map(|n| n.to_string_lossy())
            .unwrap_or_default();

        // Check for generated/minified file suffixes.
        for suffix in GENERATED_SUFFIXES {
            if file_name.ends_with(suffix) {
                return true;
            }
        }

        // Check for source map files.
        if file_name.ends_with(".map") {
            return true;
        }

        false
    }

    /// Make a path relative to the project root.
    fn make_relative<'a>(&'a self, path: &'a Path) -> PathBuf {
        if path.is_absolute() && path.starts_with(&self.project_root) {
            path.strip_prefix(&self.project_root)
                .unwrap_or(path)
                .to_path_buf()
        } else if path.is_absolute() {
            path.to_path_buf()
        } else {
            // Already relative.
            path.to_path_buf()
        }
    }

    /// Check if a path has a binary extension.
    fn is_binary_by_extension(&self, path: &Path) -> bool {
        let ext = path
            .extension()
            .map(|e| e.to_string_lossy().to_lowercase())
            .unwrap_or_default();

        BINARY_EXTENSIONS.contains(&ext.as_str())
    }

    /// Match a single pattern against a path string.
    fn match_pattern(&self, pattern: &IgnorePattern, path_str: &str) -> bool {
        // Normalize separators to forward slashes.
        let normalized = path_str.replace('\\', "/");

        if pattern.has_separator {
            // Anchored pattern: match against full path.
            pattern.regex.is_match(&normalized)
        } else {
            // Unanchored pattern: match against any path component.
            if pattern.regex.is_match(&normalized) {
                return true;
            }
            // Check each component (file/directory name).
            normalized
                .split('/')
                .any(|component| pattern.regex.is_match(component))
        }
    }

    /// Add built-in default ignore patterns.
    fn add_default_patterns(patterns: &mut Vec<IgnorePattern>) {
        let defaults = [
            // Version control.
            ".git/",
            ".hg/",
            ".svn/",
            // Rust build artifacts.
            "target/",
            // JavaScript/TypeScript.
            "node_modules/",
            // Lock files.
            "*.lock",
            // Build output directories.
            "dist/",
            "build/",
            "out/",
            // Python.
            "__pycache__/",
            ".venv/",
            "venv/",
            "*.egg-info/",
            // IDE files.
            ".idea/",
            ".vscode/",
            "*.swp",
            "*.swo",
            "*~",
            // OS files.
            ".DS_Store",
            "Thumbs.db",
            // Coverage and test output.
            "coverage/",
            ".nyc_output/",
            ".coverage/",
            "htmlcov/",
            // Dependency directories.
            ".bundle/",
            // Cache directories.
            ".cache/",
            ".parcel-cache/",
            ".turbo/",
            // Log files.
            "*.log",
        ];

        for default in defaults {
            if let Some(pat) = Self::parse_pattern(default) {
                patterns.push(pat);
            }
        }
    }

    /// Load patterns from a file.
    fn load_file(path: &Path, patterns: &mut Vec<IgnorePattern>, label: &str) {
        match std::fs::read_to_string(path) {
            Ok(content) => {
                let count = Self::parse_patterns(&content, patterns);
                debug!("Loaded {} patterns from {}", count, label);
            }
            Err(e) => {
                warn!("Failed to read {}: {}", label, e);
            }
        }
    }

    /// Parse multiple patterns from file content.
    fn parse_patterns(content: &str, patterns: &mut Vec<IgnorePattern>) -> usize {
        let mut count = 0;
        for line in content.lines() {
            let trimmed = line.trim();
            // Skip empty lines and comments.
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }
            if let Some(pat) = Self::parse_pattern(trimmed) {
                patterns.push(pat);
                count += 1;
            }
        }
        count
    }

    /// Parse a single pattern line into a compiled IgnorePattern.
    fn parse_pattern(line: &str) -> Option<IgnorePattern> {
        let raw = line.to_string();

        // Check for negation.
        let (negated, pattern_str) = if let Some(stripped) = line.strip_prefix('!') {
            (true, stripped)
        } else {
            (false, line)
        };

        // Skip empty patterns after stripping negation.
        if pattern_str.is_empty() {
            return None;
        }

        // Check for directory-only pattern (trailing /).
        let (dir_only, pattern_str) = if let Some(stripped) = pattern_str.strip_suffix('/') {
            (true, stripped)
        } else {
            (false, pattern_str)
        };

        // Check for root-anchored pattern (leading /).
        // In gitignore, a leading / means the pattern is relative to the directory
        // the file is in. For our purposes, it means anchored at the root.
        let (root_anchored, pattern_str) = if let Some(stripped) = pattern_str.strip_prefix('/') {
            (true, stripped)
        } else {
            (false, pattern_str)
        };

        // Check if pattern contains a path separator (anchored).
        // Root-anchored patterns are always anchored.
        let has_separator = pattern_str.contains('/') || root_anchored;

        // Convert gitignore glob to regex.
        let regex = Self::glob_to_regex(pattern_str, has_separator, dir_only);

        match regex::Regex::new(&regex) {
            Ok(compiled) => Some(IgnorePattern {
                raw,
                negated,
                has_separator,
                regex: compiled,
            }),
            Err(e) => {
                warn!("Invalid ignore pattern '{}': {}", line, e);
                None
            }
        }
    }

    /// Convert a gitignore-style glob pattern to a regex string.
    ///
    /// When `dir_only` is true, the regex also matches any path underneath
    /// the matched directory (e.g., `dist` matches `dist/bundle.js`).
    fn glob_to_regex(pattern: &str, anchored: bool, dir_only: bool) -> String {
        let mut result = String::from("(?i)^");

        if !anchored {
            // Unanchored patterns can match at any depth.
            result.push_str("(?:.*/)?");
        }

        let chars: Vec<char> = pattern.chars().collect();
        let mut i = 0;

        while i < chars.len() {
            match chars[i] {
                '*' => {
                    if i + 1 < chars.len() && chars[i + 1] == '*' {
                        // ** = match any path (including separators).
                        if i + 2 < chars.len() && chars[i + 2] == '/' {
                            result.push_str("(?:.+/)?"); // **/ = any directory prefix
                            i += 3;
                        } else {
                            result.push_str(".*");
                            i += 2;
                        }
                    } else {
                        // * = match anything except path separator.
                        result.push_str("[^/]*");
                        i += 1;
                    }
                }
                '?' => {
                    result.push_str("[^/]");
                    i += 1;
                }
                '[' => {
                    // Character class: pass through until closing ].
                    result.push('[');
                    i += 1;
                    if i < chars.len() && chars[i] == '!' {
                        result.push('^');
                        i += 1;
                    }
                    while i < chars.len() && chars[i] != ']' {
                        result.push(chars[i]);
                        i += 1;
                    }
                    if i < chars.len() {
                        result.push(']');
                        i += 1;
                    }
                }
                '.' | '(' | ')' | '+' | '^' | '$' | '|' | '\\' | '{' | '}' => {
                    // Escape regex metacharacters.
                    result.push('\\');
                    result.push(chars[i]);
                    i += 1;
                }
                c => {
                    result.push(c);
                    i += 1;
                }
            }
        }

        // For dir_only patterns, also match anything under the matched path.
        if dir_only {
            result.push_str("(?:/.*)?");
        }

        result.push('$');
        result
    }

    /// Return the number of loaded patterns (useful for diagnostics).
    pub fn pattern_count(&self) -> usize {
        self.patterns.len()
    }

    /// Return the project root this ignorer was loaded for.
    pub fn project_root(&self) -> &Path {
        &self.project_root
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    /// Helper to create a RustyCodeIgnore with custom patterns for testing.
    fn from_patterns(pattern_strs: &[&str]) -> RustyCodeIgnore {
        let mut patterns = Vec::new();
        // Always include defaults.
        RustyCodeIgnore::add_default_patterns(&mut patterns);
        for p in pattern_strs {
            if let Some(pat) = RustyCodeIgnore::parse_pattern(p) {
                patterns.push(pat);
            }
        }
        RustyCodeIgnore {
            patterns,
            project_root: PathBuf::from("/test/project"),
        }
    }

    // ── Default Patterns ───────────────────────────────────────────────────────

    #[test]
    fn test_default_ignores_target() {
        let ignorer = RustyCodeIgnore::defaults_only();
        assert!(ignorer.should_ignore(Path::new("target/debug/main.o")));
        assert!(ignorer.should_ignore(Path::new("target/release/app")));
    }

    #[test]
    fn test_default_ignores_node_modules() {
        let ignorer = RustyCodeIgnore::defaults_only();
        assert!(ignorer.should_ignore(Path::new("node_modules/react/index.js")));
    }

    #[test]
    fn test_default_ignores_git() {
        let ignorer = RustyCodeIgnore::defaults_only();
        assert!(ignorer.should_ignore(Path::new(".git/HEAD")));
    }

    #[test]
    fn test_default_ignores_lock_files() {
        let ignorer = RustyCodeIgnore::defaults_only();
        assert!(ignorer.should_ignore(Path::new("Cargo.lock")));
        // package-lock.json has .json extension, not .lock -- it should NOT
        // be caught by *.lock. Use an explicit pattern in .rustycodeignore
        // if you want to exclude it.
        assert!(!ignorer.should_ignore(Path::new("package-lock.json")));
    }

    #[test]
    fn test_default_ignores_build_dirs() {
        let ignorer = RustyCodeIgnore::defaults_only();
        assert!(ignorer.should_ignore(Path::new("dist/bundle.js")));
        assert!(ignorer.should_ignore(Path::new("build/output.o")));
        assert!(ignorer.should_ignore(Path::new("out/production/app.jar")));
    }

    #[test]
    fn test_default_ignores_python() {
        let ignorer = RustyCodeIgnore::defaults_only();
        assert!(ignorer.should_ignore(Path::new("__pycache__/module.pyc")));
        assert!(ignorer.should_ignore(Path::new(".venv/bin/python")));
    }

    #[test]
    fn test_default_ignores_binary_extensions() {
        let ignorer = RustyCodeIgnore::defaults_only();
        assert!(ignorer.should_ignore(Path::new("lib/native.so")));
        assert!(ignorer.should_ignore(Path::new("lib/native.dylib")));
        assert!(ignorer.should_ignore(Path::new("bin/app.exe")));
        assert!(ignorer.should_ignore(Path::new("module.wasm")));
    }

    #[test]
    fn test_default_ignores_ide_files() {
        let ignorer = RustyCodeIgnore::defaults_only();
        assert!(ignorer.should_ignore(Path::new(".idea/workspace.xml")));
        assert!(ignorer.should_ignore(Path::new(".vscode/settings.json")));
    }

    #[test]
    fn test_default_does_not_ignore_source() {
        let ignorer = RustyCodeIgnore::defaults_only();
        assert!(!ignorer.should_ignore(Path::new("src/main.rs")));
        assert!(!ignorer.should_ignore(Path::new("lib/index.ts")));
        assert!(!ignorer.should_ignore(Path::new("README.md")));
    }

    // ── Glob Pattern Matching ──────────────────────────────────────────────────

    #[test]
    fn test_star_pattern() {
        let ignorer = from_patterns(&["*.log"]);
        assert!(ignorer.should_ignore(Path::new("app.log")));
        assert!(ignorer.should_ignore(Path::new("logs/app.log")));
        assert!(!ignorer.should_ignore(Path::new("app.rs")));
    }

    #[test]
    fn test_directory_pattern() {
        let ignorer = from_patterns(&["cache/"]);
        // Directory pattern matches at any depth.
        assert!(ignorer.should_ignore(Path::new("cache/data.json")));
    }

    #[test]
    fn test_double_star_pattern() {
        let ignorer = from_patterns(&["**/temp/"]);
        assert!(ignorer.should_ignore(Path::new("temp/data.txt")));
        assert!(ignorer.should_ignore(Path::new("src/temp/data.txt")));
    }

    #[test]
    fn test_negation_pattern() {
        let ignorer = from_patterns(&["*.log", "!important.log"]);
        assert!(ignorer.should_ignore(Path::new("debug.log")));
        assert!(!ignorer.should_ignore(Path::new("important.log")));
    }

    #[test]
    fn test_anchored_pattern() {
        let ignorer = from_patterns(&["/build/"]);
        assert!(ignorer.should_ignore(Path::new("build/output.js")));
    }

    // ── Content Filtering ──────────────────────────────────────────────────────

    #[test]
    fn test_should_ignore_content_minified() {
        let ignorer = RustyCodeIgnore::defaults_only();
        assert!(ignorer.should_ignore_content(Path::new("app.min.js")));
        assert!(ignorer.should_ignore_content(Path::new("style.min.css")));
    }

    #[test]
    fn test_should_ignore_content_source_map() {
        let ignorer = RustyCodeIgnore::defaults_only();
        assert!(ignorer.should_ignore_content(Path::new("app.js.map")));
    }

    #[test]
    fn test_should_ignore_content_normal_source() {
        let ignorer = RustyCodeIgnore::defaults_only();
        assert!(!ignorer.should_ignore_content(Path::new("src/main.rs")));
        assert!(!ignorer.should_ignore_content(Path::new("lib/utils.ts")));
    }

    // ── File Loading ───────────────────────────────────────────────────────────

    #[test]
    fn test_load_from_temp_dir() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();

        // Create .rustycodeignore.
        fs::write(root.join(".rustycodeignore"), "*.secret\ncache/\n").unwrap();

        let ignorer = RustyCodeIgnore::load(root);
        assert!(ignorer.should_ignore(Path::new("keys.secret")));
        assert!(ignorer.should_ignore(Path::new("cache/data.json")));
        assert!(!ignorer.should_ignore(Path::new("src/main.rs")));
    }

    #[test]
    fn test_load_gitignore_fallback() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();

        // Only create .gitignore, no .rustycodeignore.
        fs::write(root.join(".gitignore"), "*.tmp\nenv.local\n").unwrap();

        let ignorer = RustyCodeIgnore::load(root);
        assert!(ignorer.should_ignore(Path::new("temp.tmp")));
        assert!(ignorer.should_ignore(Path::new("env.local")));
    }

    #[test]
    fn test_load_both_files() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();

        fs::write(root.join(".gitignore"), "*.tmp\n").unwrap();
        fs::write(root.join(".rustycodeignore"), "*.secret\n").unwrap();

        let ignorer = RustyCodeIgnore::load(root);
        // Patterns from both files should apply.
        assert!(ignorer.should_ignore(Path::new("temp.tmp"))); // from .gitignore
        assert!(ignorer.should_ignore(Path::new("keys.secret"))); // from .rustycodeignore
    }

    // ── Comments and Empty Lines ───────────────────────────────────────────────

    #[test]
    fn test_comments_and_empty_lines_ignored() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();

        fs::write(
            root.join(".rustycodeignore"),
            "# This is a comment\n\n  \n*.log\n# Another comment\n",
        )
        .unwrap();

        let ignorer = RustyCodeIgnore::load(root);
        assert!(ignorer.should_ignore(Path::new("app.log")));
        assert!(!ignorer.should_ignore(Path::new("app.rs")));
    }

    // ── Absolute Path Handling ─────────────────────────────────────────────────

    #[test]
    fn test_absolute_path_under_project_root() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_path_buf();

        fs::write(root.join(".rustycodeignore"), "secrets/\n").unwrap();

        let ignorer = RustyCodeIgnore::load(&root);
        let abs_path = root.join("secrets/keys.pem");
        assert!(ignorer.should_ignore(&abs_path));
    }

    // ── Edge Cases ─────────────────────────────────────────────────────────────

    #[test]
    fn test_empty_pattern_file() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();

        fs::write(root.join(".rustycodeignore"), "").unwrap();

        let ignorer = RustyCodeIgnore::load(root);
        // Defaults should still apply.
        assert!(ignorer.should_ignore(Path::new("target/debug/main")));
    }

    #[test]
    fn test_no_ignore_files() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();

        let ignorer = RustyCodeIgnore::load(root);
        // Only defaults apply.
        assert!(ignorer.should_ignore(Path::new("target/debug/main")));
        assert!(!ignorer.should_ignore(Path::new("src/main.rs")));
    }

    #[test]
    fn test_case_insensitive_matching() {
        let ignorer = from_patterns(&["*.log"]);
        assert!(ignorer.should_ignore(Path::new("app.LOG")));
        assert!(ignorer.should_ignore(Path::new("App.Log")));
    }

    #[test]
    fn test_pattern_count_includes_defaults() {
        let ignorer = RustyCodeIgnore::defaults_only();
        assert!(ignorer.pattern_count() > 0);
    }
}
