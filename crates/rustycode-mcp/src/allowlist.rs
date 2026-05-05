//! MCP Tool Allowlist Manager
//!
//! Provides session-based and persistent allowlist for MCP tools.
//! Supports auto-approval at server level or individual tool level.

use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;
use tracing::{info, warn};

/// Allowlist entry representing an auto-approved tool or server
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum AllowlistEntry {
    /// Allow all tools from this server
    Server(String),
    /// Allow specific tool from a server
    Tool { server: String, tool: String },
}

impl AllowlistEntry {
    /// Check if this entry matches a given server/tool combination
    pub fn matches(&self, server: &str, tool: &str) -> bool {
        match self {
            Self::Server(s) => s == server,
            Self::Tool { server: s, tool: t } => s == server && t == tool,
        }
    }

    /// Get the server name from this entry
    pub fn server_name(&self) -> &str {
        match self {
            Self::Server(s) => s,
            Self::Tool { server: s, .. } => s,
        }
    }

    /// Get the tool name if this is a tool-specific entry
    pub fn tool_name(&self) -> Option<&str> {
        match self {
            Self::Server(_) => None,
            Self::Tool { tool: t, .. } => Some(t),
        }
    }
}

/// Session-based allowlist (in-memory, cleared on restart)
pub struct SessionAllowlist {
    entries: HashSet<AllowlistEntry>,
}

impl SessionAllowlist {
    pub fn new() -> Self {
        Self {
            entries: HashSet::new(),
        }
    }

    /// Add an entry to the session allowlist
    pub fn add(&mut self, entry: AllowlistEntry) {
        info!("Added to session allowlist: {:?}", entry);
        self.entries.insert(entry);
    }

    /// Remove an entry from the session allowlist
    pub fn remove(&mut self, entry: &AllowlistEntry) {
        info!("Removed from session allowlist: {:?}", entry);
        self.entries.remove(entry);
    }

    /// Clear all entries for a specific server
    pub fn clear_server(&mut self, server_name: &str) {
        let to_remove: Vec<_> = self
            .entries
            .iter()
            .filter(|e| e.server_name() == server_name)
            .cloned()
            .collect();
        for entry in to_remove {
            self.entries.remove(&entry);
        }
    }

    /// Check if a server/tool combination is allowed in this session
    pub fn is_allowed(&self, server: &str, tool: &str) -> bool {
        self.entries.iter().any(|e| e.matches(server, tool))
    }

    /// Get all entries
    pub const fn entries(&self) -> &HashSet<AllowlistEntry> {
        &self.entries
    }

    /// Get entries for a specific server
    pub fn entries_for_server(&self, server: &str) -> Vec<&AllowlistEntry> {
        self.entries
            .iter()
            .filter(|e| e.server_name() == server)
            .collect()
    }
}

impl Default for SessionAllowlist {
    fn default() -> Self {
        Self::new()
    }
}

/// Persistent allowlist stored on disk
#[derive(Debug)]
pub struct PersistentAllowlist {
    file_path: PathBuf,
    entries: HashSet<AllowlistEntry>,
}

impl PersistentAllowlist {
    const FILENAME: &'static str = "mcp-allowlist.json";

    pub fn new(file_path: Option<PathBuf>) -> Result<Self, std::io::Error> {
        let file_path = file_path.unwrap_or_else(Self::default_path);
        let entries = Self::load_entries(&file_path)?;

        Ok(Self { file_path, entries })
    }

    /// Get the default allowlist file path
    fn default_path() -> PathBuf {
        // Use ~/.rustycode/ directory
        let mut path = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
        path.push(".rustycode");
        path.push(Self::FILENAME);
        path
    }

    /// Load entries from file
    fn load_entries(file_path: &PathBuf) -> Result<HashSet<AllowlistEntry>, std::io::Error> {
        if !file_path.exists() {
            return Ok(HashSet::new());
        }

        let content = fs::read_to_string(file_path)?;
        let entries: Vec<AllowlistEntry> = serde_json::from_str(&content).map_err(|e| {
            warn!("Failed to parse allowlist file: {}", e);
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("corrupt allowlist: {e}"),
            )
        })?;

