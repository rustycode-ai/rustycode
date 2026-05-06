//! LSP configuration types shared between config, LSP client, and tools.
//!
//! Pure data types (no I/O) for LSP server configuration.
//! Filesystem-dependent methods (`from_path`, `detect_root_dir`) remain
//! in `rustycode-lsp` to keep protocol free of I/O dependencies.

use std::fmt;

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

impl LanguageId {
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
            Self::Python => Some(("pyright-langserver", &["--stdin"])),
            Self::Go => Some(("gopls", &["serve"])),
            Self::C | Self::Cpp => Some(("clangd", &[])),
            Self::Java => Some(("jdtls", &[])),
            Self::Ruby => Some(("solargraph", &["stdio"])),
            Self::Php => Some(("phpactor", &["language-server"])),
            Self::Unknown => None,
        }
    }
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

    #[test]
    fn language_id_str_roundtrip() {
        assert_eq!(LanguageId::Rust.language_id_str(), "rust");
        assert_eq!(LanguageId::TypeScript.language_id_str(), "typescript");
        assert_eq!(LanguageId::Unknown.language_id_str(), "unknown");
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
