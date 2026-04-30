//! Skill activation management
//!
//! Handles activation and deactivation of skills for auto-triggering.
//! Provides configuration persistence and state management.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::PathBuf;
use tracing::{debug, info, warn};

use super::lifecycle::{SkillLifecycle, SkillLifecycleState};

/// Active skills configuration file path
fn config_path() -> Result<PathBuf> {
    let skills_dir = crate::skills::installer::skills_dir()?;
    Ok(skills_dir.join(".active.json"))
}

/// Active skills configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ActivationConfig {
    /// Set of active skill names
    pub active_skills: HashSet<String>,

    /// Trigger conditions per skill
    pub skill_triggers: HashMap<String, Vec<crate::skills::manager::TriggerCondition>>,
}

impl ActivationConfig {
    /// Create new empty configuration
    pub fn new() -> Self {
        Self::default()
    }

    /// Load configuration from file
    pub fn load() -> Result<Self> {
        let path = config_path()?;

        if !path.exists() {
            debug!("No active skills config found, creating new one");
            return Ok(Self::default());
        }

        let content = fs::read_to_string(&path)
            .with_context(|| format!("Failed to read activation config: {:?}", path))?;

        let config: Self = serde_json::from_str(&content)
            .with_context(|| format!("Failed to parse activation config: {:?}", path))?;

        debug!(
            "Loaded activation config with {} active skills",
            config.active_skills.len()
        );
        Ok(config)
    }

    /// Save configuration to file
    pub fn save(&self) -> Result<()> {
        let path = config_path()?;

        // Ensure parent directory exists
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create config directory: {:?}", parent))?;
        }

        let content = serde_json::to_string_pretty(self)
            .with_context(|| "Failed to serialize activation config")?;

        fs::write(&path, content)
            .with_context(|| format!("Failed to write activation config: {:?}", path))?;

        debug!(
            "Saved activation config with {} active skills",
            self.active_skills.len()
        );
        Ok(())
    }

    /// Check if skill is active
    pub fn is_active(&self, name: &str) -> bool {
        self.active_skills.contains(name)
    }

    /// Activate a skill
    pub fn activate(&mut self, name: &str) {
        self.active_skills.insert(name.to_string());
        debug!("Activated skill: {}", name);
    }

    /// Deactivate a skill
    pub fn deactivate(&mut self, name: &str) {
        self.active_skills.remove(name);
        debug!("Deactivated skill: {}", name);
    }

    /// Set triggers for a skill
    pub fn set_triggers(
        &mut self,
        name: &str,
        triggers: Vec<crate::skills::manager::TriggerCondition>,
    ) {
        self.skill_triggers.insert(name.to_string(), triggers);
    }

    /// Get triggers for a skill
    pub fn get_triggers(&self, name: &str) -> Option<&[crate::skills::manager::TriggerCondition]> {
        self.skill_triggers.get(name).map(|v| v.as_slice())
    }

    /// Get all active skill names
    pub fn active_skill_names(&self) -> Vec<String> {
        let mut names: Vec<_> = self.active_skills.iter().cloned().collect();
        names.sort();
        names
    }
}

/// Activate a skill for auto-triggering
pub fn activate_skill(name: &str) -> Result<()> {
    info!("Activating skill: {}", name);

    // Check if skill is installed
    if !crate::skills::installer::is_installed(name) {
        anyhow::bail!("Cannot activate skill '{}': not installed", name);
    }

    // Load configuration
    let mut config = ActivationConfig::load()?;

    // Activate skill
    config.activate(name);

    // Save configuration
    config.save()?;

    info!("Successfully activated skill: {}", name);
    Ok(())
}

/// Deactivate a skill
pub fn deactivate_skill(name: &str) -> Result<()> {
    info!("Deactivating skill: {}", name);

    // Load configuration
    let mut config = ActivationConfig::load()?;

    // Check if skill is active
    if !config.is_active(name) {
        warn!("Skill '{}' is not active", name);
        return Ok(());
    }

    // Deactivate skill
    config.deactivate(name);

    // Save configuration
    config.save()?;

    info!("Successfully deactivated skill: {}", name);
    Ok(())
}

/// Check if skill is active
pub fn is_active(name: &str) -> bool {
    match ActivationConfig::load() {
        Ok(config) => config.is_active(name),
        Err(e) => {
            warn!("Failed to load activation config: {}", e);
            false
        }
    }
}

