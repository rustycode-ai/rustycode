//! Marketplace index structures and data types
//!
//! Defines the core data structures for representing marketplace items
//! including skills, tools, and MCP servers.

use serde::{Deserialize, Serialize};

/// A marketplace item that can be installed
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketplaceItem {
    /// Unique identifier for the item
    pub id: String,
    /// Display name
    pub name: String,
    /// Short description
    pub description: String,
    /// Type of item (Skill, Tool, MCP)
    pub item_type: ItemType,
    /// Category (e.g., "Agent", "Developer", "Utility")
    pub category: String,
    /// Current version
    pub version: String,
    /// Author/maintainer
    pub author: String,
    /// User rating (0.0 to 5.0)
    pub rating: f64,
    /// Download count
    pub downloads: usize,
    /// Git repository or download URL
    pub url: String,
    /// Whether currently installed
    pub installed: bool,
    /// Installed version (if installed)
    pub installed_version: Option<String>,
    /// Homepage URL (optional)
    pub homepage: Option<String>,
    /// Tags for search
    pub tags: Vec<String>,
    /// Dependencies required
    pub dependencies: Vec<String>,
    /// Minimum compatible version
    pub min_compatible_version: Option<String>,
}

/// Type of marketplace item
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ItemType {
    /// AI agent/assistant skill
    #[serde(rename = "skill")]
    Skill,
    /// CLI tool or utility
    #[serde(rename = "tool")]
    Tool,
    /// Model Context Protocol server
    #[serde(rename = "mcp")]
    MCP,
}

impl ItemType {
    /// Get display icon for the item type
    pub fn icon(&self) -> &str {
        match self {
            ItemType::Skill => "⚡",
            ItemType::Tool => "🔧",
            ItemType::MCP => "🌐",
        }
    }

    /// Get display name for the item type
    pub fn display_name(&self) -> &str {
        match self {
            ItemType::Skill => "Skill",
            ItemType::Tool => "Tool",
            ItemType::MCP => "MCP Server",
        }
    }
}

/// Information about an available update
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateAvailable {
    /// Item that has an update
    pub item: MarketplaceItem,
    /// Current installed version
    pub current_version: String,
    /// New version available
    pub new_version: String,
    /// Update type (major, minor, patch)
    pub update_type: UpdateType,
}

/// Type of update
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[non_exhaustive]
pub enum UpdateType {
    /// Major version update (breaking changes)
    Major,
    /// Minor version update (new features)
    Minor,
    /// Patch version update (bug fixes)
    Patch,
}

impl MarketplaceItem {
    /// Format rating as stars
    pub fn rating_stars(&self) -> String {
        let full_stars = (self.rating / 1.0).floor() as usize;
        let half_star = (self.rating % 1.0) >= 0.5;
        let empty_stars = 5 - full_stars - usize::from(half_star);

        let mut stars = "★".repeat(full_stars);
        if half_star {
            stars.push('½');
        }
        stars.push_str(&"☆".repeat(empty_stars));
        stars
    }

    /// Format download count (e.g., "1.2k", "15k")
    pub fn format_downloads(&self) -> String {
        if self.downloads >= 1_000_000 {
            format!("{:.1}M", self.downloads as f64 / 1_000_000.0)
        } else if self.downloads >= 1_000 {
            format!("{:.1}k", self.downloads as f64 / 1_000.0)
        } else {
            format!("{}", self.downloads)
        }
    }

    /// Check if an update is available
    pub fn has_update(&self) -> bool {
        if let (Some(installed), Some(min)) =
            (&self.installed_version, &self.min_compatible_version)
        {
            return installed != &self.version && self.version >= *min;
        }
        false
    }

