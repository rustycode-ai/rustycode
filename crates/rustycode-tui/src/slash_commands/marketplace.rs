//! Marketplace slash commands
//!
//! Provides commands for browsing, searching, installing, and managing
//! marketplace items (skills, tools, and MCP servers).

use crate::marketplace::{
    client::{
        fetch_marketplace_index, filter_by_category, filter_by_type, get_installed_items,
        get_updatable_items, search_marketplace,
    },
    index::ItemType,
    installer::{install_item, uninstall_item},
    updates::{check_updates, update_all, update_item},
};

/// Handle marketplace commands
pub async fn handle_marketplace_command(input: &str) -> Result<Option<String>, String> {
    let parts: Vec<&str> = input.split_whitespace().collect();

    if parts.len() < 2 {
        return Ok(Some(
            "Usage: /marketplace [browse|search|install|uninstall|update|list|info]\n\
             Or: /marketplace (opens marketplace browser)"
                .to_string(),
        ));
    }

    let subcommand = parts[1];

    match subcommand {
        "browse" | "show" => cmd_browse_marketplace(&parts[1..]).await,
        "search" => cmd_search_marketplace(&parts[1..]).await,
        "install" => cmd_install_item(&parts[1..]).await,
        "uninstall" => cmd_uninstall_item(&parts[1..]).await,
        "update" => cmd_update_items(&parts[1..]).await,
        "list" => cmd_list_items(&parts[1..]).await,
        "info" => cmd_item_info(&parts[1..]).await,
        "skills" => cmd_browse_category(&parts[1..], ItemType::Skill).await,
        "tools" => cmd_browse_category(&parts[1..], ItemType::Tool).await,
        "mcp" => cmd_browse_category(&parts[1..], ItemType::MCP).await,
        _ => Ok(Some(format!(
            "Unknown marketplace command: {}\n\
             Usage: /marketplace <browse|search|install|uninstall|update|list|info|skills|tools|mcp>",
            subcommand
        ))),
    }
}

/// Browse marketplace items
async fn cmd_browse_marketplace(parts: &[&str]) -> Result<Option<String>, String> {
    let items = fetch_marketplace_index()
        .await
        .map_err(|e| format!("Failed to fetch marketplace: {}", e))?;

    let (filtered, label) = if parts.len() > 2 {
        match parts[2] {
            "skills" => (filter_by_type(&items, &ItemType::Skill), "Skills"),
            "tools" => (filter_by_type(&items, &ItemType::Tool), "Tools"),
            "mcp" => (filter_by_type(&items, &ItemType::MCP), "MCP Servers"),
            category => (filter_by_category(&items, category), category),
        }
    } else {
        (items.clone(), "All Items")
    };

    if filtered.is_empty() {
        return Ok(Some(format!("No {} found in marketplace.", label)));
    }

    let mut output = format!("🛒 Marketplace - {} ({})\n", label, filtered.len());
    output.push_str(&"─".repeat(80));
    output.push('\n');

    for item in filtered.iter().take(20) {
        output.push_str(&format_marketplace_item(item));
    }

    if filtered.len() > 20 {
        output.push_str(&format!("\n... and {} more items\n", filtered.len() - 20));
    }

    output.push_str("\n💡 Use /marketplace search <query> to find specific items\n");
    output.push_str("Use /marketplace install <id> to install an item");

    Ok(Some(output))
}

/// Search marketplace items
async fn cmd_search_marketplace(parts: &[&str]) -> Result<Option<String>, String> {
    if parts.len() < 3 {
        return Ok(Some("Usage: /marketplace search <query>".to_string()));
    }

    let query = parts[2..].join(" ");
    let items = fetch_marketplace_index()
        .await
        .map_err(|e| format!("Failed to fetch marketplace: {}", e))?;

    let results = search_marketplace(&items, &query);

    if results.is_empty() {
        return Ok(Some(format!("No results found for '{}'", query)));
    }

    let mut output = format!("🔍 Search results for '{}' ({})\n", query, results.len());
    output.push_str(&"─".repeat(80));
    output.push('\n');

    for item in results.iter().take(20) {
        output.push_str(&format_marketplace_item(item));
    }

    if results.len() > 20 {
        output.push_str(&format!("\n... and {} more results\n", results.len() - 20));
    }

    output.push_str("\n💡 Use /marketplace install <id> to install an item");

    Ok(Some(output))
}

