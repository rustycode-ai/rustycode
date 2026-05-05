//! Tool registry implementation
//!
//! This module provides the core tool registry with registration,
//! discovery, and metadata management.

use anyhow::Result;
use rustycode_tools_api::{
    default_tool_set, extended_tool_set, Tool, ToolInfo, ToolRegistry as ApiRegistry, ToolTier,
};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use tracing::{debug, info};

/// Configuration for the tool registry
#[derive(Debug, Clone)]
pub struct RegistryConfig {
    /// Whether to enable plugin loading
    pub enable_plugins: bool,

    /// Plugin directories to scan
    pub plugin_dirs: Vec<std::path::PathBuf>,

    /// Whether to enable tool discovery
    pub enable_discovery: bool,
}

impl Default for RegistryConfig {
    fn default() -> Self {
        Self {
            enable_plugins: true,
            plugin_dirs: vec!["./plugins".into()],
            enable_discovery: true,
        }
    }
}

/// Enhanced tool registry with discovery and plugin support
pub struct ToolRegistry {
    config: RegistryConfig,
    api_registry: Arc<RwLock<ApiRegistry>>,
    metadata: HashMap<String, ToolMetadata>,
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self::with_config(RegistryConfig::default())
    }

    /// Create a new tool registry with custom configuration
    pub fn with_config(config: RegistryConfig) -> Self {
        Self {
            config,
            api_registry: Arc::new(RwLock::new(ApiRegistry::new())),
            metadata: HashMap::new(),
        }
    }

    /// Register a tool
    pub fn register<T: Tool + 'static>(&mut self, tool: T) -> Result<()> {
        let name = tool.name().to_string();
        let description = tool.description().to_string();

        // Register in API registry
        {
            let mut registry = self
                .api_registry
                .write()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            registry.register(tool);
        }

        // Store metadata
        self.metadata.insert(
            name.clone(),
            ToolMetadata {
                name: name.clone(),
                description,
                version: "1.0.0".to_string(),  // Default version
                author: "Unknown".to_string(), // Default author
                tags: vec![],
                dependencies: vec![],
            },
        );

        debug!("Registered tool: {}", name);
        Ok(())
    }

    /// Get a tool by name
    pub fn get(&self, _name: &str) -> Option<std::sync::MutexGuard<'_, dyn Tool + 'static>> {
        // This is a simplified implementation
        // In practice, we'd need to handle the lifetime issues properly
        None
    }

    /// List all registered tools
    pub fn list(&self) -> Vec<ToolInfo> {
        self.api_registry
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .list()
    }

    /// List registered tools available for a given tier.
    pub fn list_for_tier(&self, tier: ToolTier) -> Vec<ToolInfo> {
        let all = self.list();
        match tier {
            ToolTier::Full => all,
            ToolTier::Extended => {
                let allowed = default_tool_set()
                    .into_iter()
                    .chain(extended_tool_set())
                    .collect::<std::collections::HashSet<_>>();
                all.into_iter()
                    .filter(|tool| allowed.contains(tool.name.as_str()))
                    .collect()
            }
            ToolTier::Default => {
                let allowed = default_tool_set();
                all.into_iter()
                    .filter(|tool| allowed.contains(tool.name.as_str()))
                    .collect()
            }
        }
    }

    /// Get tool metadata
    pub fn metadata(&self, name: &str) -> Option<&ToolMetadata> {
        self.metadata.get(name)
    }

    /// Discover and load tools from plugins
    #[allow(clippy::unused_async)]
    pub async fn discover_tools(&self) -> Result<()> {
        if !self.config.enable_discovery {
            return Ok(());
        }

        info!("Discovering tools from plugins...");

        // TODO: Implement plugin discovery
        // This would scan plugin directories and load tools dynamically

        Ok(())
    }

    /// Get the underlying API registry
    pub fn api_registry(&self) -> Arc<RwLock<ApiRegistry>> {
        Arc::clone(&self.api_registry)
    }
}

/// Tool metadata for enhanced registry features
#[derive(Debug, Clone)]
pub struct ToolMetadata {
    /// Tool name
    pub name: String,

    /// Tool description
    pub description: String,

    /// Tool version
    pub version: String,

    /// Tool author
    pub author: String,

    /// Tool tags for categorization
    pub tags: Vec<String>,

    /// Tool dependencies
    pub dependencies: Vec<String>,
}

