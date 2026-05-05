//! Plugin loader with dependency-aware load ordering.
//!
//! Resolves plugin dependencies via topological sort before loading,
//! ensuring all dependencies are satisfied and no cycles exist.

use crate::dependency_resolver::DependencyResolver;
use crate::plugin_error::PluginError;
use crate::plugin_manifest::PluginManifest;

/// Plugin loader that resolves dependencies and produces a load order.
pub struct PluginLoader {
    resolver: DependencyResolver,
}

impl Default for PluginLoader {
    fn default() -> Self {
        Self::new()
    }
}

impl PluginLoader {
    /// Create a new plugin loader.
    pub fn new() -> Self {
        Self {
            resolver: DependencyResolver::new(),
        }
    }

    /// Add a plugin manifest to the loader.
    pub fn add_manifest(&mut self, manifest: PluginManifest) -> Result<(), PluginError> {
        self.resolver.register(manifest)
    }

    /// Add multiple plugin manifests at once.
    pub fn add_manifests(&mut self, manifests: Vec<PluginManifest>) -> Result<(), PluginError> {
        self.resolver.register_all(manifests)
    }

    /// Resolve the load order for a specific plugin and its dependencies.
    pub fn resolve_load_order(&self, plugin_name: &str) -> Result<Vec<String>, PluginError> {
        self.resolver.resolve(plugin_name)
    }

    /// Resolve a global load order for all registered plugins.
    ///
    /// Returns plugin names in dependency-safe order (dependencies first).
    pub fn resolve_all(&self) -> Result<Vec<String>, PluginError> {
        self.resolver.validate_all()
    }

    /// Get a registered plugin manifest by name.
    pub fn get_plugin(&self, name: &str) -> Option<&PluginManifest> {
        self.resolver.get_plugin(name)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn manifest(
        name: &str,
        version: &str,
        deps: Option<HashMap<String, String>>,
    ) -> PluginManifest {
        PluginManifest {
            name: name.to_string(),
            version: version.to_string(),
            description: None,
            authors: None,
            dependencies: deps,
            permissions: None,
            config_schema: None,
            entry_point: None,
        }
    }

    #[test]
    fn load_order_no_deps() {
        let mut loader = PluginLoader::new();
        loader.add_manifest(manifest("a", "1.0.0", None)).unwrap();
        assert_eq!(loader.resolve_load_order("a").unwrap(), vec!["a"]);
    }

    #[test]
    fn load_order_with_deps() {
        let mut loader = PluginLoader::new();
        loader
            .add_manifest(manifest("base", "1.0.0", None))
            .unwrap();
        let mut deps = HashMap::new();
        deps.insert("base".into(), "1.0.0".into());
        loader
            .add_manifest(manifest("plugin", "1.0.0", Some(deps)))
            .unwrap();

        let order = loader.resolve_load_order("plugin").unwrap();
        assert_eq!(order, vec!["base", "plugin"]);
    }

    #[test]
    fn resolve_all_deduplicates() {
        let mut loader = PluginLoader::new();
        loader.add_manifest(manifest("a", "1.0.0", None)).unwrap();
        let mut deps = HashMap::new();
        deps.insert("a".into(), "1.0.0".into());
        loader
            .add_manifest(manifest("b", "1.0.0", Some(deps)))
            .unwrap();

        let all = loader.resolve_all().unwrap();
        assert_eq!(all.len(), 2);
    }

    #[test]
    fn missing_dependency_fails() {
        let mut loader = PluginLoader::new();
        let mut deps = HashMap::new();
        deps.insert("missing".into(), "1.0.0".into());
        loader
            .add_manifest(manifest("a", "1.0.0", Some(deps)))
            .unwrap();

        assert!(loader.resolve_load_order("a").is_err());
    }

    #[test]
    fn get_plugin_returns_manifest() {
        let mut loader = PluginLoader::new();
        loader.add_manifest(manifest("a", "2.0.0", None)).unwrap();

        let m = loader.get_plugin("a").unwrap();
        assert_eq!(m.version, "2.0.0");
        assert!(loader.get_plugin("b").is_none());
    }
}
