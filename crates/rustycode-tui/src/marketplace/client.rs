//! Marketplace client for fetching and searching items
//!
//! Provides functions to fetch the marketplace index from remote sources
//! or use built-in index, and search functionality.

use super::index::{ItemType, MarketplaceItem};
use anyhow::{Context, Result};
use std::fs;
use std::path::PathBuf;
use std::time::Duration;

/// Default remote registry URL
const DEFAULT_REGISTRY_URL: &str =
    "https://raw.githubusercontent.com/anthropics/claude-marketplace/main/index.json";

/// Cache file name for the registry
const REGISTRY_CACHE_FILE: &str = "marketplace-index.json";

/// Cache TTL (24 hours)
const CACHE_TTL: Duration = Duration::from_hours(24);

/// Marketplace registry configuration
#[derive(Debug, Clone)]
pub struct RegistryConfig {
    /// Remote URL for the registry
    pub registry_url: String,
    /// Whether to use the remote registry
    pub use_remote: bool,
    /// Cache TTL
    pub cache_ttl: Duration,
    /// Force refresh (bypass cache)
    pub force_refresh: bool,
}

impl Default for RegistryConfig {
    fn default() -> Self {
        Self {
            registry_url: DEFAULT_REGISTRY_URL.to_string(),
            use_remote: true,
            cache_ttl: CACHE_TTL,
            force_refresh: false,
        }
    }
}

impl RegistryConfig {
    /// Create a new config with a custom registry URL
    pub fn with_url(mut self, url: impl Into<String>) -> Self {
        self.registry_url = url.into();
        self
    }

    /// Create a config that only uses built-in index
    pub fn builtin_only() -> Self {
        Self {
            registry_url: DEFAULT_REGISTRY_URL.to_string(),
            use_remote: false,
            cache_ttl: CACHE_TTL,
            force_refresh: false,
        }
    }

    /// Create a config that forces a refresh
    pub fn force_refresh(mut self) -> Self {
        self.force_refresh = true;
        self
    }
}

/// Cached registry metadata
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct RegistryCache {
    /// Items in the cache
    items: Vec<MarketplaceItem>,
    /// When the cache was created
    cached_at: chrono::DateTime<chrono::Utc>,
    /// Cache version (for format changes)
    version: u32,
}

impl RegistryCache {
    /// Check if the cache is still valid
    fn is_valid(&self, ttl: Duration) -> bool {
        let age: chrono::Duration = chrono::Utc::now() - self.cached_at;
        let age_duration: Duration = age.to_std().unwrap_or(Duration::ZERO);
        age_duration < ttl
    }
}

/// Fetch marketplace index from remote or built-in sources
pub async fn fetch_marketplace_index() -> Result<Vec<MarketplaceItem>> {
    fetch_marketplace_index_with_config(RegistryConfig::default()).await
}

/// Fetch marketplace index with custom configuration
pub async fn fetch_marketplace_index_with_config(
    config: RegistryConfig,
) -> Result<Vec<MarketplaceItem>> {
    if !config.use_remote {
        return Ok(builtin_index());
    }

    // Check cache first
    let cache_path = get_cache_path()?;

    if !config.force_refresh {
        if let Ok(cached) = load_cache(&cache_path) {
            if cached.is_valid(config.cache_ttl) {
                tracing::debug!("Using cached marketplace index");
                return Ok(cached.items);
            }
        }
    }

    // Fetch from remote
    match fetch_remote_registry(&config.registry_url).await {
        Ok(items) => {
            // Update cache
            if let Err(e) = save_cache(&cache_path, &items) {
                tracing::warn!("Failed to cache marketplace index: {}", e);
            }
            Ok(items)
        }
        Err(e) => {
            tracing::warn!(
                "Failed to fetch remote registry: {}, falling back to cache",
                e
            );

            // Try to use stale cache
            if let Ok(cached) = load_cache(&cache_path) {
                tracing::info!("Using stale cache");
                return Ok(cached.items);
            }

            // Fall back to built-in
            tracing::info!("Using built-in index");
            Ok(builtin_index())
        }
    }
}

