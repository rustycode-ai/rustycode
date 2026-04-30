//! Skill installer and uninstaller
//!
//! Handles installation, uninstallation, and validation of skills
//! from the marketplace or git repositories.

use anyhow::{Context, Result};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use tracing::{debug, info, warn};

use super::lifecycle::SkillLifecycle;

/// Default skills directory path
pub fn skills_dir() -> Result<PathBuf> {
    let home =
        dirs::home_dir().ok_or_else(|| anyhow::anyhow!("Cannot determine home directory"))?;
    Ok(home.join(".claude").join("skills"))
}

/// Installed skills registry file path
pub fn registry_path() -> Result<PathBuf> {
    let skills_dir = skills_dir()?;
    Ok(skills_dir.join(".installed.json"))
}

/// Install a skill from marketplace
pub async fn install_skill(name: &str) -> Result<SkillLifecycle> {
    info!("Installing skill: {}", name);

    // 1. Look up skill in marketplace to get repository URL
    let marketplace_items = crate::marketplace::client::fetch_marketplace_index().await?;
    let skill_item = marketplace_items.iter().find(|item| {
        item.name == name && item.item_type == crate::marketplace::index::ItemType::Skill
    });

    let repository_url = if let Some(item) = skill_item {
        // Use the URL from marketplace
        item.url.clone()
    } else {
        // Fallback to default GitHub URL pattern
        format!("https://github.com/rustycode/skills-{}", name)
    };

    // 2. Prepare installation directory
    let base_dir = skills_dir()?;
    fs::create_dir_all(&base_dir)
        .with_context(|| format!("Failed to create skills directory: {:?}", base_dir))?;

    let skill_dir = base_dir.join(name);

    if skill_dir.exists() {
        anyhow::bail!("Skill '{}' is already installed", name);
    }

    // 3. Clone repository
    clone_repository(&repository_url, &skill_dir)
        .await
        .with_context(|| format!("Failed to clone skill repository: {}", repository_url))?;

    // 4. Validate skill
    validate_skill(&skill_dir)
        .with_context(|| format!("Skill validation failed for '{}'", name))?;

    // 5. Load skill metadata
    let _skill = load_skill_metadata(&skill_dir)?;

    // 6. Create installation metadata
    let lifecycle = SkillLifecycle::installed(repository_url.clone(), skill_dir.clone());

    // 7. Save to registry
    save_installed_skill(name, &lifecycle)?;

    info!("Successfully installed skill: {}", name);
    Ok(lifecycle)
}

/// Uninstall a skill
pub async fn uninstall_skill(name: &str) -> Result<()> {
    info!("Uninstalling skill: {}", name);

    // 1. Check if installed
    let base_dir = skills_dir()?;
    let skill_dir = base_dir.join(name);

    if !skill_dir.exists() {
        anyhow::bail!("Skill '{}' is not installed", name);
    }

    // 2. Deactivate if active (check with manager)
    // This is handled by the caller via the activation module

    // 3. Remove from filesystem
    fs::remove_dir_all(&skill_dir)
        .with_context(|| format!("Failed to remove skill directory: {:?}", skill_dir))?;

    // 4. Remove from registry
    remove_from_registry(name)?;

    info!("Successfully uninstalled skill: {}", name);
    Ok(())
}

/// Clone a git repository
async fn clone_repository(url: &str, dest: &Path) -> Result<()> {
    debug!("Cloning repository {} to {:?}", url, dest);

    let dest_str = dest.to_str().ok_or_else(|| {
        anyhow::anyhow!(
            "destination path contains invalid UTF-8: {}",
            dest.display()
        )
    })?;

    let output = Command::new("git")
        .args(["clone", "--depth", "1", url, dest_str])
        .output()
        .context("Failed to execute git clone command")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("git clone failed: {}", stderr);
    }

    Ok(())
}

/// Pull latest changes from repository
pub async fn update_repository(skill_path: &Path) -> Result<String> {
    debug!("Updating repository at {:?}", skill_path);

    let output = Command::new("git")
        .args(["pull", "--ff-only"])
        .current_dir(skill_path)
        .output()
        .context("Failed to execute git pull command")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("git pull failed: {}", stderr);
    }

    // Get current commit hash
    let hash_output = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(skill_path)
        .output()
        .context("Failed to get git commit hash")?;

    if !hash_output.status.success() {
        let stderr = String::from_utf8_lossy(&hash_output.stderr);
        anyhow::bail!("git rev-parse failed: {}", stderr);
    }

    let commit_hash = String::from_utf8(hash_output.stdout)?.trim().to_string();

    Ok(commit_hash)
}

