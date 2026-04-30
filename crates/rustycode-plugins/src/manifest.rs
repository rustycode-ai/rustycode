//! Plugin manifest format, parsing, and validation
//!
//! Supports both TOML and JSON formats for plugin manifests.
//! Manifests declare metadata, dependencies, permissions, and configuration schema.

use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use std::collections::HashMap;
use std::str::FromStr;

use crate::error::PluginError;

/// Plugin manifest containing metadata, dependencies, and configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginManifest {
    /// Plugin name (must be unique)
    pub name: String,

    /// Plugin version in semver format (e.g., "1.0.0")
    pub version: String,

    /// Human-readable description
    pub description: Option<String>,

    /// Plugin authors
    pub authors: Option<Vec<String>>,

    /// Plugin dependencies (name -> version spec, e.g., ">=1.0.0")
    pub dependencies: Option<HashMap<String, String>>,

    /// Permissions this plugin requires
    pub permissions: Option<Vec<String>>,

    /// JSON Schema for plugin configuration
    pub config_schema: Option<JsonValue>,

    /// Entry point (binary path or module name)
    pub entry_point: Option<String>,
}

impl PluginManifest {
    /// Parse a manifest from TOML string
    pub fn from_toml(content: &str) -> Result<Self, PluginError> {
        #[cfg(feature = "toml")]
        {
            use toml;
            toml::from_str(content)
                .map_err(|e| PluginError::configuration_error(format!("TOML parse error: {e}")))
        }
        #[cfg(not(feature = "toml"))]
        {
            let _ = content;
            Err(PluginError::configuration_error(
                "TOML support requires 'toml' feature".to_string(),
            ))
        }
    }

    /// Parse a manifest from JSON string
    pub fn from_json(content: &str) -> Result<Self, PluginError> {
        serde_json::from_str(content)
            .map_err(|e| PluginError::configuration_error(format!("JSON parse error: {e}")))
    }

    /// Parse a manifest from a string, auto-detecting format
    pub fn parse_from_str(content: &str) -> Result<Self, PluginError> {
        // Try JSON first
        if let Ok(manifest) = Self::from_json(content) {
            return Ok(manifest);
        }

        // Then try TOML
        #[cfg(feature = "toml")]
        {
            Self::from_toml(content)
        }

        #[cfg(not(feature = "toml"))]
        {
            Err(PluginError::configuration_error(
                "Could not parse manifest as JSON or TOML".to_string(),
            ))
        }
    }

    /// Validate the manifest
    pub fn validate(&self) -> Result<(), PluginError> {
        // Validate name is not empty
        if self.name.is_empty() {
            return Err(PluginError::configuration_error(
                "Plugin name cannot be empty".to_string(),
            ));
        }

        // Validate name format (alphanumeric + underscore + hyphen)
        if !self
            .name
            .chars()
            .all(|c| c.is_alphanumeric() || c == '_' || c == '-')
        {
            return Err(PluginError::configuration_error(
                "Plugin name must contain only alphanumeric characters, underscores, or hyphens"
                    .to_string(),
            ));
        }

        // Validate version is valid semver
        if !Self::is_valid_semver(&self.version) {
            return Err(PluginError::configuration_error(format!(
                "Invalid version format: {}",
                self.version
            )));
        }

        // Validate permissions if present
        if let Some(permissions) = &self.permissions {
            for perm in permissions {
                if !Self::is_valid_permission(perm) {
                    return Err(PluginError::configuration_error(format!(
                        "Invalid permission: {perm}"
                    )));
                }
            }
        }

        Ok(())
    }

    /// Check if a string is a valid semantic version
    fn is_valid_semver(version: &str) -> bool {
        let parts: Vec<&str> = version.split('.').collect();
        if parts.len() != 3 {
            return false;
        }

        parts.iter().all(|part| part.parse::<u32>().is_ok())
    }

