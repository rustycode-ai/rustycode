//! Context-Aware Permission Store
//!
//! Persistent permission storage with context hashing, expiry support,
//! and atomic file persistence. Ported from goose's `permission_store.rs`.
//!
//! # Features
//!
//! - **Context-aware permissions**: Hashes tool arguments to differentiate
//!   similar calls (e.g., "allow `rm` on /tmp" vs "deny `rm` on /etc")
//! - **Expiry support**: Temporary permissions with configurable TTL
//! - **Atomic persistence**: Uses temp file + rename for safe writes
//! - **Auto-cleanup**: Removes expired entries on load
//!
//! # Example
//!
//! ```ignore
//! use rustycode_tools_security::permission_store::{PermissionStore, PermissionRecord};
//!
//! let mut store = PermissionStore::new("/tmp/permissions.json");
//! store.allow("bash", "ls -la", None);           // permanent
//! store.allow("bash", "rm /tmp/*", Some(3600));  // 1 hour
//! store.deny("bash", "rm -rf /");
//!
//! assert!(store.is_allowed("bash", "ls -la"));
//! assert!(!store.is_allowed("bash", "rm -rf /"));
//! ```

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tracing;

/// A stored permission record with optional expiry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionRecord {
    /// Tool name (e.g., "bash", "`write_file`")
    pub tool_name: String,
    /// Whether this permission allows or denies the action
    pub allowed: bool,
    /// Hash of the tool arguments for context-aware matching
    pub context_hash: String,
    /// Human-readable description of the context (for debugging)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub readable_context: Option<String>,
    /// Unix timestamp when this record was created
    pub created_at: i64,
    /// Optional Unix timestamp when this record expires
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<i64>,
}

impl PermissionRecord {
    pub fn new(
        tool_name: impl Into<String>,
        context: &str,
        allowed: bool,
        ttl_secs: Option<u64>,
    ) -> Self {
        let now = chrono::Utc::now().timestamp();
        Self {
            tool_name: tool_name.into(),
            allowed,
            context_hash: hash_context(context),
            readable_context: Some(context.to_string()).filter(|s| !s.is_empty()),
            created_at: now,
            expires_at: ttl_secs.map(|ttl| now + ttl as i64),
        }
    }

    /// Check if this record has expired.
    pub fn is_expired(&self) -> bool {
        if let Some(expires) = self.expires_at {
            chrono::Utc::now().timestamp() >= expires
        } else {
            false
        }
    }

    /// Check if this record matches the given tool + context.
    pub fn matches(&self, tool_name: &str, context: &str) -> bool {
        self.tool_name == tool_name && self.context_hash == hash_context(context)
    }

    /// Check if this record matches by tool name only (wildcard).
    pub fn matches_tool(&self, tool_name: &str) -> bool {
        self.tool_name == tool_name && self.context_hash == hash_context("")
    }
}

