//! Skill update management
//!
//! Handles checking for and applying updates to installed skills.

use anyhow::{Context, Result};
use tracing::{debug, error, info, warn};

use super::installer::{skills_dir, update_repository};

/// Update information for a skill
#[derive(Debug, Clone)]
pub struct UpdateInfo {
    /// Skill name
    pub name: String,

    /// Current version
    pub current_version: String,

    /// Latest version
    pub latest_version: String,

    /// Whether an update is available
    pub update_available: bool,

    /// Changelog or changes description
    pub changelog: Option<String>,
}

/// Check for updates for a specific skill
pub async fn check_for_updates(name: &str) -> Result<UpdateInfo> {
    debug!("Checking for updates for skill: {}", name);

    // Get skill path
    let base_dir = skills_dir()?;
    let skill_path = base_dir.join(name);

    if !skill_path.exists() {
        anyhow::bail!("Skill '{}' is not installed", name);
    }

    // Get current commit hash
    let current_version = get_current_version(&skill_path)?;

    // Fetch latest from remote
    fetch_remote(&skill_path).await?;

    // Get latest commit hash
    let latest_version = get_latest_version(&skill_path)?;

    let update_available = current_version != latest_version;

    debug!(
        "Skill '{}' update check: current={}, latest={}, available={}",
        name, current_version, latest_version, update_available
    );

    Ok(UpdateInfo {
        name: name.to_string(),
        current_version,
        latest_version,
        update_available,
        // Note: Changelog parsing from git log is a future enhancement
        // Would require cloning repo and running git log <version_range>...
        changelog: None,
    })
}

/// Check for updates for all installed skills
pub async fn check_all_updates() -> Result<Vec<UpdateInfo>> {
    info!("Checking for updates for all installed skills");

    let installed = super::installer::list_installed_skills()?;
    let mut updates = Vec::new();

    for name in installed {
        match check_for_updates(&name).await {
            Ok(info) => {
                if info.update_available {
                    updates.push(info);
                }
            }
            Err(e) => {
                warn!("Failed to check updates for '{}': {}", name, e);
            }
        }
    }

    info!("Found {} skills with updates available", updates.len());
    Ok(updates)
}

/// Update a specific skill
pub async fn update_skill(name: &str) -> Result<UpdateInfo> {
    info!("Updating skill: {}", name);

    // Check if skill is installed
    let base_dir = skills_dir()?;
    let skill_path = base_dir.join(name);

    if !skill_path.exists() {
        anyhow::bail!("Skill '{}' is not installed", name);
    }

    // Get current version
    let current_version = get_current_version(&skill_path)?;

    // Pull latest changes
    let new_version = update_repository(&skill_path)
        .await
        .with_context(|| format!("Failed to update skill '{}'", name))?;

    let update_available = current_version != new_version.clone();
    let new_version_str = new_version.clone();

    let update_info = UpdateInfo {
        name: name.to_string(),
        current_version: current_version.clone(),
        latest_version: new_version,
        update_available,
        changelog: get_changes_since(&skill_path, &current_version)?,
    };

    // Update installation metadata
    update_installation_metadata(name, new_version_str)?;

    info!("Successfully updated skill '{}'", name);
    Ok(update_info)
}

/// Update all installed skills
pub async fn update_all_skills() -> Result<Vec<UpdateInfo>> {
    info!("Updating all installed skills");

    let available_updates = check_all_updates().await?;
    let mut updated = Vec::new();

    for update_info in available_updates {
        match update_skill(&update_info.name).await {
            Ok(info) => {
                updated.push(info);
            }
            Err(e) => {
                error!("Failed to update skill '{}': {}", update_info.name, e);
            }
        }
    }

    info!("Successfully updated {} skills", updated.len());
    Ok(updated)
}

/// Get current version (commit hash) of skill
fn get_current_version(path: &std::path::Path) -> Result<String> {
    let output = std::process::Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(path)
        .output()
        .context("Failed to get current version")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("git rev-parse failed: {}", stderr);
    }

    Ok(String::from_utf8(output.stdout)?.trim().to_string())
}

/// Fetch latest from remote without merging
async fn fetch_remote(path: &std::path::Path) -> Result<()> {
    let output = std::process::Command::new("git")
        .args(["fetch", "origin"])
        .current_dir(path)
        .output()
        .context("Failed to fetch from remote")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("git fetch failed: {}", stderr);
    }

    Ok(())
}

/// Get latest version from remote
fn get_latest_version(path: &std::path::Path) -> Result<String> {
    let output = std::process::Command::new("git")
        .args(["rev-parse", "origin/HEAD"])
        .current_dir(path)
        .output()
        .context("Failed to get latest version")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("git rev-parse failed: {}", stderr);
    }

    Ok(String::from_utf8(output.stdout)?.trim().to_string())
}

