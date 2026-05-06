//! LSP configuration types shared between config, LSP client, and tools.

use std::fmt;
use std::path::{Path, PathBuf};

/// Supported programming languages with known LSP server configurations.
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
    Unknown,
}

impl LanguageId {
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

    pub const fn default_server_command(&self) -> Option<(&'static str, &'static [&'static str])> {
        match self {
            Self::Rust => Some(("rust-analyzer", &[])),
            Self::TypeScript | Self::JavaScript => {
                Some(("typescript-language-server", &["--stdio"]))
            }
            Self::Python => Some(("pyright-langserver", &["--stdin"])),
            Self::Go => Some(("gopls", &["serve"])),
            Self::C | Self::Cpp => Some(("clangd", &[])),
            Self::Java => Some(("jdtls", &[])),
            Self::Ruby => Some(("solargraph", &["stdio"])),
            Self::Php => Some(("phpactor", &["language-server"])),
            Self::Unknown => None,
        }
    }

    /// Detect language from a file path using extension, then shebang fallback.
    pub fn from_path(path: &Path) -> Self {
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

        if let Some(interpreter) = read_shebang_interpreter(path) {
            match interpreter.as_str() {
                "python" | "python3" | "python2" => return Self::Python,
                "node" | "nodejs" | "deno" | "bun" => return Self::JavaScript,
                "ruby" | "ruby2.7" | "ruby3.0" => return Self::Ruby,
                "go" => return Self::Go,
                _ => {}
            }
        }

        Self::Unknown
    }

    /// Detect the project root by walking up from `start` looking for marker files.
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

    let shebang = &line[2..];
    if let Some(rest) = shebang.strip_prefix("/usr/bin/env ") {
        let interpreter = rest.split_whitespace().next()?;
        let base = interpreter.trim_end_matches(|c: char| c.is_ascii_digit() || c == '.');
        return Some(base.to_string());
    }

    if let Some(filename) = shebang.rsplit('/').next() {
        let base = filename.trim_end_matches(|c: char| c.is_ascii_digit() || c == '.');
        if !base.is_empty() {
            return Some(base.to_string());
        }
    }

    None
}

impl fmt::Display for LanguageId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.language_id_str())
    }
}

/// User-configurable LSP server specification.
///
/// Stored as a map keyed by language name (e.g., "rust", "typescript") in config.
/// Overrides the built-in defaults.
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn from_path_extensions() {
        assert_eq!(
            LanguageId::from_path(Path::new("main.rs")),
            LanguageId::Rust
        );
        assert_eq!(
            LanguageId::from_path(Path::new("app.ts")),
            LanguageId::TypeScript
        );
        assert_eq!(
            LanguageId::from_path(Path::new("app.tsx")),
            LanguageId::TypeScript
        );
        assert_eq!(
            LanguageId::from_path(Path::new("app.js")),
            LanguageId::JavaScript
        );
        assert_eq!(
            LanguageId::from_path(Path::new("app.jsx")),
            LanguageId::JavaScript
        );
        assert_eq!(
            LanguageId::from_path(Path::new("main.py")),
            LanguageId::Python
        );
        assert_eq!(LanguageId::from_path(Path::new("main.go")), LanguageId::Go);
        assert_eq!(LanguageId::from_path(Path::new("main.c")), LanguageId::C);
        assert_eq!(
            LanguageId::from_path(Path::new("main.cpp")),
            LanguageId::Cpp
        );
        assert_eq!(
            LanguageId::from_path(Path::new("Main.java")),
            LanguageId::Java
        );
        assert_eq!(LanguageId::from_path(Path::new("app.rb")), LanguageId::Ruby);
        assert_eq!(
            LanguageId::from_path(Path::new("index.php")),
            LanguageId::Php
        );
    }

    #[test]
    fn from_path_unknown() {
        assert_eq!(
            LanguageId::from_path(Path::new("Makefile")),
            LanguageId::Unknown
        );
        assert_eq!(
            LanguageId::from_path(Path::new("README.md")),
            LanguageId::Unknown
        );
    }

    #[test]
    fn from_str_roundtrip() {
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
            assert_eq!(lang, parsed);
        }
        assert!("cobol".parse::<LanguageId>().is_err());
    }

    #[test]
    fn display() {
        assert_eq!(format!("{}", LanguageId::Rust), "rust");
        assert_eq!(format!("{}", LanguageId::Unknown), "unknown");
    }

    #[test]
    fn default_server_command_rust() {
        let (cmd, args) = LanguageId::Rust.default_server_command().unwrap();
        assert_eq!(cmd, "rust-analyzer");
        assert!(args.is_empty());
    }

    #[test]
    fn extensions_non_empty_for_known() {
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
            assert!(!lang.extensions().is_empty());
        }
    }

    #[test]
    fn extensions_unknown_is_empty() {
        assert!(LanguageId::Unknown.extensions().is_empty());
    }

    #[test]
    fn lsp_config_defaults_cover_all_languages() {
        let defaults = LspConfig::defaults();
        assert!(defaults.servers.contains_key("rust"));
        assert!(defaults.servers.contains_key("typescript"));
        assert!(defaults.servers.contains_key("python"));
        assert!(defaults.servers.contains_key("go"));
    }

    #[test]
    fn lsp_config_resolve_user_override() {
        let mut config = LspConfig::default();
        config.servers.insert(
            "rust".into(),
            LspServerConfig::new("custom-ra", vec!["--flag".into()]),
        );
        let resolved = config.resolve(LanguageId::Rust).unwrap();
        assert_eq!(resolved.command, "custom-ra");
    }

    #[test]
    fn lsp_config_resolve_falls_back_to_default() {
        let config = LspConfig::default();
        let resolved = config.resolve(LanguageId::Rust).unwrap();
        assert_eq!(resolved.command, "rust-analyzer");
    }

    #[test]
    fn lsp_config_resolve_disabled_returns_none() {
        let mut config = LspConfig::default();
        config.servers.insert(
            "rust".into(),
            LspServerConfig {
                command: "ra".into(),
                args: vec![],
                env: std::collections::HashMap::new(),
                enabled: false,
            },
        );
        assert!(config.resolve(LanguageId::Rust).is_none());
    }
}
