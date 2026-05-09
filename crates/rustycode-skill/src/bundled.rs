use std::fmt::Write;

use crate::types::{
    ActivationSpec, ExecutionContext, LifecycleState, SkillDefinition, SkillEffortLevel,
    SkillQuality, SkillSource,
};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillifyRequest {
    pub name: String,
    pub description: String,
    pub when_to_use: String,
    pub steps: Vec<String>,
    pub arguments: Vec<String>,
    pub allowed_tools: Vec<String>,
    pub target_dir: TargetDir,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TargetDir {
    Project,
    User,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillifyResult {
    pub skill_id: String,
    pub path: PathBuf,
    pub content: String,
}

pub struct SkillifyBuilder {
    name: String,
    description: String,
    when_to_use: String,
    steps: Vec<String>,
    arguments: Vec<String>,
    allowed_tools: Vec<String>,
}

impl SkillifyBuilder {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            description: String::new(),
            when_to_use: String::new(),
            steps: Vec::new(),
            arguments: Vec::new(),
            allowed_tools: Vec::new(),
        }
    }

    pub fn description(mut self, desc: &str) -> Self {
        self.description = desc.to_string();
        self
    }

    pub fn when_to_use(mut self, when: &str) -> Self {
        self.when_to_use = when.to_string();
        self
    }

    pub fn step(mut self, step: &str) -> Self {
        self.steps.push(step.to_string());
        self
    }

    pub fn argument(mut self, arg: &str) -> Self {
        self.arguments.push(arg.to_string());
        self
    }

    pub fn allowed_tool(mut self, tool: &str) -> Self {
        self.allowed_tools.push(tool.to_string());
        self
    }

    pub fn build(self) -> SkillDefinition {
        let content = self.generate_markdown();

        SkillDefinition {
            id: self.name.clone(),
            name: self.name.clone(),
            description: self.description.clone(),
            when_to_use: self.when_to_use.clone(),
            source: SkillSource::Dynamic,
            version: "1.0".to_string(),
            activation: ActivationSpec::manual(),
            effort: SkillEffortLevel::Medium,
            context: ExecutionContext::Inline,
            procedure: None,
            allowed_tools: self.allowed_tools,
            user_invocable: true,
            model_invocable: true,
            agent: None,
            model_override: None,
            argument_hint: if self.arguments.is_empty() {
                None
            } else {
                Some(format!("<{}>", self.arguments.join("> <")))
            },
            categories: vec![],
            excludes: vec![],
            gotchas: vec![],
            quality: SkillQuality::default_new(),
            lifecycle_state: LifecycleState::Active,
            content_path: PathBuf::new(),
            content: Some(content),
        }
    }

    pub fn generate_markdown(&self) -> String {
        let mut md = String::new();

        md.push_str("---\n");
        let _ = writeln!(md, "name: {}", self.name);
        if !self.description.is_empty() {
            let _ = writeln!(md, "description: \"{}\"", self.description);
        }
        md.push_str("version: \"1.0\"\n");
        md.push_str("effort: medium\n");
        if !self.when_to_use.is_empty() {
            let _ = writeln!(md, "when-to-use: \"{}\"", self.when_to_use);
        }
        if !self.arguments.is_empty() {
            md.push_str("arguments:\n");
            for arg in &self.arguments {
                let _ = writeln!(md, "  - {arg}");
            }
        }
        if !self.allowed_tools.is_empty() {
            md.push_str("allowed-tools:\n");
            for tool in &self.allowed_tools {
                let _ = writeln!(md, "  - {tool}");
            }
        }
        md.push_str("---\n\n");
        let _ = writeln!(md, "# {}\n", self.name);

        if !self.description.is_empty() {
            let _ = writeln!(md, "{}\n", self.description);
        }

        if !self.steps.is_empty() {
            md.push_str("## Steps\n\n");
            for (i, step) in self.steps.iter().enumerate() {
                let _ = writeln!(md, "### {}. {step}\n", i + 1);
            }
        }

        md
    }
}

pub fn write_skill_to_dir(
    skill: &SkillDefinition,
    base_dir: &Path,
) -> anyhow::Result<SkillifyResult> {
    let skill_dir = base_dir.join(&skill.id);
    std::fs::create_dir_all(&skill_dir)?;

    let content = skill.content.as_deref().unwrap_or("");
    let skill_md = skill_dir.join("SKILL.md");
    std::fs::write(&skill_md, content)?;

    Ok(SkillifyResult {
        skill_id: skill.id.clone(),
        path: skill_md,
        content: content.to_string(),
    })
}

