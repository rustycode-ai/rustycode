//! Update checker for marketplace items
//!
//! Provides functionality to check for and install updates for
//! installed marketplace items.

use super::index::{MarketplaceItem, UpdateAvailable, UpdateType};
use super::installer;
use anyhow::Result;

/// Check for available updates for all installed items
pub async fn check_updates(items: &[MarketplaceItem]) -> Result<Vec<UpdateAvailable>> {
    let mut updates = Vec::new();

    for item in items {
        if item.installed && item.has_update() {
            if let Some(installed_version) = &item.installed_version {
                let update_type = determine_update_type(installed_version, &item.version);

                updates.push(UpdateAvailable {
                    item: item.clone(),
                    current_version: installed_version.clone(),
                    new_version: item.version.clone(),
                    update_type,
                });
            }
        }
    }

    Ok(updates)
}

/// Update all installed items that have updates available
pub async fn update_all(items: &[MarketplaceItem]) -> Result<usize> {
    let updates = check_updates(items).await?;
    let mut updated_count = 0;

    for update in &updates {
        match installer::install_item(&update.item).await {
            Ok(_) => {
                updated_count += 1;
                tracing::info!(
                    "Updated {} from {} to {}",
                    update.item.id,
                    update.current_version,
                    update.new_version
                );
            }
            Err(e) => {
                tracing::error!("Failed to update {}: {}", update.item.id, e);
            }
        }
    }

    Ok(updated_count)
}

/// Update a specific item by ID
pub async fn update_item(items: &[MarketplaceItem], item_id: &str) -> Result<bool> {
    let item = items
        .iter()
        .find(|i| i.id == item_id)
        .ok_or_else(|| anyhow::anyhow!("Item '{}' not found", item_id))?;

    if !item.installed {
        return Err(anyhow::anyhow!("Item '{}' is not installed", item_id));
    }

    if !item.has_update() {
        return Ok(false); // No update available
    }

    installer::uninstall_item(item).await?;
    installer::install_item(item).await?;
    Ok(true)
}

/// Determine the type of update (major, minor, patch)
fn determine_update_type(current: &str, new: &str) -> UpdateType {
    // Parse version strings (simplified semver parsing)
    let current_parts: Vec<u32> = current.split('.').filter_map(|s| s.parse().ok()).collect();

    let new_parts: Vec<u32> = new.split('.').filter_map(|s| s.parse().ok()).collect();

    if current_parts.is_empty() || new_parts.is_empty() {
        return UpdateType::Patch; // Default to patch if parsing fails
    }

    let current_major = current_parts.first().unwrap_or(&0);
    let new_major = new_parts.first().unwrap_or(&0);

    let current_minor = current_parts.get(1).unwrap_or(&0);
    let new_minor = new_parts.get(1).unwrap_or(&0);

    if new_major > current_major {
        UpdateType::Major
    } else if new_minor > current_minor {
        UpdateType::Minor
    } else {
        UpdateType::Patch
    }
}

/// Get update type display string
pub fn update_type_display(update_type: &UpdateType) -> &str {
    match update_type {
        UpdateType::Major => "Major (breaking changes)",
        UpdateType::Minor => "Minor (new features)",
        UpdateType::Patch => "Patch (bug fixes)",
    }
}

/// Get update type color for terminal display
pub fn update_type_color(update_type: &UpdateType) -> &str {
    match update_type {
        UpdateType::Major => "red",    // Warning color
        UpdateType::Minor => "yellow", // Caution color
        UpdateType::Patch => "green",  // Safe color
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_determine_update_type() {
        assert_eq!(determine_update_type("1.0.0", "2.0.0"), UpdateType::Major);
        assert_eq!(determine_update_type("1.0.0", "1.1.0"), UpdateType::Minor);
        assert_eq!(determine_update_type("1.0.0", "1.0.1"), UpdateType::Patch);
    }

    #[test]
    fn test_update_type_display() {
        assert_eq!(
            update_type_display(&UpdateType::Major),
            "Major (breaking changes)"
        );
        assert_eq!(
            update_type_display(&UpdateType::Minor),
            "Minor (new features)"
        );
        assert_eq!(update_type_display(&UpdateType::Patch), "Patch (bug fixes)");
    }

    #[test]
    fn test_update_type_color() {
        assert_eq!(update_type_color(&UpdateType::Major), "red");
        assert_eq!(update_type_color(&UpdateType::Minor), "yellow");
        assert_eq!(update_type_color(&UpdateType::Patch), "green");
    }
}