/// Toggle skill activation state
pub fn toggle_skill(name: &str) -> Result<bool> {
    if is_active(name) {
        deactivate_skill(name)?;
        Ok(false)
    } else {
        activate_skill(name)?;
        Ok(true)
    }
}

/// Get all active skills
pub fn get_active_skills() -> Result<Vec<String>> {
    let config = ActivationConfig::load()?;
    Ok(config.active_skill_names())
}

/// Set trigger conditions for a skill
pub fn set_skill_triggers(
    name: &str,
    triggers: Vec<crate::skills::manager::TriggerCondition>,
) -> Result<()> {
    let mut config = ActivationConfig::load()?;
    config.set_triggers(name, triggers);
    config.save()?;
    Ok(())
}

/// Get trigger conditions for a skill
pub fn get_skill_triggers(name: &str) -> Option<Vec<crate::skills::manager::TriggerCondition>> {
    match ActivationConfig::load() {
        Ok(config) => config.get_triggers(name).map(|v| v.to_vec()),
        Err(e) => {
            warn!("Failed to load activation config: {}", e);
            None
        }
    }
}

/// Sync lifecycle state with activation config
pub fn sync_activation_state(lifecycle: &mut SkillLifecycle) -> Result<()> {
    if let Some(installation) = &lifecycle.installation {
        let skill_name = installation
            .install_path
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| anyhow::anyhow!("Invalid skill path"))?;

        let active = is_active(skill_name);

        lifecycle.state = if active {
            SkillLifecycleState::Active
        } else {
            SkillLifecycleState::Inactive
        };
    }

    Ok(())
}

/// Activate multiple skills at once
pub fn activate_skills(names: &[&str]) -> Result<usize> {
    let mut config = ActivationConfig::load()?;
    let mut count = 0;

    for name in names {
        if crate::skills::installer::is_installed(name) {
            config.activate(name);
            count += 1;
        } else {
            warn!("Cannot activate '{}': not installed", name);
        }
    }

    config.save()?;
    info!("Activated {} skills", count);
    Ok(count)
}

/// Deactivate all skills
pub fn deactivate_all() -> Result<()> {
    info!("Deactivating all skills");

    let config = ActivationConfig::load()?;
    let count = config.active_skills.len();

    let new_config = ActivationConfig::default();
    new_config.save()?;

    info!("Deactivated {} skills", count);
    Ok(())
}

/// Bulk activation with trigger configuration
pub fn configure_skills(
    configurations: Vec<(String, Vec<crate::skills::manager::TriggerCondition>)>,
) -> Result<usize> {
    let mut config = ActivationConfig::load()?;
    let mut count = 0;

    for (name, triggers) in configurations {
        if crate::skills::installer::is_installed(&name) {
            config.activate(&name);
            config.set_triggers(&name, triggers);
            count += 1;
        } else {
            warn!("Cannot configure '{}': not installed", name);
        }
    }

    config.save()?;
    info!("Configured {} skills", count);
    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_activation_config_default() {
        let config = ActivationConfig::default();
        assert_eq!(config.active_skills.len(), 0);
        assert_eq!(config.skill_triggers.len(), 0);
    }

    #[test]
    fn test_activation_config_activate_deactivate() {
        let mut config = ActivationConfig::new();
        assert!(!config.is_active("test-skill"));

        config.activate("test-skill");
        assert!(config.is_active("test-skill"));

        config.deactivate("test-skill");
        assert!(!config.is_active("test-skill"));
    }

    #[test]
    fn test_activation_config_triggers() {
        let mut config = ActivationConfig::new();
        let triggers = vec![
            crate::skills::manager::TriggerCondition::OnCommit,
            crate::skills::manager::TriggerCondition::OnError,
        ];

        config.set_triggers("test-skill", triggers.clone());
        assert_eq!(config.get_triggers("test-skill"), Some(triggers.as_slice()));
    }

    #[test]
    fn test_activation_config_serialization() {
        let mut config = ActivationConfig::new();
        config.activate("skill1");
        config.activate("skill2");

        let json = serde_json::to_string(&config);
        assert!(json.is_ok());

        let deserialized: Result<ActivationConfig, _> = serde_json::from_str(&json.unwrap());
        assert!(deserialized.is_ok());

        let parsed = deserialized.unwrap();
        assert!(parsed.is_active("skill1"));
        assert!(parsed.is_active("skill2"));
    }
}
