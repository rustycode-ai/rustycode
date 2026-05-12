//! Application Path Management
//!
//! Centralized path resolution for config, data, and state directories.
//! Inspired by goose's `config/paths.rs` but using the `dirs` crate for
//! cross-platform directory resolution.
//!
//! Supports a `RUSTYCODE_PATH_ROOT` environment variable override for testing
//! and portable installations. When set, all directories are resolved relative
//! to that root instead of using platform-specific locations.
//!
//! # Platform Locations
//!
//! | Directory | macOS                    | Linux                    | Windows                      |
//! |-----------|--------------------------|--------------------------|------------------------------|
//! | Config    | ~/Library/Application Support/rustycode | ~/.config/rustycode    | %APPDATA%\rustycode          |
//! | Data      | ~/Library/Application Support/rustycode | ~/.local/share/rustycode | %APPDATA%\rustycode        |
//! | State     | ~/Library/Application Support/rustycode | ~/.local/state/rustycode | %LOCALAPPDATA%\rustycode   |
//!
//! # Example
//!
//! ```
//! use rustycode_tools::app_paths::AppPaths;
//!
//! let config_dir = AppPaths::config_dir();
//! let data_dir = AppPaths::data_dir();
//!
//! // Get a specific subpath
//! let permissions_file = AppPaths::in_config_dir("tool_permissions.yaml");
//! let session_dir = AppPaths::in_data_dir("sessions");
//! ```

use std::path::PathBuf;

/// Directory type for path resolution.
enum DirType {
    Config,
    Data,
    State,
}

/// Centralized application path resolution.
///
/// Provides static methods for resolving platform-specific directories
/// for configuration, data, and state storage. All paths can be
/// overridden by setting the `RUSTYCODE_PATH_ROOT` environment variable.
pub struct AppPaths;

impl AppPaths {
    /// Resolve a directory of the given type.
    ///
    /// If `RUSTYCODE_PATH_ROOT` is set, returns `{root}/config`,
    /// `{root}/data`, or `{root}/state`. Otherwise uses platform-specific
    /// directories via the `dirs` crate.
    fn get_dir(dir_type: DirType) -> PathBuf {
        if let Ok(test_root) = std::env::var("RUSTYCODE_PATH_ROOT") {
            let base = PathBuf::from(test_root);
            match dir_type {
                DirType::Config => base.join("config"),
                DirType::Data => base.join("data"),
                DirType::State => base.join("state"),
            }
        } else {
            match dir_type {
                DirType::Config => dirs::config_dir()
                    .unwrap_or_else(|| PathBuf::from("."))
                    .join("rustycode"),
                DirType::Data => dirs::data_dir()
                    .unwrap_or_else(|| PathBuf::from("."))
                    .join("rustycode"),
                DirType::State => dirs::state_dir()
                    .or_else(dirs::data_dir)
                    .unwrap_or_else(|| PathBuf::from("."))
                    .join("rustycode"),
            }
        }
    }

    /// Get the configuration directory.
    ///
    /// Platform locations:
    /// - macOS: `~/Library/Application Support/rustycode`
    /// - Linux: `~/.config/rustycode`
    /// - Windows: `%APPDATA%\rustycode`
    pub fn config_dir() -> PathBuf {
        Self::get_dir(DirType::Config)
    }

    /// Get the data directory.
    ///
    /// Platform locations:
    /// - macOS: `~/Library/Application Support/rustycode`
    /// - Linux: `~/.local/share/rustycode`
    /// - Windows: `%APPDATA%\rustycode`
    pub fn data_dir() -> PathBuf {
        Self::get_dir(DirType::Data)
    }

    /// Get the state directory.
    ///
    /// Falls back to the data directory if the platform doesn't
    /// distinguish between data and state.
    ///
    /// Platform locations:
    /// - macOS: `~/Library/Application Support/rustycode`
    /// - Linux: `~/.local/state/rustycode`
    /// - Windows: `%LOCALAPPDATA%\rustycode`
    pub fn state_dir() -> PathBuf {
        Self::get_dir(DirType::State)
    }

    /// Get a subpath within the configuration directory.
    pub fn in_config_dir(subpath: &str) -> PathBuf {
        Self::config_dir().join(subpath)
    }

    /// Get a subpath within the data directory.
    pub fn in_data_dir(subpath: &str) -> PathBuf {
        Self::data_dir().join(subpath)
    }