impl ToolMetadata {
    /// Create new tool metadata
    pub fn new(name: String, description: String) -> Self {
        Self {
            name,
            description,
            version: "1.0.0".to_string(),
            author: "Unknown".to_string(),
            tags: vec![],
            dependencies: vec![],
        }
    }

    /// Add a tag
    pub fn with_tag(mut self, tag: String) -> Self {
        self.tags.push(tag);
        self
    }

    /// Set version
    pub fn with_version(mut self, version: String) -> Self {
        self.version = version;
        self
    }

    /// Set author
    pub fn with_author(mut self, author: String) -> Self {
        self.author = author;
        self
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn test_registry_config_default() {
        let config = RegistryConfig::default();
        assert!(config.enable_plugins);
        assert!(config.enable_discovery);
        assert_eq!(config.plugin_dirs.len(), 1);
    }

    #[test]
    fn test_registry_config_custom() {
        let config = RegistryConfig {
            enable_plugins: false,
            plugin_dirs: vec![],
            enable_discovery: false,
        };
        assert!(!config.enable_plugins);
        assert!(!config.enable_discovery);
        assert!(config.plugin_dirs.is_empty());
    }

    #[test]
    fn test_tool_registry_new() {
        let registry = ToolRegistry::new();
        let tools = registry.list();
        assert!(tools.is_empty());
    }

    #[test]
    fn test_tool_registry_default() {
        let registry = ToolRegistry::default();
        let tools = registry.list();
        assert!(tools.is_empty());
    }

    #[test]
    fn test_tool_metadata_new() {
        let meta = ToolMetadata::new("bash".to_string(), "Run commands".to_string());
        assert_eq!(meta.name, "bash");
        assert_eq!(meta.description, "Run commands");
        assert_eq!(meta.version, "1.0.0");
        assert_eq!(meta.author, "Unknown");
        assert!(meta.tags.is_empty());
        assert!(meta.dependencies.is_empty());
    }

    #[test]
    fn test_tool_metadata_with_tag() {
        let meta = ToolMetadata::new("bash".to_string(), "desc".to_string())
            .with_tag("system".to_string())
            .with_tag("dangerous".to_string());
        assert_eq!(meta.tags, vec!["system", "dangerous"]);
    }

    #[test]
    fn test_tool_metadata_with_version() {
        let meta = ToolMetadata::new("bash".to_string(), "desc".to_string())
            .with_version("2.0.0".to_string());
        assert_eq!(meta.version, "2.0.0");
    }

    #[test]
    fn test_tool_metadata_with_author() {
        let meta = ToolMetadata::new("bash".to_string(), "desc".to_string())
            .with_author("Nat".to_string());
        assert_eq!(meta.author, "Nat");
    }

    #[derive(Clone)]
    struct StubTool {
        name: &'static str,
    }

    impl Tool for StubTool {
        fn name(&self) -> &str {
            self.name
        }

        fn description(&self) -> &'static str {
            "stub"
        }

        fn parameters_schema(&self) -> serde_json::Value {
            serde_json::json!({})
        }

        fn execute(
            &self,
            _params: serde_json::Value,
            _ctx: &rustycode_tools_api::ToolContext,
        ) -> anyhow::Result<rustycode_tools_api::ToolOutput> {
            Ok(rustycode_tools_api::ToolOutput::text("ok"))
        }
    }

    #[test]
    fn list_for_tier_filters_tools() {
        let mut registry = ToolRegistry::new();
        registry.register(StubTool { name: "read_file" }).unwrap();
        registry.register(StubTool { name: "web_fetch" }).unwrap();

        let default = registry.list_for_tier(ToolTier::Default);
        assert_eq!(default.len(), 1);
        assert_eq!(default[0].name, "read_file");

        let extended = registry.list_for_tier(ToolTier::Extended);
        assert_eq!(extended.len(), 2);

        let full = registry.list_for_tier(ToolTier::Full);
        assert_eq!(full.len(), 2);
    }

    #[test]
    fn test_tool_metadata_builder_chain() {
        let meta = ToolMetadata::new("tool".to_string(), "desc".to_string())
            .with_version("3.0.0".to_string())
            .with_author("Team".to_string())
            .with_tag("core".to_string());
        assert_eq!(meta.version, "3.0.0");
        assert_eq!(meta.author, "Team");
        assert_eq!(meta.tags, vec!["core"]);
    }

