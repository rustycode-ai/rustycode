//! Domain context loading and management.
//!
//! Reads project-specific domain context from `.rustycode/domain.yaml`,
//! providing architecture rules, preferred patterns, build/test commands,
//! and autonomy level configuration. Composes a `ProjectDescriptor` with
//! an `AutonomyConfig` to provide a unified domain context for the agent.

use anyhow::Result;
use std::path::{Path, PathBuf};

use crate::autonomy::{AutonomyConfig, AutonomyLevel};
use crate::project_descriptor::ProjectDescriptor;

/// Project-specific domain context loaded from `.rustycode/domain.yaml`.
///
/// Contains everything the agent needs to know about the project's
/// conventions, architecture rules, and autonomy configuration.
/// Composes a `ProjectDescriptor` (raw data) with an `AutonomyConfig`
/// (permission gating).
#[derive(Debug, Clone, Default)]
pub struct DomainContext {
    /// The underlying project descriptor.
    pub descriptor: ProjectDescriptor,

    /// The resolved autonomy configuration.
    pub autonomy: AutonomyConfig,

    /// Path the domain context was loaded from.
    pub loaded_from: Option<PathBuf>,
}

impl DomainContext {
    /// Load domain context from a YAML file.
    pub fn load_from_yaml(path: &Path) -> Result<Self> {
        let descriptor = ProjectDescriptor::load_from_file(path)?;
        let autonomy = AutonomyConfig {
            default_level: descriptor.autonomy_default,
            overrides: descriptor.autonomy_overrides.clone(),
            ..AutonomyConfig::default()
        };

        Ok(Self {
            loaded_from: Some(path.to_path_buf()),
            descriptor,
            autonomy,
        })
    }

    /// Save domain context to a YAML file.
    ///
    /// Syncs the autonomy config back into the descriptor before persisting
    /// so that the saved file is consistent with the in-memory state.
    pub fn save_to_yaml(&self, path: &Path) -> Result<()> {
        let mut descriptor = self.descriptor.clone();
        descriptor.autonomy_default = self.autonomy.default_level;
        descriptor
            .autonomy_overrides
            .clone_from(&self.autonomy.overrides);
        descriptor.save_to_file(path)
    }

    /// Validate the domain context for consistency.
    pub fn validate(&self) -> Result<Vec<crate::project_descriptor::ValidationWarning>> {
        self.descriptor.validate()
    }

    /// Create a default domain context for a given project path.
    ///
    /// Produces a minimal context with the project name extracted from
    /// the directory name.
    pub fn default_for_path(project_dir: &Path) -> Self {
        let descriptor = ProjectDescriptor::default_for_path(project_dir);
        Self {
            descriptor,
            autonomy: AutonomyConfig::default(),
            loaded_from: None,
        }
    }

    /// Discover domain.yaml by searching in the given directory.
    ///
    /// Search order:
    /// 1. `<dir>/.rustycode/domain.yaml`
    /// 2. `<dir>/domain.yaml`
    ///
    /// Returns `Ok(None)` if no domain file is found.
    pub fn discover(dir: &Path) -> Result<Option<PathBuf>> {
        ProjectDescriptor::discover(dir)
    }

    /// Resolve the effective autonomy level for a given task type.
    ///
    /// Checks per-task overrides first, then falls back to the default.
    #[must_use]
    pub fn resolve_autonomy(&self, task_type: &str) -> AutonomyLevel {
        self.autonomy.resolve_level(task_type)
    }

    /// Generate a formatted prompt section from the domain context.
    ///
    /// Returns an empty string if the context has no useful information.
    #[must_use]
    pub fn to_prompt_section(&self) -> String {
        self.descriptor.to_prompt_section()
    }

    // --- Convenience accessors ---

    /// Project name.
    #[must_use]
    pub fn project_name(&self) -> &str {
        &self.descriptor.project_name
    }

    /// Primary programming language.
    #[must_use]
    pub fn language(&self) -> &str {
        &self.descriptor.language
    }

    /// Build commands.
    #[must_use]
    pub fn build_commands(&self) -> &[String] {
        &self.descriptor.build_commands
    }

    /// Test commands.
    #[must_use]
    pub fn test_commands(&self) -> &[String] {
        &self.descriptor.test_commands
    }

    /// Architecture rules.
    #[must_use]
    pub fn architecture_rules(&self) -> &[String] {
        &self.descriptor.architecture_rules
    }

