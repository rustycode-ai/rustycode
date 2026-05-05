//! MCP Server Enablement Manager
//!
//! Provides multi-level server enablement:
//! 1. Admin kill switch (allowlist/excludelist)
//! 2. Session disable (in-memory, cleared on restart)
//! 3. File-based enablement (persistent)

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::PathBuf;
use tracing::{info, warn};

/// Server enablement state stored in config file
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerEnablementState {
    pub enabled: bool,
}

/// Server enablement config (file format)
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ServerEnablementConfig {
    #[serde(default)]
    pub servers: HashMap<String, ServerEnablementState>,
}

/// Result of `can_load_server` check
#[derive(Debug, Clone)]
pub struct ServerLoadResult {
    pub allowed: bool,
    pub reason: Option<String>,
    pub block_type: Option<BlockType>,
}

/// Type of block applied to a server
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum BlockType {
    /// Blocked by admin kill switch
    Admin,
    /// Blocked by allowlist (not in list)
    Allowlist,
    /// Blocked by excludelist
    Excludelist,
    /// Disabled for this session
    Session,
    /// Disabled in file config
    Enablement,
}

/// Display state for UI
#[derive(Debug, Clone)]
pub struct ServerDisplayState {
    /// Effective enabled state (considering all levels)
    pub enabled: bool,
    /// True if disabled via session flag
    pub is_session_disabled: bool,
    /// True if disabled in file config
    pub is_persistent_disabled: bool,
    /// True if blocked by admin allowlist
    pub is_admin_blocked: bool,
    /// True if blocked by admin excludelist
    pub is_excludelist_blocked: bool,
}

/// Server enablement manager
pub struct ServerEnablementManager {
    config_file_path: PathBuf,
    session_disabled: HashSet<String>,
    admin_enabled: bool,
    admin_allowlist: Option<HashSet<String>>,
    admin_excludelist: Option<HashSet<String>>,
}

impl ServerEnablementManager {
    const FILENAME: &'static str = "mcp-server-enablement.json";

    pub fn new() -> Result<Self, std::io::Error> {
        Self::with_config_path(None)
    }

    /// Create with custom config path
    pub fn with_config_path(config_dir: Option<PathBuf>) -> Result<Self, std::io::Error> {
        let config_dir = config_dir.unwrap_or_else(Self::default_config_dir);
        let config_file_path = config_dir.join(Self::FILENAME);

        // Ensure directory exists
        if let Some(parent) = config_file_path.parent() {
            fs::create_dir_all(parent)?;
        }

        Ok(Self {
            config_file_path,
            session_disabled: HashSet::new(),
            admin_enabled: true,
            admin_allowlist: None,
            admin_excludelist: None,
        })
    }

    /// Get the default config directory
    fn default_config_dir() -> PathBuf {
        let mut path = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
        path.push(".rustycode");
        path
    }

    /// Set admin-level configuration
    pub fn set_admin_config(
        &mut self,
        enabled: bool,
        allowlist: Option<Vec<String>>,
        excludelist: Option<Vec<String>>,
    ) {
        self.admin_enabled = enabled;
        self.admin_allowlist = allowlist.map(|list| list.into_iter().collect());
        self.admin_excludelist = excludelist.map(|list| list.into_iter().collect());
    }

    /// Load config from file
    fn load_config(&self) -> ServerEnablementConfig {
        if !self.config_file_path.exists() {
            return ServerEnablementConfig::default();
        }

        let content = fs::read_to_string(&self.config_file_path).unwrap_or_else(|e| {
            warn!("Failed to read enablement config: {}", e);
            String::new()
        });

        serde_json::from_str(&content).unwrap_or_else(|e| {
            warn!("Failed to parse enablement config: {}", e);
            ServerEnablementConfig::default()
        })
    }

    /// Save config to file
    fn save_config(&self, config: &ServerEnablementConfig) -> Result<(), std::io::Error> {
        let content = serde_json::to_string_pretty(config)?;
        let tmp_path = self.config_file_path.with_extension("json.tmp");
        fs::write(&tmp_path, &content)?;
        fs::rename(&tmp_path, &self.config_file_path)?;
        Ok(())
    }

    /// Check if server is enabled in file config (default: true)
    pub fn is_file_enabled(&self, server_id: &str) -> bool {
        let config = self.load_config();
        config.servers.get(server_id).is_none_or(|s| s.enabled)
    }

