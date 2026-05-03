//! Plugin metadata for versioning, dependencies, and documentation

use serde::{Deserialize, Serialize};

/// Metadata about a plugin including version, authors, and dependencies
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginMetadata {
    /// Plugin name (must be unique in registry)
    pub name: String,

    /// Plugin version (semver format)
    pub version: String,

    /// Human-readable description
    pub description: String,

    /// Plugin authors
    pub authors: Vec<String>,

    /// Plugin dependencies (list of plugin names required by this plugin)
    pub dependencies: Vec<String>,

    /// Minimum compatible API version
    pub min_api_version: String,

    /// Optional URL to plugin documentation
    pub documentation_url: Option<String>,

    /// Optional URL to plugin repository
    pub repository_url: Option<String>,

    /// License identifier (SPDX format recommended)
    pub license: Option<String>,
}

impl PluginMetadata {
    /// Create new plugin metadata
    pub fn new(
        name: impl Into<String>,
        version: impl Into<String>,
        description: impl Into<String>,
    ) -> Self {
        Self {
            name: name.into(),
            version: version.into(),
            description: description.into(),
            authors: vec![],
            dependencies: vec![],
            min_api_version: "0.1.0".to_string(),
            documentation_url: None,
            repository_url: None,
            license: None,
        }
    }

    /// Add an author
    pub fn with_author(mut self, author: impl Into<String>) -> Self {
        self.authors.push(author.into());
        self
    }

    /// Add authors from a vec
    pub fn with_authors(mut self, authors: Vec<String>) -> Self {
        self.authors.extend(authors);
        self
    }

    /// Add a dependency
    pub fn with_dependency(mut self, dependency: impl Into<String>) -> Self {
        self.dependencies.push(dependency.into());
        self
    }

    /// Add dependencies
    pub fn with_dependencies(mut self, dependencies: Vec<String>) -> Self {
        self.dependencies.extend(dependencies);
        self
    }

    /// Set the minimum API version
    pub fn with_min_api_version(mut self, version: impl Into<String>) -> Self {
        self.min_api_version = version.into();
        self
    }

    /// Set documentation URL
    pub fn with_documentation_url(mut self, url: impl Into<String>) -> Self {
        self.documentation_url = Some(url.into());
        self
    }

    /// Set repository URL
    pub fn with_repository_url(mut self, url: impl Into<String>) -> Self {
        self.repository_url = Some(url.into());
        self
    }

    /// Set license
    pub fn with_license(mut self, license: impl Into<String>) -> Self {
        self.license = Some(license.into());
        self
    }

    /// Check if this plugin depends on another
    pub fn depends_on(&self, plugin_name: &str) -> bool {
        self.dependencies.contains(&plugin_name.to_string())
    }

