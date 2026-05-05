//! Plugin permission system

use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::str::FromStr;

/// Permission required by a plugin
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Permission {
    /// Read files from the filesystem
    ReadFile,

    /// Write files to the filesystem
    WriteFile,

    /// Execute shell commands
    ExecuteCommand,

    /// Make network requests
    NetworkRequest,

    /// Show desktop notifications
    Notification,

    /// Access clipboard
    Clipboard,

    /// Access workspace context
    WorkspaceContext,

    /// Modify UI state
    UIControl,

    /// Access conversation history
    ConversationHistory,

    /// Unknown/custom permission
    Unknown(String),
}

impl std::fmt::Display for Permission {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Permission::ReadFile => write!(f, "read files"),
            Permission::WriteFile => write!(f, "write files"),
            Permission::ExecuteCommand => write!(f, "execute commands"),
            Permission::NetworkRequest => write!(f, "make network requests"),
            Permission::Notification => write!(f, "show notifications"),
            Permission::Clipboard => write!(f, "access clipboard"),
            Permission::WorkspaceContext => write!(f, "access workspace context"),
            Permission::UIControl => write!(f, "control UI"),
            Permission::ConversationHistory => write!(f, "access conversation history"),
            Permission::Unknown(s) => write!(f, "{}", s),
        }
    }
}

impl FromStr for Permission {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "read_file" => Ok(Permission::ReadFile),
            "write_file" => Ok(Permission::WriteFile),
            "execute_command" => Ok(Permission::ExecuteCommand),
            "network_request" => Ok(Permission::NetworkRequest),
            "notification" => Ok(Permission::Notification),
            "clipboard" => Ok(Permission::Clipboard),
            "workspace_context" => Ok(Permission::WorkspaceContext),
            "ui_control" => Ok(Permission::UIControl),
            "conversation_history" => Ok(Permission::ConversationHistory),
            other => Ok(Permission::Unknown(other.to_string())),
        }
    }
}

/// Permission set for a plugin
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginPermissions {
    /// Set of required permissions
    pub permissions: HashSet<Permission>,

    /// Whether user has granted these permissions
    pub granted: bool,
}

impl PluginPermissions {
    /// Create new permission set from string list
    pub fn from_strings(perm_strings: Vec<String>) -> Self {
        let permissions = perm_strings
            .iter()
            .filter_map(|s| Permission::from_str(s).ok())
            .collect();

        Self {
            permissions,
            granted: false,
        }
    }

    pub fn has(&self, perm: &Permission) -> bool {
        self.granted && self.permissions.contains(perm)
    }

    /// Grant all permissions
    pub fn grant(&mut self) {
        self.granted = true;
    }

    /// Revoke all permissions
    pub fn revoke(&mut self) {
        self.granted = false;
    }

    /// Get human-readable permission list
    pub fn describe(&self) -> Vec<String> {
        self.permissions.iter().map(|p| p.to_string()).collect()
    }
}

/// Permission check result
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum PermissionCheck {
    /// Permission granted
    Granted,

    /// Permission denied
    Denied,

    /// Permission requires user confirmation
    RequiresConfirmation,
}

/// Permission manager for plugins
pub struct PermissionManager {
    /// Granted permissions per plugin
    granted: std::collections::HashMap<String, HashSet<Permission>>,

    /// Denied permissions per plugin
    denied: std::collections::HashMap<String, HashSet<Permission>>,
}

impl PermissionManager {
    /// Create new permission manager
    pub fn new() -> Self {
        Self {
            granted: std::collections::HashMap::new(),
            denied: std::collections::HashMap::new(),
        }
    }

    pub fn check(&self, plugin_name: &str, perm: &Permission) -> PermissionCheck {
        if let Some(denied) = self.denied.get(plugin_name) {
            if denied.contains(perm) {
                return PermissionCheck::Denied;
            }
        }

        if let Some(granted) = self.granted.get(plugin_name) {
            if granted.contains(perm) {
                return PermissionCheck::Granted;
            }
        }

        PermissionCheck::RequiresConfirmation
    }

    /// Grant permission to a plugin
    pub fn grant(&mut self, plugin_name: &str, perm: Permission) {
        self.granted
            .entry(plugin_name.to_string())
            .or_default()
            .insert(perm.clone());

        // Remove from denied if present
        if let Some(denied) = self.denied.get_mut(plugin_name) {
            denied.remove(&perm);
        }
    }

    /// Deny permission to a plugin
    pub fn deny(&mut self, plugin_name: &str, perm: Permission) {
        self.denied
            .entry(plugin_name.to_string())
            .or_default()
            .insert(perm.clone());

        // Remove from granted if present
        if let Some(granted) = self.granted.get_mut(plugin_name) {
            granted.remove(&perm);
        }
    }

    /// Grant all permissions to a plugin
    pub fn grant_all(&mut self, plugin_name: &str, permissions: &[Permission]) {
        for perm in permissions {
            self.grant(plugin_name, perm.clone());
        }
    }

    /// Clear all permissions for a plugin (e.g., on unload)
    pub fn clear_plugin(&mut self, plugin_name: &str) {
        self.granted.remove(plugin_name);
        self.denied.remove(plugin_name);
    }
}

impl Default for PermissionManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_permission_from_str() {
        assert_eq!(
            Permission::from_str("read_file").unwrap(),
            Permission::ReadFile
        );
        assert_eq!(
            Permission::from_str("notification").unwrap(),
            Permission::Notification
        );
        assert_eq!(
            Permission::from_str("custom_perm").unwrap(),
            Permission::Unknown("custom_perm".to_string())
        );
    }

    #[test]
    fn test_plugin_permissions_from_strings() {
        let perms = PluginPermissions::from_strings(vec![
            "read_file".to_string(),
            "notification".to_string(),
        ]);

        assert_eq!(perms.permissions.len(), 2);
        assert!(!perms.granted);
    }

    #[test]
    fn test_plugin_permissions_grant() {
        let mut perms = PluginPermissions::from_strings(vec!["read_file".to_string()]);
        assert!(!perms.has(&Permission::ReadFile));

        perms.grant();
        assert!(perms.has(&Permission::ReadFile));
    }

    #[test]
    fn test_permission_manager() {
        let mut mgr = PermissionManager::new();

        // First check requires confirmation
        assert_eq!(
            mgr.check("test", &Permission::ReadFile),
            PermissionCheck::RequiresConfirmation
        );

        // Grant permission
        mgr.grant("test", Permission::ReadFile);
        assert_eq!(
            mgr.check("test", &Permission::ReadFile),
            PermissionCheck::Granted
        );

        // Deny permission
        mgr.deny("test", Permission::ExecuteCommand);
        assert_eq!(
            mgr.check("test", &Permission::ExecuteCommand),
            PermissionCheck::Denied
        );
    }

    #[test]
    fn test_permission_describe() {
        let perms = PluginPermissions::from_strings(vec![
            "read_file".to_string(),
            "notification".to_string(),
        ]);

        let desc = perms.describe();
        assert_eq!(desc.len(), 2);
        assert!(desc.contains(&"read files".to_string()));
        assert!(desc.contains(&"show notifications".to_string()));
    }
}