/// Fetch registry from a remote URL
async fn fetch_remote_registry(url: &str) -> Result<Vec<MarketplaceItem>> {
    tracing::info!("Fetching marketplace index from {}", url);

    // Use HTTP client with timeout
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .context("Failed to build HTTP client")?;

    let response = client
        .get(url)
        .send()
        .await
        .context("Failed to fetch registry")?;

    if !response.status().is_success() {
        let status = response.status();
        let error_text = response
            .text()
            .await
            .unwrap_or_else(|_| "unable to read error".to_string());
        return Err(anyhow::anyhow!(
            "Registry request failed: {} - {}",
            status,
            error_text
        ));
    }

    let items: Vec<MarketplaceItem> = response
        .json()
        .await
        .context("Failed to parse registry JSON")?;

    // Verify the registry
    verify_registry(&items)?;

    Ok(items)
}

/// Verify the integrity of a registry
fn verify_registry(items: &[MarketplaceItem]) -> Result<()> {
    // Basic validation
    for item in items {
        if item.id.is_empty() {
            return Err(anyhow::anyhow!("Registry contains item with empty ID"));
        }
        if item.name.is_empty() {
            return Err(anyhow::anyhow!("Registry contains item with empty name"));
        }
        if item.url.is_empty() {
            return Err(anyhow::anyhow!(
                "Registry contains item '{}' with empty URL",
                item.id
            ));
        }
    }

    tracing::debug!("Verified {} items in registry", items.len());
    Ok(())
}

/// Load cached registry from disk
fn load_cache(path: &PathBuf) -> Result<RegistryCache> {
    let content = fs::read_to_string(path).context("Failed to read cache file")?;
    let cache: RegistryCache =
        serde_json::from_str(&content).context("Failed to parse cache file")?;
    Ok(cache)
}

/// Save registry to cache
fn save_cache(path: &PathBuf, items: &[MarketplaceItem]) -> Result<()> {
    // Ensure parent directory exists
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).context("Failed to create cache directory")?;
    }

    let cache = RegistryCache {
        items: items.to_vec(),
        cached_at: chrono::Utc::now(),
        version: 1,
    };

    let content = serde_json::to_string_pretty(&cache).context("Failed to serialize cache")?;

    fs::write(path, content).context("Failed to write cache file")?;

    tracing::debug!("Cached marketplace index to {}", path.display());
    Ok(())
}

/// Get the cache file path
fn get_cache_path() -> Result<PathBuf> {
    let cache_dir = dirs::cache_dir()
        .or_else(|| dirs::home_dir().map(|h| h.join(".cache")))
        .context("Failed to determine cache directory")?;

    Ok(cache_dir.join("claude-code").join(REGISTRY_CACHE_FILE))
}

/// Clear the registry cache
pub fn clear_registry_cache() -> Result<()> {
    let cache_path = get_cache_path()?;
    if cache_path.exists() {
        fs::remove_file(&cache_path).context("Failed to remove cache file")?;
        tracing::info!("Cleared marketplace registry cache");
    }
    Ok(())
}

/// Search marketplace items by query
pub fn search_marketplace(items: &[MarketplaceItem], query: &str) -> Vec<MarketplaceItem> {
    let query_lower = query.to_lowercase();

    items
        .iter()
        .filter(|item| {
            // Search in name
            item.name.to_lowercase().contains(&query_lower)
            // Search in description
            || item.description.to_lowercase().contains(&query_lower)
            // Search in category
            || item.category.to_lowercase().contains(&query_lower)
            // Search in author
            || item.author.to_lowercase().contains(&query_lower)
            // Search in tags
            || item.tags.iter().any(|tag| tag.to_lowercase().contains(&query_lower))
        })
        .cloned()
        .collect()
}

/// Filter marketplace items by type
pub fn filter_by_type(items: &[MarketplaceItem], item_type: &ItemType) -> Vec<MarketplaceItem> {
    items
        .iter()
        .filter(|item| item.item_type == *item_type)
        .cloned()
        .collect()
}

/// Filter marketplace items by category
pub fn filter_by_category(items: &[MarketplaceItem], category: &str) -> Vec<MarketplaceItem> {
    items
        .iter()
        .filter(|item| item.category.to_lowercase() == category.to_lowercase())
        .cloned()
        .collect()
}