    /// Get install status indicator
    pub fn status_indicator(&self) -> &str {
        if self.installed {
            if self.has_update() {
                "↑"
            } else {
                "✓"
            }
        } else {
            ""
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_item_type_icons() {
        assert_eq!(ItemType::Skill.icon(), "⚡");
        assert_eq!(ItemType::Tool.icon(), "🔧");
        assert_eq!(ItemType::MCP.icon(), "🌐");
    }

    #[test]
    fn test_rating_stars() {
        let item = MarketplaceItem {
            id: "test".to_string(),
            name: "Test".to_string(),
            description: "Test".to_string(),
            item_type: ItemType::Skill,
            category: "Test".to_string(),
            version: "1.0.0".to_string(),
            author: "Test".to_string(),
            rating: 4.5,
            downloads: 1000,
            url: "https://example.com".to_string(),
            installed: false,
            installed_version: None,
            homepage: None,
            tags: vec![],
            dependencies: vec![],
            min_compatible_version: None,
        };

        assert_eq!(item.rating_stars(), "★★★★½");
    }

    #[test]
    fn test_format_downloads() {
        let mut item = MarketplaceItem {
            id: "test".to_string(),
            name: "Test".to_string(),
            description: "Test".to_string(),
            item_type: ItemType::Skill,
            category: "Test".to_string(),
            version: "1.0.0".to_string(),
            author: "Test".to_string(),
            rating: 4.5,
            downloads: 1500,
            url: "https://example.com".to_string(),
            installed: false,
            installed_version: None,
            homepage: None,
            tags: vec![],
            dependencies: vec![],
            min_compatible_version: None,
        };

        assert_eq!(item.format_downloads(), "1.5k");

        item.downloads = 1_500_000;
        assert_eq!(item.format_downloads(), "1.5M");

        item.downloads = 500;
        assert_eq!(item.format_downloads(), "500");
    }

    // --- ItemType serde and equality ---

    #[test]
    fn item_type_serde_roundtrip() {
        for it in &[ItemType::Skill, ItemType::Tool, ItemType::MCP] {
            let json = serde_json::to_string(it).unwrap();
            let back: ItemType = serde_json::from_str(&json).unwrap();
            assert_eq!(&back, it);
        }
    }

    #[test]
    fn item_type_serde_renames() {
        assert_eq!(
            serde_json::to_string(&ItemType::Skill).unwrap(),
            "\"skill\""
        );
        assert_eq!(serde_json::to_string(&ItemType::Tool).unwrap(), "\"tool\"");
        assert_eq!(serde_json::to_string(&ItemType::MCP).unwrap(), "\"mcp\"");
    }

    #[test]
    fn item_type_display_names() {
        assert_eq!(ItemType::Skill.display_name(), "Skill");
        assert_eq!(ItemType::Tool.display_name(), "Tool");
        assert_eq!(ItemType::MCP.display_name(), "MCP Server");
    }

    // --- UpdateType serde ---

    #[test]
    fn update_type_serde_roundtrip() {
        for ut in &[UpdateType::Major, UpdateType::Minor, UpdateType::Patch] {
            let json = serde_json::to_string(ut).unwrap();
            let back: UpdateType = serde_json::from_str(&json).unwrap();
            assert_eq!(&back, ut);
        }
    }

    // --- MarketplaceItem serde ---

    #[test]
    fn marketplace_item_serde_roundtrip() {
        let item = MarketplaceItem {
            id: "my-skill".into(),
            name: "My Skill".into(),
            description: "A great skill".into(),
            item_type: ItemType::Tool,
            category: "Developer".into(),
            version: "2.1.0".into(),
            author: "nat".into(),
            rating: 3.0,
            downloads: 999,
            url: "https://example.com/skill".into(),
            installed: true,
            installed_version: Some("2.0.0".into()),
            homepage: Some("https://example.com".into()),
            tags: vec!["rust".into(), "ai".into()],
            dependencies: vec!["tool-a".into()],
            min_compatible_version: Some("1.0.0".into()),
        };
        let json = serde_json::to_string(&item).unwrap();
        let decoded: MarketplaceItem = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.id, "my-skill");
        assert_eq!(decoded.item_type, ItemType::Tool);
        assert!(decoded.installed);
        assert_eq!(decoded.tags, vec!["rust", "ai"]);
    }

    // --- UpdateAvailable serde ---

    #[test]
    fn update_available_serde() {
        let ua = UpdateAvailable {
            item: MarketplaceItem {
                id: "x".into(),
                name: "X".into(),
                description: "desc".into(),
                item_type: ItemType::MCP,
                category: "Util".into(),
                version: "2.0.0".into(),
                author: "a".into(),
                rating: 5.0,
                downloads: 0,
                url: "u".into(),
                installed: true,
                installed_version: Some("1.0.0".into()),
                homepage: None,
                tags: vec![],
                dependencies: vec![],
                min_compatible_version: None,
            },
            current_version: "1.0.0".into(),
            new_version: "2.0.0".into(),
            update_type: UpdateType::Major,
        };
        let json = serde_json::to_string(&ua).unwrap();
        let decoded: UpdateAvailable = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.current_version, "1.0.0");
        assert_eq!(decoded.update_type, UpdateType::Major);
    }

