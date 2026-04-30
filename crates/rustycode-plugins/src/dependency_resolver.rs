//! Plugin dependency resolution and validation
//!
//! Handles dependency parsing, cycle detection, and version constraint validation.

use std::collections::{HashMap, HashSet};

use crate::error::PluginError;
use crate::manifest::{DependencySpec, PluginManifest};

/// Resolver for plugin dependencies
pub struct DependencyResolver {
    /// Available plugins and their manifests
    plugins: HashMap<String, PluginManifest>,
}

impl DependencyResolver {
    /// Create a new dependency resolver
    pub fn new() -> Self {
        Self {
            plugins: HashMap::new(),
        }
    }

    /// Register a plugin manifest with the resolver
    pub fn register(&mut self, manifest: PluginManifest) -> Result<(), PluginError> {
        manifest.validate()?;
        self.plugins.insert(manifest.name.clone(), manifest);
        Ok(())
    }

    /// Register multiple plugins at once
    pub fn register_all(&mut self, manifests: Vec<PluginManifest>) -> Result<(), PluginError> {
        for manifest in manifests {
            self.register(manifest)?;
        }
        Ok(())
    }

    /// Get a plugin manifest by name
    pub fn get_plugin(&self, name: &str) -> Option<&PluginManifest> {
        self.plugins.get(name)
    }

    /// Resolve dependencies for a plugin in load order
    ///
    /// Returns a list of plugin names in the order they should be loaded
    /// (dependencies before dependents).
    ///
    /// Returns an error if:
    /// - A required dependency is missing
    /// - A dependency version constraint cannot be satisfied
    /// - A circular dependency is detected
    pub fn resolve(&self, plugin_name: &str) -> Result<Vec<String>, PluginError> {
        // Check if plugin exists
        if !self.plugins.contains_key(plugin_name) {
            return Err(PluginError::not_found(plugin_name));
        }

        // Check for circular dependencies
        let mut visited = HashSet::new();
        let mut rec_stack = HashSet::new();
        self.check_cycles(plugin_name, &mut visited, &mut rec_stack)?;

        // Perform topological sort
        let mut visited = HashSet::new();
        let mut result = Vec::new();
        self.topological_sort(plugin_name, &mut visited, &mut result)?;

        Ok(result)
    }

    /// Detect circular dependencies using DFS
    fn check_cycles(
        &self,
        plugin_name: &str,
        visited: &mut HashSet<String>,
        rec_stack: &mut HashSet<String>,
    ) -> Result<(), PluginError> {
        visited.insert(plugin_name.to_string());
        rec_stack.insert(plugin_name.to_string());

        if let Some(manifest) = self.plugins.get(plugin_name) {
            for dep_name in manifest.get_dependencies() {
                if !visited.contains(dep_name) {
                    self.check_cycles(dep_name, visited, rec_stack)?;
                } else if rec_stack.contains(dep_name) {
                    // Found a cycle, try to construct the cycle path
                    let cycle_path = self.find_cycle_path(plugin_name, dep_name);
                    return Err(PluginError::LoadingFailed {
                        reason: format!("circular dependency detected: {cycle_path}"),
                    });
                }
            }
        }

        rec_stack.remove(plugin_name);
        Ok(())
    }

    /// Reconstruct the cycle path for error reporting
    fn find_cycle_path(&self, from: &str, to: &str) -> String {
        let mut path = vec![from.to_string()];
        let mut visited = HashSet::new();
        self.build_path(from, to, &mut path, &mut visited);
        path.join(" -> ")
    }

    fn build_path(
        &self,
        current: &str,
        target: &str,
        path: &mut Vec<String>,
        visited: &mut HashSet<String>,
    ) -> bool {
        if current == target {
            path.push(target.to_string());
            return true;
        }

        if visited.contains(current) {
            return false;
        }

        visited.insert(current.to_string());

        if let Some(manifest) = self.plugins.get(current) {
            for dep_name in manifest.get_dependencies() {
                path.push(dep_name.to_string());
                if self.build_path(dep_name, target, path, visited) {
                    return true;
                }
                path.pop();
            }
        }

        false
    }

