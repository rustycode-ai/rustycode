//! Language identification and detection types.
//!
//! Provides a typed [`LanguageId`] enum replacing raw string literals for language
//! identification throughout the LSP subsystem. Also includes utilities for
//! detecting language from file paths (extension + shebang) and discovering
//! project root directories via marker files.

use std::fmt;
use std::path::{Path, PathBuf};

/// Supported programming languages with known LSP server configurations.
///
/// This enum replaces raw `&str` literals like `"rust"`, `"typescript"`, etc.
/// throughout the codebase, providing type safety and centralized language mapping.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum LanguageId {
    Rust,
    TypeScript,
    JavaScript,
    Python,
    Go,
    C,
    Cpp,
    Java,
    Ruby,
    Php,
    /// Unrecognized language — caller must decide how to handle.
    Unknown,
}

/// User-configurable LSP server specification.
///
/// Stored as a map keyed by language name (e.g., "rust", "typescript") in config.
/// Overrides the built-in defaults in [`LanguageId::default_server_command`].
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct LspServerConfig {
    /// Command to start the language server
    pub command: String,
    /// Arguments to pass to the command
    #[serde(default)]
    pub args: Vec<String>,
    /// Environment variables for the server process
    #[serde(default)]
    pub env: std::collections::HashMap<String, String>,
    /// Whether this server config is active
    #[serde(default = "default_enabled")]
    pub enabled: bool,
}

const fn default_enabled() -> bool {
    true
}

impl LspServerConfig {
    /// Create from command and args
    pub fn new(command: impl Into<String>, args: Vec<String>) -> Self {
        Self {
            command: command.into(),
            args,
            env: std::collections::HashMap::new(),
            enabled: true,
        }
    }
}

/// LSP configuration: user overrides merged over built-in defaults.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct LspConfig {
    /// Per-language server overrides, keyed by `language_id_str` (e.g., "rust", "typescript")
    #[serde(default)]
    pub servers: std::collections::HashMap<String, LspServerConfig>,
}

impl LspConfig {
    /// Return built-in default server configs for all supported languages.
    pub fn defaults() -> Self {
        let mut servers = std::collections::HashMap::new();
        servers.insert("rust".into(), LspServerConfig::new("rust-analyzer", vec![]));
        servers.insert(
            "typescript".into(),
            LspServerConfig::new("typescript-language-server", vec!["--stdio".into()]),
        );
        servers.insert(
            "javascript".into(),
            LspServerConfig::new("typescript-language-server", vec!["--stdio".into()]),
        );
        servers.insert(
            "python".into(),
            LspServerConfig::new("pyright-langserver", vec!["--stdin".into()]),
        );
        servers.insert(
            "go".into(),
            LspServerConfig::new("gopls", vec!["serve".into()]),
        );
        servers.insert("c".into(), LspServerConfig::new("clangd", vec![]));
        servers.insert("cpp".into(), LspServerConfig::new("clangd", vec![]));
        servers.insert("java".into(), LspServerConfig::new("jdtls", vec![]));
        servers.insert(
            "ruby".into(),
            LspServerConfig::new("solargraph", vec!["stdio".into()]),
        );
        servers.insert(
            "php".into(),
            LspServerConfig::new("phpactor", vec!["language-server".into()]),
        );
        Self { servers }
    }

    /// Resolve the effective config for a language: user override if present and enabled,
    /// otherwise built-in default.
    pub fn resolve(&self, language: LanguageId) -> Option<LspServerConfig> {
        let key = language.language_id_str();
        // User overrides take precedence
        if let Some(override_config) = self.servers.get(key) {
            if override_config.enabled {
                return Some(override_config.clone());
            }
            return None;
        }
        // Fall back to built-in defaults
        Self::defaults().servers.get(key).cloned()
    }
}
impl LanguageId {
    /// Detect language from a file path using extension and (optionally) shebang.
    ///
    /// # Detection order
    /// 1. File extension (primary)
    /// 2. Shebang line (fallback for scripts without clear extensions)
    ///
    /// Returns [`LanguageId::Unknown`] for unrecognized files instead of
    /// defaulting to a specific language.
    pub fn from_path(path: &Path) -> Self {
        // 1. Try extension-based detection
        if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            match ext {
                "rs" => return Self::Rust,
                "ts" | "tsx" => return Self::TypeScript,
                "js" | "jsx" | "mjs" | "cjs" => return Self::JavaScript,
                "py" | "pyi" | "pyw" => return Self::Python,
                "go" => return Self::Go,
                "c" | "h" => return Self::C,
                "cpp" | "cxx" | "cc" | "hpp" | "hxx" | "hh" => return Self::Cpp,
                "java" => return Self::Java,
                "rb" | "erb" => return Self::Ruby,
                "php" | "phtml" => return Self::Php,
                _ => {}
            }
        }

