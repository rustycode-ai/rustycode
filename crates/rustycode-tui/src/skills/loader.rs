//! Skill loader from filesystem
//!
//! Loads skills from `.claude/skills/` directory and parses `skill.md` files.

use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};

/// Skill loaded from filesystem
#[derive(Debug, Clone)]
pub struct Skill {
    /// Skill name (e.g., "code-review")
    pub name: String,

    /// Human-readable description (e.g., "Review code for quality and security")
    pub description: String,

    /// Skill category (agent, testing, security, etc.)
    pub category: SkillCategory,

    /// Parameters required by this skill
    pub parameters: Vec<SkillParameter>,

    /// Commands provided by this skill
    pub commands: Vec<SkillCommand>,

    /// Instructions for the LLM - when and how to use this skill
    pub instructions: String,

    /// Path to skill directory
    pub path: PathBuf,
}

/// Command provided by a skill
#[derive(Debug, Clone)]
pub struct SkillCommand {
    /// Command name (e.g., "progress")
    pub name: String,

    /// Command invocation (e.g., "/orchestra-progress" or "orchestra:progress")
    pub invocation: String,

    /// Human-readable description
    pub description: String,
}

/// Skill category with icon mapping
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum SkillCategory {
    /// ⚡ Agent skills (code-review, planner, etc.)
    Agent,

    /// 🧪 Testing skills (tdd-guide, e2e-runner, etc.)
    Testing,

    /// 🔒 Security skills (security-review, etc.)
    Security,

    /// 📝 Planning skills (planner, architect, etc.)
    Planning,

    /// 🎨 Design skills (frontend-design, pptx, xlsx, etc.)
    Design,

    /// 🐛 Debugging skills (build-error-resolver, etc.)
    Debug,

    /// 🔧 Tool/utility skills
    Tools,
}

impl SkillCategory {
    /// Get icon for this category
    pub fn icon(self) -> &'static str {
        match self {
            Self::Agent => "⚡",
            Self::Testing => "🧪",
            Self::Security => "🔒",
            Self::Planning => "📝",
            Self::Design => "🎨",
            Self::Debug => "🐛",
            Self::Tools => "🔧",
        }
    }

    /// Parse from string
    pub fn parse_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "agent" => Some(Self::Agent),
            "testing" => Some(Self::Testing),
            "security" => Some(Self::Security),
            "planning" => Some(Self::Planning),
            "design" => Some(Self::Design),
            "debug" => Some(Self::Debug),
            "tools" => Some(Self::Tools),
            _ => None,
        }
    }
}

/// Skill parameter definition
#[derive(Debug, Clone)]
pub struct SkillParameter {
    /// Parameter name (e.g., "target")
    pub name: String,

    /// Parameter description (e.g., "file or directory to review")
    pub description: String,

    /// Whether parameter is required
    pub required: bool,

    /// Parameter type (text, file, directory, etc.)
    pub param_type: ParamType,
}

/// Parameter type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ParamType {
    /// Free-form text input
    Text,

    /// File path
    File,

    /// Directory path
    Directory,

    /// Number
    Number,
}

/// Skill loader from `.claude/skills/` directory
pub struct SkillLoader {
    /// Base path for skills directory
    base_path: PathBuf,
}

impl SkillLoader {
    /// Create new skill loader
    pub fn new() -> Self {
        let base_path = Self::default_path();
        Self { base_path }
    }

    /// Create with custom base path
    pub fn with_path<P: AsRef<Path>>(path: P) -> Self {
        Self {
            base_path: path.as_ref().to_path_buf(),
        }
    }

    /// Get default skills path (~/.claude/skills/)
    fn default_path() -> PathBuf {
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
        home.join(".claude").join("skills")
    }

    /// Load all skills from filesystem
    pub fn load_all(&self) -> Result<Vec<Skill>> {
        let mut skills = Vec::new();

        if !self.base_path.exists() {
            tracing::debug!("Skills directory does not exist: {:?}", self.base_path);
            return Ok(skills);
        }

        let entries = fs::read_dir(&self.base_path)
            .with_context(|| format!("Failed to read skills directory: {:?}", self.base_path))?;

        for entry in entries {
            let entry = entry?;
            let path = entry.path();

            // Skip non-directories
            if !path.is_dir() {
                continue;
            }

            // Try to load skill from this directory
            if let Some(skill) = self.load_skill(&path)? {
                skills.push(skill);
            }
        }

        Ok(skills)
    }