pub fn bundled_skills() -> Vec<SkillDefinition> {
    vec![
        SkillifyBuilder::new("debug")
            .description("Systematic debugging tool using instrumentation for stepping, variable inspection, and stack traces.")
            .when_to_use("Use when encountering bugs, test failures, or unexpected behavior.")
            .allowed_tool("Bash")
            .allowed_tool("Read")
            .allowed_tool("Grep")
            .build(),
        SkillifyBuilder::new("worktree")
            .description("Manage isolated development environments using git worktrees.")
            .when_to_use("Use when testing, managing concurrent tasks, or isolating sandbox environments.")
            .allowed_tool("worktree_create")
            .allowed_tool("worktree_list")
            .allowed_tool("worktree_delete")
            .build(),
        SkillifyBuilder::new("research")
            .description("Conduct comprehensive web research by searching for information and fetching web content.")
            .when_to_use("Use when performing deep research, gathering market data, or verifying facts.")
            .allowed_tool("web_search")
            .allowed_tool("WebFetch")
            .build(),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builder_minimal() {
        let skill = SkillifyBuilder::new("my-skill").build();
        assert_eq!(skill.id, "my-skill");
        assert_eq!(skill.name, "my-skill");
        assert_eq!(skill.source, SkillSource::Dynamic);
        assert_eq!(skill.activation.mode, crate::types::ActivationMode::Manual);
        assert!(skill.user_invocable);
    }

    #[test]
    fn builder_full() {
        let skill = SkillifyBuilder::new("full-skill")
            .description("A full skill")
            .when_to_use("Use when needed")
            .step("Do thing 1")
            .step("Do thing 2")
            .argument("file_path")
            .allowed_tool("Read")
            .build();

        assert_eq!(skill.description, "A full skill");
        assert_eq!(skill.when_to_use, "Use when needed");
        assert_eq!(skill.allowed_tools, vec!["Read"]);
        assert_eq!(skill.argument_hint, Some("<file_path>".to_string()));
    }

    #[test]
    fn generate_markdown_has_frontmatter() {
        let md = SkillifyBuilder::new("test")
            .description("Test skill")
            .generate_markdown();

        assert!(md.starts_with("---"));
        assert!(md.contains("name: test"));
        assert!(md.contains("# test"));
        assert!(md.contains("Test skill"));
    }

    #[test]
    fn generate_markdown_with_steps() {
        let md = SkillifyBuilder::new("stepped")
            .step("First step")
            .step("Second step")
            .generate_markdown();

        assert!(md.contains("## Steps"));
        assert!(md.contains("### 1. First step"));
        assert!(md.contains("### 2. Second step"));
    }

    #[test]
    fn generate_markdown_with_arguments() {
        let md = SkillifyBuilder::new("arg-skill")
            .argument("path")
            .argument("mode")
            .generate_markdown();

        assert!(md.contains("arguments:"));
        assert!(md.contains("  - path"));
        assert!(md.contains("  - mode"));
    }

    #[test]
    fn generate_markdown_with_tools() {
        let md = SkillifyBuilder::new("tool-skill")
            .allowed_tool("Bash")
            .allowed_tool("Read")
            .generate_markdown();

        assert!(md.contains("allowed-tools:"));
        assert!(md.contains("  - bash"));
        assert!(md.contains("  - read_file"));
    }

    #[test]
    fn generate_markdown_minimal() {
        let md = SkillifyBuilder::new("bare").generate_markdown();
        assert!(md.starts_with("---"));
        assert!(md.contains("name: bare"));
        assert!(md.contains("# bare"));
        assert!(!md.contains("arguments:"));
        assert!(!md.contains("allowed-tools:"));
    }

    #[test]
    fn write_skill_creates_file() {
        let dir = std::env::temp_dir().join(format!("rustycode-skillify-{}", uuid::Uuid::new_v4()));
        let skill = SkillifyBuilder::new("written-skill")
            .description("Written to disk")
            .build();

        let result = write_skill_to_dir(&skill, &dir).unwrap();
        assert_eq!(result.skill_id, "written-skill");
        assert!(result.path.exists());
        assert!(result.path.ends_with("SKILL.md"));

        let content = std::fs::read_to_string(&result.path).unwrap();
        assert!(content.contains("name: written-skill"));

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn builder_no_args_no_hint() {
        let skill = SkillifyBuilder::new("no-args").build();
        assert!(skill.argument_hint.is_none());
    }

    #[test]
    fn builder_with_args_formats_hint() {
        let skill = SkillifyBuilder::new("deploy")
            .argument("environment")
            .argument("region")
            .build();
        assert_eq!(
            skill.argument_hint.as_deref(),
            Some("<environment> <region>")
        );
    }

    #[test]
    fn builder_default_source_is_dynamic() {
        let skill = SkillifyBuilder::new("test").build();
        assert_eq!(skill.source, SkillSource::Dynamic);
    }

    #[test]
    fn builder_default_activation_is_manual() {
        let skill = SkillifyBuilder::new("test").build();
        assert_eq!(skill.activation.mode, crate::types::ActivationMode::Manual);
    }

    #[test]
    fn builder_with_steps_includes_numbered() {
        let md = SkillifyBuilder::new("multi-step")
            .step("First thing")
            .step("Second thing")
            .step("Third thing")
            .generate_markdown();
        assert!(md.contains("### 1. First thing"));
        assert!(md.contains("### 2. Second thing"));
        assert!(md.contains("### 3. Third thing"));
    }

    #[test]
    fn builder_with_when_to_use() {
        let md = SkillifyBuilder::new("cond")
            .when_to_use("When you need to deploy")
            .generate_markdown();
        assert!(md.contains("when-to-use:"));
        assert!(md.contains("When you need to deploy"));
    }

    #[test]
    fn write_skill_idempotent() {
        let dir =
            std::env::temp_dir().join(format!("rustycode-skillify-idem-{}", uuid::Uuid::new_v4()));
        let skill = SkillifyBuilder::new("idem").description("v1").build();
        write_skill_to_dir(&skill, &dir).unwrap();

        // Write again with same ID
        let skill2 = SkillifyBuilder::new("idem").description("v2").build();
        let result = write_skill_to_dir(&skill2, &dir).unwrap();
        let content = std::fs::read_to_string(&result.path).unwrap();
        assert!(content.contains("v2"));

        std::fs::remove_dir_all(&dir).ok();
    }
}
