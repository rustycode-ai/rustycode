//! Installation system for marketplace items
//!
//! Handles installation, uninstallation, and status tracking for
//! marketplace items (skills, tools, and MCP servers).

use super::index::{ItemType, MarketplaceItem};
use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Install a marketplace item
pub async fn install_item(item: &MarketplaceItem) -> Result<()> {
    match item.item_type {
        ItemType::Skill => install_skill(item).await,
        ItemType::Tool => install_tool(item).await,
        ItemType::MCP => install_mcp(item).await,
    }
}

/// Uninstall a marketplace item
pub async fn uninstall_item(item: &MarketplaceItem) -> Result<()> {
    match item.item_type {
        ItemType::Skill => uninstall_skill(item).await,
        ItemType::Tool => uninstall_tool(item).await,
        ItemType::MCP => uninstall_mcp(item).await,
    }
}

/// Check if an item is installed
pub fn is_installed(item: &MarketplaceItem) -> bool {
    match item.item_type {
        ItemType::Skill => is_skill_installed(item),
        ItemType::Tool => is_tool_installed(item),
        ItemType::MCP => is_mcp_installed(item),
    }
}

/// Get installed version of an item
pub fn get_installed_version(item: &MarketplaceItem) -> Option<String> {
    if !is_installed(item) {
        return None;
    }

    match item.item_type {
        ItemType::Skill => get_skill_version(item),
        ItemType::Tool => get_tool_version(item),
        ItemType::MCP => get_mcp_version(item),
    }
}

// Skill installation

async fn install_skill(item: &MarketplaceItem) -> Result<()> {
    let skills_dir = get_skills_dir()?;
    let skill_path = skills_dir.join(&item.id);

    // Check if already installed
    if skill_path.exists() {
        return Err(anyhow::anyhow!("Skill '{}' is already installed", item.id));
    }

    // Create skills directory if it doesn't exist
    fs::create_dir_all(&skills_dir).context("Failed to create skills directory")?;

    // Clone the repository
    let status = Command::new("git")
        .args([
            "clone",
            &item.url,
            skill_path
                .to_str()
                .ok_or_else(|| anyhow::anyhow!("Invalid skill path: non-UTF-8 characters"))?,
        ])
        .status()
        .context("Failed to clone skill repository")?;

    if !status.success() {
        return Err(anyhow::anyhow!("Failed to clone skill repository"));
    }

    // Validate skill structure
    validate_skill(&skill_path)?;

    // Create metadata file
    write_skill_metadata(item, &skill_path)?;

    Ok(())
}

async fn uninstall_skill(item: &MarketplaceItem) -> Result<()> {
    let skills_dir = get_skills_dir()?;
    let skill_path = skills_dir.join(&item.id);

    if !skill_path.exists() {
        return Err(anyhow::anyhow!("Skill '{}' is not installed", item.id));
    }

    // Remove the skill directory
    fs::remove_dir_all(&skill_path).context("Failed to remove skill directory")?;

    Ok(())
}

fn is_skill_installed(item: &MarketplaceItem) -> bool {
    let skills_dir = match get_skills_dir() {
        Ok(dir) => dir,
        Err(_) => return false,
    };
    let skill_path = skills_dir.join(&item.id);
    skill_path.exists()
}

fn get_skill_version(item: &MarketplaceItem) -> Option<String> {
    let skills_dir = get_skills_dir().ok()?;
    let skill_path = skills_dir.join(&item.id);
    let metadata_path = skill_path.join(".metadata.json");

    if !metadata_path.exists() {
        return None;
    }

    // Read metadata and extract version
    // For now, return a placeholder
    Some("unknown".to_string())
}

fn validate_skill(path: &Path) -> Result<()> {
    // Check for skill.md or skill.json
    let skill_md = path.join("skill.md");
    let skill_json = path.join("skill.json");

    if !skill_md.exists() && !skill_json.exists() {
        return Err(anyhow::anyhow!(
            "Invalid skill structure: missing skill.md or skill.json"
        ));
    }

    Ok(())
}

fn write_skill_metadata(item: &MarketplaceItem, path: &Path) -> Result<()> {
    use serde_json::json;

    let metadata = json!({
        "id": item.id,
        "name": item.name,
        "version": item.version,
        "author": item.author,
        "installed_at": chrono::Utc::now().to_rfc3339(),
        "url": item.url,
    });

    let metadata_path = path.join(".metadata.json");
    fs::write(metadata_path, serde_json::to_string_pretty(&metadata)?)
        .context("Failed to write skill metadata")?;

    Ok(())
}

// Tool installation

async fn install_tool(item: &MarketplaceItem) -> Result<()> {
    let tools_dir = get_tools_dir()?;
    let tool_path = tools_dir.join(&item.id);

    if tool_path.exists() {
        return Err(anyhow::anyhow!("Tool '{}' is already installed", item.id));
    }

    fs::create_dir_all(&tools_dir).context("Failed to create tools directory")?;

    // Clone the repository
    let status = Command::new("git")
        .args([
            "clone",
            &item.url,
            tool_path
                .to_str()
                .ok_or_else(|| anyhow::anyhow!("Invalid tool path: non-UTF-8 characters"))?,
        ])
        .status()
        .context("Failed to clone tool repository")?;

    if !status.success() {
        return Err(anyhow::anyhow!("Failed to clone tool repository"));
    }

    // Write metadata
    write_tool_metadata(item, &tool_path)?;

    Ok(())
}