/// Get installed items
pub fn installed_items(items: &[MarketplaceItem]) -> Vec<MarketplaceItem> {
    items
        .iter()
        .filter(|item| item.installed)
        .cloned()
        .collect()
}

/// Get items with available updates
pub fn updatable_items(items: &[MarketplaceItem]) -> Vec<MarketplaceItem> {
    items
        .iter()
        .filter(|item| item.installed && item.has_update())
        .cloned()
        .collect()
}

/// Built-in marketplace index with popular items
fn builtin_index() -> Vec<MarketplaceItem> {
    vec![
        // Skills
        MarketplaceItem {
            id: "code-review".to_string(),
            name: "code-review".to_string(),
            description: "Review code for quality, security, and best practices".to_string(),
            item_type: ItemType::Skill,
            category: "Agent".to_string(),
            version: "1.2.0".to_string(),
            author: "Claude Official".to_string(),
            rating: 4.8,
            downloads: 12_500,
            url: "https://github.com/anthropics/claude-code-skills".to_string(),
            installed: false,
            installed_version: None,
            homepage: Some("https://docs.anthropic.com".to_string()),
            tags: vec![
                "review".to_string(),
                "quality".to_string(),
                "security".to_string(),
            ],
            dependencies: vec![],
            min_compatible_version: Some("1.0.0".to_string()),
        },
        MarketplaceItem {
            id: "tdd-guide".to_string(),
            name: "tdd-guide".to_string(),
            description: "Test-driven development workflow assistant".to_string(),
            item_type: ItemType::Skill,
            category: "Agent".to_string(),
            version: "1.0.5".to_string(),
            author: "Community".to_string(),
            rating: 4.6,
            downloads: 8_200,
            url: "https://github.com/example/tdd-skill".to_string(),
            installed: false,
            installed_version: None,
            homepage: None,
            tags: vec![
                "testing".to_string(),
                "tdd".to_string(),
                "workflow".to_string(),
            ],
            dependencies: vec![],
            min_compatible_version: None,
        },
        MarketplaceItem {
            id: "architect".to_string(),
            name: "architect".to_string(),
            description: "System design and architecture planning agent".to_string(),
            item_type: ItemType::Skill,
            category: "Agent".to_string(),
            version: "2.0.1".to_string(),
            author: "Claude Official".to_string(),
            rating: 4.9,
            downloads: 15_300,
            url: "https://github.com/anthropics/architect-skill".to_string(),
            installed: false,
            installed_version: None,
            homepage: Some("https://docs.anthropic.com".to_string()),
            tags: vec![
                "architecture".to_string(),
                "design".to_string(),
                "planning".to_string(),
            ],
            dependencies: vec![],
            min_compatible_version: Some("1.5.0".to_string()),
        },
        MarketplaceItem {
            id: "security-reviewer".to_string(),
            name: "security-reviewer".to_string(),
            description: "Security analysis and vulnerability detection".to_string(),
            item_type: ItemType::Skill,
            category: "Agent".to_string(),
            version: "1.1.0".to_string(),
            author: "Security Team".to_string(),
            rating: 4.7,
            downloads: 6_800,
            url: "https://github.com/security/reviewer-skill".to_string(),
            installed: false,
            installed_version: None,
            homepage: None,
            tags: vec![
                "security".to_string(),
                "audit".to_string(),
                "vulnerability".to_string(),
            ],
            dependencies: vec![],
            min_compatible_version: None,
        },
        MarketplaceItem {
            id: "refactor-cleaner".to_string(),
            name: "refactor-cleaner".to_string(),
            description: "Dead code cleanup and refactoring assistant".to_string(),
            item_type: ItemType::Skill,
            category: "Agent".to_string(),
            version: "1.0.3".to_string(),
            author: "Community".to_string(),
            rating: 4.4,
            downloads: 4_200,
            url: "https://github.com/example/refactor-skill".to_string(),
            installed: false,
            installed_version: None,
            homepage: None,
            tags: vec![
                "refactor".to_string(),
                "cleanup".to_string(),
                "optimization".to_string(),
            ],
            dependencies: vec![],
            min_compatible_version: None,
        },
        // Tools
        MarketplaceItem {
            id: "file-watcher".to_string(),
            name: "file-watcher".to_string(),
            description: "Watch files and auto-run commands on changes".to_string(),
            item_type: ItemType::Tool,
            category: "Developer".to_string(),
            version: "2.1.0".to_string(),
            author: "DevTools".to_string(),
            rating: 4.5,
            downloads: 9_100,
            url: "https://github.com/tools/file-watcher".to_string(),
            installed: false,
            installed_version: None,
            homepage: Some("https://filewatcher.dev".to_string()),
            tags: vec![
                "watch".to_string(),
                "automation".to_string(),
                "build".to_string(),
            ],
            dependencies: vec!["notify".to_string()],
            min_compatible_version: None,
        },
        MarketplaceItem {
            id: "git-integration".to_string(),
            name: "git-integration".to_string(),
            description: "Enhanced Git integration with advanced features".to_string(),
            item_type: ItemType::Tool,
            category: "Developer".to_string(),
            version: "1.5.2".to_string(),
            author: "GitTools".to_string(),
            rating: 4.8,
            downloads: 18_700,
            url: "https://github.com/tools/git-integration".to_string(),
            installed: false,
            installed_version: None,
            homepage: None,
            tags: vec![
                "git".to_string(),
                "version-control".to_string(),
                "workflow".to_string(),
            ],
            dependencies: vec!["git2".to_string()],
            min_compatible_version: Some("1.0.0".to_string()),
        },
        MarketplaceItem {
            id: "docker-manager".to_string(),
            name: "docker-manager".to_string(),
            description: "Docker container management interface".to_string(),
            item_type: ItemType::Tool,
            category: "DevOps".to_string(),
            version: "1.2.0".to_string(),
            author: "DevOps Team".to_string(),
            rating: 4.6,
            downloads: 7_300,
            url: "https://github.com/tools/docker-manager".to_string(),
            installed: false,
            installed_version: None,
            homepage: None,
            tags: vec![
                "docker".to_string(),
                "containers".to_string(),
                "devops".to_string(),
            ],
            dependencies: vec!["docker".to_string()],
            min_compatible_version: None,
        },
        MarketplaceItem {
            id: "database-client".to_string(),
            name: "database-client".to_string(),
            description: "Universal database client and query tool".to_string(),
            item_type: ItemType::Tool,
            category: "Database".to_string(),
            version: "1.0.8".to_string(),
            author: "DBTools".to_string(),
            rating: 4.4,
            downloads: 5_600,
            url: "https://github.com/tools/db-client".to_string(),
            installed: false,
            installed_version: None,
            homepage: None,
            tags: vec![
                "database".to_string(),
                "sql".to_string(),
                "query".to_string(),
            ],
            dependencies: vec!["sqlx".to_string()],
            min_compatible_version: None,
        },
        // MCP Servers
        MarketplaceItem {
            id: "github-mcp".to_string(),
            name: "github-mcp".to_string(),
            description: "GitHub integration MCP server".to_string(),
            item_type: ItemType::MCP,
            category: "Integration".to_string(),
            version: "1.3.0".to_string(),
            author: "ModelContext".to_string(),
            rating: 4.9,
            downloads: 22_400,
            url: "https://github.com/mcp/github-server".to_string(),
            installed: false,
            installed_version: None,
            homepage: Some("https://modelcontextprotocol.com".to_string()),
            tags: vec![
                "github".to_string(),
                "integration".to_string(),
                "mcp".to_string(),
            ],
            dependencies: vec!["@modelcontextprotocol/server-github".to_string()],
            min_compatible_version: Some("1.0.0".to_string()),
        },
        MarketplaceItem {
            id: "filesystem-mcp".to_string(),
            name: "filesystem-mcp".to_string(),
            description: "Filesystem access and management MCP server".to_string(),
            item_type: ItemType::MCP,
            category: "Integration".to_string(),
            version: "1.1.0".to_string(),
            author: "ModelContext".to_string(),
            rating: 4.7,
            downloads: 14_800,
            url: "https://github.com/mcp/filesystem-server".to_string(),
            installed: false,
            installed_version: None,
            homepage: None,
            tags: vec![
                "filesystem".to_string(),
                "files".to_string(),
                "mcp".to_string(),
            ],
            dependencies: vec!["@modelcontextprotocol/server-filesystem".to_string()],
            min_compatible_version: None,
        },
        MarketplaceItem {
            id: "postgres-mcp".to_string(),
            name: "postgres-mcp".to_string(),
            description: "PostgreSQL database MCP server".to_string(),
            item_type: ItemType::MCP,
            category: "Database".to_string(),
            version: "1.0.5".to_string(),
            author: "Database MCP".to_string(),
            rating: 4.6,
            downloads: 8_900,
            url: "https://github.com/mcp/postgres-server".to_string(),
            installed: false,
            installed_version: None,
            homepage: None,
            tags: vec![
                "postgres".to_string(),
                "database".to_string(),
                "sql".to_string(),
                "mcp".to_string(),
            ],
            dependencies: vec!["@modelcontextprotocol/server-postgres".to_string()],
            min_compatible_version: None,
        },
        MarketplaceItem {
            id: "slack-mcp".to_string(),
            name: "slack-mcp".to_string(),
            description: "Slack integration MCP server".to_string(),
            item_type: ItemType::MCP,
            category: "Integration".to_string(),
            version: "1.0.2".to_string(),
            author: "ModelContext".to_string(),
            rating: 4.5,
            downloads: 6_200,
            url: "https://github.com/mcp/slack-server".to_string(),
            installed: false,
            installed_version: None,
            homepage: None,
            tags: vec![
                "slack".to_string(),
                "communication".to_string(),
                "mcp".to_string(),
            ],
            dependencies: vec!["@modelcontextprotocol/server-slack".to_string()],
            min_compatible_version: None,
        },
        MarketplaceItem {
            id: "brave-search-mcp".to_string(),
            name: "brave-search-mcp".to_string(),
            description: "Brave search integration MCP server".to_string(),
            item_type: ItemType::MCP,
            category: "Search".to_string(),
            version: "1.2.0".to_string(),
            author: "ModelContext".to_string(),
            rating: 4.8,
            downloads: 11_300,
            url: "https://github.com/mcp/brave-search-server".to_string(),
            installed: false,
            installed_version: None,
            homepage: Some("https://search.brave.com".to_string()),
            tags: vec![
                "search".to_string(),
                "brave".to_string(),
                "web".to_string(),
                "mcp".to_string(),
            ],
            dependencies: vec!["@modelcontextprotocol/server-brave-search".to_string()],
            min_compatible_version: None,
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_search_by_name() {
        let items = builtin_index();
        let results = search_marketplace(&items, "git");
        assert!(!results.is_empty());
        assert!(results.iter().any(|item| item.id.contains("git")));
    }

    #[test]
    fn test_search_by_description() {
        let items = builtin_index();
        let results = search_marketplace(&items, "security");
        assert!(!results.is_empty());
    }

    #[test]
    fn test_filter_by_type() {
        let items = builtin_index();
        let skills = filter_by_type(&items, &ItemType::Skill);
        assert!(!skills.is_empty());
        assert!(skills.iter().all(|item| item.item_type == ItemType::Skill));
    }

    #[test]
    fn test_filter_by_category() {
        let items = builtin_index();
        let agents = filter_by_category(&items, "Agent");
        assert!(!agents.is_empty());
        assert!(agents.iter().all(|item| item.category == "Agent"));
    }

    #[test]
    fn test_builtin_index_not_empty() {
        let items = builtin_index();
        assert!(!items.is_empty());
        assert!(items.len() >= 10); // Should have at least 10 items
    }

    #[test]
    fn test_registry_config_default() {
        let config = RegistryConfig::default();
        assert!(config.use_remote);
        assert_eq!(config.registry_url, DEFAULT_REGISTRY_URL);
        assert!(!config.force_refresh);
    }

    #[test]
    fn test_registry_config_builtin_only() {
        let config = RegistryConfig::builtin_only();
        assert!(!config.use_remote);
    }

    #[test]
    fn test_registry_config_with_url() {
        let config = RegistryConfig::default().with_url("https://example.com/registry.json");
        assert_eq!(config.registry_url, "https://example.com/registry.json");
    }

    #[test]
    fn test_registry_config_force_refresh() {
        let config = RegistryConfig::default().force_refresh();
        assert!(config.force_refresh);
    }

    #[test]
    fn test_verify_registry_valid() {
        let items = vec![MarketplaceItem {
            id: "test-item".to_string(),
            name: "Test Item".to_string(),
            description: "A test item".to_string(),
            item_type: ItemType::Skill,
            category: "Test".to_string(),
            version: "1.0.0".to_string(),
            author: "Test Author".to_string(),
            rating: 4.5,
            downloads: 100,
            url: "https://example.com".to_string(),
            installed: false,
            installed_version: None,
            homepage: None,
            tags: vec![],
            dependencies: vec![],
            min_compatible_version: None,
        }];

        assert!(verify_registry(&items).is_ok());
    }

    #[test]
    fn test_verify_registry_empty_id() {
        let items = vec![MarketplaceItem {
            id: "".to_string(),
            name: "Test Item".to_string(),
            description: "A test item".to_string(),
            item_type: ItemType::Skill,
            category: "Test".to_string(),
            version: "1.0.0".to_string(),
            author: "Test Author".to_string(),
            rating: 4.5,
            downloads: 100,
            url: "https://example.com".to_string(),
            installed: false,
            installed_version: None,
            homepage: None,
            tags: vec![],
            dependencies: vec![],
            min_compatible_version: None,
        }];

        assert!(verify_registry(&items).is_err());
    }

    #[test]
    fn test_verify_registry_empty_url() {
        let items = vec![MarketplaceItem {
            id: "test-item".to_string(),
            name: "Test Item".to_string(),
            description: "A test item".to_string(),
            item_type: ItemType::Skill,
            category: "Test".to_string(),
            version: "1.0.0".to_string(),
            author: "Test Author".to_string(),
            rating: 4.5,
            downloads: 100,
            url: "".to_string(),
            installed: false,
            installed_version: None,
            homepage: None,
            tags: vec![],
            dependencies: vec![],
            min_compatible_version: None,
        }];

        assert!(verify_registry(&items).is_err());
    }

    #[test]
    fn test_installed_items() {
        let items = builtin_index();
        let mut items_with_installed = items.clone();

        // Mark one as installed
        if let Some(item) = items_with_installed.get_mut(0) {
            item.installed = true;
        }

        let installed = installed_items(&items_with_installed);
        assert_eq!(installed.len(), 1);
        assert!(installed[0].installed);
    }

    #[test]
    fn test_updatable_items() {
        let items = vec![
            MarketplaceItem {
                id: "item1".to_string(),
                name: "Item 1".to_string(),
                description: "Test".to_string(),
                item_type: ItemType::Skill,
                category: "Test".to_string(),
                version: "2.0.0".to_string(),
                author: "Test".to_string(),
                rating: 4.5,
                downloads: 100,
                url: "https://example.com".to_string(),
                installed: true,
                installed_version: Some("1.0.0".to_string()),
                homepage: None,
                tags: vec![],
                dependencies: vec![],
                min_compatible_version: Some("1.0.0".to_string()),
            },
            MarketplaceItem {
                id: "item2".to_string(),
                name: "Item 2".to_string(),
                description: "Test".to_string(),
                item_type: ItemType::Skill,
                category: "Test".to_string(),
                version: "1.0.0".to_string(),
                author: "Test".to_string(),
                rating: 4.5,
                downloads: 100,
                url: "https://example.com".to_string(),
                installed: true,
                installed_version: Some("1.0.0".to_string()),
                homepage: None,
                tags: vec![],
                dependencies: vec![],
                min_compatible_version: Some("1.0.0".to_string()),
            },
        ];

        let updatable = updatable_items(&items);
        assert_eq!(updatable.len(), 1);
        assert_eq!(updatable[0].id, "item1");
    }
}
