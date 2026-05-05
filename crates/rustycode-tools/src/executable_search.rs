//! Executable Search Path Resolution
//!
//! Builder for constructing platform-aware PATH variables that include
//! common tool locations beyond the default system PATH.
//!
//! Inspired by goose's `config/search_path.rs`. Provides a builder pattern
//! for composing search paths with optional additions (npm global, cargo bin,
//! etc.) and resolving executable names to their full paths.
//!
//! # Example
//!
//! ```ignore
//! use rustycode_tools::executable_search::SearchPathBuilder;
//!
//! // Build a PATH that includes common tool locations
//! let path = SearchPathBuilder::new()
//!     .with_cargo_bin()
//!     .with_npm_global()
//!     .build();
//!
//! // Resolve an executable
//! let node_path = SearchPathBuilder::new()
//!     .with_npm_global()
//!     .resolve("node");
//! ```

use std::env;
use std::ffi::{OsStr, OsString};
use std::path::PathBuf;

/// Builder for constructing extended search paths for executable resolution.
///
/// Starts with platform-specific common directories and allows chaining
/// additional tool-specific paths. The final PATH is built by prepending
/// these directories to the system PATH.
pub struct SearchPathBuilder {
    paths: Vec<PathBuf>,
}

impl SearchPathBuilder {
    /// Create a new builder with platform-specific default paths.
    ///
    /// Default paths include:
    /// - `~/.local/bin` (Unix)
    /// - `/usr/local/bin` (Unix)
    /// - `/opt/homebrew/bin` (macOS Apple Silicon)
    /// - `/opt/local/bin` (macOS `MacPorts`)
    /// - `%ProgramFiles%\Chocolatey\bin` (Windows)
    /// - `~\scoop\shims` (Windows)
    pub fn new() -> Self {
        let mut paths = Vec::new();

        if let Some(home) = dirs::home_dir() {
            paths.push(home.join(".local").join("bin"));
        }

        #[cfg(unix)]
        {
            paths.push(PathBuf::from("/usr/local/bin"));
        }

        if cfg!(target_os = "macos") {
            paths.push(PathBuf::from("/opt/homebrew/bin"));
            paths.push(PathBuf::from("/opt/local/bin"));
        }

        #[cfg(target_os = "windows")]
        {
            if let Some(programfiles) = std::env::var("ProgramFiles").ok() {
                paths.push(PathBuf::from(programfiles).join("Chocolatey").join("bin"));
            }
            if let Some(home) = dirs::home_dir() {
                paths.push(home.join("scoop").join("shims"));
            }
        }

        Self { paths }
    }

    /// Create an empty builder with no default paths.
    pub const fn empty() -> Self {
        Self { paths: Vec::new() }
    }

    /// Add Cargo's bin directory (`~/.cargo/bin`).
    pub fn with_cargo_bin(mut self) -> Self {
        if let Some(home) = dirs::home_dir() {
            self.paths.push(home.join(".cargo").join("bin"));
        }
        self
    }

    /// Add npm's global bin directory.
    ///
    /// Platform-specific:
    /// - Unix: `~/.npm-global/bin`
    /// - Windows: `%APPDATA%\npm`
    pub fn with_npm_global(mut self) -> Self {
        if cfg!(windows) {
            if let Some(appdata) = dirs::data_dir() {
                self.paths.push(appdata.join("npm"));
            }
        } else if let Some(home) = dirs::home_dir() {
            self.paths.push(home.join(".npm-global").join("bin"));
        }
        self
    }

    /// Add a custom path to the search.
    ///
    /// Supports `~` expansion via `shellexpand`.
    pub fn with_path(mut self, path: impl AsRef<str>) -> Self {
        let expanded = shellexpand::tilde(path.as_ref());
        self.paths.push(PathBuf::from(expanded.as_ref()));
        self
    }

    /// Add multiple custom paths.
    pub fn with_paths(mut self, paths: &[&str]) -> Self {
        for path in paths {
            self = self.with_path(*path);
        }
        self
    }

    /// Build the combined PATH environment variable.
    ///
    /// Prepends the configured paths to the current system PATH.
    /// Returns an `OsString` suitable for setting as the PATH env var.
    pub fn build(self) -> OsString {
        let system_path = env::var_os("PATH").unwrap_or_default();
        self.join_with_system_path(system_path)
    }

    /// Join configured paths with a system PATH value.
    fn join_with_system_path(self, system_path: OsString) -> OsString {
        let custom = self.paths.into_iter();
        let system = env::split_paths(&system_path);

        env::join_paths(custom.chain(system)).unwrap_or(system_path)
    }

    /// Resolve an executable name to its full path.
    ///
    /// Searches the configured paths first, then falls back to the
    /// system PATH. Returns the first match found.
    ///
    pub fn resolve(self, name: impl AsRef<OsStr>) -> Result<PathBuf, SearchPathError> {
        let name = name.as_ref();

        // Search configured paths first
        for dir in &self.paths {
            let candidate = dir.join(name);
            if candidate.is_file() {
                return Ok(candidate);
            }
        }

        // Fall back to system PATH using which crate equivalent
        let combined_path = self.build();
        for dir in env::split_paths(&combined_path) {
            let candidate = dir.join(name);
            if candidate.is_file() {
                return Ok(candidate);
            }
        }

        Err(SearchPathError::NotFound(
            name.to_string_lossy().to_string(),
        ))
    }

