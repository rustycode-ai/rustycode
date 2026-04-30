//! Plugin manifest and metadata

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Plugin manifest loaded from plugin.toml
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginManifest {
    /// Plugin name (unique identifier)
    pub name: String,

    /// Semantic version
    pub version: String,

    /// Human-readable description
    pub description: String,

    /// Plugin author
    #[serde(default)]
    pub author: String,

    /// Required permissions
    #[serde(default)]
    pub permissions: Vec<String>,

    /// Entry point (dynamic library file)
    pub entry_point: String,

    /// Plugin type
    #[serde(default)]
    pub plugin_type: PluginType,

    /// Slash commands provided by this plugin
    #[serde(default)]
    pub slash_commands: Vec<SlashCommand>,

    /// Theme configuration (if this is a theme plugin)
    #[serde(default)]
    pub theme: Option<ThemeConfig>,

    /// Minimum RustyCode version required
    #[serde(default)]
    pub min_rustycode_version: Option<String>,
}

/// Plugin type classification
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum PluginType {
    /// Command plugin (adds slash commands)
    #[default]
    Command,
    /// Theme plugin (color schemes)
    Theme,
    /// Hook plugin (event handlers)
    Hook,
    /// Hybrid plugin (multiple types)
    Hybrid,
}

/// Slash command definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlashCommand {
    /// Command name (without slash)
    pub name: String,

    /// Description for help text
    pub description: String,

    /// Handler function name in the plugin
    pub handler: String,

    /// Argument schema (optional)
    #[serde(default)]
    pub args: Vec<ArgSchema>,
}

/// Argument schema for slash commands
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArgSchema {
    /// Argument name
    pub name: String,

    /// Argument description
    pub description: String,

    /// Whether this argument is required
    #[serde(default)]
    pub required: bool,

    /// Argument type (for validation)
    #[serde(default = "default_arg_type")]
    pub arg_type: String,
}

fn default_arg_type() -> String {
    "string".to_string()
}

/// Theme configuration for theme plugins
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThemeConfig {
    /// Background color
    pub background: String,

    /// Foreground color
    pub foreground: String,

    /// Cursor color
    pub cursor: String,

    /// Selection color
    pub selection: String,

    /// Comment color
    pub comment: String,

    /// Primary color (for accents)
    #[serde(default)]
    pub primary: Option<String>,

    /// Secondary color
    #[serde(default)]
    pub secondary: Option<String>,

    /// Error color
    #[serde(default)]
    pub error: Option<String>,

    /// Warning color
    #[serde(default)]
    pub warning: Option<String>,
}

impl PluginManifest {
    /// Load manifest from a file
    pub fn from_path(path: &PathBuf) -> Result<Self, anyhow::Error> {
        let content = std::fs::read_to_string(path)?;
        let manifest: PluginManifest = toml::from_str(&content)
            .map_err(|e| anyhow::anyhow!("Failed to parse plugin.toml: {}", e))?;

        // Validate manifest
        manifest.validate()?;

        Ok(manifest)
    }

    /// Validate manifest fields
    fn validate(&self) -> Result<(), anyhow::Error> {
        if self.name.is_empty() {
            return Err(anyhow::anyhow!("Plugin name cannot be empty"));
        }

        if self.name.contains(' ') || self.name.contains('/') {
            return Err(anyhow::anyhow!(
                "Plugin name cannot contain spaces or slashes"
            ));
        }

        if self.entry_point.is_empty() {
            return Err(anyhow::anyhow!("Entry point cannot be empty"));
        }

        // Validate slash command names
        for cmd in &self.slash_commands {
            if cmd.name.contains(' ') {
                return Err(anyhow::anyhow!(
                    "Slash command '{}' cannot contain spaces",
                    cmd.name
                ));
            }
        }

        Ok(())
    }

    /// Get plugin directory path from manifest path
    pub fn plugin_dir(manifest_path: &Path) -> PathBuf {
        manifest_path
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| manifest_path.to_path_buf())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_parse_valid_manifest() {
        let manifest_toml = r#"
            name = "test-plugin"
            version = "0.1.0"
            description = "A test plugin"
            author = "Test Author"
            permissions = ["notification"]
            entry_point = "libtest.so"

            [[slash_commands]]
            name = "test"
            description = "Test command"
            handler = "test_handler"
        "#;

        let manifest: PluginManifest = toml::from_str(manifest_toml).unwrap();
        assert_eq!(manifest.name, "test-plugin");
        assert_eq!(manifest.version, "0.1.0");
        assert_eq!(manifest.permissions.len(), 1);
        assert_eq!(manifest.slash_commands.len(), 1);
    }

