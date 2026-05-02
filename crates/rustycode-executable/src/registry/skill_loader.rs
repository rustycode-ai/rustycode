//! Loader for Claude Code skills from ~/.claude/skills

use crate::{ExecutableUnit, ExecutableError, UnitCapabilities, AdvancedToolMetadata, ExecutionMode, UnitSource};
use crate::registry::loaders::UnitLoader;
use async_trait::async_trait;
use std::path::PathBuf;

/// Loads skills from the Claude Code skills directory
pub struct SkillLoader {
    skills_dir: PathBuf,
}

impl SkillLoader {
    #[allow(clippy::missing_const_for_fn)]
    pub fn new(skills_dir: PathBuf) -> Self {
        Self { skills_dir }
    }

    /// Scan directory for skill definitions and create units
    fn scan_skills(&self) -> Vec<ExecutableUnit> {
        let mut units = Vec::new();

        if let Ok(entries) = std::fs::read_dir(&self.skills_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() || path.extension().is_some_and(|ext| ext == "md") {
                    let name = path
                        .file_stem()
                        .and_then(|s| s.to_str())
                        .unwrap_or("unknown")
                        .to_string();

                    units.push(ExecutableUnit {
                        id: format!("skill:{name}"),
                        name: name.clone(),
                        description: format!("Skill: {name}"),
                        capabilities: UnitCapabilities {
                            can_execute_directly: false,
                            can_bundle_knowledge: true,
                            can_reason_autonomously: false,
                        },
                        advanced_metadata: AdvancedToolMetadata {
                            examples: vec![],
                            defer_loading: true,
                            search_hints: vec![name.clone()],
                            execution_strategy: ExecutionMode::Bundled,
                            result_processor: None,
                        },
                        handler: std::sync::Arc::new(crate::types::callable::NoOpCallable),
                        source: UnitSource::InstalledSkill {
                            path: path.to_string_lossy().to_string(),
                            version: None,
                        },
                        schema: None,
                        tags: vec!["skill".to_string()],
                        version: None,
                    });
                }
            }
        }

        units
    }
}

#[async_trait]
impl UnitLoader for SkillLoader {
    fn name(&self) -> &'static str {
        "skills"
    }

    async fn load_units(&self) -> Result<Vec<ExecutableUnit>, ExecutableError> {
        Ok(self.scan_skills())
    }
}
