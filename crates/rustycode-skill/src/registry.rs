use crate::metadata::parse_frontmatter_fields;
use crate::types::{
    ActivationMode, ActivationSpec, ExecutionContext, LifecycleState, SkillDefinition,
    SkillEffortLevel, SkillId, SkillQuality, SkillSource,
};
use anyhow::{Context, Result};
use std::collections::HashMap;
use std::path::Path;
use tracing::debug;

pub struct SkillRegistry {
    skills: HashMap<SkillId, SkillDefinition>,
    conditional: HashMap<SkillId, SkillDefinition>,
}

impl SkillRegistry {
    pub fn new() -> Self {
        Self {
            skills: HashMap::new(),
            conditional: HashMap::new(),
        }
    }

    /// Load all SKILL.md files from a directory, assigning them the given source.
    pub fn load_from_dir(&mut self, dir: &Path, source: SkillSource) -> Result<()> {
        if !dir.exists() {
            debug!("Skill directory does not exist: {:?}", dir);
            return Ok(());
        }

        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            let entry_path = entry.path();
            if !entry_path.is_dir() {
                continue;
            }

            let skill_md = entry_path.join("SKILL.md");
            if !skill_md.exists() {
                continue;
            }

            match self.parse_skill_file(&skill_md, source) {
                Ok(def) => {
                    let id = def.id.clone();
                    if def.activation.mode == ActivationMode::Conditional {
                        debug!(
                            "Registered conditional skill: {} (source: {:?})",
                            id, source
                        );
                        self.conditional.insert(id, def);
                    } else {
                        debug!("Registered skill: {} (source: {:?})", id, source);
                        self.skills.insert(id, def);
                    }
                }
                Err(e) => {
                    debug!("Failed to parse skill at {:?}: {}", skill_md, e);
                }
            }
        }

        Ok(())
    }

    /// Register a single bundled skill definition.
    pub fn register_bundled(&mut self, def: SkillDefinition) {
        let id = def.id.clone();
        if def.activation.mode == ActivationMode::Conditional {
            self.conditional.insert(id, def);
        } else {
            self.skills.insert(id, def);
        }
    }

    /// Register an MCP-sourced skill.
    pub fn register_mcp(&mut self, def: SkillDefinition) {
        let id = def.id.clone();
        if def.activation.mode == ActivationMode::Conditional {
            self.conditional.insert(id, def);
        } else {
            self.skills.insert(id, def);
        }
    }

    /// Get a skill by ID (active skills only).
    pub fn get(&self, id: &str) -> Option<&SkillDefinition> {
        self.skills.get(id)
    }

    /// Get all active skills.
    pub fn get_all(&self) -> Vec<&SkillDefinition> {
        self.skills.values().collect()
    }

    /// Get all conditional (latent) skills.
    pub fn get_conditional(&self) -> Vec<&SkillDefinition> {
        self.conditional.values().collect()
    }

    /// Promote a conditional skill to active after its activation conditions are met.
    pub fn promote_conditional(&mut self, id: &str) -> Option<SkillDefinition> {
        let def = self.conditional.remove(id)?;
        let mut promoted = def;
        promoted.lifecycle_state = LifecycleState::Active;
        self.skills.insert(id.to_string(), promoted);
        self.skills.get(id).cloned()
    }

    /// Number of active skills.
    pub fn active_count(&self) -> usize {
        self.skills.len()
    }

    /// Number of conditional (latent) skills.
    pub fn conditional_count(&self) -> usize {
        self.conditional.len()
    }

    /// Clear all registered skills.
    pub fn clear(&mut self) {
        self.skills.clear();
        self.conditional.clear();
    }

    #[allow(clippy::unused_self)]
    fn parse_skill_file(&self, path: &Path, source: SkillSource) -> Result<SkillDefinition> {
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read skill file: {}", path.display()))?;

        let (fm_str, body) = rustycode_protocol::frontmatter::split_frontmatter(&content)
            .unwrap_or_else(|| (String::new(), content.clone()));

        let fm = rustycode_protocol::frontmatter::parse_frontmatter_map(&fm_str);

        let fallback_description = extract_first_paragraph(&body);

        let parsed = parse_frontmatter_fields(&fm, &fallback_description);

        let name = if parsed.name.is_empty() {
            path.parent().and_then(|p| p.file_name()).map_or_else(
                || "unknown".to_string(),
                |n| n.to_string_lossy().into_owned(),
            )
        } else {
            parsed.name.clone()
        };

        let activation = if !parsed.paths.is_empty() {
            ActivationSpec::conditional(parsed.paths)
        } else if parsed.user_invocable {
            ActivationSpec::manual()
        } else {
            ActivationSpec::always()
        };

        let effort = match parsed.effort.as_deref() {
            Some("low") => SkillEffortLevel::Low,
            Some("high") => SkillEffortLevel::High,
            Some("max") => SkillEffortLevel::Max,
            _ => SkillEffortLevel::Medium,
        };

        let exec_ctx = match parsed.context.as_deref() {
            Some("fork") => ExecutionContext::Fork,
            _ => ExecutionContext::Inline,
        };

        let lifecycle_state = if activation.mode == ActivationMode::Conditional {
            LifecycleState::Latent
        } else {
            LifecycleState::Active
        };

        Ok(SkillDefinition {
            id: name.clone(),
            name,
            description: parsed.description,
            when_to_use: parsed.when_to_use.unwrap_or_default(),
            source,
            version: parsed.version,
            activation,
            effort,
            context: exec_ctx,
            procedure: None,
            allowed_tools: parsed.allowed_tools,
            user_invocable: parsed.user_invocable,
            model_invocable: parsed.model_invocable,
            agent: parsed.agent,
            model_override: parsed.model_override,
            argument_hint: parsed.argument_hint,
            categories: parsed.categories,
            excludes: parsed.excludes,
            gotchas: parsed.gotchas,
            quality: SkillQuality::default_new(),
            lifecycle_state,
            content_path: path.to_path_buf(),
            content: Some(body),
        })
    }
}

