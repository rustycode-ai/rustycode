//! Tool Permission Store
//!
//! Persistent, YAML-backed permission configuration for tools.
//! Inspired by goose's `config/permission.rs` but decoupled from MCP types.
//!
//! Manages per-tool permission levels (`AlwaysAllow`, `AskBefore`, `NeverAllow`)
//! organized by "principal" (e.g., "user", "session", "`auto_approve`").
//!
//! # Example
//!
//! ```ignore
//! use rustycode_tools::tool_permissions::{PermissionStore, PermissionLevel};
//!
//! let store = PermissionStore::new("/path/to/config/dir");
//! store.update("user", "Bash", PermissionLevel::AskBefore);
//! store.update("user", "Read", PermissionLevel::AlwaysAllow);
//!
//! let level = store.get("user", "Bash");
//! assert_eq!(level, Some(PermissionLevel::AskBefore));
//! ```

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

/// Permission file name
const PERMISSION_FILE: &str = "tool_permissions.yaml";

/// Permission level for a tool.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum PermissionLevel {
    /// Tool can always be used without prompt
    AlwaysAllow,
    /// Tool requires user confirmation before use
    AskBefore,
    /// Tool is never allowed
    NeverAllow,
}

impl PermissionLevel {
    /// Human-readable label
    pub const fn label(&self) -> &'static str {
        match self {
            Self::AlwaysAllow => "always_allow",
            Self::AskBefore => "ask_before",
            Self::NeverAllow => "never_allow",
        }
    }
}

/// Permission configuration for a single principal.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PrincipalConfig {
    /// Tools that are always allowed
    #[serde(default)]
    pub always_allow: Vec<String>,
    /// Tools that require confirmation
    #[serde(default)]
    pub ask_before: Vec<String>,
    /// Tools that are never allowed
    #[serde(default)]
    pub never_allow: Vec<String>,
}

impl PrincipalConfig {
    fn remove_tool(&mut self, tool_name: &str) {
        self.always_allow.retain(|t| t != tool_name);
        self.ask_before.retain(|t| t != tool_name);
        self.never_allow.retain(|t| t != tool_name);
    }

    fn get_level(&self, tool_name: &str) -> Option<PermissionLevel> {
        if self.always_allow.iter().any(|t| t == tool_name) {
            Some(PermissionLevel::AlwaysAllow)
        } else if self.ask_before.iter().any(|t| t == tool_name) {
            Some(PermissionLevel::AskBefore)
        } else if self.never_allow.iter().any(|t| t == tool_name) {
            Some(PermissionLevel::NeverAllow)
        } else {
            None
        }
    }

    fn set_level(&mut self, tool_name: &str, level: PermissionLevel) {
        self.remove_tool(tool_name);
        match level {
            PermissionLevel::AlwaysAllow => self.always_allow.push(tool_name.to_string()),
            PermissionLevel::AskBefore => self.ask_before.push(tool_name.to_string()),
            PermissionLevel::NeverAllow => self.never_allow.push(tool_name.to_string()),
        }
    }
}

/// Persistent tool permission store backed by a YAML file.
///
/// Thread-safe via `std::sync::Mutex`. Supports multiple principals
/// (e.g., "user", "session", "`auto_approve`") each with their own
/// tool permission settings.
pub struct PermissionStore {
    config_path: PathBuf,
    data: std::sync::Mutex<HashMap<String, PrincipalConfig>>,
}

impl PermissionStore {
    /// Create a new permission store, loading from the config directory.
    ///
    /// If the permission file doesn't exist, creates an empty store.
    /// If the file is corrupted, returns an error.
    pub fn new(config_dir: impl AsRef<Path>) -> Result<Self, PermissionError> {
        let config_dir = config_dir.as_ref();
        let config_path = config_dir.join(PERMISSION_FILE);

        let data = if config_path.exists() {
            let contents = fs::read_to_string(&config_path).map_err(|e| {
                PermissionError::IoError(format!("Failed to read {}: {}", config_path.display(), e))
            })?;
            serde_yaml::from_str(&contents).map_err(|e| {
                PermissionError::ParseError(format!(
                    "Corrupted permission config at {}: {}",
                    config_path.display(),
                    e
                ))
            })?
        } else {
            fs::create_dir_all(config_dir).map_err(|e| {
                PermissionError::IoError(format!("Failed to create config dir: {e}"))
            })?;
            HashMap::new()
        };

        Ok(Self {
            config_path,
            data: std::sync::Mutex::new(data),
        })
    }