async fn uninstall_tool(item: &MarketplaceItem) -> Result<()> {
    let tools_dir = get_tools_dir()?;
    let tool_path = tools_dir.join(&item.id);

    if !tool_path.exists() {
        return Err(anyhow::anyhow!("Tool '{}' is not installed", item.id));
    }

    fs::remove_dir_all(&tool_path).context("Failed to remove tool directory")?;

    Ok(())
}

fn is_tool_installed(item: &MarketplaceItem) -> bool {
    let tools_dir = match get_tools_dir() {
        Ok(dir) => dir,
        Err(_) => return false,
    };
    let tool_path = tools_dir.join(&item.id);
    tool_path.exists()
}

fn get_tool_version(_item: &MarketplaceItem) -> Option<String> {
    // For tools, we could read from package.json, Cargo.toml, etc.
    // For now, return placeholder
    Some("unknown".to_string())
}

fn write_tool_metadata(item: &MarketplaceItem, path: &Path) -> Result<()> {
    use serde_json::json;

    let metadata = json!({
        "id": item.id,
        "name": item.name,
        "version": item.version,
        "author": item.author,
        "installed_at": chrono::Utc::now().to_rfc3339(),
        "url": item.url,
    });

    let metadata_path = path.join(".metadata.json");
    fs::write(metadata_path, serde_json::to_string_pretty(&metadata)?)
        .context("Failed to write tool metadata")?;

    Ok(())
}

// MCP Server installation

async fn install_mcp(item: &MarketplaceItem) -> Result<()> {
    // Check if it's an npm package
    if item.dependencies.iter().any(|d| d.starts_with('@')) {
        install_mcp_npm(item).await
    } else {
        install_mcp_git(item).await
    }
}

async fn uninstall_mcp(item: &MarketplaceItem) -> Result<()> {
    let mcp_dir = get_mcp_dir()?;
    let mcp_path = mcp_dir.join(&item.id);

    if !mcp_path.exists() {
        return Err(anyhow::anyhow!("MCP server '{}' is not installed", item.id));
    }

    // Check if it's an npm package
    if item.dependencies.iter().any(|d| d.starts_with('@')) {
        uninstall_mcp_npm(item).await
    } else {
        uninstall_mcp_git(item).await
    }
}

fn is_mcp_installed(item: &MarketplaceItem) -> bool {
    let mcp_dir = match get_mcp_dir() {
        Ok(dir) => dir,
        Err(_) => return false,
    };
    let mcp_path = mcp_dir.join(&item.id);
    mcp_path.exists()
}

fn get_mcp_version(_item: &MarketplaceItem) -> Option<String> {
    // For MCP servers, we could check package.json or similar
    Some("unknown".to_string())
}

async fn install_mcp_npm(_item: &MarketplaceItem) -> Result<()> {
    // For npm-based MCP servers, we would install via npm
    // For now, this is a placeholder
    Ok(())
}

async fn uninstall_mcp_npm(_item: &MarketplaceItem) -> Result<()> {
    // Uninstall npm package
    Ok(())
}

async fn install_mcp_git(item: &MarketplaceItem) -> Result<()> {
    let mcp_dir = get_mcp_dir()?;
    let mcp_path = mcp_dir.join(&item.id);

    if mcp_path.exists() {
        return Err(anyhow::anyhow!(
            "MCP server '{}' is already installed",
            item.id
        ));
    }

    fs::create_dir_all(&mcp_dir).context("Failed to create MCP directory")?;

    // Clone the repository
    let status = Command::new("git")
        .args([
            "clone",
            &item.url,
            mcp_path
                .to_str()
                .ok_or_else(|| anyhow::anyhow!("Invalid MCP path: non-UTF-8 characters"))?,
        ])
        .status()
        .context("Failed to clone MCP repository")?;

    if !status.success() {
        return Err(anyhow::anyhow!("Failed to clone MCP repository"));
    }

    write_mcp_metadata(item, &mcp_path)?;

    Ok(())
}

async fn uninstall_mcp_git(item: &MarketplaceItem) -> Result<()> {
    let mcp_dir = get_mcp_dir()?;
    let mcp_path = mcp_dir.join(&item.id);

    if !mcp_path.exists() {
        return Err(anyhow::anyhow!("MCP server '{}' is not installed", item.id));
    }

    fs::remove_dir_all(&mcp_path).context("Failed to remove MCP directory")?;

    Ok(())
}

fn write_mcp_metadata(item: &MarketplaceItem, path: &Path) -> Result<()> {
    use serde_json::json;

    let metadata = json!({
        "id": item.id,
        "name": item.name,
        "version": item.version,
        "author": item.author,
        "installed_at": chrono::Utc::now().to_rfc3339(),
        "url": item.url,
    });

    let metadata_path = path.join(".metadata.json");
    fs::write(metadata_path, serde_json::to_string_pretty(&metadata)?)
        .context("Failed to write MCP metadata")?;

    Ok(())
}

// Directory helpers

fn get_skills_dir() -> Result<PathBuf> {
    let home = dirs::home_dir().context("Failed to determine home directory")?;
    Ok(home.join(".claude").join("skills"))
}

fn get_tools_dir() -> Result<PathBuf> {
    let home = dirs::home_dir().context("Failed to determine home directory")?;
    Ok(home.join(".claude").join("tools"))
}

fn get_mcp_dir() -> Result<PathBuf> {
    let home = dirs::home_dir().context("Failed to determine home directory")?;
    Ok(home.join(".claude").join("mcp_servers"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_directory_paths() {
        let skills_dir = get_skills_dir();
        assert!(skills_dir.is_ok());

        let tools_dir = get_tools_dir();
        assert!(tools_dir.is_ok());

        let mcp_dir = get_mcp_dir();
        assert!(mcp_dir.is_ok());
    }
}