    /// Load single skill from directory
    fn load_skill(&self, dir: &Path) -> Result<Option<Skill>> {
        // Try SKILL.md first (proper format with YAML frontmatter)
        let skill_file = dir.join("SKILL.md");

        // Fall back to skill.md (legacy format)
        let skill_file = if skill_file.exists() {
            skill_file
        } else {
            dir.join("skill.md")
        };

        if !skill_file.exists() {
            tracing::debug!("No SKILL.md or skill.md found in: {:?}", dir);
            return Ok(None);
        }

        let content = fs::read_to_string(&skill_file)
            .with_context(|| format!("Failed to read skill file: {:?}", skill_file))?;

        let skill = self.parse_skill(&content, dir)?;

        Ok(Some(skill))
    }

    /// Parse skill.md content
    fn parse_skill(&self, content: &str, path: &Path) -> Result<Skill> {
        // Check for YAML frontmatter format (SKILL.md)
        if content.starts_with("---") {
            return self.parse_yaml_frontmatter(content, path);
        }

        // Fall back to legacy format (skill.md)
        self.parse_legacy_format(content, path)
    }

    /// Parse YAML frontmatter format (SKILL.md)
    fn parse_yaml_frontmatter(&self, content: &str, path: &Path) -> Result<Skill> {
        let mut name = String::new();
        let mut description = String::new();
        let mut category = SkillCategory::Tools;
        let parameters = Vec::new();
        let mut commands = Vec::new();

        // Find frontmatter boundaries
        let lines: Vec<&str> = content.lines().collect();
        let frontmatter_start = lines.iter().position(|l| l.trim() == "---");
        let frontmatter_end = frontmatter_start.and_then(|start| {
            lines
                .iter()
                .skip(start + 1)
                .position(|l| l.trim() == "---")
                .map(|end| start + 1 + end)
        });

        let (yaml_lines, _rest) =
            if let (Some(start), Some(end)) = (frontmatter_start, frontmatter_end) {
                (&lines[start + 1..end], &lines[end + 1..])
            } else {
                anyhow::bail!("Invalid YAML frontmatter format");
            };

        // Parse YAML frontmatter
        for line in yaml_lines {
            let line = line.trim();

            // Skip empty lines and comments
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            // Parse key: value pairs
            if let Some((key, value)) = line.split_once(':') {
                let key = key.trim();
                let value = value.trim().trim_matches('"');

                match key {
                    "name" => name = value.to_string(),
                    "description" => description = value.to_string(),
                    "category" => {
                        category = SkillCategory::parse_str(value).unwrap_or(SkillCategory::Tools);
                    }
                    "commands" => {
                        // Commands are parsed as a separate list
                        // Format: commands: [cmd1, cmd2, cmd3]
                        let cmds: Vec<&str> = value
                            .trim_matches(['[', ']'])
                            .split(',')
                            .map(|s| s.trim().trim_matches('"'))
                            .filter(|s| !s.is_empty())
                            .collect();

                        for cmd in cmds {
                            commands.push(SkillCommand {
                                name: cmd.to_string(),
                                invocation: format!("/{}", cmd),
                                description: format!("Execute {}", cmd),
                            });
                        }
                    }
                    _ => {}
                }
            }
        }

        // If no commands specified, auto-generate from skill name
        if commands.is_empty() {
            commands.push(SkillCommand {
                name: name.clone(),
                invocation: format!("/{}", name),
                description: description.clone(),
            });
        }

        // Validate required fields
        if name.is_empty() {
            anyhow::bail!("Skill missing 'name' field in YAML frontmatter");
        }

        if description.is_empty() {
            description = format!("Run {}", name);
        }

        // Try to load instructions from instructions.md
        let instructions = self.load_instructions(path);

        Ok(Skill {
            name,
            description,
            category,
            parameters,
            commands,
            instructions,
            path: path.to_path_buf(),
        })
    }