    // --- rating_stars edge cases ---

    #[test]
    fn rating_stars_zero() {
        let item = MarketplaceItem {
            rating: 0.0,
            ..test_item()
        };
        assert_eq!(item.rating_stars(), "☆☆☆☆☆");
    }

    #[test]
    fn rating_stars_five() {
        let item = MarketplaceItem {
            rating: 5.0,
            ..test_item()
        };
        assert_eq!(item.rating_stars(), "★★★★★");
    }

    // --- has_update ---

    #[test]
    fn has_update_true() {
        let item = MarketplaceItem {
            installed: true,
            installed_version: Some("1.0.0".into()),
            version: "2.0.0".into(),
            min_compatible_version: Some("1.5.0".into()),
            ..test_item()
        };
        assert!(item.has_update());
    }

    #[test]
    fn has_update_false_not_installed() {
        let item = MarketplaceItem {
            installed: false,
            ..test_item()
        };
        assert!(!item.has_update());
    }

    #[test]
    fn has_update_false_same_version() {
        let item = MarketplaceItem {
            installed: true,
            installed_version: Some("2.0.0".into()),
            version: "2.0.0".into(),
            min_compatible_version: Some("1.0.0".into()),
            ..test_item()
        };
        assert!(!item.has_update());
    }

    // --- status_indicator ---

    #[test]
    fn status_indicator_installed() {
        let item = MarketplaceItem {
            installed: true,
            ..test_item()
        };
        assert_eq!(item.status_indicator(), "✓");
    }

    #[test]
    fn status_indicator_not_installed() {
        let item = MarketplaceItem {
            installed: false,
            ..test_item()
        };
        assert_eq!(item.status_indicator(), "");
    }

    #[test]
    fn status_indicator_has_update() {
        let item = MarketplaceItem {
            installed: true,
            installed_version: Some("1.0.0".into()),
            version: "2.0.0".into(),
            min_compatible_version: Some("1.0.0".into()),
            ..test_item()
        };
        assert_eq!(item.status_indicator(), "↑");
    }

    // --- format_downloads edge cases ---

    #[test]
    fn format_downloads_exact_million() {
        let item = MarketplaceItem {
            downloads: 1_000_000,
            ..test_item()
        };
        assert_eq!(item.format_downloads(), "1.0M");
    }

    #[test]
    fn format_downloads_exact_thousand() {
        let item = MarketplaceItem {
            downloads: 1_000,
            ..test_item()
        };
        assert_eq!(item.format_downloads(), "1.0k");
    }

    // helper
    fn test_item() -> MarketplaceItem {
        MarketplaceItem {
            id: "test".into(),
            name: "Test".into(),
            description: "Test".into(),
            item_type: ItemType::Skill,
            category: "Test".into(),
            version: "1.0.0".into(),
            author: "Test".into(),
            rating: 4.0,
            downloads: 100,
            url: "https://example.com".into(),
            installed: false,
            installed_version: None,
            homepage: None,
            tags: vec![],
            dependencies: vec![],
            min_compatible_version: None,
        }
    }
}
