//! Error types for the plugin system

use thiserror::Error;

/// Errors that can occur in the plugin system
#[derive(Error, Debug, Clone)]
#[non_exhaustive]
pub enum PluginError {
    /// Plugin with this name already exists
    #[error("plugin '{name}' is already registered")]
    AlreadyRegistered { name: String },

    /// Plugin not found in registry
    #[error("plugin '{name}' not found")]
    NotFound { name: String },

    /// Plugin failed to initialize
    #[error("plugin '{name}' failed to initialize: {reason}")]
    InitializationFailed { name: String, reason: String },

    /// Plugin failed to shutdown
    #[error("plugin '{name}' failed to shutdown: {reason}")]
    ShutdownFailed { name: String, reason: String },

    /// Plugin is in invalid state for the requested operation
    #[error("plugin '{name}' is in invalid state: {reason}")]
    InvalidState { name: String, reason: String },

    /// Plugin loading failed
    #[error("failed to load plugin: {reason}")]
    LoadingFailed { reason: String },

    /// Plugin version mismatch or incompatibility
    #[error("plugin '{name}' version mismatch: {reason}")]
    VersionMismatch { name: String, reason: String },

    /// Missing dependency
    #[error("plugin '{name}' has missing dependency: {dependency}")]
    MissingDependency { name: String, dependency: String },

    /// Configuration error
    #[error("plugin configuration error: {reason}")]
    ConfigurationError { reason: String },

    /// Generic plugin error
    #[error("plugin error: {reason}")]
    Other { reason: String },
}

impl PluginError {
    /// Create a new "already registered" error
    pub fn already_registered(name: impl Into<String>) -> Self {
        Self::AlreadyRegistered { name: name.into() }
    }

    /// Create a new "not found" error
    pub fn not_found(name: impl Into<String>) -> Self {
        Self::NotFound { name: name.into() }
    }

    /// Create a new "initialization failed" error
    pub fn initialization_failed(name: impl Into<String>, reason: impl Into<String>) -> Self {
        Self::InitializationFailed {
            name: name.into(),
            reason: reason.into(),
        }
    }

    /// Create a new "shutdown failed" error
    pub fn shutdown_failed(name: impl Into<String>, reason: impl Into<String>) -> Self {
        Self::ShutdownFailed {
            name: name.into(),
            reason: reason.into(),
        }
    }

    /// Create a new "invalid state" error
    pub fn invalid_state(name: impl Into<String>, reason: impl Into<String>) -> Self {
        Self::InvalidState {
            name: name.into(),
            reason: reason.into(),
        }
    }

    /// Create a new "loading failed" error
    pub fn loading_failed(reason: impl Into<String>) -> Self {
        Self::LoadingFailed {
            reason: reason.into(),
        }
    }

    /// Create a new "version mismatch" error
    pub fn version_mismatch(name: impl Into<String>, reason: impl Into<String>) -> Self {
        Self::VersionMismatch {
            name: name.into(),
            reason: reason.into(),
        }
    }

    /// Create a new "missing dependency" error
    pub fn missing_dependency(name: impl Into<String>, dependency: impl Into<String>) -> Self {
        Self::MissingDependency {
            name: name.into(),
            dependency: dependency.into(),
        }
    }

    /// Create a new "configuration error"
    pub fn configuration_error(reason: impl Into<String>) -> Self {
        Self::ConfigurationError {
            reason: reason.into(),
        }
    }

    /// Create a generic plugin error
    pub fn other(reason: impl Into<String>) -> Self {
        Self::Other {
            reason: reason.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_already_registered_error() {
        let err = PluginError::already_registered("test_plugin");
        assert!(err.to_string().contains("test_plugin"));
        assert!(err.to_string().contains("already registered"));
    }

    #[test]
    fn test_not_found_error() {
        let err = PluginError::not_found("missing_plugin");
        assert!(err.to_string().contains("missing_plugin"));
        assert!(err.to_string().contains("not found"));
    }

    #[test]
    fn test_initialization_failed_error() {
        let err = PluginError::initialization_failed("bad_plugin", "connection timeout");
        assert!(err.to_string().contains("bad_plugin"));
        assert!(err.to_string().contains("connection timeout"));
    }

    #[test]
    fn test_invalid_state_error() {
        let err = PluginError::invalid_state("plugin", "already disabled");
        assert!(err.to_string().contains("plugin"));
        assert!(err.to_string().contains("already disabled"));
    }

    #[test]
    fn test_clone_error() {
        let err = PluginError::not_found("test");
        let cloned = err.clone();
        assert_eq!(err.to_string(), cloned.to_string());
    }

    #[test]
    fn test_shutdown_failed_error() {
        let err = PluginError::shutdown_failed("srv", "timeout");
        let msg = err.to_string();
        assert!(msg.contains("srv"));
        assert!(msg.contains("timeout"));
        assert!(msg.contains("shutdown"));
    }

    #[test]
    fn test_loading_failed_error() {
        let err = PluginError::loading_failed("bad format");
        assert!(err.to_string().contains("bad format"));
        assert!(err.to_string().contains("load"));
    }

    #[test]
    fn test_version_mismatch_error() {
        let err = PluginError::version_mismatch("myplug", "requires v2");
        let msg = err.to_string();
        assert!(msg.contains("myplug"));
        assert!(msg.contains("requires v2"));
        assert!(msg.contains("version"));
    }

    #[test]
    fn test_missing_dependency_error() {
        let err = PluginError::missing_dependency("plug", "dep-core");
        let msg = err.to_string();
        assert!(msg.contains("plug"));
        assert!(msg.contains("dep-core"));
        assert!(msg.contains("dependency"));
    }

    #[test]
    fn test_configuration_error() {
        let err = PluginError::configuration_error("invalid port");
        let msg = err.to_string();
        assert!(msg.contains("invalid port"));
        assert!(msg.contains("configuration"));
    }

    #[test]
    fn test_other_error() {
        let err = PluginError::other("something went wrong");
        let msg = err.to_string();
        assert!(msg.contains("something went wrong"));
    }

    #[test]
    fn test_debug_format() {
        let err = PluginError::not_found("debug-plug");
        let debug = format!("{:?}", err);
        assert!(debug.contains("NotFound"));
        assert!(debug.contains("debug-plug"));
    }

    #[test]
    fn test_all_variants_clone() {
        let errors = vec![
            PluginError::already_registered("a"),
            PluginError::not_found("b"),
            PluginError::initialization_failed("c", "fail"),
            PluginError::shutdown_failed("d", "timeout"),
            PluginError::invalid_state("e", "bad"),
            PluginError::loading_failed("corrupt"),
            PluginError::version_mismatch("f", "v1 vs v2"),
            PluginError::missing_dependency("g", "dep"),
            PluginError::configuration_error("cfg"),
            PluginError::other("misc"),
        ];
        for err in &errors {
            let cloned = err.clone();
            assert_eq!(err.to_string(), cloned.to_string());
        }
    }
}