    /// Check if a permission string is valid
    fn is_valid_permission(permission: &str) -> bool {
        matches!(
            permission,
            "file_system"
                | "network"
                | "subprocess"
                | "environment"
                | "system_clock"
                | "random"
                | "process_info"
        )
    }

    /// Get dependencies as a list
    pub fn get_dependencies(&self) -> Vec<&str> {
        self.dependencies
            .as_ref()
            .map(|deps| deps.keys().map(String::as_str).collect())
            .unwrap_or_default()
    }

    /// Check if this manifest depends on a plugin
    pub fn depends_on(&self, plugin_name: &str) -> bool {
        self.dependencies
            .as_ref()
            .is_some_and(|deps| deps.contains_key(plugin_name))
    }

    /// Get the version spec for a dependency
    pub fn get_dependency_version(&self, plugin_name: &str) -> Option<&str> {
        self.dependencies
            .as_ref()
            .and_then(|deps| deps.get(plugin_name).map(String::as_str))
    }

    /// Check if a permission is required
    pub fn requires_permission(&self, permission: &str) -> bool {
        self.permissions
            .as_ref()
            .is_some_and(|perms| perms.contains(&permission.to_string()))
    }
}

/// Dependency version specification (e.g., "1.0.0", ">=1.0.0", "1.0.x", "^1.0.0")
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum DependencySpec {
    /// Exact version (e.g., "1.0.0")
    Exact(String),

    /// Caret version (e.g., "^1.0.0" - compatible with 1.0.0)
    Caret(String),

    /// Greater than or equal (e.g., ">=1.0.0")
    GreaterOrEqual(String),

    /// Less than (e.g., "<2.0.0")
    Less(String),

    /// Wildcard (e.g., "1.0.x")
    Wildcard(String),

    /// Any version
    Any,
}

impl DependencySpec {
    /// Parse a version specification string
    pub fn parse_from_str(spec: &str) -> Result<Self, PluginError> {
        let spec = spec.trim();

        if spec == "*" {
            return Ok(Self::Any);
        }

        if let Some(version) = spec.strip_prefix('^') {
            return Ok(Self::Caret(version.to_string()));
        }

        if let Some(version) = spec.strip_prefix(">=") {
            return Ok(Self::GreaterOrEqual(version.to_string()));
        }

        if let Some(version) = spec.strip_prefix('<') {
            return Ok(Self::Less(version.to_string()));
        }

        if spec.ends_with('x') {
            return Ok(Self::Wildcard(spec.to_string()));
        }

        // Default to exact version
        Ok(Self::Exact(spec.to_string()))
    }

    /// Check if a version satisfies this spec
    pub fn satisfies(&self, version: &str) -> bool {
        match self {
            Self::Any => true,
            Self::Exact(spec_ver) => version == spec_ver,
            Self::Caret(spec_ver) => {
                // ^1.0.0 matches >=1.0.0 and <2.0.0
                let spec_parts: Vec<&str> = spec_ver.split('.').collect();
                let ver_parts: Vec<&str> = version.split('.').collect();

                if spec_parts.len() != 3 || ver_parts.len() != 3 {
                    return false;
                }

                // Must match major.minor.patch for caret
                if let (Ok(spec_major), Ok(spec_minor), Ok(spec_patch)) = (
                    spec_parts[0].parse::<u32>(),
                    spec_parts[1].parse::<u32>(),
                    spec_parts[2].parse::<u32>(),
                ) {
                    if let (Ok(ver_major), Ok(ver_minor), Ok(ver_patch)) = (
                        ver_parts[0].parse::<u32>(),
                        ver_parts[1].parse::<u32>(),
                        ver_parts[2].parse::<u32>(),
                    ) {
                        // Major must match exactly
                        if ver_major != spec_major {
                            return false;
                        }
                        // If minor differs, version must have greater minor
                        if ver_minor != spec_minor {
                            return ver_minor > spec_minor;
                        }
                        // ver_minor == spec_minor, so check patch
                        return ver_patch >= spec_patch;
                    }
                }
                false
            }
            Self::GreaterOrEqual(spec_ver) => Self::compare_versions(version, spec_ver) >= 0,
            Self::Less(spec_ver) => Self::compare_versions(version, spec_ver) < 0,
            Self::Wildcard(spec_pattern) => {
                // 1.0.x matches 1.0.0, 1.0.1, 1.0.2, etc
                let spec_ver = spec_pattern.replace('x', "0");
                let spec_parts: Vec<&str> = spec_ver.split('.').collect();
                let ver_parts: Vec<&str> = version.split('.').collect();

                if spec_parts.len() != 3 || ver_parts.len() != 3 {
                    return false;
                }

                // Compare major and minor, patch can be anything
                spec_parts[0] == ver_parts[0] && spec_parts[1] == ver_parts[1]
            }
        }
    }