    /// Check if server is disabled for this session
    pub fn is_session_disabled(&self, server_id: &str) -> bool {
        let normalized_id = server_id.to_lowercase().trim().to_string();
        self.session_disabled.contains(&normalized_id)
    }

    /// Check if server can be loaded (all levels considered)
    pub fn can_load_server(&self, server_id: &str) -> ServerLoadResult {
        let normalized_id = server_id.to_lowercase().trim().to_string();

        // 1. Admin kill switch
        if !self.admin_enabled {
            return ServerLoadResult {
                allowed: false,
                reason: Some(
                    "MCP servers are disabled by administrator. Check admin settings or contact your admin."
                        .to_string(),
                ),
                block_type: Some(BlockType::Admin),
            };
        }

        // 2. Admin allowlist check
        if let Some(ref allowlist) = self.admin_allowlist {
            if !allowlist.is_empty() && !allowlist.contains(&normalized_id) {
                return ServerLoadResult {
                    allowed: false,
                    reason: Some(format!(
                        "Server '{server_id}' is not in the admin allowlist. Add it to the allowed list to enable."
                    )),
                    block_type: Some(BlockType::Allowlist),
                };
            }
        }

        // 3. Admin excludelist check
        if let Some(ref excludelist) = self.admin_excludelist {
            if excludelist.contains(&normalized_id) {
                return ServerLoadResult {
                    allowed: false,
                    reason: Some(format!(
                        "Server '{server_id}' is blocked by the admin excludelist."
                    )),
                    block_type: Some(BlockType::Excludelist),
                };
            }
        }

        // 4. Session disable check
        if self.is_session_disabled(&normalized_id) {
            return ServerLoadResult {
                allowed: false,
                reason: Some(format!(
                    "Server '{server_id}' is disabled for this session."
                )),
                block_type: Some(BlockType::Session),
            };
        }

        // 5. File-based enablement check
        if !self.is_file_enabled(&normalized_id) {
            return ServerLoadResult {
                allowed: false,
                reason: Some(format!(
                    "Server '{server_id}' is disabled. Enable it to use."
                )),
                block_type: Some(BlockType::Enablement),
            };
        }

        ServerLoadResult {
            allowed: true,
            reason: None,
            block_type: None,
        }
    }

    /// Get display state for a server
    pub async fn display_state(&self, server_id: &str) -> ServerDisplayState {
        let normalized_id = server_id.to_lowercase().trim().to_string();

        let is_session_disabled = self.is_session_disabled(&normalized_id);
        let is_persistent_disabled = !self.is_file_enabled(&normalized_id);

        let is_admin_blocked = if let Some(ref allowlist) = self.admin_allowlist {
            !allowlist.is_empty() && !allowlist.contains(&normalized_id)
        } else {
            false
        };

        let is_excludelist_blocked = self
            .admin_excludelist
            .as_ref()
            .is_some_and(|list| list.contains(&normalized_id));

        let enabled = !is_session_disabled
            && !is_persistent_disabled
            && !is_admin_blocked
            && !is_excludelist_blocked
            && self.admin_enabled;

        ServerDisplayState {
            enabled,
            is_session_disabled,
            is_persistent_disabled,
            is_admin_blocked,
            is_excludelist_blocked,
        }
    }

    /// Enable a server persistently
    pub fn enable_server(&self, server_id: &str) -> Result<(), std::io::Error> {
        let normalized_id = server_id.to_lowercase().trim().to_string();
        let mut config = self.load_config();

        // Remove from disabled list (default is enabled)
        if config.servers.remove(&normalized_id).is_some() {
            self.save_config(&config)?;
            info!("Enabled MCP server '{}'", server_id);
        }

        Ok(())
    }

    /// Disable a server persistently
    pub fn disable_server(&self, server_id: &str) -> Result<(), std::io::Error> {
        let normalized_id = server_id.to_lowercase().trim().to_string();
        let mut config = self.load_config();

        config
            .servers
            .insert(normalized_id, ServerEnablementState { enabled: false });
        self.save_config(&config)?;
        info!("Disabled MCP server '{}'", server_id);

        Ok(())
    }

    /// Disable a server for this session only
    pub fn disable_for_session(&mut self, server_id: &str) {
        let normalized_id = server_id.to_lowercase().trim().to_string();
        self.session_disabled.insert(normalized_id);
        info!("Disabled MCP server '{}' for this session", server_id);
    }