    /// Get a subpath within the state directory.
    pub fn in_state_dir(subpath: &str) -> PathBuf {
        Self::state_dir().join(subpath)
    }

    /// Ensure a directory exists, creating it and any parents if needed.
    ///
    /// Returns the directory path on success.
    pub fn ensure_dir(path: &std::path::Path) -> std::io::Result<PathBuf> {
        std::fs::create_dir_all(path)?;
        Ok(path.to_path_buf())
    }

    /// Ensure the config directory exists.
    pub fn ensure_config_dir() -> std::io::Result<PathBuf> {
        Self::ensure_dir(&Self::config_dir())
    }

    /// Ensure the data directory exists.
    pub fn ensure_data_dir() -> std::io::Result<PathBuf> {
        Self::ensure_dir(&Self::data_dir())
    }

    /// Ensure the state directory exists.
    pub fn ensure_state_dir() -> std::io::Result<PathBuf> {
        Self::ensure_dir(&Self::state_dir())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_dir_returns_path() {
        let dir = AppPaths::config_dir();
        assert!(!dir.as_os_str().is_empty());
    }

    #[test]
    fn test_data_dir_returns_path() {
        let dir = AppPaths::data_dir();
        assert!(!dir.as_os_str().is_empty());
    }

    #[test]
    fn test_state_dir_returns_path() {
        let dir = AppPaths::state_dir();
        assert!(!dir.as_os_str().is_empty());
    }

    #[test]
    fn test_in_config_dir() {
        let config_dir = AppPaths::config_dir();
        let path = AppPaths::in_config_dir("tool_permissions.yaml");
        assert!(path.to_string_lossy().ends_with("tool_permissions.yaml"));
        // Use components comparison to handle symlinks or path normalization differences
        let config_components: Vec<_> = config_dir.components().collect();
        let path_components: Vec<_> = path.components().collect();
        assert!(
            path_components.starts_with(&config_components),
            "path {} does not start with config_dir {}",
            path.display(),
            config_dir.display()
        );
    }

    #[test]
    fn test_in_data_dir() {
        let data_dir = AppPaths::data_dir();
        let path = AppPaths::in_data_dir("sessions");
        assert!(path.to_string_lossy().ends_with("sessions"));
        assert!(path.starts_with(&data_dir));
    }

    #[test]
    fn test_in_state_dir() {
        let state_dir = AppPaths::state_dir();
        let path = AppPaths::in_state_dir("logs");
        assert!(path.to_string_lossy().ends_with("logs"));
        assert!(path.starts_with(&state_dir));
    }

    #[test]
    fn test_env_override() {
        let temp = tempfile::tempdir().unwrap();

        // Save original value
        let original = std::env::var("RUSTYCODE_PATH_ROOT").ok();

        std::env::set_var("RUSTYCODE_PATH_ROOT", temp.path().as_os_str());

        let config = AppPaths::config_dir();
        let data = AppPaths::data_dir();
        let state = AppPaths::state_dir();

        assert_eq!(config, temp.path().join("config"));
        assert_eq!(data, temp.path().join("data"));
        assert_eq!(state, temp.path().join("state"));

        // Restore original value
        match original {
            Some(v) => std::env::set_var("RUSTYCODE_PATH_ROOT", v),
            None => std::env::remove_var("RUSTYCODE_PATH_ROOT"),
        }
    }

    #[test]
    fn test_ensure_dir_creates_directory() {
        let temp = tempfile::tempdir().unwrap();
        let new_dir = temp.path().join("nested").join("sub").join("dir");

        let result = AppPaths::ensure_dir(&new_dir);
        assert!(result.is_ok());
        assert!(new_dir.is_dir());
    }

    #[test]
    fn test_ensure_dir_idempotent() {
        let temp = tempfile::tempdir().unwrap();
        let dir = temp.path().join("existing");

        // Create twice
        AppPaths::ensure_dir(&dir).unwrap();
        AppPaths::ensure_dir(&dir).unwrap();

        assert!(dir.is_dir());
    }

    #[test]
    fn test_all_dirs_end_with_rustycode() {
        // Without env override, all dirs should end with "rustycode"
        let config = AppPaths::config_dir();
        let data = AppPaths::data_dir();
        let state = AppPaths::state_dir();

        assert!(config.to_string_lossy().ends_with("rustycode"));
        assert!(data.to_string_lossy().ends_with("rustycode"));
        assert!(state.to_string_lossy().ends_with("rustycode"));
    }
}