/// Validate skill directory structure
pub fn validate_skill(dir: &Path) -> Result<()> {
    // Check skill.md exists
    let skill_file = dir.join("skill.md");
    if !skill_file.exists() {
        anyhow::bail!("skill.md not found in {:?}", dir);
    }

    // Check it's not empty
    let content = fs::read_to_string(&skill_file)
        .with_context(|| format!("Failed to read skill.md: {:?}", skill_file))?;

    if content.trim().is_empty() {
        anyhow::bail!("skill.md is empty in {:?}", dir);
    }

    // Validate basic structure
    if !content.contains("name:") {
        warn!("skill.md missing 'name' field in {:?}", dir);
    }

    // Check for README (recommended but not required)
    let readme_file = dir.join("README.md");
    if !readme_file.exists() {
        debug!(
            "No README.md found in {:?} (recommended but not required)",
            dir
        );
    }

    debug!("Skill validation passed for {:?}", dir);
    Ok(())
}

/// Load skill metadata from skill.md
fn load_skill_metadata(dir: &Path) -> Result<crate::skills::Skill> {
    use crate::skills::loader::SkillLoader;

    let loader = SkillLoader::with_path(dir.parent().unwrap_or(dir));
    let skills = loader
        .load_all()
        .with_context(|| format!("Failed to load skills from {:?}", dir))?;

    skills
        .into_iter()
        .find(|s| s.path == dir)
        .ok_or_else(|| anyhow::anyhow!("Skill not found after loading"))
}

/// Save skill to installed registry
fn save_installed_skill(name: &str, lifecycle: &SkillLifecycle) -> Result<()> {
    use std::collections::HashMap;

    let registry_path = registry_path()?;
    let mut registry: HashMap<String, SkillLifecycle> = if registry_path.exists() {
        let content = fs::read_to_string(&registry_path)?;
        serde_json::from_str(&content).unwrap_or_default()
    } else {
        HashMap::new()
    };

    registry.insert(name.to_string(), lifecycle.clone());

    let content = serde_json::to_string_pretty(&registry)?;
    fs::write(&registry_path, content)?;

    debug!("Saved '{}' to installed registry", name);
    Ok(())
}

/// Remove skill from registry
fn remove_from_registry(name: &str) -> Result<()> {
    use std::collections::HashMap;

    let registry_path = registry_path()?;

    if !registry_path.exists() {
        return Ok(());
    }

    let content = fs::read_to_string(&registry_path)?;
    let mut registry: HashMap<String, SkillLifecycle> = serde_json::from_str(&content)?;

    registry.remove(name);

    let new_content = serde_json::to_string_pretty(&registry)?;
    fs::write(&registry_path, new_content)?;

    debug!("Removed '{}' from installed registry", name);
    Ok(())
}

/// Load all installed skills from registry
pub fn load_installed_skills() -> Result<HashMap<String, SkillLifecycle>> {
    let registry_path = registry_path()?;

    if !registry_path.exists() {
        debug!("No installed skills registry found");
        return Ok(HashMap::new());
    }

    let content = fs::read_to_string(&registry_path)?;
    let registry: HashMap<String, SkillLifecycle> = serde_json::from_str(&content)?;

    debug!("Loaded {} installed skills from registry", registry.len());
    Ok(registry)
}

/// Check if skill is installed
pub fn is_installed(name: &str) -> bool {
    let base_dir = skills_dir();
    if let Ok(dir) = base_dir {
        let skill_dir = dir.join(name);
        skill_dir.exists()
    } else {
        false
    }
}

/// Get list of all installed skill names
pub fn list_installed_skills() -> Result<Vec<String>> {
    let base_dir = skills_dir()?;

    if !base_dir.exists() {
        return Ok(Vec::new());
    }

    let mut skills = Vec::new();
    let entries = fs::read_dir(&base_dir)?;

    for entry in entries {
        let entry = entry?;
        let path = entry.path();

        // Skip non-directories and hidden files
        if !path.is_dir()
            || path
                .file_name()
                .and_then(|n| n.to_str())
                .map(|s| s.starts_with('.'))
                .unwrap_or(false)
        {
            continue;
        }

        if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
            skills.push(name.to_string());
        }
    }

    skills.sort();
    Ok(skills)
}

/// Verify skill installation integrity
pub fn verify_skill(name: &str) -> Result<bool> {
    let base_dir = skills_dir()?;
    let skill_dir = base_dir.join(name);

    if !skill_dir.exists() {
        return Ok(false);
    }

    // Try to validate skill structure
    match validate_skill(&skill_dir) {
        Ok(()) => Ok(true),
        Err(e) => {
            warn!("Skill '{}' verification failed: {}", name, e);
            Ok(false)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_skills_dir() {
        let dir = skills_dir();
        assert!(dir.is_ok());
        let path = dir.unwrap();
        assert!(path.ends_with(".claude/skills"));
    }

    #[test]
    fn test_registry_path() {
        let path = registry_path();
        assert!(path.is_ok());
        let path = path.unwrap();
        assert!(path.ends_with(".claude/skills/.installed.json"));
    }

    #[test]
    fn test_lifecycle_serialization() {
        let lifecycle = SkillLifecycle::installed(
            "https://github.com/test/skill".to_string(),
            PathBuf::from("/test/skill"),
        );

        let json = serde_json::to_string(&lifecycle);
        assert!(json.is_ok());

        let deserialized: Result<SkillLifecycle, _> = serde_json::from_str(&json.unwrap());
        assert!(deserialized.is_ok());
    }
}