        Ok(entries.into_iter().collect())
    }

    /// Save entries to file
    fn save_entries(&self) -> Result<(), std::io::Error> {
        // Ensure directory exists
        if let Some(parent) = self.file_path.parent() {
            fs::create_dir_all(parent)?;
        }

        let entries: Vec<_> = self.entries.iter().cloned().collect();
        let content = serde_json::to_string_pretty(&entries)?;
        let tmp_path = self.file_path.with_extension("json.tmp");
        fs::write(&tmp_path, &content)?;
        if let Err(e) = fs::rename(&tmp_path, &self.file_path) {
            let _ = fs::remove_file(&tmp_path);
            return Err(e);
        }
        info!("Saved persistent allowlist to {:?}", self.file_path);
        Ok(())
    }

    /// Add an entry to the persistent allowlist
    pub fn add(&mut self, entry: AllowlistEntry) -> Result<(), std::io::Error> {
        info!("Added to persistent allowlist: {:?}", entry);
        self.entries.insert(entry);
        self.save_entries()?;
        Ok(())
    }

    /// Remove an entry from the persistent allowlist
    pub fn remove(&mut self, entry: &AllowlistEntry) -> Result<(), std::io::Error> {
        info!("Removed from persistent allowlist: {:?}", entry);
        self.entries.remove(entry);
        self.save_entries()?;
        Ok(())
    }

    /// Clear all entries for a specific server
    pub fn clear_server(&mut self, server_name: &str) -> Result<(), std::io::Error> {
        let to_remove: Vec<_> = self
            .entries
            .iter()
            .filter(|e| e.server_name() == server_name)
            .cloned()
            .collect();
        for entry in to_remove {
            self.entries.remove(&entry);
        }
        self.save_entries()?;
        Ok(())
    }

    /// Check if a server/tool combination is allowed persistently
    pub fn is_allowed(&self, server: &str, tool: &str) -> bool {
        self.entries.iter().any(|e| e.matches(server, tool))
    }

    /// Get all entries
    pub const fn entries(&self) -> &HashSet<AllowlistEntry> {
        &self.entries
    }

    /// Get entries for a specific server
    pub fn entries_for_server(&self, server: &str) -> Vec<&AllowlistEntry> {
        self.entries
            .iter()
            .filter(|e| e.server_name() == server)
            .collect()
    }

    /// Get the file path
    pub const fn file_path(&self) -> &PathBuf {
        &self.file_path
    }
}

/// Combined allowlist manager (session + persistent)
pub struct AllowlistManager {
    session: SessionAllowlist,
    persistent: PersistentAllowlist,
}

impl AllowlistManager {
    pub fn new() -> Result<Self, std::io::Error> {
        Ok(Self {
            session: SessionAllowlist::new(),
            persistent: PersistentAllowlist::new(None)?,
        })
    }

    /// Create with custom file path
    pub fn with_file_path(file_path: PathBuf) -> Result<Self, std::io::Error> {
        Ok(Self {
            session: SessionAllowlist::new(),
            persistent: PersistentAllowlist::new(Some(file_path))?,
        })
    }

    /// Create an empty allowlist manager (no persistent storage).
    pub fn empty() -> Self {
        Self {
            session: SessionAllowlist::new(),
            persistent: PersistentAllowlist {
                file_path: PathBuf::new(),
                entries: HashSet::new(),
            },
        }
    }

    /// Add an entry to session allowlist only
    pub fn add_session(&mut self, entry: AllowlistEntry) {
        self.session.add(entry);
    }

    /// Add an entry to persistent allowlist
    pub fn add_persistent(&mut self, entry: AllowlistEntry) -> Result<(), std::io::Error> {
        let entry_clone = entry.clone();
        self.persistent.add(entry)?;
        // Also add to session for immediate effect
        self.session.add(entry_clone);
        Ok(())
    }

    /// Remove an entry from both session and persistent allowlists
    pub fn remove(&mut self, entry: &AllowlistEntry) -> Result<(), std::io::Error> {
        self.session.remove(entry);
        self.persistent.remove(entry)?;
        Ok(())
    }

    /// Clear all entries for a server (both session and persistent)
    pub fn clear_server(&mut self, server_name: &str) -> Result<(), std::io::Error> {
        self.session.clear_server(server_name);
        self.persistent.clear_server(server_name)?;
        Ok(())
    }

    /// Check if a server/tool combination is allowed (session or persistent)
    pub fn is_allowed(&self, server: &str, tool: &str) -> bool {
        self.session.is_allowed(server, tool) || self.persistent.is_allowed(server, tool)
    }

