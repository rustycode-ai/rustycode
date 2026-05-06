//! Marketplace registry management

use super::index::{ItemType, MarketplaceItem};
use anyhow::Result;
use std::collections::{HashMap, HashSet};

/// Registry validation errors
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum RegistryError {
    #[error("Item '{id}' not found in registry")]
    ItemNotFound { id: String },

    #[error("Circular dependency detected: {0}")]
    CircularDependency(String),

    #[error("Version conflict for '{item}': installed {installed}, required {required}")]
    VersionConflict {
        item: String,
        installed: String,
        required: String,
    },

    #[error("Missing dependency: {0}")]
    MissingDependency(String),

    #[error("Invalid item signature: {0}")]
    InvalidSignature(String),
}

/// Result of dependency resolution
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct ResolutionResult {
    /// Items to install in order
    pub install_order: Vec<String>,
    /// Items that need updates
    pub updates_needed: Vec<String>,
    /// Conflicts detected
    pub conflicts: Vec<String>,
    /// Warnings (non-fatal)
    pub warnings: Vec<String>,
}

impl ResolutionResult {
    pub fn is_successful(&self) -> bool {
        self.conflicts.is_empty()
    }
}

/// Registry manager for advanced operations
#[non_exhaustive]
pub struct RegistryManager {
    items: Vec<MarketplaceItem>,
    item_index: HashMap<String, usize>,
}

impl RegistryManager {
    pub fn new(items: Vec<MarketplaceItem>) -> Self {
        let mut item_index = HashMap::new();
        for (idx, item) in items.iter().enumerate() {
            item_index.insert(item.id.clone(), idx);
        }

        Self { items, item_index }
    }

    /// Get all registry items.
    pub fn items(&self) -> &[MarketplaceItem] {
        &self.items
    }

    /// Get an item by ID
    pub fn item(&self, id: &str) -> Option<&MarketplaceItem> {
        self.item_index.get(id).and_then(|&idx| self.items.get(idx))
    }

    /// Get all items of a specific type
    pub fn items_by_type(&self, item_type: &ItemType) -> Vec<&MarketplaceItem> {
        self.items
            .iter()
            .filter(|item| &item.item_type == item_type)
            .collect()
    }

    /// Get all items in a category
    pub fn items_by_category(&self, category: &str) -> Vec<&MarketplaceItem> {
        self.items
            .iter()
            .filter(|item| item.category.eq_ignore_ascii_case(category))
            .collect()
    }

    /// Resolve dependencies for an item
    pub fn resolve_dependencies(
        &self,
        item_id: &str,
        installed_versions: &HashMap<String, String>,
    ) -> Result<ResolutionResult> {
        let mut install_order = Vec::new();
        let mut updates_needed = Vec::new();
        let mut conflicts = Vec::new();
        let mut warnings = Vec::new();
        let mut visited = HashSet::new();
        let mut visiting = HashSet::new();

        self.resolve_dependencies_recursive(
            item_id,
            installed_versions,
            &mut install_order,
            &mut updates_needed,
            &mut conflicts,
            &mut warnings,
            &mut visited,
            &mut visiting,
        )?;

        Ok(ResolutionResult {
            install_order,
            updates_needed,
            conflicts,
            warnings,
        })
    }

    /// Recursively resolve dependencies
    #[allow(clippy::too_many_arguments)]
    fn resolve_dependencies_recursive(
        &self,
        item_id: &str,
        installed_versions: &HashMap<String, String>,
        install_order: &mut Vec<String>,
        updates_needed: &mut Vec<String>,
        conflicts: &mut Vec<String>,
        warnings: &mut Vec<String>,
        visited: &mut HashSet<String>,
        visiting: &mut HashSet<String>,
    ) -> Result<()> {
        // Check for circular dependencies
        if visiting.contains(item_id) {
            return Err(RegistryError::CircularDependency(item_id.to_string()).into());
        }

        // Skip if already visited
        if visited.contains(item_id) {
            return Ok(());
        }

        visiting.insert(item_id.to_string());

        // Get the item
        let item = self
            .item(item_id)
            .ok_or_else(|| RegistryError::ItemNotFound {
                id: item_id.to_string(),
            })?;

        // Process dependencies
        for dep_id in &item.dependencies {
            // Skip system dependencies (like @modelcontextprotocol/*)
            if dep_id.starts_with('@') || dep_id.starts_with("npm:") {
                warnings.push(format!("System dependency: {}", dep_id));
                continue;
            }

            // Check if dependency is in registry
            if let Some(dep_item) = self.item(dep_id) {
                // Check if installed version is compatible
                if let Some(installed_ver) = installed_versions.get(dep_id) {
                    if let Some(min_ver) = &dep_item.min_compatible_version {
                        if !is_version_compatible(installed_ver, min_ver) {
                            conflicts.push(format!(
                                "Dependency '{}': installed {} but requires at least {}",
                                dep_id, installed_ver, min_ver
                            ));
                            updates_needed.push(dep_id.clone());
                        }
                    }
                } else {
                    // Dependency not installed, add to install order
                    self.resolve_dependencies_recursive(
                        dep_id,
                        installed_versions,
                        install_order,
                        updates_needed,
                        conflicts,
                        warnings,
                        visited,
                        visiting,
                    )?;
                }
            } else {
                warnings.push(format!("Dependency '{}' not found in registry", dep_id));
            }
        }

        visiting.remove(item_id);
        visited.insert(item_id.to_string());
        install_order.push(item_id.to_string());

        Ok(())
    }