    #[test]
    fn test_validate_invalid_name() {
        let manifest = PluginManifest {
            name: "test plugin".to_string(),
            version: "0.1.0".to_string(),
            description: "Test".to_string(),
            author: "Test".to_string(),
            permissions: vec![],
            entry_point: "libtest.so".to_string(),
            plugin_type: PluginType::Command,
            slash_commands: vec![],
            theme: None,
            min_rustycode_version: None,
        };

        assert!(manifest.validate().is_err());
    }

    #[test]
    fn test_validate_invalid_command_name() {
        let manifest = PluginManifest {
            name: "test-plugin".to_string(),
            version: "0.1.0".to_string(),
            description: "Test".to_string(),
            author: "Test".to_string(),
            permissions: vec![],
            entry_point: "libtest.so".to_string(),
            plugin_type: PluginType::Command,
            slash_commands: vec![SlashCommand {
                name: "test command".to_string(),
                description: "Test".to_string(),
                handler: "test_handler".to_string(),
                args: vec![],
            }],
            theme: None,
            min_rustycode_version: None,
        };

        assert!(manifest.validate().is_err());
    }

    #[test]
    fn test_plugin_dir() {
        let mut temp_file = NamedTempFile::new().unwrap();
        writeln!(temp_file, "test").unwrap();

        let path = temp_file.path().to_path_buf();
        let dir = PluginManifest::plugin_dir(&path);

        assert_eq!(dir, path.parent().unwrap());
    }

    // --- PluginType serde ---

    #[test]
    fn plugin_type_serde_roundtrip() {
        for pt in &[
            PluginType::Command,
            PluginType::Theme,
            PluginType::Hook,
            PluginType::Hybrid,
        ] {
            let json = serde_json::to_string(pt).unwrap();
            let back: PluginType = serde_json::from_str(&json).unwrap();
            assert_eq!(&back, pt);
        }
    }

    #[test]
    fn plugin_type_serde_renames() {
        assert_eq!(
            serde_json::to_string(&PluginType::Command).unwrap(),
            "\"command\""
        );
        assert_eq!(
            serde_json::to_string(&PluginType::Theme).unwrap(),
            "\"theme\""
        );
        assert_eq!(
            serde_json::to_string(&PluginType::Hook).unwrap(),
            "\"hook\""
        );
        assert_eq!(
            serde_json::to_string(&PluginType::Hybrid).unwrap(),
            "\"hybrid\""
        );
    }

    #[test]
    fn plugin_type_default_is_command() {
        assert_eq!(PluginType::default(), PluginType::Command);
    }

    // --- SlashCommand serde ---

