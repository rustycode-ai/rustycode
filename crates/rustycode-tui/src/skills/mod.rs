//! Skills system for RustyCode TUI
//!
//! This module provides a VS Code-style command palette for skills with:
//! - Fuzzy search and filtering
//! - Keyboard navigation
//! - Skill parameter input
//! - Active skill status tracking
//!
//! ## Usage
//!
//! ```rust,ignore
//! use rustycode_tui::skills::{Skill, SkillLoader, fuzzy_match};
//!
//! // Load skills from filesystem
//! let loader = SkillLoader::new("/path/to/skills");
//! let skills = loader.load_all()?;
//!
//! // Search for skills
//! let results = fuzzy_match("code", &skills);
//! ```

pub mod composition;
pub mod loader;
pub mod param_input;
pub mod search;

pub use composition::{
    AliasTarget, CompositionManager, CompositionStep, ExecutionMode, SkillAlias, SkillComposition,
    SkillExecutionEntry, SkillTemplate, StepCondition,
};
pub use loader::{Skill, SkillCategory, SkillLoader};
pub use param_input::ParamInput;
pub use search::fuzzy_match;

// Additional modules (implemented by other tasks)
pub mod context_analyzer;
pub mod manager;
pub mod preferences;
pub mod suggester;

// Skill lifecycle management modules
pub mod activation;
pub mod as_tool;
pub mod installer;
pub mod lifecycle;
pub mod updater;

// Re-exports for suggestion system
pub use context_analyzer::{ContextAnalyzer, SkillTrigger};
pub use manager::{SkillStateManager, SkillStatus, TriggerCondition};
pub use preferences::{SuggestionFrequency, SuggestionPreferences};
pub use suggester::{SkillSuggester, SkillSuggestion};

// Re-exports for lifecycle management
pub use activation::{
    activate_skill, activate_skills, configure_skills, deactivate_all, deactivate_skill,
    get_active_skills, get_skill_triggers, is_active, set_skill_triggers, sync_activation_state,
    toggle_skill,
};
pub use installer::{
    install_skill, is_installed, list_installed_skills, load_installed_skills, uninstall_skill,
    update_repository, validate_skill, verify_skill,
};
pub use lifecycle::{InstallationMetadata, SkillLifecycle, SkillLifecycleState, SkillStatistics};
pub use updater::{
    check_all_updates, check_for_updates, get_update_history, get_update_stats,
    has_uncommitted_changes, rollback_skill, update_all_skills, update_skill, UpdateInfo,
    UpdateStats,
};
