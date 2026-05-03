//! Plugin status tracking

use serde::{Deserialize, Serialize};
use std::fmt;

/// Status of a plugin in the plugin registry
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum PluginStatus {
    /// Plugin is being loaded
    Loading,

    /// Plugin has been loaded but not yet initialized
    Loaded,

    /// Plugin is active and ready for use
    Active,

    /// Plugin is disabled but can be re-enabled
    Disabled,

    /// Plugin failed with an error message
    Failed(String),
}

impl fmt::Display for PluginStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Loading => write!(f, "Loading"),
            Self::Loaded => write!(f, "Loaded"),
            Self::Active => write!(f, "Active"),
            Self::Disabled => write!(f, "Disabled"),
            Self::Failed(reason) => write!(f, "Failed({reason})"),
        }
    }
}

impl PluginStatus {
    /// Check if the plugin is in an active/usable state
    pub const fn is_active(&self) -> bool {
        matches!(self, Self::Active)
    }

    /// Check if the plugin has failed
    pub const fn is_failed(&self) -> bool {
        matches!(self, Self::Failed(_))
    }

    /// Check if the plugin is loading or loaded
    pub const fn is_loaded(&self) -> bool {
        matches!(self, Self::Loaded | Self::Active | Self::Loading)
    }

    /// Get the failure reason if the plugin failed
    pub fn failure_reason(&self) -> Option<&str> {
        match self {
            Self::Failed(reason) => Some(reason),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_status_active() {
        let status = PluginStatus::Active;
        assert!(status.is_active());
        assert!(!status.is_failed());
        assert!(status.is_loaded());
    }

    #[test]
    fn test_status_failed() {
        let status = PluginStatus::Failed("initialization error".to_string());
        assert!(!status.is_active());
        assert!(status.is_failed());
        assert_eq!(status.failure_reason(), Some("initialization error"));
    }

    #[test]
    fn test_status_disabled() {
        let status = PluginStatus::Disabled;
        assert!(!status.is_active());
        assert!(!status.is_failed());
        assert!(!status.is_loaded());
    }

    #[test]
    fn test_status_display() {
        assert_eq!(PluginStatus::Active.to_string(), "Active");
        assert_eq!(PluginStatus::Loading.to_string(), "Loading");
        assert_eq!(PluginStatus::Disabled.to_string(), "Disabled");
        assert!(PluginStatus::Failed("error".to_string())
            .to_string()
            .contains("Failed"));
    }

    #[test]
    fn test_status_serde() {
        let status = PluginStatus::Active;
        let json = serde_json::to_string(&status).unwrap();
        let deserialized: PluginStatus = serde_json::from_str(&json).unwrap();
        assert_eq!(status, deserialized);
    }

    #[test]
    fn test_loading_state() {
        let status = PluginStatus::Loading;
        assert!(!status.is_active());
        assert!(!status.is_failed());
        assert!(status.is_loaded());
        assert!(status.failure_reason().is_none());
    }

    #[test]
    fn test_loaded_state() {
        let status = PluginStatus::Loaded;
        assert!(!status.is_active());
        assert!(!status.is_failed());
        assert!(status.is_loaded());
        assert!(status.failure_reason().is_none());
    }

    #[test]
    fn test_failed_display_shows_reason() {
        let status = PluginStatus::Failed("timeout".to_string());
        let msg = status.to_string();
        assert_eq!(msg, "Failed(timeout)");
    }

    #[test]
    fn test_failed_empty_reason() {
        let status = PluginStatus::Failed(String::new());
        assert!(status.is_failed());
        assert_eq!(status.failure_reason(), Some(""));
    }

    #[test]
    fn test_serde_roundtrip_all_variants() {
        let variants = vec![
            PluginStatus::Loading,
            PluginStatus::Loaded,
            PluginStatus::Active,
            PluginStatus::Disabled,
            PluginStatus::Failed("oops".to_string()),
        ];
        for v in &variants {
            let json = serde_json::to_string(v).unwrap();
            let back: PluginStatus = serde_json::from_str(&json).unwrap();
            assert_eq!(*v, back);
        }
    }

    #[test]
    fn test_clone_preserves_all() {
        let orig = PluginStatus::Failed("err".to_string());
        let cloned = orig.clone();
        assert_eq!(orig, cloned);
        assert_eq!(orig.failure_reason(), cloned.failure_reason());
    }

    #[test]
    fn test_debug_format() {
        let status = PluginStatus::Active;
        let debug = format!("{:?}", status);
        assert!(debug.contains("Active"));
    }

    #[test]
    fn test_display_all_variants() {
        assert_eq!(PluginStatus::Loading.to_string(), "Loading");
        assert_eq!(PluginStatus::Loaded.to_string(), "Loaded");
        assert_eq!(PluginStatus::Active.to_string(), "Active");
        assert_eq!(PluginStatus::Disabled.to_string(), "Disabled");
        assert_eq!(PluginStatus::Failed("x".into()).to_string(), "Failed(x)");
    }
}