/// Get changes since a specific commit
fn get_changes_since(path: &std::path::Path, since: &str) -> Result<Option<String>> {
    let output = std::process::Command::new("git")
        .args(["log", "--oneline", &format!("{}..HEAD", since)])
        .current_dir(path)
        .output()
        .context("Failed to get changes")?;

    if !output.status.success() {
        return Ok(None);
    }

    let changes = String::from_utf8(output.stdout)?.trim().to_string();
    if changes.is_empty() {
        Ok(None)
    } else {
        Ok(Some(changes))
    }
}

/// Update installation metadata after update
fn update_installation_metadata(name: &str, new_version: String) -> Result<()> {
    use super::installer::load_installed_skills;

    let mut registry = load_installed_skills()?;

    if let Some(lifecycle) = registry.get_mut(name) {
        if let Some(installation) = &mut lifecycle.installation {
            installation.update_version(new_version);
        }
    }

    // Save updated registry
    let content = serde_json::to_string_pretty(&registry)?;
    let registry_path = super::installer::registry_path()?;
    std::fs::write(&registry_path, content)?;

    debug!("Updated installation metadata for '{}'", name);
    Ok(())
}

/// Rollback a skill to previous version
pub async fn rollback_skill(name: &str, commit: &str) -> Result<()> {
    info!("Rolling back skill '{}' to commit {}", name, commit);

    let base_dir = skills_dir()?;
    let skill_path = base_dir.join(name);

    if !skill_path.exists() {
        anyhow::bail!("Skill '{}' is not installed", name);
    }

    // Checkout specific commit
    let output = std::process::Command::new("git")
        .args(["checkout", commit])
        .current_dir(&skill_path)
        .output()
        .context("Failed to checkout commit")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("git checkout failed: {}", stderr);
    }

    // Update installation metadata
    let new_version = get_current_version(&skill_path)?;
    update_installation_metadata(name, new_version)?;

    info!("Successfully rolled back '{}' to {}", name, commit);
    Ok(())
}

/// Get update history for a skill
pub fn get_update_history(name: &str, limit: usize) -> Result<Vec<String>> {
    let base_dir = skills_dir()?;
    let skill_path = base_dir.join(name);

    if !skill_path.exists() {
        anyhow::bail!("Skill '{}' is not installed", name);
    }

    let output = std::process::Command::new("git")
        .args([
            "log",
            "--oneline",
            "--format",
            "%h %s",
            &format!("-{}", limit),
        ])
        .current_dir(&skill_path)
        .output()
        .context("Failed to get update history")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("git log failed: {}", stderr);
    }

    let history = String::from_utf8(output.stdout)?;
    Ok(history.lines().map(|s| s.to_string()).collect())
}

/// Check if skill has uncommitted changes
pub fn has_uncommitted_changes(name: &str) -> Result<bool> {
    let base_dir = skills_dir()?;
    let skill_path = base_dir.join(name);

    if !skill_path.exists() {
        anyhow::bail!("Skill '{}' is not installed", name);
    }

    let output = std::process::Command::new("git")
        .args(["status", "--porcelain"])
        .current_dir(&skill_path)
        .output()
        .context("Failed to check git status")?;

    let status = String::from_utf8(output.stdout)?;
    Ok(!status.trim().is_empty())
}

/// Skill update statistics
#[derive(Debug, Clone)]
pub struct UpdateStats {
    pub total_installed: usize,
    pub updates_available: usize,
    pub last_checked: Option<chrono::DateTime<chrono::Utc>>,
}

/// Get update statistics for all skills
pub async fn get_update_stats() -> Result<UpdateStats> {
    let installed = super::installer::list_installed_skills()?;
    let updates_available = check_all_updates().await?.len();

    // Track last check time using current time
    let now = chrono::Utc::now();
    let last_checked = Some(now);

    Ok(UpdateStats {
        total_installed: installed.len(),
        updates_available,
        last_checked,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_update_info_creation() {
        let info = UpdateInfo {
            name: "test-skill".to_string(),
            current_version: "abc123".to_string(),
            latest_version: "def456".to_string(),
            update_available: true,
            changelog: Some("Fixed bugs".to_string()),
        };

        assert_eq!(info.name, "test-skill");
        assert!(info.update_available);
    }

    #[test]
    fn test_update_stats() {
        let stats = UpdateStats {
            total_installed: 10,
            updates_available: 3,
            last_checked: None,
        };

        assert_eq!(stats.total_installed, 10);
        assert_eq!(stats.updates_available, 3);
    }
}