    /// Compare two version strings (simple numeric comparison)
    fn compare_versions(ver1: &str, ver2: &str) -> i32 {
        let parts1: Vec<u32> = ver1
            .split('.')
            .filter_map(|p| p.parse::<u32>().ok())
            .collect();
        let parts2: Vec<u32> = ver2
            .split('.')
            .filter_map(|p| p.parse::<u32>().ok())
            .collect();

        for i in 0..3 {
            let p1 = parts1.get(i).copied().unwrap_or(0);
            let p2 = parts2.get(i).copied().unwrap_or(0);
            if p1 < p2 {
                return -1;
            } else if p1 > p2 {
                return 1;
            }
        }
        0
    }
}

impl FromStr for DependencySpec {
    type Err = PluginError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse_from_str(s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_manifest_json_parsing() {
        let json = r#"{
            "name": "test_plugin",
            "version": "1.0.0",
            "description": "A test plugin"
        }"#;

        let manifest = PluginManifest::from_json(json).unwrap();
        assert_eq!(manifest.name, "test_plugin");
        assert_eq!(manifest.version, "1.0.0");
        assert_eq!(manifest.description, Some("A test plugin".to_string()));
    }

    #[test]
    fn test_manifest_json_with_dependencies() {
        let json = r#"{
            "name": "plugin_a",
            "version": "2.0.0",
            "description": "Plugin A",
            "dependencies": {
                "plugin_b": ">=1.0.0",
                "plugin_c": "^2.0.0"
            }
        }"#;