    /// Clear session disable for a server
    pub fn clear_session_disable(&mut self, server_id: &str) {
        let normalized_id = server_id.to_lowercase().trim().to_string();
        self.session_disabled.remove(&normalized_id);
    }

    /// Get all display states
    pub async fn all_display_states(
        &self,
        server_ids: &[String],
    ) -> HashMap<String, ServerDisplayState> {
        let mut result = HashMap::new();
        for server_id in server_ids {
            result.insert(server_id.clone(), self.display_state(server_id).await);
        }
        result
    }

    /// Get the config file path
    pub const fn config_file_path(&self) -> &PathBuf {
        &self.config_file_path
    }

    /// Check if admin is enabled
    pub const fn is_admin_enabled(&self) -> bool {
        self.admin_enabled
    }

    /// Get admin allowlist
    pub const fn admin_allowlist(&self) -> Option<&HashSet<String>> {
        self.admin_allowlist.as_ref()
    }

    /// Get admin excludelist
    pub const fn admin_excludelist(&self) -> Option<&HashSet<String>> {
        self.admin_excludelist.as_ref()
    }
}

impl Default for ServerEnablementManager {
    fn default() -> Self {
        Self::new().unwrap_or_else(|e| {
            warn!(
                "Failed to create ServerEnablementManager, using fallback: {}",
                e
            );
            Self {
                config_file_path: PathBuf::from(".rustycode/mcp-enablement.json"),
                session_disabled: HashSet::new(),
                admin_enabled: true,
                admin_allowlist: None,
                admin_excludelist: None,
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_manager() -> ServerEnablementManager {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let id = COUNTER.fetch_add(1, Ordering::Relaxed);
        let config_dir = std::env::temp_dir().join(format!(
            "rustycode-test-enablement-{}-{}",
            std::process::id(),
            id
        ));
        fs::create_dir_all(&config_dir).ok();
        ServerEnablementManager::with_config_path(Some(config_dir)).unwrap()
    }

    #[test]
    fn test_server_enablement_default() {
        let manager = create_test_manager();
        // Default: all servers enabled
        assert!(manager.is_file_enabled("test-server"));
        assert!(!manager.is_session_disabled("test-server"));
    }

    #[test]
    fn test_session_disable() {
        let mut manager = create_test_manager();
        manager.disable_for_session("test-server");

        assert!(manager.is_session_disabled("test-server"));
        assert!(!manager.is_session_disabled("other-server"));

        manager.clear_session_disable("test-server");
        assert!(!manager.is_session_disabled("test-server"));
    }

    #[test]
    fn test_persistent_disable() {
        let manager = create_test_manager();

        assert!(manager.is_file_enabled("test-server"));

        manager.disable_server("test-server").unwrap();
        assert!(!manager.is_file_enabled("test-server"));

        manager.enable_server("test-server").unwrap();
        assert!(manager.is_file_enabled("test-server"));
    }

    #[test]
    fn test_can_load_server_admin_disabled() {
        let mut manager = create_test_manager();
        manager.set_admin_config(false, None, None);

        let result = manager.can_load_server("test-server");
        assert!(!result.allowed);
        assert_eq!(result.block_type, Some(BlockType::Admin));
    }

    #[test]
    fn test_can_load_server_allowlist() {
        let mut manager = create_test_manager();
        manager.set_admin_config(true, Some(vec!["allowed-server".to_string()]), None);

        let result1 = manager.can_load_server("allowed-server");
        assert!(result1.allowed);

        let result2 = manager.can_load_server("other-server");
        assert!(!result2.allowed);
        assert_eq!(result2.block_type, Some(BlockType::Allowlist));
    }

    #[test]
    fn test_can_load_server_excludelist() {
        let mut manager = create_test_manager();
        manager.set_admin_config(true, None, Some(vec!["blocked-server".to_string()]));

        let result1 = manager.can_load_server("allowed-server");
        assert!(result1.allowed);

        let result2 = manager.can_load_server("blocked-server");
        assert!(!result2.allowed);
        assert_eq!(result2.block_type, Some(BlockType::Excludelist));
    }

    #[tokio::test]
    async fn test_display_state() {
        let manager = create_test_manager();

        let state = manager.display_state("test-server").await;
        assert!(state.enabled);
        assert!(!state.is_session_disabled);
        assert!(!state.is_persistent_disabled);
        assert!(!state.is_admin_blocked);
        assert!(!state.is_excludelist_blocked);
    }

    #[test]
    fn test_block_type_variants() {
        let admin = BlockType::Admin;
        let allowlist = BlockType::Allowlist;
        let excludelist = BlockType::Excludelist;
        let session = BlockType::Session;
        let enablement = BlockType::Enablement;

        // Ensure they are all distinct
        assert_ne!(admin, allowlist);
        assert_ne!(allowlist, excludelist);
        assert_ne!(excludelist, session);
        assert_ne!(session, enablement);
    }

    #[test]
    fn test_server_enablement_config_serialization() {
        let config = ServerEnablementConfig::default();
        let json = serde_json::to_string(&config).unwrap();
        let parsed: ServerEnablementConfig = serde_json::from_str(&json).unwrap();
        assert!(parsed.servers.is_empty());

        let mut config = ServerEnablementConfig::default();
        config
            .servers
            .insert("srv".to_string(), ServerEnablementState { enabled: false });
        let json = serde_json::to_string(&config).unwrap();
        assert!(json.contains("srv"));
        assert!(json.contains("enabled"));
    }

    #[test]
    fn test_server_enablement_state_serialization() {
        let state = ServerEnablementState { enabled: true };
        let json = serde_json::to_string(&state).unwrap();
        let parsed: ServerEnablementState = serde_json::from_str(&json).unwrap();
        assert!(parsed.enabled);

        let state = ServerEnablementState { enabled: false };
        let json = serde_json::to_string(&state).unwrap();
        let parsed: ServerEnablementState = serde_json::from_str(&json).unwrap();
        assert!(!parsed.enabled);
    }

    #[test]
    fn test_can_load_server_defaults_allowed() {
        let manager = create_test_manager();
        let result = manager.can_load_server("any-server");
        assert!(result.allowed);
        assert!(result.reason.is_none());
        assert!(result.block_type.is_none());
    }

    #[test]
    fn test_can_load_server_session_disabled() {
        let mut manager = create_test_manager();
        manager.disable_for_session("my-server");

        let result = manager.can_load_server("my-server");
        assert!(!result.allowed);
        assert_eq!(result.block_type, Some(BlockType::Session));
        assert!(result.reason.unwrap().contains("session"));
    }

    #[test]
    fn test_can_load_server_enablement_disabled() {
        let manager = create_test_manager();
        manager.disable_server("my-server").unwrap();

        let result = manager.can_load_server("my-server");
        assert!(!result.allowed);
        assert_eq!(result.block_type, Some(BlockType::Enablement));
    }

    #[test]
    fn test_can_load_server_case_insensitive() {
        let mut manager = create_test_manager();
        manager.disable_for_session("My-Server");

        assert!(manager.is_session_disabled("my-server"));
        assert!(manager.is_session_disabled("MY-SERVER"));
        assert!(manager.is_session_disabled("My-Server"));
    }

    #[test]
    fn test_can_load_server_whitespace_trim() {
        let mut manager = create_test_manager();
        manager.disable_for_session("  my-server  ");

        assert!(manager.is_session_disabled("my-server"));
    }

    #[test]
    fn test_enable_server_idempotent() {
        let manager = create_test_manager();
        // Enable a server that was never disabled
        manager.enable_server("srv").unwrap();
        assert!(manager.is_file_enabled("srv"));
        // Enable again should be fine
        manager.enable_server("srv").unwrap();
        assert!(manager.is_file_enabled("srv"));
    }

    #[test]
    fn test_admin_accessors() {
        let mut manager = create_test_manager();
        assert!(manager.is_admin_enabled());
        assert!(manager.admin_allowlist().is_none());
        assert!(manager.admin_excludelist().is_none());

        manager.set_admin_config(
            true,
            Some(vec!["a".to_string()]),
            Some(vec!["b".to_string()]),
        );
        assert!(manager.is_admin_enabled());
        assert!(manager.admin_allowlist().is_some());
        assert!(manager.admin_excludelist().is_some());
    }

    #[tokio::test]
    async fn test_get_all_display_states() {
        let manager = create_test_manager();
        let server_ids = vec!["srv1".to_string(), "srv2".to_string()];
        let states = manager.all_display_states(&server_ids).await;
        assert_eq!(states.len(), 2);
        assert!(states.get("srv1").unwrap().enabled);
        assert!(states.get("srv2").unwrap().enabled);
    }

    #[test]
    fn test_config_file_path_accessible() {
        let manager = create_test_manager();
        assert!(manager
            .config_file_path()
            .to_string_lossy()
            .contains("mcp-server-enablement.json"));
    }
}
