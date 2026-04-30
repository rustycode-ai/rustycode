//! Skill state manager
//!
//! Manages the runtime state of skills, including active skills, status tracking,
//! and execution lifecycle.

use super::loader::Skill as BaseSkill;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::time::Instant;

/// Trigger condition for auto-running skills
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[non_exhaustive]
pub enum TriggerCondition {
    /// Trigger on git commit
    OnCommit,
    /// Trigger on file save/change
    OnFileChange,
    /// Trigger on error
    OnError,
    /// Manual trigger only
    ManualOnly,
}

/// Runtime status of a skill
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[non_exhaustive]
pub enum SkillStatus {
    /// Skill is available but not active
    Inactive,
    /// Skill is active and will auto-trigger
    Active,
    /// Skill is currently running
    Running,
    /// Last run failed
    Error(String),
}

/// Extended skill with runtime state
#[derive(Debug, Clone)]
pub struct SkillState {
    /// Base skill metadata
    pub base: BaseSkill,
    /// Current status
    pub status: SkillStatus,
    /// Whether auto-triggering is enabled
    pub auto_enabled: bool,
    /// Trigger conditions
    pub triggers: Vec<TriggerCondition>,
    /// Last run timestamp
    pub last_run: Option<Instant>,
    /// Run count
    pub run_count: usize,
    /// Error count
    pub error_count: usize,
}

impl SkillState {
    /// Create a new skill state from base skill
    pub fn from_base(base: BaseSkill) -> Self {
        Self {
            base,
            status: SkillStatus::Inactive,
            auto_enabled: false,
            triggers: vec![TriggerCondition::ManualOnly],
            last_run: None,
            run_count: 0,
            error_count: 0,
        }
    }

    /// Check if skill should trigger on the given condition
    pub fn should_trigger(&self, condition: &TriggerCondition) -> bool {
        self.auto_enabled && self.triggers.contains(condition)
    }

    /// Mark skill as running
    pub fn mark_running(&mut self) {
        self.status = SkillStatus::Running;
    }

    /// Mark skill as completed successfully
    pub fn mark_success(&mut self) {
        self.status = if self.auto_enabled {
            SkillStatus::Active
        } else {
            SkillStatus::Inactive
        };
        self.last_run = Some(Instant::now());
        self.run_count += 1;
    }

    /// Mark skill as failed
    pub fn mark_error(&mut self, error: String) {
        self.status = SkillStatus::Error(error);
        self.last_run = Some(Instant::now());
        self.run_count += 1;
        self.error_count += 1;
    }

    /// Toggle auto-enable state
    pub fn toggle_auto(&mut self) {
        self.auto_enabled = !self.auto_enabled;
        self.status = if self.auto_enabled {
            SkillStatus::Active
        } else {
            SkillStatus::Inactive
        };
    }

    /// Get formatted last run time
    pub fn last_run_display(&self) -> String {
        if let Some(last_run) = self.last_run {
            let duration = last_run.elapsed();
            let seconds = duration.as_secs();

            if seconds < 60 {
                format!("{}s ago", seconds)
            } else if seconds < 3600 {
                format!("{}m ago", seconds / 60)
            } else {
                format!("{}h ago", seconds / 3600)
            }
        } else {
            "Never".to_string()
        }
    }
}

/// Manages skill state and activation
#[derive(Debug)]
pub struct SkillStateManager {
    /// All available skills
    pub skills: Vec<SkillState>,
    /// Currently running skills (by name)
    pub running: HashSet<String>,
}

impl SkillStateManager {
    /// Create a new skill state manager
    pub fn new() -> Self {
        Self {
            skills: Vec::new(),
            running: HashSet::new(),
        }
    }

    /// Load skills from disk
    pub async fn load_skills(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        // Use the existing loader
        let loader = super::loader::SkillLoader::new();
        let base_skills = loader.load_all()?;

        // Convert to skill states
        self.skills = base_skills.into_iter().map(SkillState::from_base).collect();

        tracing::debug!("Loaded {} skill states", self.skills.len());
        Ok(())
    }

    /// Find skill by name
    pub fn find_skill(&self, name: &str) -> Option<&SkillState> {
        self.skills
            .iter()
            .find(|s| s.base.name.eq_ignore_ascii_case(name))
    }

    /// Find mutable skill by name
    pub fn find_skill_mut(&mut self, name: &str) -> Option<&mut SkillState> {
        self.skills
            .iter_mut()
            .find(|s| s.base.name.eq_ignore_ascii_case(name))
    }

    /// Activate a skill for auto-triggering
    pub fn activate_skill(&mut self, name: &str) -> Result<(), String> {
        let skill = self
            .find_skill_mut(name)
            .ok_or_else(|| format!("Skill '{}' not found", name))?;

        skill.auto_enabled = true;
        skill.status = SkillStatus::Active;
        tracing::info!("Activated skill: {}", name);
        Ok(())
    }