    /// Topological sort with DFS
    fn topological_sort(
        &self,
        plugin_name: &str,
        visited: &mut HashSet<String>,
        result: &mut Vec<String>,
    ) -> Result<(), PluginError> {
        if visited.contains(plugin_name) {
            return Ok(());
        }

        visited.insert(plugin_name.to_string());

        if let Some(manifest) = self.plugins.get(plugin_name) {
            for dep_name in manifest.get_dependencies() {
                // Validate dependency exists
                if !self.plugins.contains_key(dep_name) {
                    return Err(PluginError::missing_dependency(plugin_name, dep_name));
                }

                // Validate version if constraint is specified
                let dep_manifest = &self.plugins[dep_name];
                if let Some(version_spec) = manifest.get_dependency_version(dep_name) {
                    let spec = DependencySpec::parse_from_str(version_spec)?;
                    if !spec.satisfies(&dep_manifest.version) {
                        return Err(PluginError::version_mismatch(
                            plugin_name,
                            format!(
                                "dependency {} requires {}, but have {}",
                                dep_name, version_spec, dep_manifest.version
                            ),
                        ));
                    }
                }

                // Recursively resolve
                self.topological_sort(dep_name, visited, result)?;
            }
        }

        result.push(plugin_name.to_string());
        Ok(())
    }

    /// Validate all registered plugins
    ///
    /// Checks that:
    /// - All manifests are valid
    /// - All dependencies can be resolved
    /// - No circular dependencies exist
    /// - All version constraints are satisfiable
    pub fn validate_all(&self) -> Result<Vec<String>, PluginError> {
        let mut all_plugins = Vec::new();

        for plugin_name in self.plugins.keys() {
            let order = self.resolve(plugin_name)?;
            all_plugins.extend(order);
        }

        // Deduplicate while preserving order
        let mut seen = HashSet::new();
        all_plugins.retain(|p| seen.insert(p.clone()));

        Ok(all_plugins)
    }
}

impl Default for DependencyResolver {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::PluginManifest;

    fn create_manifest(
        name: &str,
        version: &str,
        dependencies: Option<HashMap<String, String>>,
    ) -> PluginManifest {
        PluginManifest {
            name: name.to_string(),
            version: version.to_string(),
            description: None,
            authors: None,
            dependencies,
            permissions: None,
            config_schema: None,
            entry_point: None,
        }
    }

    #[test]
    fn test_resolver_new() {
        let resolver = DependencyResolver::new();
        assert_eq!(resolver.plugins.len(), 0);
    }

    #[test]
    fn test_resolver_register_single() {
        let mut resolver = DependencyResolver::new();
        let manifest = create_manifest("plugin_a", "1.0.0", None);
        assert!(resolver.register(manifest).is_ok());
        assert!(resolver.get_plugin("plugin_a").is_some());
    }

    #[test]
    fn test_resolver_register_multiple() {
        let mut resolver = DependencyResolver::new();
        let manifests = vec![
            create_manifest("plugin_a", "1.0.0", None),
            create_manifest("plugin_b", "1.0.0", None),
        ];
        assert!(resolver.register_all(manifests).is_ok());
        assert!(resolver.get_plugin("plugin_a").is_some());
        assert!(resolver.get_plugin("plugin_b").is_some());
    }

    #[test]
    fn test_resolver_single_plugin_no_deps() {
        let mut resolver = DependencyResolver::new();
        resolver
            .register(create_manifest("plugin_a", "1.0.0", None))
            .unwrap();

        let order = resolver.resolve("plugin_a").unwrap();
        assert_eq!(order, vec!["plugin_a"]);
    }