    /// Get all dependencies
    pub fn get_dependencies(&self) -> &[String] {
        &self.dependencies
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metadata_new() {
        let meta = PluginMetadata::new("test_plugin", "1.0.0", "A test plugin");
        assert_eq!(meta.name, "test_plugin");
        assert_eq!(meta.version, "1.0.0");
        assert_eq!(meta.description, "A test plugin");
        assert_eq!(meta.min_api_version, "0.1.0");
    }

    #[test]
    fn test_metadata_with_author() {
        let meta = PluginMetadata::new("test", "1.0.0", "desc")
            .with_author("Alice")
            .with_author("Bob");
        assert_eq!(meta.authors, vec!["Alice", "Bob"]);
    }

    #[test]
    fn test_metadata_with_dependencies() {
        let meta = PluginMetadata::new("test", "1.0.0", "desc")
            .with_dependency("plugin_a")
            .with_dependency("plugin_b");
        assert!(meta.depends_on("plugin_a"));
        assert!(meta.depends_on("plugin_b"));
        assert!(!meta.depends_on("plugin_c"));
    }

    #[test]
    fn test_metadata_builder_chain() {
        let meta = PluginMetadata::new("test", "1.0.0", "desc")
            .with_author("Alice")
            .with_dependency("base_plugin")
            .with_license("MIT")
            .with_documentation_url("https://example.com/docs");

        assert_eq!(meta.authors, vec!["Alice"]);
        assert_eq!(meta.dependencies, vec!["base_plugin"]);
        assert_eq!(meta.license, Some("MIT".to_string()));
        assert!(meta.documentation_url.is_some());
    }

    #[test]
    fn test_metadata_serialization() {
        let meta = PluginMetadata::new("test", "1.0.0", "A test plugin").with_author("Author");

        let json = serde_json::to_string(&meta).unwrap();
        let deserialized: PluginMetadata = serde_json::from_str(&json).unwrap();

        assert_eq!(meta.name, deserialized.name);
        assert_eq!(meta.version, deserialized.version);
        assert_eq!(meta.authors, deserialized.authors);
    }

    #[test]
    fn test_metadata_with_authors_vec() {
        let meta =
            PluginMetadata::new("p", "1.0.0", "d").with_authors(vec!["A".into(), "B".into()]);
        assert_eq!(meta.authors, vec!["A", "B"]);
    }

    #[test]
    fn test_metadata_with_dependencies_vec() {
        let meta = PluginMetadata::new("p", "1.0.0", "d")
            .with_dependencies(vec!["dep1".into(), "dep2".into()]);
        assert_eq!(meta.get_dependencies(), &["dep1", "dep2"]);
    }

    #[test]
    fn test_metadata_full_builder() {
        let meta = PluginMetadata::new("plug", "2.0.0", "desc")
            .with_author("Dev")
            .with_dependency("core")
            .with_min_api_version("1.0.0")
            .with_documentation_url("https://docs.example.com")
            .with_repository_url("https://github.com/example/plug")
            .with_license("Apache-2.0");

        assert_eq!(meta.name, "plug");
        assert_eq!(meta.version, "2.0.0");
        assert_eq!(meta.authors, vec!["Dev"]);
        assert!(meta.depends_on("core"));
        assert_eq!(meta.min_api_version, "1.0.0");
        assert_eq!(
            meta.documentation_url,
            Some("https://docs.example.com".into())
        );
        assert_eq!(
            meta.repository_url,
            Some("https://github.com/example/plug".into())
        );
        assert_eq!(meta.license, Some("Apache-2.0".into()));
    }

    #[test]
    fn test_metadata_serialization_roundtrip_full() {
        let meta = PluginMetadata::new("plug", "1.0.0", "test plugin")
            .with_author("Author")
            .with_dependency("dep")
            .with_min_api_version("0.5.0")
            .with_documentation_url("https://docs.rs")
            .with_repository_url("https://github.com/test")
            .with_license("MIT");

        let json = serde_json::to_string(&meta).unwrap();
        let back: PluginMetadata = serde_json::from_str(&json).unwrap();

        assert_eq!(meta.name, back.name);
        assert_eq!(meta.version, back.version);
        assert_eq!(meta.description, back.description);
        assert_eq!(meta.authors, back.authors);
        assert_eq!(meta.dependencies, back.dependencies);
        assert_eq!(meta.min_api_version, back.min_api_version);
        assert_eq!(meta.documentation_url, back.documentation_url);
        assert_eq!(meta.repository_url, back.repository_url);
        assert_eq!(meta.license, back.license);
    }

    #[test]
    fn test_metadata_defaults() {
        let meta = PluginMetadata::new("p", "1.0.0", "d");
        assert!(meta.authors.is_empty());
        assert!(meta.dependencies.is_empty());
        assert_eq!(meta.min_api_version, "0.1.0");
        assert!(meta.documentation_url.is_none());
        assert!(meta.repository_url.is_none());
        assert!(meta.license.is_none());
    }

    #[test]
    fn test_metadata_debug_format() {
        let meta = PluginMetadata::new("test", "1.0.0", "desc");
        let debug = format!("{:?}", meta);
        assert!(debug.contains("test"));
        assert!(debug.contains("1.0.0"));
    }

    #[test]
    fn test_metadata_clone() {
        let meta = PluginMetadata::new("p", "1.0.0", "d").with_author("A");
        let cloned = meta.clone();
        assert_eq!(meta.name, cloned.name);
        assert_eq!(meta.authors, cloned.authors);
    }

    #[test]
    fn test_depends_on_case_sensitive() {
        let meta = PluginMetadata::new("p", "1.0.0", "d").with_dependency("Core");
        assert!(meta.depends_on("Core"));
        assert!(!meta.depends_on("core"));
    }

    #[test]
    fn test_get_dependencies_empty() {
        let meta = PluginMetadata::new("p", "1.0.0", "d");
        assert!(meta.get_dependencies().is_empty());
    }
}