    /// Deactivate a skill
    pub fn deactivate_skill(&mut self, name: &str) -> Result<(), String> {
        let skill = self
            .find_skill_mut(name)
            .ok_or_else(|| format!("Skill '{}' not found", name))?;

        skill.auto_enabled = false;
        skill.status = SkillStatus::Inactive;
        tracing::info!("Deactivated skill: {}", name);
        Ok(())
    }

    /// Get skills that should trigger on a condition
    pub fn get_triggered_skills(&self, condition: &TriggerCondition) -> Vec<&SkillState> {
        self.skills
            .iter()
            .filter(|s| s.should_trigger(condition))
            .collect()
    }

    /// Get active skills
    pub fn get_active_skills(&self) -> Vec<&SkillState> {
        self.skills.iter().filter(|s| s.auto_enabled).collect()
    }

    /// Generate skill context for LLM - includes instructions from all active skills
    /// that help the LLM determine when to use them
    pub fn get_llm_skill_context(&self) -> String {
        let active_skills = self.get_active_skills();

        if active_skills.is_empty() {
            return String::new();
        }

        let mut context = String::from("# Active Skills\n\n");
        context
            .push_str("The following skills are available and should be used when relevant:\n\n");

        for skill in active_skills {
            context.push_str(&skill.base.to_llm_context());
            context.push_str("---\n\n");
        }

        context
    }

    /// Mark skill as running
    pub fn mark_running(&mut self, name: &str) -> Result<(), String> {
        let skill = self
            .find_skill_mut(name)
            .ok_or_else(|| format!("Skill '{}' not found", name))?;

        skill.mark_running();
        self.running.insert(name.to_lowercase());
        Ok(())
    }

    /// Mark skill as completed
    pub fn mark_completed(&mut self, name: &str, success: bool, error: Option<String>) {
        if let Some(skill) = self.find_skill_mut(name) {
            if success {
                skill.mark_success();
            } else if let Some(err) = error {
                skill.mark_error(err);
            }
            self.running.remove(&name.to_lowercase());
        }
    }

    /// Get skill count
    pub fn skill_count(&self) -> usize {
        self.skills.len()
    }

    /// Get active skill count
    pub fn active_count(&self) -> usize {
        self.skills.iter().filter(|s| s.auto_enabled).count()
    }
}

impl Default for SkillStateManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::skills::loader::{Skill, SkillCategory};
    use std::path::PathBuf;

    #[test]
    fn test_skill_state_creation() {
        let base_skill = Skill {
            name: "test-skill".to_string(),
            description: "A test skill".to_string(),
            category: SkillCategory::Agent,
            parameters: vec![],
            commands: vec![],
            instructions: String::new(),
            path: PathBuf::from("/test"),
        };

        let state = SkillState::from_base(base_skill);
        assert!(!state.auto_enabled);
        assert_eq!(state.status, SkillStatus::Inactive);
        assert_eq!(state.run_count, 0);
    }

    #[test]
    fn test_skill_toggle_auto() {
        let base_skill = Skill {
            name: "test-skill".to_string(),
            description: "A test skill".to_string(),
            category: SkillCategory::Agent,
            parameters: vec![],
            commands: vec![],
            instructions: String::new(),
            path: PathBuf::from("/test"),
        };

        let mut state = SkillState::from_base(base_skill);
        assert!(!state.auto_enabled);

        state.toggle_auto();
        assert!(state.auto_enabled);
        assert_eq!(state.status, SkillStatus::Active);

        state.toggle_auto();
        assert!(!state.auto_enabled);
        assert_eq!(state.status, SkillStatus::Inactive);
    }

    #[test]
    fn test_should_trigger() {
        let base_skill = Skill {
            name: "test-skill".to_string(),
            description: "A test skill".to_string(),
            category: SkillCategory::Agent,
            parameters: vec![],
            commands: vec![],
            instructions: String::new(),
            path: PathBuf::from("/test"),
        };

        let mut state = SkillState::from_base(base_skill);
        state.triggers = vec![TriggerCondition::OnCommit, TriggerCondition::OnError];
        state.auto_enabled = true;

        assert!(state.should_trigger(&TriggerCondition::OnCommit));
        assert!(state.should_trigger(&TriggerCondition::OnError));
        assert!(!state.should_trigger(&TriggerCondition::OnFileChange));
    }

    #[test]
    fn test_skill_lifecycle() {
        let base_skill = Skill {
            name: "test-skill".to_string(),
            description: "A test skill".to_string(),
            category: SkillCategory::Agent,
            parameters: vec![],
            commands: vec![],
            instructions: String::new(),
            path: PathBuf::from("/test"),
        };

        let mut state = SkillState::from_base(base_skill);
        assert_eq!(state.run_count, 0);

        state.mark_running();
        assert_eq!(state.status, SkillStatus::Running);

        state.mark_success();
        assert_eq!(state.run_count, 1);
        assert_eq!(state.status, SkillStatus::Inactive);

        state.mark_error("Test error".to_string());
        assert!(matches!(state.status, SkillStatus::Error(_)));
    }
}