    /// Check for version conflicts among installed items
    pub fn check_conflicts(&self, installed: &HashMap<String, String>) -> Vec<String> {
        let mut conflicts = Vec::new();

        for (id, version) in installed {
            if let Some(item) = self.item(id) {
                // Check if installed version is compatible
                if let Some(min_ver) = &item.min_compatible_version {
                    if !is_version_compatible(version, min_ver) {
                        conflicts.push(format!(
                            "Item '{}': version {} is below minimum {}",
                            id, version, min_ver
                        ));
                    }
                }

                // Check dependencies
                for dep_id in &item.dependencies {
                    if let Some(dep_item) = self.item(dep_id) {
                        if let Some(dep_version) = installed.get(dep_id) {
                            if let Some(min_ver) = &dep_item.min_compatible_version {
                                if !is_version_compatible(dep_version, min_ver) {
                                    conflicts.push(format!(
                                        "Dependency '{}' for '{}': version {} is below minimum {}",
                                        dep_id, id, dep_version, min_ver
                                    ));
                                }
                            }
                        }
                    }
                }
            }
        }

        conflicts
    }

    /// Find items that have updates available
    pub fn find_updates(&self, installed: &HashMap<String, String>) -> Vec<&MarketplaceItem> {
        self.items
            .iter()
            .filter(|item| {
                if let Some(installed_ver) = installed.get(&item.id) {
                    installed_ver != &item.version
                } else {
                    false
                }
            })
            .collect()
    }

    /// Get statistics about the registry
    pub fn statistics(&self) -> RegistryStatistics {
        let mut by_type = HashMap::new();
        let mut by_category = HashMap::new();

        for item in &self.items {
            *by_type.entry(item.item_type.clone()).or_insert(0) += 1;
            *by_category.entry(item.category.clone()).or_insert(0) += 1;
        }

        RegistryStatistics {
            total_items: self.items.len(),
            by_type,
            by_category,
            installed_count: self.items.iter().filter(|i| i.installed).count(),
            total_downloads: self.items.iter().map(|i| i.downloads).sum(),
        }
    }

    /// Validate item structure and metadata
    pub fn validate_item(&self, item: &MarketplaceItem) -> Result<()> {
        if item.id.is_empty() {
            return Err(anyhow::anyhow!("Item ID cannot be empty"));
        }

        if item.name.is_empty() {
            return Err(anyhow::anyhow!("Item name cannot be empty"));
        }

        if item.url.is_empty() {
            return Err(anyhow::anyhow!("Item URL cannot be empty"));
        }

        // Validate URL format
        if !item.url.starts_with("http://") && !item.url.starts_with("https://") {
            return Err(anyhow::anyhow!("Item URL must be HTTP or HTTPS"));
        }

        // Validate version format (basic semver check)
        if !is_valid_version(&item.version) {
            return Err(anyhow::anyhow!("Invalid version format: {}", item.version));
        }

        // Validate rating range
        if item.rating < 0.0 || item.rating > 5.0 {
            return Err(anyhow::anyhow!("Rating must be between 0.0 and 5.0"));
        }

        Ok(())
    }

    /// Verify item signature
    ///
    /// **Note:** Signature verification is not yet implemented. This method
    /// logs a warning and returns `Ok(())`. Do not rely on it for security.
    pub fn verify_signature(&self, _item: &MarketplaceItem) -> Result<()> {
        tracing::warn!(
            "marketplace signature verification is not yet implemented — \
             item authenticity cannot be verified"
        );
        Ok(())
    }
}

/// Registry statistics
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct RegistryStatistics {
    pub total_items: usize,
    pub by_type: HashMap<ItemType, usize>,
    pub by_category: HashMap<String, usize>,
    pub installed_count: usize,
    pub total_downloads: usize,
}

fn is_version_compatible(installed: &str, minimum: &str) -> bool {
    let installed_parts: Vec<u32> = parse_version(installed);
    let minimum_parts: Vec<u32> = parse_version(minimum);

    for (i, min_part) in minimum_parts.iter().enumerate() {
        let installed_part = installed_parts.get(i).unwrap_or(&0);
        if installed_part < min_part {
            return false;
        }
        if installed_part > min_part {
            return true;
        }
    }

    true
}

/// Parse a version string into components
fn parse_version(version: &str) -> Vec<u32> {
    version.split('.').filter_map(|s| s.parse().ok()).collect()
}