    /// Get the reason why a tool is allowed
    pub fn check_allowlist_status(&self, server: &str, tool: &str) -> AllowlistStatus {
        if self.session.is_allowed(server, tool) && self.persistent.is_allowed(server, tool) {
            AllowlistStatus::AllowedBoth
        } else if self.persistent.is_allowed(server, tool) {
            AllowlistStatus::AllowedPersistent
        } else if self.session.is_allowed(server, tool) {
            AllowlistStatus::AllowedSession
        } else {
            AllowlistStatus::NotAllowed
        }
    }

    /// Get all entries (combined)
    pub fn entries(&self) -> Vec<&AllowlistEntry> {
        let mut entries: Vec<_> = self.persistent.entries().iter().collect();
        for entry in self.session.entries() {
            if !entries.contains(&entry) {
                entries.push(entry);
            }
        }
        entries
    }

    /// Get entries for a specific server
    pub fn entries_for_server(&self, server: &str) -> Vec<&AllowlistEntry> {
        let mut entries: Vec<_> = self.persistent.entries_for_server(server);
        for entry in self.session.entries_for_server(server) {
            if !entries.contains(&entry) {
                entries.push(entry);
            }
        }
        entries
    }

    /// Get the session allowlist (for testing)
    pub const fn session(&self) -> &SessionAllowlist {
        &self.session
    }

    /// Get the persistent allowlist
    pub const fn persistent(&self) -> &PersistentAllowlist {
        &self.persistent
    }
}

impl Default for AllowlistManager {
    fn default() -> Self {
        Self::new().unwrap_or_else(|e| {
            warn!("Failed to create persistent allowlist, using empty: {}", e);
            Self::empty()
        })
    }
}

/// Allowlist status for a tool
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum AllowlistStatus {
    /// Not allowed - requires confirmation
    NotAllowed,
    /// Allowed for this session only
    AllowedSession,
    /// Allowed persistently (across sessions)
    AllowedPersistent,
    /// Allowed both ways (redundant but explicit)
    AllowedBoth,
}

impl AllowlistStatus {
    pub const fn requires_confirmation(&self) -> bool {
        matches!(self, Self::NotAllowed)
    }