    #[test]
    fn test_get_metadata_not_found() {
        let registry = ToolRegistry::new();
        assert!(registry.metadata("nonexistent").is_none());
    }

    #[test]
    fn test_api_registry_accessible() {
        let registry = ToolRegistry::new();
        let api = registry.api_registry();
        let is_empty = api
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .list()
            .is_empty();
        assert!(is_empty);
    }

    #[tokio::test]
    async fn test_discover_tools_disabled() {
        let config = RegistryConfig {
            enable_discovery: false,
            ..Default::default()
        };
        let registry = ToolRegistry::with_config(config);
        let result = registry.discover_tools().await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_discover_tools_enabled() {
        let registry = ToolRegistry::new();
        let result = registry.discover_tools().await;
        assert!(result.is_ok());
    }

    // --- Registration tests with mock tool ---

    /// A minimal mock tool for testing registration.
    struct MockTool {
        name: String,
        desc: String,
    }

    impl MockTool {
        fn new(name: &str, desc: &str) -> Self {
            Self {
                name: name.to_string(),
                desc: desc.to_string(),
            }
        }
    }

    impl rustycode_tools_api::Tool for MockTool {
        fn name(&self) -> &str {
            &self.name
        }
        fn description(&self) -> &str {
            &self.desc
        }
        fn parameters_schema(&self) -> serde_json::Value {
            serde_json::json!({"type": "object"})
        }
        fn execute(
            &self,
            _params: serde_json::Value,
            _ctx: &rustycode_tools_api::ToolContext,
        ) -> anyhow::Result<rustycode_tools_api::ToolOutput> {
            Ok(rustycode_tools_api::ToolOutput::text("ok"))
        }
    }

    #[test]
    fn test_register_single_tool() {
        let mut registry = ToolRegistry::new();
        registry
            .register(MockTool::new("bash", "Run shell commands"))
            .unwrap();

        let tools = registry.list();
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].name, "bash");
        assert_eq!(tools[0].description, "Run shell commands");
    }

    #[test]
    fn test_register_multiple_tools() {
        let mut registry = ToolRegistry::new();
        registry
            .register(MockTool::new("bash", "Run commands"))
            .unwrap();
        registry
            .register(MockTool::new("read", "Read files"))
            .unwrap();
        registry
            .register(MockTool::new("write", "Write files"))
            .unwrap();

        let tools = registry.list();
        assert_eq!(tools.len(), 3);
        let names: Vec<&str> = tools.iter().map(|t| t.name.as_str()).collect();
        assert!(names.contains(&"bash"));
        assert!(names.contains(&"read"));
        assert!(names.contains(&"write"));
    }

    #[test]
    fn test_register_stores_metadata() {
        let mut registry = ToolRegistry::new();
        registry
            .register(MockTool::new("grep", "Search files"))
            .unwrap();

        let meta = registry.metadata("grep");
        assert!(meta.is_some());
        let meta = meta.unwrap();
        assert_eq!(meta.name, "grep");
        assert_eq!(meta.description, "Search files");
    }

    #[test]
    fn test_register_overwrites_existing() {
        let mut registry = ToolRegistry::new();
        registry.register(MockTool::new("tool", "v1")).unwrap();
        registry.register(MockTool::new("tool", "v2")).unwrap();

        let tools = registry.list();
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].description, "v2");
    }

    #[test]
    fn test_get_returns_none_unimplemented() {
        let mut registry = ToolRegistry::new();
        registry.register(MockTool::new("bash", "Run")).unwrap();
        assert!(registry.get("bash").is_none());
    }

    #[test]
    fn test_list_empty_registry() {
        let registry = ToolRegistry::new();
        assert!(registry.list().is_empty());
    }

    #[test]
    fn test_api_registry_reflects_registrations() {
        let mut registry = ToolRegistry::new();
        registry.register(MockTool::new("tool1", "First")).unwrap();
        registry.register(MockTool::new("tool2", "Second")).unwrap();

        let api = registry.api_registry();
        let api_tools = api
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .list();
        assert_eq!(api_tools.len(), 2);
    }

    #[test]
    fn test_with_config_no_plugins() {
        let config = RegistryConfig {
            enable_plugins: false,
            plugin_dirs: vec![],
            enable_discovery: false,
        };
        let registry = ToolRegistry::with_config(config);
        assert!(registry.list().is_empty());
    }
}