    /// Create an in-memory permission store (for testing).
    pub fn new_in_memory() -> Self {
        Self {
            config_path: PathBuf::from("/dev/null"),
            data: std::sync::Mutex::new(HashMap::new()),
        }
    }

    /// Get the permission level for a tool under a principal.
    pub fn get(&self, principal: &str, tool_name: &str) -> Option<PermissionLevel> {
        self.data
            .lock()
            .ok()
            .and_then(|guard| guard.get(principal).and_then(|c| c.get_level(tool_name)))
    }

    /// Set the permission level for a tool under a principal.
    ///
    /// Persists the change to disk immediately.
    pub fn update(
        &self,
        principal: &str,
        tool_name: &str,
        level: PermissionLevel,
    ) -> Result<(), PermissionError> {
        {
            let mut guard = self.data.lock().map_err(|_| {
                PermissionError::LockError("Failed to acquire permission store lock".to_string())
            })?;
            guard
                .entry(principal.to_string())
                .or_default()
                .set_level(tool_name, level);
        }
        self.persist()
    }

    /// Get all principal names in the store.
    pub fn principals(&self) -> Vec<String> {
        self.data
            .lock()
            .map(|guard| guard.keys().cloned().collect())
            .unwrap_or_default()
    }

    /// Get all tools and their levels for a principal.
    pub fn list_permissions(&self, principal: &str) -> Vec<(String, PermissionLevel)> {
        let mut result = Vec::new();
        if let Ok(guard) = self.data.lock() {
            if let Some(config) = guard.get(principal) {
                for tool in &config.always_allow {
                    result.push((tool.clone(), PermissionLevel::AlwaysAllow));
                }
                for tool in &config.ask_before {
                    result.push((tool.clone(), PermissionLevel::AskBefore));
                }
                for tool in &config.never_allow {
                    result.push((tool.clone(), PermissionLevel::NeverAllow));
                }
            }
        }
        result.sort_by(|a, b| a.0.cmp(&b.0));
        result
    }

    /// Remove all entries for tools starting with a prefix.
    ///
    /// Useful for cleaning up permissions when removing an extension.
    pub fn remove_by_prefix(&self, prefix: &str) -> Result<(), PermissionError> {
        {
            let mut guard = self.data.lock().map_err(|_| {
                PermissionError::LockError("Failed to acquire permission store lock".to_string())
            })?;
            for config in guard.values_mut() {
                config.always_allow.retain(|t| !t.starts_with(prefix));
                config.ask_before.retain(|t| !t.starts_with(prefix));
                config.never_allow.retain(|t| !t.starts_with(prefix));
            }
        }
        self.persist()
    }

    /// Remove all entries for a specific tool across all principals.
    pub fn remove_tool(&self, tool_name: &str) -> Result<(), PermissionError> {
        {
            let mut guard = self.data.lock().map_err(|_| {
                PermissionError::LockError("Failed to acquire permission store lock".to_string())
            })?;
            for config in guard.values_mut() {
                config.remove_tool(tool_name);
            }
        }
        self.persist()
    }

    /// Clear all permission data.
    pub fn clear(&self) -> Result<(), PermissionError> {
        {
            let mut guard = self.data.lock().map_err(|_| {
                PermissionError::LockError("Failed to acquire permission store lock".to_string())
            })?;
            guard.clear();
        }
        self.persist()
    }

    /// Check if a tool is allowed under a principal.
    ///
    /// Returns `true` if the tool has `AlwaysAllow` permission.
    /// Returns `false` if `NeverAllow` or no permission set.
    pub fn is_allowed(&self, principal: &str, tool_name: &str) -> bool {
        matches!(
            self.get(principal, tool_name),
            Some(PermissionLevel::AlwaysAllow)
        )
    }

    /// Check if a tool requires confirmation under a principal.
    pub fn needs_confirmation(&self, principal: &str, tool_name: &str) -> bool {
        matches!(
            self.get(principal, tool_name),
            Some(PermissionLevel::AskBefore)
        )
    }

    /// Check if a tool is blocked under a principal.
    pub fn is_blocked(&self, principal: &str, tool_name: &str) -> bool {
        matches!(
            self.get(principal, tool_name),
            Some(PermissionLevel::NeverAllow)
        )
    }