    /// Preferred patterns.
    #[must_use]
    pub fn preferred_patterns(&self) -> &[String] {
        &self.descriptor.preferred_patterns
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use std::io::Write;

    fn temp_dir() -> tempfile::TempDir {
        tempfile::tempdir().unwrap()
    }

    #[test]
    fn load_domain_context_from_yaml() {
        let dir = temp_dir();
        let path = dir.path().join("domain.yaml");
        let mut f = std::fs::File::create(&path).unwrap();
        write!(
            f,
            r#"project_name: test-project
language: rust
build_commands:
  - cargo build
test_commands:
  - cargo test
architecture_rules:
  - "No unwrap in production code"
preferred_patterns:
  - builder-pattern
"#
        )
        .unwrap();

        let ctx = DomainContext::load_from_yaml(&path).unwrap();
        assert_eq!(ctx.project_name(), "test-project");
        assert_eq!(ctx.language(), "rust");
        assert_eq!(ctx.build_commands(), &["cargo build"]);
        assert_eq!(ctx.test_commands(), &["cargo test"]);
        assert_eq!(ctx.loaded_from, Some(path));
    }

    #[test]
    fn load_domain_context_missing_file() {
        let result = DomainContext::load_from_yaml(Path::new("/nonexistent/domain.yaml"));
        assert!(result.is_err());
    }

    #[test]
    fn load_domain_context_invalid_yaml() {
        let dir = temp_dir();
        let path = dir.path().join("domain.yaml");
        std::fs::write(&path, "invalid: [yaml: content").unwrap();
        let result = DomainContext::load_from_yaml(&path);
        assert!(result.is_err());
    }

    #[test]
    fn default_domain_context_safe_values() {
        let ctx = DomainContext::default();
        assert!(ctx.project_name().is_empty());
        assert!(ctx.language().is_empty());
        assert_eq!(ctx.autonomy.default_level, AutonomyLevel::L1);
        assert!(ctx.autonomy.overrides.is_empty());
    }

    #[test]
    fn default_for_path_extracts_project_name() {
        let ctx = DomainContext::default_for_path(Path::new("/home/user/my-project"));
        assert_eq!(ctx.project_name(), "my-project");
        assert!(ctx.loaded_from.is_none());
    }

    #[test]
    fn discover_domain_yaml_in_rustycode_dir() {
        let dir = temp_dir();
        let rustycode_dir = dir.path().join(".rustycode");
        std::fs::create_dir_all(&rustycode_dir).unwrap();
        std::fs::write(
            rustycode_dir.join("domain.yaml"),
            "project_name: discovered\nlanguage: go\n",
        )
        .unwrap();

        let path = DomainContext::discover(dir.path()).unwrap();
        assert!(path.is_some());
        let ctx = DomainContext::load_from_yaml(&path.unwrap()).unwrap();
        assert_eq!(ctx.project_name(), "discovered");
        assert_eq!(ctx.language(), "go");
    }

    #[test]
    fn discover_returns_none_when_absent() {
        let dir = temp_dir();
        let result = DomainContext::discover(dir.path()).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn resolve_autonomy_level_with_overrides() {
        let mut ctx = DomainContext::default();
        ctx.autonomy.default_level = AutonomyLevel::L2;
        ctx.autonomy.overrides = {
            let mut map = std::collections::HashMap::new();
            map.insert("code_review".to_string(), AutonomyLevel::L3);
            map.insert("database_migration".to_string(), AutonomyLevel::L0);
            map
        };

        assert_eq!(ctx.resolve_autonomy("code_review"), AutonomyLevel::L3);
        assert_eq!(
            ctx.resolve_autonomy("database_migration"),
            AutonomyLevel::L0
        );
        assert_eq!(ctx.resolve_autonomy("unknown"), AutonomyLevel::L2);
    }

    #[test]
    fn to_prompt_section_generates_formatted_context() {
        let mut ctx = DomainContext::default();
        ctx.descriptor.project_name = "my-api".to_string();
        ctx.descriptor.language = "typescript".to_string();
        ctx.descriptor.build_commands = vec!["npm run build".to_string()];
        ctx.descriptor.test_commands = vec!["npm test".to_string()];
        ctx.descriptor.architecture_rules = vec!["No business logic in controllers".to_string()];
        ctx.descriptor.preferred_patterns = vec!["repository-pattern".to_string()];

        let section = ctx.to_prompt_section();
        assert!(section.contains("my-api"));
        assert!(section.contains("typescript"));
        assert!(section.contains("npm run build"));
        assert!(section.contains("npm test"));
        assert!(section.contains("No business logic in controllers"));
        assert!(section.contains("repository-pattern"));
    }

    #[test]
    fn to_prompt_section_empty_returns_empty() {
        let ctx = DomainContext::default();
        let section = ctx.to_prompt_section();
        assert!(section.is_empty() || section.trim().is_empty());
    }

    #[test]
    fn save_and_reload_roundtrip() {
        let dir = temp_dir();
        let path = dir.path().join("domain.yaml");

        let mut ctx = DomainContext::default();
        ctx.descriptor.project_name = "roundtrip".to_string();
        ctx.descriptor.language = "python".to_string();
        ctx.descriptor.build_commands = vec!["python -m build".to_string()];
        ctx.descriptor.autonomy_default = AutonomyLevel::L3;
        ctx.autonomy.default_level = AutonomyLevel::L3;

        ctx.save_to_yaml(&path).unwrap();

        let loaded = DomainContext::load_from_yaml(&path).unwrap();
        assert_eq!(loaded.project_name(), "roundtrip");
        assert_eq!(loaded.language(), "python");
        assert_eq!(loaded.autonomy.default_level, AutonomyLevel::L3);
    }

    #[test]
    fn validate_returns_warnings() {
        let ctx = DomainContext::default();
        let warnings = ctx.validate().unwrap();
        assert!(!warnings.is_empty());
        let has_name_warning = warnings.iter().any(|w| w.field == "project_name");
        assert!(has_name_warning);
    }

    #[test]
    fn validate_complete_context_has_fewer_warnings() {
        let mut ctx = DomainContext::default();
        ctx.descriptor.project_name = "complete".to_string();
        ctx.descriptor.language = "rust".to_string();
        ctx.descriptor.tech_stack = vec![crate::project_descriptor::TechComponent {
            name: "tokio".to_string(),
            version: None,
            category: None,
        }];
        ctx.descriptor.build_commands = vec!["cargo build".to_string()];
        ctx.descriptor.test_commands = vec!["cargo test".to_string()];
        let warnings = ctx.validate().unwrap();
        assert!(warnings.is_empty());
    }
}