        // 2. Try shebang-based detection for scripts
        if let Some(interpreter) = read_shebang_interpreter(path) {
            match interpreter.as_str() {
                "python" | "python3" | "python2" => return Self::Python,
                "node" | "nodejs" | "deno" | "bun" => return Self::JavaScript,
                "ruby" | "ruby2.7" | "ruby3.0" => return Self::Ruby,
                "go" => return Self::Go,
                "bash" | "sh" | "zsh" | "fish" => return Self::Unknown, // Shell scripts
                _ => {}
            }
        }

        Self::Unknown
    }

    /// Detect the project root directory by searching upward for marker files.
    ///
    /// Walks up the directory tree (max 20 hops) looking for common project markers
    /// like `Cargo.toml`, `package.json`, `go.mod`, etc.
    ///
    /// Returns `None` if no project marker is found within the search depth.
    pub fn detect_root_dir(start: &Path) -> Option<PathBuf> {
        static MARKERS: &[&str] = &[
            "Cargo.toml",
            "package.json",
            "tsconfig.json",
            "go.mod",
            "go.work",
            "pyproject.toml",
            "setup.py",
            "requirements.txt",
            "pom.xml",
            "build.gradle",
            "build.gradle.kts",
            "CMakeLists.txt",
            "Gemfile",
            "composer.json",
            ".git",
        ];

        let mut current = start;
        for _ in 0..20 {
            for marker in MARKERS {
                if current.join(marker).exists() {
                    return Some(current.to_path_buf());
                }
            }
            current = current.parent()?;
        }
        None
    }

    /// Returns the LSP language identifier string (e.g., `"rust"`, `"typescript"`).
    ///
    /// This is the string sent to LSP servers in `TextDocumentIdentifier.language_id`.
    pub const fn language_id_str(&self) -> &'static str {
        match self {
            Self::Rust => "rust",
            Self::TypeScript => "typescript",
            Self::JavaScript => "javascript",
            Self::Python => "python",
            Self::Go => "go",
            Self::C => "c",
            Self::Cpp => "cpp",
            Self::Java => "java",
            Self::Ruby => "ruby",
            Self::Php => "php",
            Self::Unknown => "unknown",
        }
    }

    /// Returns typical file extensions for this language.
    pub const fn extensions(&self) -> &'static [&'static str] {
        match self {
            Self::Rust => &["rs"],
            Self::TypeScript => &["ts", "tsx"],
            Self::JavaScript => &["js", "jsx", "mjs", "cjs"],
            Self::Python => &["py", "pyi", "pyw"],
            Self::Go => &["go"],
            Self::C => &["c", "h"],
            Self::Cpp => &["cpp", "cxx", "cc", "hpp", "hxx", "hh"],
            Self::Java => &["java"],
            Self::Ruby => &["rb", "erb"],
            Self::Php => &["php", "phtml"],
            Self::Unknown => &[],
        }
    }

    /// Returns the LSP server command name (e.g., `"rust-analyzer"`, `"gopls"`).
    ///
    /// Returns `None` for languages without a built-in server configuration.
    pub const fn default_server_command(&self) -> Option<(&'static str, &'static [&'static str])> {
        match self {
            Self::Rust => Some(("rust-analyzer", &[])),
            Self::TypeScript | Self::JavaScript => {
                Some(("typescript-language-server", &["--stdio"]))
            }
            Self::Python => Some(("pyright-langserver", &["--stdio"])),
            Self::Go => Some(("gopls", &["serve"])),
            _ => None,
        }
    }
}

impl fmt::Display for LanguageId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.language_id_str())
    }
}

impl std::str::FromStr for LanguageId {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "rust" => Ok(Self::Rust),
            "typescript" | "ts" => Ok(Self::TypeScript),
            "javascript" | "js" => Ok(Self::JavaScript),
            "python" | "py" => Ok(Self::Python),
            "go" => Ok(Self::Go),
            "c" => Ok(Self::C),
            "cpp" | "c++" => Ok(Self::Cpp),
            "java" => Ok(Self::Java),
            "ruby" | "rb" => Ok(Self::Ruby),
            "php" => Ok(Self::Php),
            _ => Err(()),
        }
    }
}

