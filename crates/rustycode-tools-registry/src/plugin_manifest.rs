//! Plugin manifest format, parsing, and validation.
//!
//! Supports JSON format for plugin manifests. Manifests declare metadata,
//! dependencies, permissions, and configuration schema.

use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use std::collections::HashMap;
use std::str::FromStr;

use crate::plugin_error::PluginError;

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
    /// Parse a manifest from JSON string
    pub fn from_json(content: &str) -> Result<Self, PluginError> {
        serde_json::from_str(content)
            .map_err(|e| PluginError::configuration_error(format!("JSON parse error: {e}")))
    }

    /// Parse a manifest from a string, auto-detecting format
    pub fn parse_from_str(content: &str) -> Result<Self, PluginError> {
        Self::from_json(content)
    }

    /// Validate the manifest
    pub fn validate(&self) -> Result<(), PluginError> {
        if self.name.is_empty() {
            return Err(PluginError::configuration_error(
                "Plugin name cannot be empty".to_string(),
            ));
        }

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

        if !Self::is_valid_semver(&self.version) {
            return Err(PluginError::configuration_error(format!(
                "Invalid version format: {}",
                self.version
            )));
        }

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
                | "memory"
                | "llm_access"
                | "tool_execution"
        )
    }

    /// Get dependencies as a list of names
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

/// Dependency version specification (e.g., "1.0.0", ">=1.0.0", "^1.0.0", "1.0.x")
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum DependencySpec {
    /// Exact version (e.g., "1.0.0")
    Exact(String),
    /// Caret version (e.g., "^1.0.0" - compatible with 1.x)
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

        Ok(Self::Exact(spec.to_string()))
    }

    /// Check if a version satisfies this spec
    pub fn satisfies(&self, version: &str) -> bool {
        match self {
            Self::Any => true,
            Self::Exact(spec_ver) => version == spec_ver,
            Self::Caret(spec_ver) => {
                let spec_parts: Vec<&str> = spec_ver.split('.').collect();
                let ver_parts: Vec<&str> = version.split('.').collect();

                if spec_parts.len() != 3 || ver_parts.len() != 3 {
                    return false;
                }

                let Ok(spec_major) = spec_parts[0].parse::<u32>() else {
                    return false;
                };
                let Ok(spec_minor) = spec_parts[1].parse::<u32>() else {
                    return false;
                };
                let Ok(spec_patch) = spec_parts[2].parse::<u32>() else {
                    return false;
                };
                let Ok(ver_major) = ver_parts[0].parse::<u32>() else {
                    return false;
                };
                let Ok(ver_minor) = ver_parts[1].parse::<u32>() else {
                    return false;
                };
                let Ok(ver_patch) = ver_parts[2].parse::<u32>() else {
                    return false;
                };

                if ver_major != spec_major {
                    return false;
                }
                if ver_minor != spec_minor {
                    return ver_minor > spec_minor;
                }
                ver_patch >= spec_patch
            }
            Self::GreaterOrEqual(spec_ver) => Self::compare_versions(version, spec_ver) >= 0,
            Self::Less(spec_ver) => Self::compare_versions(version, spec_ver) < 0,
            Self::Wildcard(spec_pattern) => {
                let spec_ver = spec_pattern.replace('x', "0");
                let spec_parts: Vec<&str> = spec_ver.split('.').collect();
                let ver_parts: Vec<&str> = version.split('.').collect();

                if spec_parts.len() != 3 || ver_parts.len() != 3 {
                    return false;
                }

                spec_parts[0] == ver_parts[0] && spec_parts[1] == ver_parts[1]
            }
        }
    }

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
        let json = r#"{"name": "test_plugin", "version": "1.0.0", "description": "A test plugin"}"#;
        let manifest = PluginManifest::from_json(json).unwrap();
        assert_eq!(manifest.name, "test_plugin");
        assert_eq!(manifest.version, "1.0.0");
    }

    #[test]
    fn test_manifest_json_with_dependencies() {
        let json = r#"{
            "name": "plugin_a",
            "version": "2.0.0",
            "dependencies": {"plugin_b": ">=1.0.0", "plugin_c": "^2.0.0"}
        }"#;
        let manifest = PluginManifest::from_json(json).unwrap();
        assert!(manifest.depends_on("plugin_b"));
        assert!(!manifest.depends_on("plugin_d"));
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
    fn test_manifest_validate_invalid_version() {
        let manifest = PluginManifest {
            name: "test".to_string(),
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
    fn test_dependency_spec_exact() {
        let spec = DependencySpec::parse_from_str("1.0.0").unwrap();
        assert!(spec.satisfies("1.0.0"));
        assert!(!spec.satisfies("1.0.1"));
    }

    #[test]
    fn test_dependency_spec_caret() {
        let spec = DependencySpec::parse_from_str("^1.0.0").unwrap();
        assert!(spec.satisfies("1.0.0"));
        assert!(spec.satisfies("1.1.0"));
        assert!(!spec.satisfies("2.0.0"));
    }

    #[test]
    fn test_dependency_spec_greater_or_equal() {
        let spec = DependencySpec::parse_from_str(">=1.0.0").unwrap();
        assert!(spec.satisfies("1.0.0"));
        assert!(spec.satisfies("2.0.0"));
        assert!(!spec.satisfies("0.9.0"));
    }

    #[test]
    fn test_dependency_spec_less() {
        let spec = DependencySpec::parse_from_str("<2.0.0").unwrap();
        assert!(spec.satisfies("1.9.9"));
        assert!(!spec.satisfies("2.0.0"));
    }

    #[test]
    fn test_dependency_spec_wildcard() {
        let spec = DependencySpec::parse_from_str("1.0.x").unwrap();
        assert!(spec.satisfies("1.0.0"));
        assert!(spec.satisfies("1.0.99"));
        assert!(!spec.satisfies("1.1.0"));
    }

    #[test]
    fn test_dependency_spec_any() {
        let spec = DependencySpec::parse_from_str("*").unwrap();
        assert!(spec.satisfies("0.0.0"));
        assert!(spec.satisfies("999.999.999"));
    }

    #[test]
    fn test_dependency_spec_caret_patch() {
        let spec = DependencySpec::Caret("1.2.5".to_string());
        assert!(!spec.satisfies("1.2.4"));
        assert!(spec.satisfies("1.2.5"));
        assert!(spec.satisfies("1.2.6"));
        assert!(spec.satisfies("1.3.0"));
        assert!(!spec.satisfies("2.0.0"));
    }
}