impl Default for SkillRegistry {
    fn default() -> Self {
        Self::new()
    }
}

fn extract_first_paragraph(content: &str) -> String {
    content
        .lines()
        .skip_while(|line| !line.starts_with('#'))
        .skip(1)
        .find(|line| !line.trim().is_empty())
        .map(|s| s.trim().to_string())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn temp_dir() -> std::path::PathBuf {
        let path =
            std::env::temp_dir().join(format!("rustycode-skill-registry-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn new_registry_is_empty() {
        let reg = SkillRegistry::new();
        assert_eq!(reg.active_count(), 0);
        assert_eq!(reg.conditional_count(), 0);
    }

    #[test]
    fn default_registry_is_empty() {
        let reg = SkillRegistry::default();
        assert_eq!(reg.active_count(), 0);
    }

    #[test]
    fn load_from_dir_with_skill() {
        let dir = temp_dir();
        let skill_dir = dir.join("my-skill");
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(
            skill_dir.join("SKILL.md"),
            "---\nname: my-skill\neffort: high\n---\n# My Skill\n\nDoes things.\n",
        )
        .unwrap();

        let mut reg = SkillRegistry::new();
        reg.load_from_dir(&dir, SkillSource::User).unwrap();
        assert_eq!(reg.active_count(), 1);

        let skill = reg.get("my-skill").unwrap();
        assert_eq!(skill.name, "my-skill");
        assert_eq!(skill.description, "Does things.");
        assert_eq!(skill.source, SkillSource::User);
        assert_eq!(skill.effort, SkillEffortLevel::High);
    }

    #[test]
    fn load_from_dir_conditional_skill() {
        let dir = temp_dir();
        let skill_dir = dir.join("rust-skill");
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(
            skill_dir.join("SKILL.md"),
            "---\nname: rust-skill\npaths:\n  - \"*.rs\"\n  - \"src/**/*.rs\"\n---\n# Rust Skill\n\nFor Rust files.\n",
        )
        .unwrap();

        let mut reg = SkillRegistry::new();
        reg.load_from_dir(&dir, SkillSource::Project).unwrap();
        assert_eq!(reg.active_count(), 0);
        assert_eq!(reg.conditional_count(), 1);

        let conditional = reg.get_conditional();
        assert_eq!(conditional.len(), 1);
        assert_eq!(conditional[0].name, "rust-skill");
        assert_eq!(conditional[0].activation.mode, ActivationMode::Conditional);
        assert!(conditional[0]
            .activation
            .paths
            .contains(&"*.rs".to_string()));
    }

    #[test]
    fn promote_conditional() {
        let dir = temp_dir();
        let skill_dir = dir.join("cond");
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(
            skill_dir.join("SKILL.md"),
            "---\nname: cond\npaths:\n  - \"*.py\"\n---\n# Cond\n\nPython skill.\n",
        )
        .unwrap();

        let mut reg = SkillRegistry::new();
        reg.load_from_dir(&dir, SkillSource::Project).unwrap();
        assert_eq!(reg.conditional_count(), 1);

        let promoted = reg.promote_conditional("cond");
        assert!(promoted.is_some());
        assert_eq!(reg.active_count(), 1);
        assert_eq!(reg.conditional_count(), 0);

        let skill = reg.get("cond").unwrap();
        assert_eq!(skill.lifecycle_state, LifecycleState::Active);
    }

    #[test]
    fn promote_nonexistent_returns_none() {
        let mut reg = SkillRegistry::new();
        assert!(reg.promote_conditional("nope").is_none());
    }

    #[test]
    fn load_from_nonexistent_dir() {
        let mut reg = SkillRegistry::new();
        let result = reg.load_from_dir(Path::new("/nonexistent/path"), SkillSource::Bundled);
        assert!(result.is_ok());
        assert_eq!(reg.active_count(), 0);
    }

    #[test]
    fn clear_removes_all() {
        let dir = temp_dir();
        let skill_dir = dir.join("s1");
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(
            skill_dir.join("SKILL.md"),
            "---\nname: s1\n---\n# S1\n\nSkill.\n",
        )
        .unwrap();

        let mut reg = SkillRegistry::new();
        reg.load_from_dir(&dir, SkillSource::User).unwrap();
        assert_eq!(reg.active_count(), 1);

        reg.clear();
        assert_eq!(reg.active_count(), 0);
        assert_eq!(reg.conditional_count(), 0);
    }

    #[test]
    fn register_bundled() {
        let mut reg = SkillRegistry::new();
        let def = SkillDefinition {
            id: "bundled-test".to_string(),
            name: "bundled-test".to_string(),
            description: "A bundled skill".to_string(),
            when_to_use: String::new(),
            source: SkillSource::Bundled,
            version: String::new(),
            activation: ActivationSpec::always(),
            effort: SkillEffortLevel::default(),
            context: ExecutionContext::default(),
            procedure: None,
            allowed_tools: vec![],
            user_invocable: true,
            model_invocable: true,
            agent: None,
            model_override: None,
            argument_hint: None,
            categories: vec![],
            excludes: vec![],
            gotchas: vec![],
            quality: SkillQuality::default(),
            lifecycle_state: LifecycleState::Active,
            content_path: std::path::PathBuf::new(),
            content: None,
        };
        reg.register_bundled(def);
        assert_eq!(reg.active_count(), 1);
        assert!(reg.get("bundled-test").is_some());
    }

    #[test]
    fn skill_without_frontmatter_uses_dir_name() {
        let dir = temp_dir();
        let skill_dir = dir.join("fallback-name");
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(
            skill_dir.join("SKILL.md"),
            "# Fallback Name\n\nUses dir name.\n",
        )
        .unwrap();

        let mut reg = SkillRegistry::new();
        reg.load_from_dir(&dir, SkillSource::User).unwrap();
        assert_eq!(reg.active_count(), 1);
        let skill = reg.get("fallback-name").unwrap();
        assert_eq!(skill.name, "fallback-name");
        assert_eq!(skill.description, "Uses dir name.");
    }

    #[test]
    fn get_all_returns_all_active() {
        let dir = temp_dir();
        for name in &["alpha", "beta", "gamma"] {
            let skill_dir = dir.join(name);
            fs::create_dir_all(&skill_dir).unwrap();
            fs::write(
                skill_dir.join("SKILL.md"),
                format!("---\nname: {name}\n---\n# {name}\n\nSkill.\n"),
            )
            .unwrap();
        }

        let mut reg = SkillRegistry::new();
        reg.load_from_dir(&dir, SkillSource::User).unwrap();
        assert_eq!(reg.get_all().len(), 3);
    }
}