    #[test]
    fn slash_command_serde() {
        let cmd = SlashCommand {
            name: "deploy".into(),
            description: "Deploy the app".into(),
            handler: "handle_deploy".into(),
            args: vec![ArgSchema {
                name: "env".into(),
                description: "Target environment".into(),
                required: true,
                arg_type: "string".into(),
            }],
        };
        let json = serde_json::to_string(&cmd).unwrap();
        let decoded: SlashCommand = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.name, "deploy");
        assert_eq!(decoded.args.len(), 1);
        assert!(decoded.args[0].required);
    }

    // --- ArgSchema serde ---

    #[test]
    fn arg_schema_serde() {
        let arg = ArgSchema {
            name: "count".into(),
            description: "Number of items".into(),
            required: false,
            arg_type: "number".into(),
        };
        let json = serde_json::to_string(&arg).unwrap();
        let decoded: ArgSchema = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.name, "count");
        assert!(!decoded.required);
    }

    #[test]
    fn arg_schema_default_type_is_string() {
        let json = r#"{"name":"x","description":"x"}"#;
        let decoded: ArgSchema = serde_json::from_str(json).unwrap();
        assert_eq!(decoded.arg_type, "string");
    }

    // --- ThemeConfig serde ---

    #[test]
    fn theme_config_serde() {
        let theme = ThemeConfig {
            background: "#000".into(),
            foreground: "#fff".into(),
            cursor: "#f00".into(),
            selection: "#00f".into(),
            comment: "#888".into(),
            primary: Some("#ff0".into()),
            secondary: None,
            error: Some("#f44".into()),
            warning: None,
        };
        let json = serde_json::to_string(&theme).unwrap();
        let decoded: ThemeConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.background, "#000");
        assert_eq!(decoded.primary, Some("#ff0".into()));
        assert!(decoded.secondary.is_none());
    }

    // --- PluginManifest serde ---

    #[test]
    fn plugin_manifest_serde_roundtrip() {
        let manifest = PluginManifest {
            name: "my-plugin".into(),
            version: "1.0.0".into(),
            description: "Test plugin".into(),
            author: "nat".into(),
            permissions: vec!["fs".into(), "net".into()],
            entry_point: "libmy.so".into(),
            plugin_type: PluginType::Hook,
            slash_commands: vec![],
            theme: None,
            min_rustycode_version: Some("0.1.0".into()),
        };
        let json = serde_json::to_string(&manifest).unwrap();
        let decoded: PluginManifest = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.name, "my-plugin");
        assert_eq!(decoded.permissions.len(), 2);
        assert_eq!(decoded.plugin_type, PluginType::Hook);
        assert_eq!(decoded.min_rustycode_version, Some("0.1.0".into()));
    }

    // --- Validation ---

    #[test]
    fn validate_valid_manifest() {
        let manifest = PluginManifest {
            name: "valid-plugin".into(),
            version: "0.1.0".into(),
            description: "desc".into(),
            author: "".into(),
            permissions: vec![],
            entry_point: "libvalid.so".into(),
            plugin_type: PluginType::Command,
            slash_commands: vec![],
            theme: None,
            min_rustycode_version: None,
        };
        assert!(manifest.validate().is_ok());
    }

    #[test]
    fn validate_empty_name_fails() {
        let manifest = PluginManifest {
            name: "".into(),
            version: "0.1.0".into(),
            description: "desc".into(),
            author: "".into(),
            permissions: vec![],
            entry_point: "lib.so".into(),
            plugin_type: PluginType::Command,
            slash_commands: vec![],
            theme: None,
            min_rustycode_version: None,
        };
        assert!(manifest.validate().is_err());
    }

    #[test]
    fn validate_empty_entry_point_fails() {
        let manifest = PluginManifest {
            name: "test".into(),
            version: "0.1.0".into(),
            description: "desc".into(),
            author: "".into(),
            permissions: vec![],
            entry_point: "".into(),
            plugin_type: PluginType::Command,
            slash_commands: vec![],
            theme: None,
            min_rustycode_version: None,
        };
        assert!(manifest.validate().is_err());
    }

    #[test]
    fn validate_name_with_slash_fails() {
        let manifest = PluginManifest {
            name: "bad/name".into(),
            version: "0.1.0".into(),
            description: "desc".into(),
            author: "".into(),
            permissions: vec![],
            entry_point: "lib.so".into(),
            plugin_type: PluginType::Command,
            slash_commands: vec![],
            theme: None,
            min_rustycode_version: None,
        };
        assert!(manifest.validate().is_err());
    }

    // --- TOML parsing ---

    #[test]
    fn parse_minimal_toml_manifest() {
        let toml = r#"
            name = "minimal"
            version = "0.1.0"
            description = "Minimal plugin"
            entry_point = "libmin.so"
        "#;
        let manifest: PluginManifest = toml::from_str(toml).unwrap();
        assert_eq!(manifest.name, "minimal");
        assert!(manifest.author.is_empty());
        assert!(manifest.permissions.is_empty());
        assert!(manifest.slash_commands.is_empty());
        assert_eq!(manifest.plugin_type, PluginType::Command);
    }

    #[test]
    fn parse_theme_manifest_toml() {
        let toml = r##"
            name = "dark-theme"
            version = "1.0.0"
            description = "Dark theme"
            entry_point = "libtheme.so"
            plugin_type = "theme"

            [theme]
            background = "#1e1e1e"
            foreground = "#d4d4d4"
            cursor = "#ffffff"
            selection = "#264f78"
            comment = "#6a9955"
        "##;
        let manifest: PluginManifest = toml::from_str(toml).unwrap();
        assert_eq!(manifest.plugin_type, PluginType::Theme);
        let theme = manifest.theme.unwrap();
        assert_eq!(theme.background, "#1e1e1e");
    }
}