/// Install a marketplace item
async fn cmd_install_item(parts: &[&str]) -> Result<Option<String>, String> {
    if parts.len() < 3 {
        return Ok(Some("Usage: /marketplace install <item-id>".to_string()));
    }

    let item_id = parts[2];
    let items = fetch_marketplace_index()
        .await
        .map_err(|e| format!("Failed to fetch marketplace: {}", e))?;

    let item = items
        .iter()
        .find(|i| i.id == item_id)
        .ok_or_else(|| format!("Item '{}' not found in marketplace", item_id))?;

    if item.installed {
        return Ok(Some(format!(
            "✓ {} is already installed (v{})",
            item.name,
            item.installed_version
                .as_ref()
                .unwrap_or(&"unknown".to_string())
        )));
    }

    install_item(item)
        .await
        .map_err(|e| format!("Failed to install {}: {}", item.name, e))?;

    Ok(Some(format!(
        "✓ Successfully installed {} v{}\n\
         Type: {}\n\
         Category: {}\n\
         Author: {}",
        item.name,
        item.version,
        item.item_type.display_name(),
        item.category,
        item.author
    )))
}

/// Uninstall a marketplace item
async fn cmd_uninstall_item(parts: &[&str]) -> Result<Option<String>, String> {
    if parts.len() < 3 {
        return Ok(Some("Usage: /marketplace uninstall <item-id>".to_string()));
    }

    let item_id = parts[2];
    let items = fetch_marketplace_index()
        .await
        .map_err(|e| format!("Failed to fetch marketplace: {}", e))?;

    let item = items
        .iter()
        .find(|i| i.id == item_id)
        .ok_or_else(|| format!("Item '{}' not found in marketplace", item_id))?;

    if !item.installed {
        return Ok(Some(format!("✓ {} is not installed", item.name)));
    }

    uninstall_item(item)
        .await
        .map_err(|e| format!("Failed to uninstall {}: {}", item.name, e))?;

    Ok(Some(format!("✓ Successfully uninstalled {}", item.name)))
}

/// Update marketplace items
async fn cmd_update_items(parts: &[&str]) -> Result<Option<String>, String> {
    let items = fetch_marketplace_index()
        .await
        .map_err(|e| format!("Failed to fetch marketplace: {}", e))?;

    if parts.len() > 2 {
        // Update specific item
        let item_id = parts[2];
        update_item(&items, item_id)
            .await
            .map_err(|e| format!("Failed to update {}: {}", item_id, e))?;

        Ok(Some(format!("✓ Updated {}", item_id)))
    } else {
        // Update all items
        let updates = check_updates(&items)
            .await
            .map_err(|e| format!("Failed to check for updates: {}", e))?;

        if updates.is_empty() {
            return Ok(Some("✓ All items are up to date!".to_string()));
        }

        let updated_count = update_all(&items)
            .await
            .map_err(|e| format!("Failed to update items: {}", e))?;

        Ok(Some(format!(
            "✓ Updated {} item(s)\n\n\
             Updates installed:\n\
             {}",
            updated_count,
            updates
                .iter()
                .map(|u| format!(
                    "  • {}: {} → {}",
                    u.item.name, u.current_version, u.new_version
                ))
                .collect::<Vec<_>>()
                .join("\n")
        )))
    }
}

/// List installed items
async fn cmd_list_items(parts: &[&str]) -> Result<Option<String>, String> {
    let items = fetch_marketplace_index()
        .await
        .map_err(|e| format!("Failed to fetch marketplace: {}", e))?;

    let (items_to_show, label) = if parts.len() > 2 {
        match parts[2] {
            "installed" => (get_installed_items(&items), "Installed"),
            "updates" => (get_updatable_items(&items), "Updates Available"),
            category => (filter_by_category(&items, category), category),
        }
    } else {
        (get_installed_items(&items), "Installed")
    };

    if items_to_show.is_empty() {
        return Ok(Some(format!("No {} items found.", label)));
    }

    let mut output = format!("📦 {} Items ({})\n", label, items_to_show.len());
    output.push_str(&"─".repeat(80));
    output.push('\n');

    for item in &items_to_show {
        output.push_str(&format_marketplace_item(item));
    }

    Ok(Some(output))
}