    /// Check if an executable exists in the search path.
    pub fn exists(&self, name: impl AsRef<OsStr>) -> bool {
        let name = name.as_ref();

        for dir in &self.paths {
            if dir.join(name).is_file() {
                return true;
            }
        }

        // Check system PATH
        if let Ok(system_path) = env::var("PATH") {
            for dir in env::split_paths(&system_path) {
                if dir.join(name).is_file() {
                    return true;
                }
            }
        }

        false
    }

    /// Get the list of configured custom paths (without system PATH).
    pub fn custom_paths(&self) -> &[PathBuf] {
        &self.paths
    }

    /// Deduplicate paths, keeping the first occurrence.
    pub fn deduplicated(mut self) -> Self {
        let mut seen = std::collections::HashSet::new();
        self.paths.retain(|p| seen.insert(p.clone()));
        self
    }

    /// Filter out paths that don't exist on the filesystem.
    pub fn existing_only(mut self) -> Self {
        self.paths.retain(|p| p.is_dir());
        self
    }
}

impl Default for SearchPathBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Error type for executable resolution.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum SearchPathError {
    #[error("executable not found: {0}")]
    NotFound(String),

    #[error("path join error: {0}")]
    JoinError(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn test_builder_includes_default_paths() {
        let builder = SearchPathBuilder::new();
        let paths = builder.custom_paths();

        assert!(!paths.is_empty());
        assert!(paths.iter().any(|p| p.to_string_lossy().contains(".local")));
    }

    #[test]
    fn test_empty_builder() {
        let builder = SearchPathBuilder::empty();
        assert!(builder.custom_paths().is_empty());
    }

    #[test]
    fn test_with_cargo_bin() {
        let builder = SearchPathBuilder::empty().with_cargo_bin();
        let paths = builder.custom_paths();

        assert!(paths.iter().any(|p| p.to_string_lossy().contains(".cargo")));
    }

    #[test]
    fn test_with_npm_global() {
        let builder = SearchPathBuilder::empty().with_npm_global();
        let paths = builder.custom_paths();

        if cfg!(unix) {
            assert!(paths.iter().any(|p| p.to_string_lossy().contains("npm")));
        }
    }

    #[test]
    fn test_with_custom_path() {
        let builder = SearchPathBuilder::empty().with_path("/custom/path");
        let paths = builder.custom_paths();

        assert!(paths.contains(&PathBuf::from("/custom/path")));
    }

    #[test]
    fn test_with_tilde_expansion() {
        let builder = SearchPathBuilder::empty().with_path("~/my-bin");
        let paths = builder.custom_paths();

        // Should expand ~ to home directory
        assert!(!paths.iter().any(|p| p.to_string_lossy().starts_with("~")));
        if let Some(home) = dirs::home_dir() {
            assert!(paths.contains(&home.join("my-bin")));
        }
    }

    #[test]
    fn test_build_includes_system_path() {
        let builder = SearchPathBuilder::new();
        let combined = builder.build();

        let system_path = env::var_os("PATH").unwrap_or_default();
        let combined_str = combined.to_string_lossy();
        let system_str = system_path.to_string_lossy();

        assert!(combined_str.contains(&*system_str));
    }

    #[test]
    fn test_resolve_nonexistent() {
        let builder = SearchPathBuilder::new();
        let result = builder.resolve("nonexistent_executable_12345_abcdef");

        assert!(result.is_err());
        match result.unwrap_err() {
            SearchPathError::NotFound(name) => {
                assert!(name.contains("nonexistent_executable_12345_abcdef"));
            }
            e => panic!("Expected NotFound, got: {}", e),
        }
    }

    #[test]
    fn test_resolve_common_executable() {
        let builder = SearchPathBuilder::new();

        #[cfg(unix)]
        let test_exec = "sh";

        #[cfg(windows)]
        let test_exec = "cmd";

        let result = builder.resolve(test_exec);
        assert!(result.is_ok());
        assert!(result.unwrap().is_file());
    }

    #[test]
    fn test_exists_check() {
        let builder = SearchPathBuilder::new();

        #[cfg(unix)]
        {
            assert!(builder.exists("sh"));
            assert!(!builder.exists("nonexistent_xyz"));
        }
    }

    #[test]
    fn test_deduplicated() {
        let builder = SearchPathBuilder::empty()
            .with_path("/usr/local/bin")
            .with_path("/custom")
            .with_path("/usr/local/bin")
            .deduplicated();

        let paths = builder.custom_paths();
        let count = paths
            .iter()
            .filter(|p| **p == Path::new("/usr/local/bin"))
            .count();
        assert_eq!(count, 1);
    }

    #[test]
    fn test_macos_homebrew_path() {
        if !cfg!(target_os = "macos") {
            return;
        }

        let builder = SearchPathBuilder::new();
        let paths = builder.custom_paths();
        let paths_str: Vec<String> = paths
            .iter()
            .map(|p| p.to_string_lossy().to_string())
            .collect();

        assert!(paths_str.iter().any(|p| p.contains("/opt/homebrew/bin")));
        assert!(paths_str.iter().any(|p| p.contains("/opt/local/bin")));
    }

    #[test]
    fn test_builder_chaining() {
        let builder = SearchPathBuilder::empty()
            .with_cargo_bin()
            .with_npm_global()
            .with_path("/custom/path");

        assert_eq!(builder.custom_paths().len(), 3);
    }

    #[test]
    fn test_with_paths_multiple() {
        let builder = SearchPathBuilder::empty().with_paths(&["/path/a", "/path/b", "/path/c"]);

        assert_eq!(builder.custom_paths().len(), 3);
    }
}