    /// Parse legacy format (skill.md)
    fn parse_legacy_format(&self, content: &str, path: &Path) -> Result<Skill> {
        let mut name = String::new();
        let mut description = String::new();
        let mut category = SkillCategory::Tools;
        let mut parameters = Vec::new();

        // Parse simple markdown frontmatter-like format
        // Expected format:
        // name: code-review
        // description: Review code for quality and security
        // category: agent
        // parameters:
        //   - name: target
        //     description: file or directory to review
        //     required: true
        //     type: file

        let mut current_param: Option<(&str, &str, &str, bool)> = None;

        for line in content.lines() {
            let line = line.trim();

            // Skip empty lines and comments
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            // Parse key: value pairs
            if let Some((key, value)) = line.split_once(':') {
                let key = key.trim();
                let value = value.trim();

                match key {
                    "name" if current_param.is_some() => {
                        // Parameter name
                        if let Some((param_name, desc, _, req)) = current_param.take() {
                            parameters.push(SkillParameter {
                                name: param_name.to_string(),
                                description: desc.to_string(),
                                required: req,
                                param_type: ParamType::Text,
                            });
                        }
                        current_param = Some((value, "", "", false));
                    }
                    "description" if current_param.is_some() => {
                        // Parameter description
                        if let Some((name, _, ty, req)) = current_param.take() {
                            current_param = Some((name, value, ty, req));
                        }
                    }
                    "name" => name = value.to_string(),
                    "description" => description = value.to_string(),
                    "category" => {
                        category = SkillCategory::parse_str(value).unwrap_or(SkillCategory::Tools);
                    }
                    "parameters" => continue, // Start of parameters section
                    "required" if current_param.is_some() => {
                        // Parameter required flag
                        let req = value.eq_ignore_ascii_case("true") || value == "true";
                        if let Some((name, desc, ty, _)) = current_param.take() {
                            current_param = Some((name, desc, ty, req));
                        }
                    }
                    "type" if current_param.is_some() => {
                        // Parameter type
                        if let Some((name, desc, _, req)) = current_param.take() {
                            current_param = Some((name, desc, value, req));
                        }
                    }
                    _ => {}
                }
            } else if line.starts_with("- ") {
                // Start of new parameter (list item)
                if let Some((param_name, desc, ty, req)) = current_param.take() {
                    parameters.push(SkillParameter {
                        name: param_name.to_string(),
                        description: desc.to_string(),
                        required: req,
                        param_type: Self::parse_param_type(ty),
                    });
                }
                current_param = None;
            }
        }

        // Don't forget the last parameter
        if let Some((param_name, desc, ty, req)) = current_param.take() {
            parameters.push(SkillParameter {
                name: param_name.to_string(),
                description: desc.to_string(),
                required: req,
                param_type: Self::parse_param_type(ty),
            });
        }

        // Validate required fields
        if name.is_empty() {
            anyhow::bail!("Skill missing 'name' field");
        }

        if description.is_empty() {
            description = format!("Run {}", name);
        }

        // Auto-generate command from skill name
        let commands = vec![SkillCommand {
            name: name.clone(),
            invocation: format!("/{}", name),
            description: description.clone(),
        }];

        Ok(Skill {
            name,
            description,
            category,
            parameters,
            commands,
            instructions: String::new(),
            path: path.to_path_buf(),
        })
    }

    /// Load instructions from instructions.md file
    fn load_instructions(&self, path: &Path) -> String {
        let instructions_path = path.join("instructions.md");
        if instructions_path.exists() {
            if let Ok(content) = fs::read_to_string(&instructions_path) {
                return content;
            }
        }
        String::new()
    }
}

impl Skill {
    /// Generate skill context for LLM - includes instructions on when/how to use this skill
    pub fn to_llm_context(&self) -> String {
        if self.instructions.is_empty() && self.description.is_empty() {
            return String::new();
        }

        let mut context = format!("## Skill: {}\n\n", self.name);

        if !self.description.is_empty() {
            context.push_str(&format!("Description: {}\n\n", self.description));
        }

        if !self.instructions.is_empty() {
            context.push_str(&format!("Instructions:\n{}\n\n", self.instructions));
        }

        if !self.commands.is_empty() {
            context.push_str("Commands:\n");
            for cmd in &self.commands {
                context.push_str(&format!("- {}: {}\n", cmd.invocation, cmd.description));
            }
            context.push('\n');
        }

        context
    }
}

impl Default for SkillLoader {
    fn default() -> Self {
        Self::new()
    }
}