/// Show item information
async fn cmd_item_info(parts: &[&str]) -> Result<Option<String>, String> {
    if parts.len() < 3 {
        return Ok(Some("Usage: /marketplace info <item-id>".to_string()));
    }

    let item_id = parts[2];
    let items = fetch_marketplace_index()
        .await
        .map_err(|e| format!("Failed to fetch marketplace: {}", e))?;

    let item = items
        .iter()
        .find(|i| i.id == item_id)
        .ok_or_else(|| format!("Item '{}' not found in marketplace", item_id))?;

    let mut output = format!("📋 {} Details\n", item.name);
    output.push_str(&"═".repeat(80));
    output.push('\n');
    output.push_str(&format!("ID: {}\n", item.id));
    output.push_str(&format!("Type: {}\n", item.item_type.display_name()));
    output.push_str(&format!("Category: {}\n", item.category));
    output.push_str(&format!("Version: {}\n", item.version));
    output.push_str(&format!("Author: {}\n", item.author));
    output.push_str(&format!(
        "Rating: {:.1}/5.0 ({})\n",
        item.rating,
        item.rating_stars()
    ));
    output.push_str(&format!("Downloads: {}\n", item.format_downloads()));
    output.push_str(&format!(
        "Status: {}\n",
        if item.installed {
            format!(
                "Installed (v{})",
                item.installed_version
                    .as_ref()
                    .unwrap_or(&"unknown".to_string())
            )
        } else {
            "Not installed".to_string()
        }
    ));
    output.push_str(&format!("Repository: {}\n", item.url));

    if let Some(homepage) = &item.homepage {
        output.push_str(&format!("Homepage: {}\n", homepage));
    }

    if !item.tags.is_empty() {
        output.push_str(&format!("Tags: {}\n", item.tags.join(", ")));
    }

    if !item.dependencies.is_empty() {
        output.push_str(&format!("Dependencies: {}\n", item.dependencies.join(", ")));
    }

    output.push_str(&format!("\n{}\n", item.description));

    if item.installed && item.has_update() {
        output.push_str(&format!(
            "\n⚠️ Update available: {} → {}\n",
            item.installed_version
                .as_ref()
                .unwrap_or(&"unknown".to_string()),
            item.version
        ));
    }

    Ok(Some(output))
}

/// Browse items by category
async fn cmd_browse_category(
    _parts: &[&str],
    item_type: ItemType,
) -> Result<Option<String>, String> {
    let items = fetch_marketplace_index()
        .await
        .map_err(|e| format!("Failed to fetch marketplace: {}", e))?;

    let filtered = filter_by_type(&items, &item_type);

    if filtered.is_empty() {
        return Ok(Some(format!(
            "No {} found in marketplace.",
            item_type.display_name()
        )));
    }

    let mut output = format!(
        "🛒 Marketplace - {} ({})\n",
        item_type.display_name(),
        filtered.len()
    );
    output.push_str(&"─".repeat(80));
    output.push('\n');

    for item in filtered.iter().take(20) {
        output.push_str(&format_marketplace_item(item));
    }

    if filtered.len() > 20 {
        output.push_str(&format!("\n... and {} more items\n", filtered.len() - 20));
    }

    output.push_str("\n💡 Use /marketplace install <id> to install an item");

    Ok(Some(output))
}

/// Format a marketplace item for display
fn format_marketplace_item(item: &crate::marketplace::index::MarketplaceItem) -> String {
    let icon = item.item_type.icon();
    let status = if item.installed { "✓" } else { "" };
    let update_indicator = if item.has_update() { "↑" } else { "" };

    format!(
        " {} {} {} {:.1} {}  {:20} [{}{}]\n\
          {}\n\
          by {} | {} downloads | v{}\n\n",
        icon,
        status,
        update_indicator,
        item.rating,
        item.rating_stars(),
        item.name,
        if item.installed { "✓" } else { "" },
        if item.has_update() { "↑" } else { "" },
        item.description,
        item.author,
        item.format_downloads(),
        item.version
    )
}