/// Hash a context string for comparison.
///
/// Uses a simple hash that's deterministic and fast.
fn hash_context(context: &str) -> String {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    context.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

/// Persistent permission store with context-aware matching.
///
/// Stores permission records in a JSON file with atomic writes.
/// Expired entries are cleaned up on load.
#[derive(Debug)]
pub struct PermissionStore {
    /// Path to the JSON persistence file
    path: PathBuf,
    /// In-memory records indexed by (`tool_name`, `context_hash`)
    records: HashMap<String, PermissionRecord>,
}

impl PermissionStore {
    /// Create a new permission store backed by a JSON file.
    ///
    /// If the file exists, records are loaded automatically.
    /// Expired entries are cleaned up during load.
    pub fn new(path: impl AsRef<Path>) -> Self {
        let path = path.as_ref().to_path_buf();
        let mut store = Self {
            path: path.clone(),
            records: HashMap::new(),
        };
        let _ = store.load();
        store
    }

    /// Create an in-memory-only store (no persistence).
    pub fn in_memory() -> Self {
        Self {
            path: PathBuf::new(),
            records: HashMap::new(),
        }
    }

    /// Get the storage key for a record.
    fn key(tool_name: &str, context_hash: &str) -> String {
        format!("{tool_name}:{context_hash}")
    }

    /// Allow a tool+context combination.
    ///
    pub fn allow(&mut self, tool_name: &str, context: &str, ttl_secs: Option<u64>) {
        let record = PermissionRecord::new(tool_name, context, true, ttl_secs);
        let key = Self::key(tool_name, &record.context_hash);
        self.records.insert(key, record);
        if let Err(e) = self.save() {
            tracing::warn!("Failed to persist permission allow: {}", e);
        }
    }

    /// Deny a tool+context combination.
    pub fn deny(&mut self, tool_name: &str, context: &str) {
        let record = PermissionRecord::new(tool_name, context, false, None);
        let key = Self::key(tool_name, &record.context_hash);
        self.records.insert(key, record);
        if let Err(e) = self.save() {
            tracing::warn!("Failed to persist permission deny: {}", e);
        }
    }

    /// Allow all operations for a tool (wildcard).
    pub fn allow_all(&mut self, tool_name: &str, ttl_secs: Option<u64>) {
        self.allow(tool_name, "", ttl_secs);
    }

    /// Deny all operations for a tool.
    pub fn deny_all(&mut self, tool_name: &str) {
        self.deny(tool_name, "");
    }

    /// Check if a tool+context is allowed.
    ///
    /// Returns `true` if:
    /// 1. A matching ALLOW record exists and hasn't expired
    /// 2. No matching DENY record exists
    pub fn is_allowed(&self, tool_name: &str, context: &str) -> bool {
        // Check specific context first
        let context_hash = hash_context(context);
        let specific_key = Self::key(tool_name, &context_hash);

        if let Some(record) = self.records.get(&specific_key) {
            if record.is_expired() {
                return false;
            }
            return record.allowed;
        }

        // Fall back to wildcard (empty context)
        let wildcard_key = Self::key(tool_name, &hash_context(""));
        if let Some(record) = self.records.get(&wildcard_key) {
            if record.is_expired() {
                return false;
            }
            return record.allowed;
        }

        // No matching record = not explicitly allowed
        false
    }

    /// Check if a tool+context is explicitly denied.
    pub fn is_denied(&self, tool_name: &str, context: &str) -> bool {
        let context_hash = hash_context(context);
        let specific_key = Self::key(tool_name, &context_hash);

        if let Some(record) = self.records.get(&specific_key) {
            if record.is_expired() {
                return false;
            }
            return !record.allowed;
        }

        let wildcard_key = Self::key(tool_name, &hash_context(""));
        if let Some(record) = self.records.get(&wildcard_key) {
            if record.is_expired() {
                return false;
            }
            return !record.allowed;
        }

        false
    }

    /// Get a specific record if it exists.
    pub fn record(&self, tool_name: &str, context: &str) -> Option<&PermissionRecord> {
        let context_hash = hash_context(context);
        let key = Self::key(tool_name, &context_hash);
        self.records.get(&key)
    }

    /// Remove a specific permission record.
    pub fn remove(&mut self, tool_name: &str, context: &str) -> bool {
        let context_hash = hash_context(context);
        let key = Self::key(tool_name, &context_hash);
        let removed = self.records.remove(&key).is_some();
        if removed {
            if let Err(e) = self.save() {
                tracing::warn!("Failed to persist permission store changes: {}", e);
            }
        }
        removed
    }

    /// Remove all records for a tool.
    pub fn remove_all_for_tool(&mut self, tool_name: &str) -> usize {
        let prefix = format!("{tool_name}:");
        let before = self.records.len();
        self.records.retain(|k, _| !k.starts_with(&prefix));
        let removed = before - self.records.len();
        if removed > 0 {
            if let Err(e) = self.save() {
                tracing::warn!("Failed to persist permission store changes: {}", e);
            }
        }
        removed
    }

    /// Get all records.
    pub fn records(&self) -> Vec<&PermissionRecord> {
        self.records.values().collect()
    }

    /// Get count of active (non-expired) records.
    pub fn active_count(&self) -> usize {
        self.records.values().filter(|r| !r.is_expired()).count()
    }

    /// Clean up expired entries.
    ///
    /// Returns the number of entries removed.
    pub fn cleanup_expired(&mut self) -> usize {
        let before = self.records.len();
        self.records.retain(|_, r| !r.is_expired());
        let removed = before - self.records.len();
        if removed > 0 {
            if let Err(e) = self.save() {
                tracing::warn!("Failed to persist permission store changes: {}", e);
            }
        }
        removed
    }

    /// Clear all records.
    pub fn clear(&mut self) {
        self.records.clear();
        if let Err(e) = self.save() {
            tracing::warn!("Failed to persist permission store clear: {}", e);
        }
    }

    /// Load records from the persistence file.
    fn load(&mut self) -> anyhow::Result<()> {
        if self.path.as_os_str().is_empty() || !self.path.exists() {
            return Ok(());
        }

        let content = std::fs::read_to_string(&self.path)?;
        let records: Vec<PermissionRecord> = serde_json::from_str(&content)?;

        self.records.clear();
        for record in records {
            if !record.is_expired() {
                let key = Self::key(&record.tool_name, &record.context_hash);
                self.records.insert(key, record);
            }
        }

        // Save cleaned-up version (expired records removed)
        if let Err(e) = self.save() {
            tracing::warn!("Failed to persist permission store cleanup: {}", e);
        }

        Ok(())
    }

    /// Save records to the persistence file using atomic write.
    fn save(&self) -> anyhow::Result<()> {
        if self.path.as_os_str().is_empty() {
            return Ok(());
        }

        // Ensure parent directory exists
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let records: Vec<&PermissionRecord> =
            self.records.values().filter(|r| !r.is_expired()).collect();
        let json = serde_json::to_string_pretty(&records)?;

        // Atomic write: write to temp file, then rename
        let temp_path = self.path.with_extension("tmp");
        std::fs::write(&temp_path, &json).inspect_err(|_| {
            let _ = std::fs::remove_file(&temp_path);
        })?;
        std::fs::rename(&temp_path, &self.path).inspect_err(|_| {
            let _ = std::fs::remove_file(&temp_path);
        })?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_allow_and_check() {
        let mut store = PermissionStore::in_memory();
        store.allow("bash", "ls -la", None);

        assert!(store.is_allowed("bash", "ls -la"));
        assert!(!store.is_allowed("bash", "rm -rf /"));
        assert!(!store.is_allowed("read_file", "anything"));
    }

    #[test]
    fn test_deny_and_check() {
        let mut store = PermissionStore::in_memory();
        store.deny("bash", "rm -rf /");

        assert!(store.is_denied("bash", "rm -rf /"));
        assert!(!store.is_allowed("bash", "rm -rf /"));
        assert!(!store.is_denied("bash", "ls -la"));
    }

    #[test]
    fn test_wildcard_allow() {
        let mut store = PermissionStore::in_memory();
        store.allow_all("read_file", None);

        assert!(store.is_allowed("read_file", "/tmp/test.txt"));
        assert!(store.is_allowed("read_file", "/etc/passwd"));
        assert!(store.is_allowed("read_file", "anything"));
    }

    #[test]
    fn test_wildcard_deny() {
        let mut store = PermissionStore::in_memory();
        store.deny_all("bash");

        assert!(store.is_denied("bash", "ls"));
        assert!(store.is_denied("bash", "rm"));
        assert!(!store.is_allowed("bash", "anything"));
    }

    #[test]
    fn test_specific_overrides_wildcard() {
        let mut store = PermissionStore::in_memory();
        store.deny_all("bash");
        store.allow("bash", "ls -la", None);

        // Specific should override wildcard
        assert!(store.is_allowed("bash", "ls -la"));
        // But wildcard still applies to other contexts
        assert!(store.is_denied("bash", "rm -rf /"));
    }

    #[test]
    fn test_expiry() {
        let mut store = PermissionStore::in_memory();
        store.allow("bash", "ls", Some(0)); // expires immediately

        // Should be expired
        assert!(!store.is_allowed("bash", "ls"));
    }

    #[test]
    fn test_cleanup_expired() {
        let mut store = PermissionStore::in_memory();
        store.allow("bash", "cmd1", Some(0)); // expired
        store.allow("bash", "cmd2", None); // permanent

        assert_eq!(store.records.len(), 2);
        let removed = store.cleanup_expired();
        assert_eq!(removed, 1);
        assert_eq!(store.records.len(), 1);
    }

    #[test]
    fn test_remove_record() {
        let mut store = PermissionStore::in_memory();
        store.allow("bash", "ls", None);

        assert!(store.is_allowed("bash", "ls"));
        assert!(store.remove("bash", "ls"));
        assert!(!store.is_allowed("bash", "ls"));
    }

    #[test]
    fn test_remove_all_for_tool() {
        let mut store = PermissionStore::in_memory();
        store.allow("bash", "ls", None);
        store.allow("bash", "cat", None);
        store.allow("read_file", "/tmp", None);

        assert_eq!(store.remove_all_for_tool("bash"), 2);
        assert_eq!(store.records.len(), 1);
    }

    #[test]
    fn test_clear() {
        let mut store = PermissionStore::in_memory();
        store.allow("bash", "ls", None);
        store.deny("bash", "rm");

        store.clear();
        assert!(store.records.is_empty());
    }

    #[test]
    fn test_active_count() {
        let mut store = PermissionStore::in_memory();
        store.allow("bash", "ls", None);
        store.allow("bash", "rm", Some(0)); // expired
        store.deny("bash", "curl");

        assert_eq!(store.active_count(), 2);
    }

    #[test]
    fn test_persistence() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("test_permissions.json");

        // Write
        {
            let mut store = PermissionStore::new(&path);
            store.allow("bash", "ls", None);
            store.deny("bash", "rm");
        }

        // Read back
        let store = PermissionStore::new(&path);
        assert!(store.is_allowed("bash", "ls"));
        assert!(store.is_denied("bash", "rm"));
        assert_eq!(store.active_count(), 2);
    }

    #[test]
    fn test_persistence_expired_cleanup() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("test_expired.json");

        // Write with expired entry
        {
            let mut store = PermissionStore::new(&path);
            store.allow("bash", "ls", None);
            store.allow("bash", "expired_cmd", Some(0)); // expires immediately
        }

        // Load should clean up expired
        let store = PermissionStore::new(&path);
        assert_eq!(store.active_count(), 1);
        assert!(store.is_allowed("bash", "ls"));
    }

    #[test]
    fn test_hash_deterministic() {
        let h1 = hash_context("ls -la");
        let h2 = hash_context("ls -la");
        assert_eq!(h1, h2);

        let h3 = hash_context("ls -lb");
        assert_ne!(h1, h3);
    }

    #[test]
    fn test_hash_empty_context() {
        let h = hash_context("");
        assert!(!h.is_empty());
    }

    #[test]
    fn test_record_creation() {
        let record = PermissionRecord::new("bash", "ls -la", true, Some(3600));

        assert_eq!(record.tool_name, "bash");
        assert!(record.allowed);
        assert!(record.readable_context.is_some());
        assert!(record.expires_at.is_some());
        assert!(!record.is_expired());
    }

    #[test]
    fn test_record_matches() {
        let record = PermissionRecord::new("bash", "ls -la", true, None);

        assert!(record.matches("bash", "ls -la"));
        assert!(!record.matches("bash", "ls -lb"));
        assert!(!record.matches("read_file", "ls -la"));
    }

    #[test]
    fn test_get_record() {
        let mut store = PermissionStore::in_memory();
        store.allow("bash", "ls -la", None);

        let record = store.record("bash", "ls -la");
        assert!(record.is_some());
        assert!(record.unwrap().allowed);

        let missing = store.record("bash", "nonexistent");
        assert!(missing.is_none());
    }
}