    #[test]
    fn test_resolver_linear_dependencies() {
        let mut resolver = DependencyResolver::new();

        // plugin_c depends on plugin_b depends on plugin_a
        resolver
            .register(create_manifest("plugin_a", "1.0.0", None))
            .unwrap();

        let mut deps = HashMap::new();
        deps.insert("plugin_a".to_string(), "1.0.0".to_string());
        resolver
            .register(create_manifest("plugin_b", "1.0.0", Some(deps)))
            .unwrap();

        let mut deps = HashMap::new();
        deps.insert("plugin_b".to_string(), "1.0.0".to_string());
        resolver
            .register(create_manifest("plugin_c", "1.0.0", Some(deps)))
            .unwrap();

        let order = resolver.resolve("plugin_c").unwrap();
        assert_eq!(order, vec!["plugin_a", "plugin_b", "plugin_c"]);
    }

    #[test]
    fn test_resolver_multiple_dependencies() {
        let mut resolver = DependencyResolver::new();

        resolver
            .register(create_manifest("plugin_a", "1.0.0", None))
            .unwrap();
        resolver
            .register(create_manifest("plugin_b", "1.0.0", None))
            .unwrap();

        let mut deps = HashMap::new();
        deps.insert("plugin_a".to_string(), "1.0.0".to_string());
        deps.insert("plugin_b".to_string(), "1.0.0".to_string());
        resolver
            .register(create_manifest("plugin_c", "1.0.0", Some(deps)))
            .unwrap();

        let order = resolver.resolve("plugin_c").unwrap();
        assert_eq!(order.len(), 3);
        assert!(order.contains(&"plugin_a".to_string()));
        assert!(order.contains(&"plugin_b".to_string()));
        assert!(order.contains(&"plugin_c".to_string()));
        assert_eq!(order.last().unwrap(), "plugin_c");
    }