        let manifest = PluginManifest::from_json(json).unwrap();
        assert_eq!(manifest.name, "plugin_a");
        assert!(manifest.depends_on("plugin_b"));
        assert!(manifest.depends_on("plugin_c"));
        assert!(!manifest.depends_on("plugin_d"));
        assert_eq!(manifest.get_dependency_version("plugin_b"), Some(">=1.0.0"));
    }

    #[test]
    fn test_manifest_json_with_permissions() {
        let json = r#"{
            "name": "test_plugin",
            "version": "1.0.0",
            "permissions": ["file_system", "network"]
        }"#;

        let manifest = PluginManifest::from_json(json).unwrap();
        assert!(manifest.requires_permission("file_system"));
        assert!(manifest.requires_permission("network"));
        assert!(!manifest.requires_permission("subprocess"));
    }

    #[test]
    fn test_manifest_json_with_config_schema() {
        let json = r#"{
            "name": "test_plugin",
            "version": "1.0.0",
            "config_schema": {
                "type": "object",
                "properties": {
                    "api_key": { "type": "string" }
                }
            }
        }"#;

        let manifest = PluginManifest::from_json(json).unwrap();
        assert!(manifest.config_schema.is_some());
    }

    #[test]
    fn test_manifest_validate_valid() {
        let manifest = PluginManifest {
            name: "valid_plugin".to_string(),
            version: "1.0.0".to_string(),
            description: None,
            authors: None,
            dependencies: None,
            permissions: None,
            config_schema: None,
            entry_point: None,
        };

        assert!(manifest.validate().is_ok());
    }

    #[test]
    fn test_manifest_validate_empty_name() {
        let manifest = PluginManifest {
            name: String::new(),
            version: "1.0.0".to_string(),
            description: None,
            authors: None,
            dependencies: None,
            permissions: None,
            config_schema: None,
            entry_point: None,
        };

        assert!(manifest.validate().is_err());
    }

    #[test]
    fn test_manifest_validate_invalid_name_chars() {
        let manifest = PluginManifest {
            name: "invalid@plugin".to_string(),
            version: "1.0.0".to_string(),
            description: None,
            authors: None,
            dependencies: None,
            permissions: None,
            config_schema: None,
            entry_point: None,
        };

        assert!(manifest.validate().is_err());
    }

    #[test]
    fn test_manifest_validate_invalid_version() {
        let manifest = PluginManifest {
            name: "test_plugin".to_string(),
            version: "1.0".to_string(),
            description: None,
            authors: None,
            dependencies: None,
            permissions: None,
            config_schema: None,
            entry_point: None,
        };

        assert!(manifest.validate().is_err());
    }

    #[test]
    fn test_manifest_validate_invalid_permission() {
        let manifest = PluginManifest {
            name: "test_plugin".to_string(),
            version: "1.0.0".to_string(),
            description: None,
            authors: None,
            dependencies: None,
            permissions: Some(vec!["invalid_permission".to_string()]),
            config_schema: None,
            entry_point: None,
        };

        assert!(manifest.validate().is_err());
    }

    #[test]
    fn test_manifest_validate_valid_permissions() {
        let manifest = PluginManifest {
            name: "test_plugin".to_string(),
            version: "1.0.0".to_string(),
            description: None,
            authors: None,
            dependencies: None,
            permissions: Some(vec!["file_system".to_string(), "network".to_string()]),
            config_schema: None,
            entry_point: None,
        };

        assert!(manifest.validate().is_ok());
    }

    // DependencySpec tests
    #[test]
    fn test_dependency_spec_exact() {
        let spec = DependencySpec::parse_from_str("1.0.0").unwrap();
        assert!(spec.satisfies("1.0.0"));
        assert!(!spec.satisfies("1.0.1"));
        assert!(!spec.satisfies("0.9.0"));
    }

    #[test]
    fn test_dependency_spec_caret() {
        let spec = DependencySpec::parse_from_str("^1.0.0").unwrap();
        assert!(spec.satisfies("1.0.0"));
        assert!(spec.satisfies("1.0.1"));
        assert!(spec.satisfies("1.1.0"));
        assert!(!spec.satisfies("2.0.0"));
        assert!(!spec.satisfies("0.9.0"));
    }

    #[test]
    fn test_dependency_spec_caret_patch_version() {
        let spec = DependencySpec::Caret("1.2.5".to_string());
        assert!(!spec.satisfies("1.2.0")); // Below patch
        assert!(!spec.satisfies("1.2.4")); // Below patch
        assert!(spec.satisfies("1.2.5")); // Exact match
        assert!(spec.satisfies("1.2.6")); // Above patch
        assert!(spec.satisfies("1.3.0")); // Above minor
        assert!(!spec.satisfies("2.0.0")); // Above major
    }

    #[test]
    fn test_dependency_spec_greater_or_equal() {
        let spec = DependencySpec::parse_from_str(">=1.0.0").unwrap();
        assert!(spec.satisfies("1.0.0"));
        assert!(spec.satisfies("1.0.1"));
        assert!(spec.satisfies("2.0.0"));
        assert!(!spec.satisfies("0.9.0"));
    }

    #[test]
    fn test_dependency_spec_less() {
        let spec = DependencySpec::parse_from_str("<2.0.0").unwrap();
        assert!(spec.satisfies("1.9.9"));
        assert!(spec.satisfies("1.0.0"));
        assert!(!spec.satisfies("2.0.0"));
        assert!(!spec.satisfies("2.0.1"));
    }

    #[test]
    fn test_dependency_spec_wildcard() {
        let spec = DependencySpec::parse_from_str("1.0.x").unwrap();
        assert!(spec.satisfies("1.0.0"));
        assert!(spec.satisfies("1.0.1"));
        assert!(spec.satisfies("1.0.99"));
        assert!(!spec.satisfies("1.1.0"));
        assert!(!spec.satisfies("2.0.0"));
    }

    #[test]
    fn test_dependency_spec_any() {
        let spec = DependencySpec::parse_from_str("*").unwrap();
        assert!(spec.satisfies("0.0.0"));
        assert!(spec.satisfies("1.0.0"));
        assert!(spec.satisfies("999.999.999"));
    }

    #[test]
    fn test_dependency_spec_parse_invalid() {
        // Invalid semver should still parse (defaults to exact)
        let spec = DependencySpec::parse_from_str("invalid").unwrap();
        assert!(spec.satisfies("invalid"));
    }

    #[test]
    fn test_is_valid_semver() {
        assert!(PluginManifest::is_valid_semver("1.0.0"));
        assert!(PluginManifest::is_valid_semver("0.0.1"));
        assert!(PluginManifest::is_valid_semver("99.99.99"));
        assert!(!PluginManifest::is_valid_semver("1.0"));
        assert!(!PluginManifest::is_valid_semver("1.0.0.0"));
        assert!(!PluginManifest::is_valid_semver("a.b.c"));
    }

    #[test]
    fn test_is_valid_permission() {
        assert!(PluginManifest::is_valid_permission("file_system"));
        assert!(PluginManifest::is_valid_permission("network"));
        assert!(PluginManifest::is_valid_permission("subprocess"));
        assert!(!PluginManifest::is_valid_permission("invalid"));
        assert!(!PluginManifest::is_valid_permission(""));
    }

    #[test]
    #[cfg(feature = "toml")]
    fn test_manifest_from_toml_valid() {
        let toml = r#"
name = "my-plugin"
version = "1.0.0"
description = "A test plugin"
authors = ["Test Author"]
entry_point = "./plugin"
permissions = ["file_system"]

[dependencies]
"base-plugin" = "^1.0.0"
"#;

        let manifest = PluginManifest::from_toml(toml).unwrap();
        assert_eq!(manifest.name, "my-plugin");
        assert_eq!(manifest.version, "1.0.0");
        assert_eq!(manifest.description, Some("A test plugin".to_string()));
        assert_eq!(manifest.authors, Some(vec!["Test Author".to_string()]));
        assert_eq!(manifest.entry_point, Some("./plugin".to_string()));
        assert!(manifest.requires_permission("file_system"));
        assert!(manifest.depends_on("base-plugin"));
        assert_eq!(
            manifest.get_dependency_version("base-plugin"),
            Some("^1.0.0")
        );
    }

    #[test]
    #[cfg(feature = "toml")]
    fn test_manifest_from_toml_invalid() {
        let invalid_toml = r#"
name = "my-plugin"
version = "1.0.0"
this is not valid toml!!!
"#;

        let result = PluginManifest::from_toml(invalid_toml);
        assert!(result.is_err());
    }

    #[test]
    #[cfg(feature = "toml")]
    fn test_manifest_from_str_auto_detect_toml() {
        let toml = r#"
name = "auto-detect-plugin"
version = "2.0.0"
description = "Testing auto-detection"
"#;

        let manifest = PluginManifest::parse_from_str(toml).unwrap();
        assert_eq!(manifest.name, "auto-detect-plugin");
        assert_eq!(manifest.version, "2.0.0");
        assert_eq!(
            manifest.description,
            Some("Testing auto-detection".to_string())
        );
    }
}