/// Check if a version string is valid (basic semver format)
fn is_valid_version(version: &str) -> bool {
    // Split version to get the core numeric parts
    let core_version = version.split('-').next().unwrap_or(version);
    let parts: Vec<&str> = core_version.split('.').collect();

    // Need at least major.minor (e.g., "1.0")
    if parts.len() < 2 || parts.len() > 3 {
        return false;
    }

    // Check that core parts are numeric
    parts.iter().all(|p| p.chars().all(|c| c.is_ascii_digit()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_items() -> Vec<MarketplaceItem> {
        vec![
            MarketplaceItem {
                id: "base".to_string(),
                name: "Base".to_string(),
                description: "Base item".to_string(),
                item_type: ItemType::Skill,
                category: "Test".to_string(),
                version: "1.0.0".to_string(),
                author: "Test".to_string(),
                rating: 4.5,
                downloads: 100,
                url: "https://example.com/base".to_string(),
                installed: false,
                installed_version: None,
                homepage: None,
                tags: vec![],
                dependencies: vec![],
                min_compatible_version: None,
            },
            MarketplaceItem {
                id: "depends-on-base".to_string(),
                name: "Depends on Base".to_string(),
                description: "Item that depends on base".to_string(),
                item_type: ItemType::Skill,
                category: "Test".to_string(),
                version: "1.0.0".to_string(),
                author: "Test".to_string(),
                rating: 4.5,
                downloads: 50,
                url: "https://example.com/dep".to_string(),
                installed: false,
                installed_version: None,
                homepage: None,
                tags: vec![],
                dependencies: vec!["base".to_string()],
                min_compatible_version: None,
            },
        ]
    }

    #[test]
    fn test_registry_manager_creation() {
        let items = create_test_items();
        let manager = RegistryManager::new(items);

        assert_eq!(manager.items.len(), 2);
        assert_eq!(manager.item_index.len(), 2);
    }

    #[test]
    fn test_get_item() {
        let items = create_test_items();
        let manager = RegistryManager::new(items);

        let item = manager.item("base");
        assert!(item.is_some());
        assert_eq!(item.unwrap().id, "base");

        let missing = manager.item("nonexistent");
        assert!(missing.is_none());
    }

    #[test]
    fn test_get_items_by_type() {
        let items = create_test_items();
        let manager = RegistryManager::new(items);

        let skills = manager.items_by_type(&ItemType::Skill);
        assert_eq!(skills.len(), 2);

        let tools = manager.items_by_type(&ItemType::Tool);
        assert_eq!(tools.len(), 0);
    }

    #[test]
    fn test_resolve_dependencies() {
        let items = create_test_items();
        let manager = RegistryManager::new(items);

        let result = manager.resolve_dependencies("depends-on-base", &HashMap::new());

        assert!(result.is_ok());
        let resolution = result.unwrap();
        assert_eq!(resolution.install_order, vec!["base", "depends-on-base"]);
        assert!(resolution.is_successful());
    }

    #[test]
    fn test_circular_dependency_detection() {
        let mut items = create_test_items();
        // Add circular dependency
        items[0].dependencies = vec!["depends-on-base".to_string()];

        let manager = RegistryManager::new(items);
        let result = manager.resolve_dependencies("base", &HashMap::new());

        assert!(result.is_err());
    }

    #[test]
    fn test_validate_item_valid() {
        let items = create_test_items();
        let manager = RegistryManager::new(items);

        let item = manager.item("base").unwrap();
        assert!(manager.validate_item(item).is_ok());
    }

    #[test]
    fn test_validate_item_empty_id() {
        let mut item = create_test_items()[0].clone();
        item.id = String::new();

        let manager = RegistryManager::new(vec![item.clone()]);
        assert!(manager.validate_item(&item).is_err());
    }

    #[test]
    fn test_validate_item_invalid_url() {
        let mut item = create_test_items()[0].clone();
        item.url = "not-a-url".to_string();

        let manager = RegistryManager::new(vec![item.clone()]);
        assert!(manager.validate_item(&item).is_err());
    }

    #[test]
    fn test_is_version_compatible() {
        assert!(is_version_compatible("2.0.0", "1.0.0"));
        assert!(is_version_compatible("1.5.0", "1.0.0"));
        assert!(is_version_compatible("1.0.1", "1.0.0"));
        assert!(!is_version_compatible("0.9.0", "1.0.0"));
        assert!(is_version_compatible("1.0.0", "1.0.0"));
    }

    #[test]
    fn test_parse_version() {
        assert_eq!(parse_version("1.2.3"), vec![1, 2, 3]);
        assert_eq!(parse_version("2.0"), vec![2, 0]);
        assert_eq!(parse_version("invalid"), Vec::<u32>::new());
    }

    #[test]
    fn test_is_valid_version() {
        assert!(is_valid_version("1.0.0"));
        assert!(is_valid_version("2.1"));
        assert!(is_valid_version("1.0.0-beta"));
        assert!(!is_valid_version("1"));
        assert!(!is_valid_version("invalid"));
    }

    #[test]
    fn test_registry_statistics() {
        let items = create_test_items();
        let manager = RegistryManager::new(items);

        let stats = manager.statistics();
        assert_eq!(stats.total_items, 2);
        assert_eq!(stats.installed_count, 0);
        assert_eq!(stats.total_downloads, 150);
    }
}