impl SkillLoader {
    fn parse_param_type(s: &str) -> ParamType {
        match s.to_lowercase().as_str() {
            "file" => ParamType::File,
            "directory" | "dir" => ParamType::Directory,
            "number" | "int" | "float" => ParamType::Number,
            _ => ParamType::Text,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_category_icons() {
        assert_eq!(SkillCategory::Agent.icon(), "⚡");
        assert_eq!(SkillCategory::Testing.icon(), "🧪");
        assert_eq!(SkillCategory::Security.icon(), "🔒");
        assert_eq!(SkillCategory::Planning.icon(), "📝");
        assert_eq!(SkillCategory::Design.icon(), "🎨");
        assert_eq!(SkillCategory::Debug.icon(), "🐛");
        assert_eq!(SkillCategory::Tools.icon(), "🔧");
    }

    #[test]
    fn test_category_from_str() {
        assert_eq!(
            SkillCategory::parse_str("agent"),
            Some(SkillCategory::Agent)
        );
        assert_eq!(
            SkillCategory::parse_str("AGENT"),
            Some(SkillCategory::Agent)
        );
        assert_eq!(
            SkillCategory::parse_str("testing"),
            Some(SkillCategory::Testing)
        );
        assert_eq!(SkillCategory::parse_str("unknown"), None);
    }

    #[test]
    fn test_param_type_parsing() {
        assert_eq!(SkillLoader::parse_param_type("file"), ParamType::File);
        assert_eq!(
            SkillLoader::parse_param_type("directory"),
            ParamType::Directory
        );
        assert_eq!(SkillLoader::parse_param_type("dir"), ParamType::Directory);
        assert_eq!(SkillLoader::parse_param_type("number"), ParamType::Number);
        assert_eq!(SkillLoader::parse_param_type("text"), ParamType::Text);
        assert_eq!(SkillLoader::parse_param_type("unknown"), ParamType::Text);
    }

    #[test]
    fn test_default_path() {
        let loader = SkillLoader::new();
        let expected = dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".claude")
            .join("skills");
        assert_eq!(loader.base_path, expected);
    }

    #[test]
    fn test_yaml_frontmatter_parsing() {
        let content = r#"---
name: test-skill
description: "A test skill for YAML parsing"
category: testing
---

# Test Skill

This is a test skill.
"#;

        let loader = SkillLoader::new();
        let result = loader.parse_yaml_frontmatter(content, &PathBuf::from("/test"));

        assert!(result.is_ok());
        let skill = result.unwrap();
        assert_eq!(skill.name, "test-skill");
        assert_eq!(skill.description, "A test skill for YAML parsing");
        assert_eq!(skill.category, SkillCategory::Testing);
        assert!(!skill.commands.is_empty());
        assert_eq!(skill.commands[0].name, "test-skill");
    }

    #[test]
    fn test_yaml_frontmatter_with_quotes() {
        let content = r#"---
name: orchestra-progress
description: "Check project progress and next steps"
---

# Orchestra Progress Skill
"#;

        let loader = SkillLoader::new();
        let result = loader.parse_yaml_frontmatter(content, &PathBuf::from("/test"));

        assert!(result.is_ok());
        let skill = result.unwrap();
        assert_eq!(skill.name, "orchestra-progress");
        assert_eq!(skill.description, "Check project progress and next steps");
        assert_eq!(skill.commands.len(), 1);
        assert_eq!(skill.commands[0].invocation, "/orchestra-progress");
    }

    #[test]
    fn test_skill_format_detection() {
        let yaml_content = r#"---
name: test-skill
description: "Test"
---
"#;

        let legacy_content = r#"name: test-skill
description: Test
"#;

        let loader = SkillLoader::new();

        // YAML format should be detected
        let yaml_result = loader.parse_skill(yaml_content, &PathBuf::from("/test"));
        assert!(yaml_result.is_ok());
        let yaml_skill = yaml_result.unwrap();
        assert_eq!(yaml_skill.name, "test-skill");
        assert!(!yaml_skill.commands.is_empty());

        // Legacy format should work too
        let legacy_result = loader.parse_skill(legacy_content, &PathBuf::from("/test"));
        assert!(legacy_result.is_ok());
        let legacy_skill = legacy_result.unwrap();
        assert_eq!(legacy_skill.name, "test-skill");
        assert!(!legacy_skill.commands.is_empty());
    }

    #[test]
    fn test_auto_command_generation() {
        let content = r#"---
name: my-skill
description: "My custom skill"
---

# My Skill
"#;

        let loader = SkillLoader::new();
        let result = loader.parse_yaml_frontmatter(content, &PathBuf::from("/test"));

        assert!(result.is_ok());
        let skill = result.unwrap();
        assert_eq!(skill.commands.len(), 1);
        assert_eq!(skill.commands[0].name, "my-skill");
        assert_eq!(skill.commands[0].invocation, "/my-skill");
    }
}