/// Read the shebang line from a file and extract the interpreter name.
///
/// Returns `None` if the file can't be read, has no shebang, or the shebang
/// doesn't contain a recognizable interpreter.
fn read_shebang_interpreter(path: &Path) -> Option<String> {
    use std::fs::File;
    use std::io::{BufRead, BufReader};

    let file = File::open(path).ok()?;
    let mut reader = BufReader::new(file);
    let mut first_line = String::new();
    reader.read_line(&mut first_line).ok()?;

    let line = first_line.trim();
    if !line.starts_with("#!") {
        return None;
    }

    // Handle "#!/usr/bin/env python3" style
    let shebang = &line[2..];
    if let Some(rest) = shebang.strip_prefix("/usr/bin/env ") {
        // Take the first word (the interpreter name)
        let interpreter = rest.split_whitespace().next()?;
        // Strip version suffix (e.g., "python3" -> "python")
        let base = interpreter.trim_end_matches(|c: char| c.is_ascii_digit() || c == '.');
        return Some(base.to_string());
    }

    // Handle "#!/usr/bin/python3" or "#!/usr/local/bin/node" style
    if let Some(filename) = shebang.rsplit('/').next() {
        let base = filename.trim_end_matches(|c: char| c.is_ascii_digit() || c == '.');
        if !base.is_empty() {
            return Some(base.to_string());
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- LanguageId::from_path tests ---

    #[test]
    fn test_from_path_rust() {
        assert_eq!(
            LanguageId::from_path(Path::new("src/main.rs")),
            LanguageId::Rust
        );
    }

    #[test]
    fn test_from_path_typescript() {
        assert_eq!(
            LanguageId::from_path(Path::new("app.ts")),
            LanguageId::TypeScript
        );
        assert_eq!(
            LanguageId::from_path(Path::new("app.tsx")),
            LanguageId::TypeScript
        );
    }

    #[test]
    fn test_from_path_javascript() {
        assert_eq!(
            LanguageId::from_path(Path::new("app.js")),
            LanguageId::JavaScript
        );
        assert_eq!(
            LanguageId::from_path(Path::new("app.jsx")),
            LanguageId::JavaScript
        );
        assert_eq!(
            LanguageId::from_path(Path::new("app.mjs")),
            LanguageId::JavaScript
        );
        assert_eq!(
            LanguageId::from_path(Path::new("app.cjs")),
            LanguageId::JavaScript
        );
    }

    #[test]
    fn test_from_path_python() {
        assert_eq!(
            LanguageId::from_path(Path::new("main.py")),
            LanguageId::Python
        );
        assert_eq!(
            LanguageId::from_path(Path::new("types.pyi")),
            LanguageId::Python
        );
        assert_eq!(
            LanguageId::from_path(Path::new("gui.pyw")),
            LanguageId::Python
        );
    }

    #[test]
    fn test_from_path_go() {
        assert_eq!(LanguageId::from_path(Path::new("main.go")), LanguageId::Go);
    }

    #[test]
    fn test_from_path_c() {
        assert_eq!(LanguageId::from_path(Path::new("main.c")), LanguageId::C);
        assert_eq!(LanguageId::from_path(Path::new("types.h")), LanguageId::C);
    }

    #[test]
    fn test_from_path_cpp() {
        assert_eq!(
            LanguageId::from_path(Path::new("main.cpp")),
            LanguageId::Cpp
        );
        assert_eq!(LanguageId::from_path(Path::new("main.cc")), LanguageId::Cpp);
        assert_eq!(
            LanguageId::from_path(Path::new("main.cxx")),
            LanguageId::Cpp
        );
        assert_eq!(
            LanguageId::from_path(Path::new("types.hpp")),
            LanguageId::Cpp
        );
    }

    #[test]
    fn test_from_path_java() {
        assert_eq!(
            LanguageId::from_path(Path::new("Main.java")),
            LanguageId::Java
        );
    }

    #[test]
    fn test_from_path_ruby() {
        assert_eq!(LanguageId::from_path(Path::new("app.rb")), LanguageId::Ruby);
    }

    #[test]
    fn test_from_path_php() {
        assert_eq!(
            LanguageId::from_path(Path::new("index.php")),
            LanguageId::Php
        );
    }

    #[test]
    fn test_from_path_unknown() {
        assert_eq!(
            LanguageId::from_path(Path::new("Makefile")),
            LanguageId::Unknown
        );
        assert_eq!(
            LanguageId::from_path(Path::new("config.toml")),
            LanguageId::Unknown
        );
        assert_eq!(
            LanguageId::from_path(Path::new("README.md")),
            LanguageId::Unknown
        );
    }

    #[test]
    fn test_from_path_no_extension() {
        assert_eq!(
            LanguageId::from_path(Path::new("script")),
            LanguageId::Unknown
        );
    }

    // --- Display tests ---

    #[test]
    fn test_display() {
        assert_eq!(format!("{}", LanguageId::Rust), "rust");
        assert_eq!(format!("{}", LanguageId::TypeScript), "typescript");
        assert_eq!(format!("{}", LanguageId::JavaScript), "javascript");
        assert_eq!(format!("{}", LanguageId::Python), "python");
        assert_eq!(format!("{}", LanguageId::Go), "go");
        assert_eq!(format!("{}", LanguageId::Unknown), "unknown");
    }

    // --- FromStr tests ---

    #[test]
    fn test_from_str() {
        assert_eq!("rust".parse::<LanguageId>(), Ok(LanguageId::Rust));
        assert_eq!(
            "typescript".parse::<LanguageId>(),
            Ok(LanguageId::TypeScript)
        );
        assert_eq!(
            "javascript".parse::<LanguageId>(),
            Ok(LanguageId::JavaScript)
        );
        assert_eq!("python".parse::<LanguageId>(), Ok(LanguageId::Python));
        assert_eq!("go".parse::<LanguageId>(), Ok(LanguageId::Go));
        assert_eq!("cobol".parse::<LanguageId>(), Err(()));
    }

    // --- Default server command tests ---

    #[test]
    fn test_default_server_command_rust() {
        let (cmd, args) = LanguageId::Rust.default_server_command().unwrap();
        assert_eq!(cmd, "rust-analyzer");
        assert!(args.is_empty());
    }

    #[test]
    fn test_default_server_command_typescript() {
        let (cmd, args) = LanguageId::TypeScript.default_server_command().unwrap();
        assert_eq!(cmd, "typescript-language-server");
        assert_eq!(args, &["--stdio"]);
    }

    #[test]
    fn test_default_server_command_javascript_shares_typescript() {
        let (cmd, args) = LanguageId::JavaScript.default_server_command().unwrap();
        assert_eq!(cmd, "typescript-language-server");
        assert_eq!(args, &["--stdio"]);
    }

    #[test]
    fn test_default_server_command_python() {
        let (cmd, args) = LanguageId::Python.default_server_command().unwrap();
        assert_eq!(cmd, "pyright-langserver");
        assert_eq!(args, &["--stdio"]);
    }

    #[test]
    fn test_default_server_command_go() {
        let (cmd, args) = LanguageId::Go.default_server_command().unwrap();
        assert_eq!(cmd, "gopls");
        assert_eq!(args, &["serve"]);
    }

    #[test]
    fn test_default_server_command_unsupported() {
        assert!(LanguageId::C.default_server_command().is_none());
        assert!(LanguageId::Cpp.default_server_command().is_none());
        assert!(LanguageId::Unknown.default_server_command().is_none());
    }

    // --- language_id_str round-trip tests ---

    #[test]
    fn test_language_id_str_roundtrip() {
        for lang in [
            LanguageId::Rust,
            LanguageId::TypeScript,
            LanguageId::JavaScript,
            LanguageId::Python,
            LanguageId::Go,
            LanguageId::C,
            LanguageId::Cpp,
            LanguageId::Java,
            LanguageId::Ruby,
            LanguageId::Php,
        ] {
            let s = lang.language_id_str();
            let parsed: LanguageId = s.parse().unwrap();
            assert_eq!(lang, parsed, "roundtrip failed for {lang:?}");
        }
    }

    // --- Extensions tests ---

    #[test]
    fn test_extensions_non_empty_for_known() {
        for lang in [
            LanguageId::Rust,
            LanguageId::TypeScript,
            LanguageId::JavaScript,
            LanguageId::Python,
            LanguageId::Go,
            LanguageId::C,
            LanguageId::Cpp,
            LanguageId::Java,
            LanguageId::Ruby,
            LanguageId::Php,
        ] {
            assert!(!lang.extensions().is_empty(), "no extensions for {lang:?}");
        }
    }

    #[test]
    fn test_extensions_unknown_is_empty() {
        assert!(LanguageId::Unknown.extensions().is_empty());
    }
}