    pub const fn description(&self) -> &'static str {
        match self {
            Self::NotAllowed => "Requires confirmation",
            Self::AllowedSession => "Allowed for this session",
            Self::AllowedPersistent => "Allowed persistently",
            Self::AllowedBoth => "Allowed (session + persistent)",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io;

    fn unique_test_path(prefix: &str) -> PathBuf {
        std::env::temp_dir().join(format!("{prefix}-{}.json", uuid::Uuid::new_v4()))
    }

    fn create_test_manager() -> AllowlistManager {
        let file_path = unique_test_path("mcp-allowlist-test");
        AllowlistManager::with_file_path(file_path).unwrap()
    }

    #[test]
    fn test_allowlist_entry_matches() {
        let server_entry = AllowlistEntry::Server("my-server".to_string());
        assert!(server_entry.matches("my-server", "any_tool"));
        assert!(!server_entry.matches("other-server", "any_tool"));

        let tool_entry = AllowlistEntry::Tool {
            server: "my-server".to_string(),
            tool: "my-tool".to_string(),
        };
        assert!(tool_entry.matches("my-server", "my-tool"));
        assert!(!tool_entry.matches("my-server", "other-tool"));
        assert!(!tool_entry.matches("other-server", "my-tool"));
    }

    #[test]
    fn test_session_allowlist() {
        let mut allowlist = SessionAllowlist::new();

        // Initially empty
        assert!(!allowlist.is_allowed("server1", "tool1"));

        // Add server-level entry
        allowlist.add(AllowlistEntry::Server("server1".to_string()));
        assert!(allowlist.is_allowed("server1", "tool1"));
        assert!(allowlist.is_allowed("server1", "tool2"));
        assert!(!allowlist.is_allowed("server2", "tool1"));

        // Add tool-level entry
        allowlist.add(AllowlistEntry::Tool {
            server: "server2".to_string(),
            tool: "tool1".to_string(),
        });
        assert!(allowlist.is_allowed("server2", "tool1"));
        assert!(!allowlist.is_allowed("server2", "tool2"));

        // Clear server
        allowlist.clear_server("server1");
        assert!(!allowlist.is_allowed("server1", "tool1"));
        assert!(!allowlist.is_allowed("server1", "tool2"));
        assert!(allowlist.is_allowed("server2", "tool1"));
    }

    #[test]
    fn test_persistent_allowlist() -> io::Result<()> {
        let file_path = unique_test_path("mcp-allowlist-persistent-test");

        // Create and add entries
        let mut allowlist = PersistentAllowlist::new(Some(file_path.clone()))?;
        allowlist.add(AllowlistEntry::Server("server1".to_string()))?;

        // Reload and verify
        let allowlist2 = PersistentAllowlist::new(Some(file_path.clone()))?;
        assert!(allowlist2.is_allowed("server1", "tool1"));
        assert!(allowlist2.is_allowed("server1", "tool2"));
        assert!(!allowlist2.is_allowed("server2", "tool1"));

        // Clean up
        fs::remove_file(file_path)?;
        Ok(())
    }

    #[test]
    fn test_allowlist_manager_combined() -> io::Result<()> {
        let mut manager = create_test_manager();

        // Initially not allowed
        assert!(!manager.is_allowed("server1", "tool1"));
        assert_eq!(
            manager.check_allowlist_status("server1", "tool1"),
            AllowlistStatus::NotAllowed
        );

        // Add to session only
        manager.add_session(AllowlistEntry::Server("server1".to_string()));
        assert!(manager.is_allowed("server1", "tool1"));
        assert_eq!(
            manager.check_allowlist_status("server1", "tool1"),
            AllowlistStatus::AllowedSession
        );

        // Add to persistent
        manager.add_persistent(AllowlistEntry::Tool {
            server: "server2".to_string(),
            tool: "tool1".to_string(),
        })?;
        assert!(manager.is_allowed("server2", "tool1"));
        assert_eq!(
            manager.check_allowlist_status("server2", "tool1"),
            AllowlistStatus::AllowedBoth
        );

        // Clean up
        manager.remove(&AllowlistEntry::Tool {
            server: "server2".to_string(),
            tool: "tool1".to_string(),
        })?;

        Ok(())
    }

    #[test]
    fn test_allowlist_entry_serialization() {
        let server_entry = AllowlistEntry::Server("my-server".to_string());
        let json = serde_json::to_string(&server_entry).unwrap();
        let parsed: AllowlistEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, server_entry);

        let tool_entry = AllowlistEntry::Tool {
            server: "srv".to_string(),
            tool: "bash".to_string(),
        };
        let json = serde_json::to_string(&tool_entry).unwrap();
        let parsed: AllowlistEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, tool_entry);
    }

    #[test]
    fn test_allowlist_entry_accessors() {
        let server_entry = AllowlistEntry::Server("my-server".to_string());
        assert_eq!(server_entry.server_name(), "my-server");
        assert!(server_entry.tool_name().is_none());

        let tool_entry = AllowlistEntry::Tool {
            server: "srv".to_string(),
            tool: "read".to_string(),
        };
        assert_eq!(tool_entry.server_name(), "srv");
        assert_eq!(tool_entry.tool_name(), Some("read"));
    }

    #[test]
    fn test_allowlist_status_methods() {
        assert!(AllowlistStatus::NotAllowed.requires_confirmation());
        assert!(!AllowlistStatus::AllowedSession.requires_confirmation());
        assert!(!AllowlistStatus::AllowedPersistent.requires_confirmation());
        assert!(!AllowlistStatus::AllowedBoth.requires_confirmation());

        assert_eq!(
            AllowlistStatus::NotAllowed.description(),
            "Requires confirmation"
        );
        assert_eq!(
            AllowlistStatus::AllowedSession.description(),
            "Allowed for this session"
        );
        assert_eq!(
            AllowlistStatus::AllowedPersistent.description(),
            "Allowed persistently"
        );
        assert_eq!(
            AllowlistStatus::AllowedBoth.description(),
            "Allowed (session + persistent)"
        );
    }

    #[test]
    fn test_session_allowlist_default() {
        let allowlist = SessionAllowlist::default();
        assert!(allowlist.entries().is_empty());
        assert!(!allowlist.is_allowed("any", "any"));
    }

    #[test]
    fn test_session_allowlist_entries_for_server() {
        let mut allowlist = SessionAllowlist::new();
        allowlist.add(AllowlistEntry::Server("srv1".to_string()));
        allowlist.add(AllowlistEntry::Tool {
            server: "srv1".to_string(),
            tool: "bash".to_string(),
        });
        allowlist.add(AllowlistEntry::Server("srv2".to_string()));

        let srv1_entries = allowlist.entries_for_server("srv1");
        assert_eq!(srv1_entries.len(), 2);

        let srv2_entries = allowlist.entries_for_server("srv2");
        assert_eq!(srv2_entries.len(), 1);

        let unknown = allowlist.entries_for_server("unknown");
        assert!(unknown.is_empty());
    }

    #[test]
    fn test_session_allowlist_remove() {
        let mut allowlist = SessionAllowlist::new();
        let entry = AllowlistEntry::Server("srv".to_string());
        allowlist.add(entry.clone());
        assert!(allowlist.is_allowed("srv", "any"));

        allowlist.remove(&entry);
        assert!(!allowlist.is_allowed("srv", "any"));
    }

    #[test]
    fn test_session_allowlist_duplicate_add() {
        let mut allowlist = SessionAllowlist::new();
        allowlist.add(AllowlistEntry::Server("srv".to_string()));
        allowlist.add(AllowlistEntry::Server("srv".to_string()));
        // HashSet deduplicates
        assert_eq!(allowlist.entries().len(), 1);
    }

    #[test]
    fn test_persistent_allowlist_file_path() -> io::Result<()> {
        let file_path = unique_test_path("mcp-allowlist-path-test");
        let allowlist = PersistentAllowlist::new(Some(file_path.clone()))?;
        assert_eq!(allowlist.file_path(), &file_path);

        // Clean up
        let _ = fs::remove_file(file_path);
        Ok(())
    }

    #[test]
    fn test_persistent_allowlist_entries_for_server() -> io::Result<()> {
        let file_path = unique_test_path("mcp-allowlist-srv-entries-test");
        let mut allowlist = PersistentAllowlist::new(Some(file_path.clone()))?;
        allowlist.add(AllowlistEntry::Tool {
            server: "srv".to_string(),
            tool: "t1".to_string(),
        })?;
        allowlist.add(AllowlistEntry::Tool {
            server: "srv".to_string(),
            tool: "t2".to_string(),
        })?;
        allowlist.add(AllowlistEntry::Server("other".to_string()))?;

        let srv_entries = allowlist.entries_for_server("srv");
        assert_eq!(srv_entries.len(), 2);

        let _ = fs::remove_file(file_path);
        Ok(())
    }

    #[test]
    fn test_persistent_allowlist_clear_server() -> io::Result<()> {
        let file_path = unique_test_path("mcp-allowlist-clear-test");
        let mut allowlist = PersistentAllowlist::new(Some(file_path.clone()))?;
        allowlist.add(AllowlistEntry::Server("srv".to_string()))?;
        assert!(allowlist.is_allowed("srv", "any"));

        allowlist.clear_server("srv")?;
        assert!(!allowlist.is_allowed("srv", "any"));

        let _ = fs::remove_file(file_path);
        Ok(())
    }

    #[test]
    fn test_allowlist_manager_entries() -> io::Result<()> {
        let mut manager = create_test_manager();
        assert!(manager.entries().is_empty());

        manager.add_session(AllowlistEntry::Server("srv1".to_string()));
        manager.add_persistent(AllowlistEntry::Server("srv2".to_string()))?;

        let entries = manager.entries();
        assert_eq!(entries.len(), 2);

        // Clean up
        manager.clear_server("srv2")?;
        Ok(())
    }

    #[test]
    fn test_allowlist_manager_entries_for_server() -> io::Result<()> {
        let mut manager = create_test_manager();
        manager.add_session(AllowlistEntry::Server("srv1".to_string()));
        manager.add_session(AllowlistEntry::Tool {
            server: "srv2".to_string(),
            tool: "t1".to_string(),
        });

        let srv1 = manager.entries_for_server("srv1");
        assert_eq!(srv1.len(), 1);
        let srv2 = manager.entries_for_server("srv2");
        assert_eq!(srv2.len(), 1);

        Ok(())
    }

    #[test]
    fn test_allowlist_manager_accessors() {
        let manager = create_test_manager();
        assert!(manager.session().entries().is_empty());
        assert!(manager.persistent().entries().is_empty());
    }

    #[test]
    fn test_corrupt_allowlist_file_returns_error() -> io::Result<()> {
        let file_path = unique_test_path("mcp-allowlist-corrupt-test");
        fs::write(&file_path, "this is not valid json{{{")?;

        let result = PersistentAllowlist::new(Some(file_path.clone()));
        assert!(
            result.is_err(),
            "Corrupt file should return an error, not silently load empty"
        );
        assert!(result.unwrap_err().kind() == io::ErrorKind::InvalidData);

        let _ = fs::remove_file(file_path);
        Ok(())
    }

    #[test]
    fn test_empty_manager_works() {
        let manager = AllowlistManager::empty();
        assert!(!manager.is_allowed("any-server", "any-tool"));
        assert!(manager.entries().is_empty());
    }
}