    #[test]
    fn test_resolver_circular_dependency() {
        let mut resolver = DependencyResolver::new();

        let mut deps_a = HashMap::new();
        deps_a.insert("plugin_b".to_string(), "1.0.0".to_string());
        resolver
            .register(create_manifest("plugin_a", "1.0.0", Some(deps_a)))
            .unwrap();

        let mut deps_b = HashMap::new();
        deps_b.insert("plugin_a".to_string(), "1.0.0".to_string());
        resolver
            .register(create_manifest("plugin_b", "1.0.0", Some(deps_b)))
            .unwrap();

        let result = resolver.resolve("plugin_a");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("circular"));
    }

    #[test]
    fn test_resolver_circular_dependency_three_way() {
        let mut resolver = DependencyResolver::new();

        let mut deps_a = HashMap::new();
        deps_a.insert("plugin_b".to_string(), "1.0.0".to_string());
        resolver
            .register(create_manifest("plugin_a", "1.0.0", Some(deps_a)))
            .unwrap();

        let mut deps_b = HashMap::new();
        deps_b.insert("plugin_c".to_string(), "1.0.0".to_string());
        resolver
            .register(create_manifest("plugin_b", "1.0.0", Some(deps_b)))
            .unwrap();

        let mut deps_c = HashMap::new();
        deps_c.insert("plugin_a".to_string(), "1.0.0".to_string());
        resolver
            .register(create_manifest("plugin_c", "1.0.0", Some(deps_c)))
            .unwrap();

        let result = resolver.resolve("plugin_a");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("circular"));
    }

    #[test]
    fn test_resolver_missing_dependency() {
        let mut resolver = DependencyResolver::new();

        let mut deps = HashMap::new();
        deps.insert("missing_plugin".to_string(), "1.0.0".to_string());
        resolver
            .register(create_manifest("plugin_a", "1.0.0", Some(deps)))
            .unwrap();

        let result = resolver.resolve("plugin_a");
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("missing dependency"));
    }

    #[test]
    fn test_resolver_version_mismatch() {
        let mut resolver = DependencyResolver::new();

        resolver
            .register(create_manifest("plugin_a", "1.0.0", None))
            .unwrap();

        let mut deps = HashMap::new();
        deps.insert("plugin_a".to_string(), "2.0.0".to_string());
        resolver
            .register(create_manifest("plugin_b", "1.0.0", Some(deps)))
            .unwrap();

        let result = resolver.resolve("plugin_b");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("version mismatch"));
    }

    #[test]
    fn test_resolver_version_spec_exact() {
        let mut resolver = DependencyResolver::new();

        resolver
            .register(create_manifest("plugin_a", "1.5.3", None))
            .unwrap();

        let mut deps = HashMap::new();
        deps.insert("plugin_a".to_string(), "1.5.3".to_string());
        resolver
            .register(create_manifest("plugin_b", "1.0.0", Some(deps)))
            .unwrap();

        let order = resolver.resolve("plugin_b").unwrap();
        assert_eq!(order, vec!["plugin_a", "plugin_b"]);
    }

    #[test]
    fn test_resolver_version_spec_caret() {
        let mut resolver = DependencyResolver::new();

        resolver
            .register(create_manifest("plugin_a", "1.5.3", None))
            .unwrap();

        let mut deps = HashMap::new();
        deps.insert("plugin_a".to_string(), "^1.0.0".to_string());
        resolver
            .register(create_manifest("plugin_b", "1.0.0", Some(deps)))
            .unwrap();

        let order = resolver.resolve("plugin_b").unwrap();
        assert_eq!(order, vec!["plugin_a", "plugin_b"]);
    }

    #[test]
    fn test_resolver_version_spec_greater_or_equal() {
        let mut resolver = DependencyResolver::new();

        resolver
            .register(create_manifest("plugin_a", "2.0.0", None))
            .unwrap();

        let mut deps = HashMap::new();
        deps.insert("plugin_a".to_string(), ">=1.0.0".to_string());
        resolver
            .register(create_manifest("plugin_b", "1.0.0", Some(deps)))
            .unwrap();

        let order = resolver.resolve("plugin_b").unwrap();
        assert_eq!(order, vec!["plugin_a", "plugin_b"]);
    }

    #[test]
    fn test_resolver_resolve_nonexistent_plugin() {
        let resolver = DependencyResolver::new();
        let result = resolver.resolve("nonexistent");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));
    }

    #[test]
    fn test_resolver_validate_all() {
        let mut resolver = DependencyResolver::new();

        resolver
            .register(create_manifest("plugin_a", "1.0.0", None))
            .unwrap();

        let mut deps = HashMap::new();
        deps.insert("plugin_a".to_string(), "1.0.0".to_string());
        resolver
            .register(create_manifest("plugin_b", "1.0.0", Some(deps)))
            .unwrap();

        let all = resolver.validate_all().unwrap();
        assert_eq!(all.len(), 2);
    }

    #[test]
    fn test_resolver_complex_dependency_graph() {
        let mut resolver = DependencyResolver::new();

        // Build a DAG: a, b <- c <- d
        resolver
            .register(create_manifest("plugin_a", "1.0.0", None))
            .unwrap();
        resolver
            .register(create_manifest("plugin_b", "1.0.0", None))
            .unwrap();

        let mut deps_c = HashMap::new();
        deps_c.insert("plugin_a".to_string(), "1.0.0".to_string());
        deps_c.insert("plugin_b".to_string(), "1.0.0".to_string());
        resolver
            .register(create_manifest("plugin_c", "1.0.0", Some(deps_c)))
            .unwrap();

        let mut deps_d = HashMap::new();
        deps_d.insert("plugin_c".to_string(), "1.0.0".to_string());
        resolver
            .register(create_manifest("plugin_d", "1.0.0", Some(deps_d)))
            .unwrap();

        let order = resolver.resolve("plugin_d").unwrap();
        assert_eq!(order.len(), 4);
        assert_eq!(order.last().unwrap(), "plugin_d");

        // Check that c comes after a and b
        let a_idx = order.iter().position(|p| p == "plugin_a").unwrap();
        let b_idx = order.iter().position(|p| p == "plugin_b").unwrap();
        let c_idx = order.iter().position(|p| p == "plugin_c").unwrap();
        let d_idx = order.iter().position(|p| p == "plugin_d").unwrap();

        assert!(a_idx < c_idx);
        assert!(b_idx < c_idx);
        assert!(c_idx < d_idx);
    }
}