    /// Persist current state to disk.
    fn persist(&self) -> Result<(), PermissionError> {
        // Skip persistence for in-memory stores (path is /dev/null)
        if self.config_path == Path::new("/dev/null") {
            return Ok(());
        }

        let guard = self.data.lock().map_err(|_| {
            PermissionError::LockError("Failed to acquire permission store lock".to_string())
        })?;
        let yaml = serde_yaml::to_string(&*guard).map_err(|e| {
            PermissionError::SerializeError(format!("Failed to serialize permissions: {e}"))
        })?;
        let tmp_path = self.config_path.with_extension("yaml.tmp");
        fs::write(&tmp_path, &yaml).map_err(|e| {
            PermissionError::IoError(format!("Failed to write {}: {}", tmp_path.display(), e))
        })?;
        fs::rename(&tmp_path, &self.config_path).map_err(|e| {
            PermissionError::IoError(format!(
                "Failed to rename {}: {}",
                self.config_path.display(),
                e
            ))
        })
    }

    /// Get the config file path.
    pub fn config_path(&self) -> &Path {
        &self.config_path
    }
}

/// Error type for permission store operations.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum PermissionError {
    #[error("IO error: {0}")]
    IoError(String),

    #[error("Parse error: {0}")]
    ParseError(String),

    #[error("Lock error: {0}")]
    LockError(String),

    #[error("Serialize error: {0}")]
    SerializeError(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_test_store() -> (PermissionStore, TempDir) {
        let temp = TempDir::new().unwrap();
        let store = PermissionStore::new(temp.path()).unwrap();
        (store, temp)
    }

    #[test]
    fn test_new_store_empty() {
        let (store, _temp) = create_test_store();
        assert!(store.principals().is_empty());
    }

    #[test]
    fn test_update_and_get() {
        let (store, _temp) = create_test_store();
        store
            .update("user", "Bash", PermissionLevel::AskBefore)
            .unwrap();
        assert_eq!(store.get("user", "Bash"), Some(PermissionLevel::AskBefore));
    }

    #[test]
    fn test_multiple_principals() {
        let (store, _temp) = create_test_store();
        store
            .update("user", "Bash", PermissionLevel::AskBefore)
            .unwrap();
        store
            .update("auto", "Bash", PermissionLevel::AlwaysAllow)
            .unwrap();

        assert_eq!(store.get("user", "Bash"), Some(PermissionLevel::AskBefore));
        assert_eq!(
            store.get("auto", "Bash"),
            Some(PermissionLevel::AlwaysAllow)
        );
    }

    #[test]
    fn test_update_replaces_level() {
        let (store, _temp) = create_test_store();
        store
            .update("user", "Bash", PermissionLevel::AlwaysAllow)
            .unwrap();
        assert_eq!(
            store.get("user", "Bash"),
            Some(PermissionLevel::AlwaysAllow)
        );

        store
            .update("user", "Bash", PermissionLevel::NeverAllow)
            .unwrap();
        assert_eq!(store.get("user", "Bash"), Some(PermissionLevel::NeverAllow));
    }

    #[test]
    fn test_not_found() {
        let (store, _temp) = create_test_store();
        assert_eq!(store.get("user", "Bash"), None);
    }

    #[test]
    fn test_is_allowed() {
        let (store, _temp) = create_test_store();
        store
            .update("user", "Read", PermissionLevel::AlwaysAllow)
            .unwrap();
        assert!(store.is_allowed("user", "Read"));
        assert!(!store.is_allowed("user", "Bash"));
    }

    #[test]
    fn test_needs_confirmation() {
        let (store, _temp) = create_test_store();
        store
            .update("user", "Bash", PermissionLevel::AskBefore)
            .unwrap();
        assert!(store.needs_confirmation("user", "Bash"));
        assert!(!store.needs_confirmation("user", "Read"));
    }

    #[test]
    fn test_is_blocked() {
        let (store, _temp) = create_test_store();
        store
            .update("user", "dangerous_tool", PermissionLevel::NeverAllow)
            .unwrap();
        assert!(store.is_blocked("user", "dangerous_tool"));
        assert!(!store.is_blocked("user", "Bash"));
    }

    #[test]
    fn test_list_permissions() {
        let (store, _temp) = create_test_store();
        store
            .update("user", "Bash", PermissionLevel::AskBefore)
            .unwrap();
        store
            .update("user", "Read", PermissionLevel::AlwaysAllow)
            .unwrap();
        store
            .update("user", "danger", PermissionLevel::NeverAllow)
            .unwrap();

        let perms = store.list_permissions("user");
        assert_eq!(perms.len(), 3);
        // Should be sorted by tool name (ASCII: uppercase before lowercase)
        assert_eq!(perms[0], ("Bash".to_string(), PermissionLevel::AskBefore));
        assert_eq!(perms[1], ("Read".to_string(), PermissionLevel::AlwaysAllow));
        assert_eq!(
            perms[2],
            ("danger".to_string(), PermissionLevel::NeverAllow)
        );
    }

    #[test]
    fn test_remove_by_prefix() {
        let (store, _temp) = create_test_store();
        store
            .update("user", "ext__tool1", PermissionLevel::AlwaysAllow)
            .unwrap();
        store
            .update("user", "ext__tool2", PermissionLevel::AskBefore)
            .unwrap();
        store
            .update("user", "other_tool", PermissionLevel::AlwaysAllow)
            .unwrap();

        store.remove_by_prefix("ext__").unwrap();

        assert_eq!(store.get("user", "ext__tool1"), None);
        assert_eq!(store.get("user", "ext__tool2"), None);
        assert_eq!(
            store.get("user", "other_tool"),
            Some(PermissionLevel::AlwaysAllow)
        );
    }

    #[test]
    fn test_remove_tool() {
        let (store, _temp) = create_test_store();
        store
            .update("user", "Bash", PermissionLevel::AskBefore)
            .unwrap();
        store
            .update("auto", "Bash", PermissionLevel::AlwaysAllow)
            .unwrap();

        store.remove_tool("Bash").unwrap();

        assert_eq!(store.get("user", "Bash"), None);
        assert_eq!(store.get("auto", "Bash"), None);
    }

    #[test]
    fn test_clear() {
        let (store, _temp) = create_test_store();
        store
            .update("user", "Bash", PermissionLevel::AskBefore)
            .unwrap();
        store.clear().unwrap();
        assert!(store.principals().is_empty());
    }

    #[test]
    fn test_persistence() {
        let temp = TempDir::new().unwrap();

        // Write permissions
        {
            let store = PermissionStore::new(temp.path()).unwrap();
            store
                .update("user", "Bash", PermissionLevel::AskBefore)
                .unwrap();
            store
                .update("user", "Read", PermissionLevel::AlwaysAllow)
                .unwrap();
        }

        // Read back from same directory
        {
            let store = PermissionStore::new(temp.path()).unwrap();
            assert_eq!(store.get("user", "Bash"), Some(PermissionLevel::AskBefore));
            assert_eq!(
                store.get("user", "Read"),
                Some(PermissionLevel::AlwaysAllow)
            );
        }
    }

    #[test]
    fn test_in_memory_store() {
        let store = PermissionStore::new_in_memory();
        store
            .update("user", "Bash", PermissionLevel::AlwaysAllow)
            .unwrap();
        assert_eq!(
            store.get("user", "Bash"),
            Some(PermissionLevel::AlwaysAllow)
        );
    }

    #[test]
    fn test_corrupted_file() {
        let temp = TempDir::new().unwrap();
        fs::write(temp.path().join(PERMISSION_FILE), "{{invalid yaml: [broken").unwrap();
        let result = PermissionStore::new(temp.path());
        assert!(result.is_err());
    }

    #[test]
    fn test_permission_level_label() {
        assert_eq!(PermissionLevel::AlwaysAllow.label(), "always_allow");
        assert_eq!(PermissionLevel::AskBefore.label(), "ask_before");
        assert_eq!(PermissionLevel::NeverAllow.label(), "never_allow");
    }

    #[test]
    fn test_all_permission_levels() {
        let (store, _temp) = create_test_store();

        store
            .update("user", "tool_a", PermissionLevel::AlwaysAllow)
            .unwrap();
        store
            .update("user", "tool_b", PermissionLevel::AskBefore)
            .unwrap();
        store
            .update("user", "tool_c", PermissionLevel::NeverAllow)
            .unwrap();

        assert!(store.is_allowed("user", "tool_a"));
        assert!(!store.is_allowed("user", "tool_b"));
        assert!(!store.is_allowed("user", "tool_c"));

        assert!(!store.needs_confirmation("user", "tool_a"));
        assert!(store.needs_confirmation("user", "tool_b"));
        assert!(!store.needs_confirmation("user", "tool_c"));

        assert!(!store.is_blocked("user", "tool_a"));
        assert!(!store.is_blocked("user", "tool_b"));
        assert!(store.is_blocked("user", "tool_c"));
    }

    #[test]
    fn test_principals() {
        let (store, _temp) = create_test_store();
        store
            .update("user", "Bash", PermissionLevel::AskBefore)
            .unwrap();
        store
            .update("auto", "Bash", PermissionLevel::AlwaysAllow)
            .unwrap();

        let mut principals = store.principals();
        principals.sort();
        assert_eq!(principals, vec!["auto", "user"]);
    }
}
