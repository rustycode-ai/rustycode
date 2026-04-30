//! Skills System
//!
//! Reusable prompt templates that can be invoked as commands.
//! Users define skills in YAML/TOML files and invoke them by name.
//! Inspired by forgecode's skills/custom commands.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

/// A skill definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Skill {
    /// Unique skill name (used to invoke)
    pub name: String,
    /// Short description shown in listings
    pub description: String,
    /// The prompt template (supports {{variable}} placeholders)
    pub prompt: String,
    /// Optional variables that the prompt expects
    #[serde(default)]
    pub variables: Vec<SkillVariable>,
    /// Optional tools to enable for this skill
    #[serde(default)]
    pub tools: Vec<String>,
    /// Optional model override
    pub model: Option<String>,
    /// Optional temperature override
    pub temperature: Option<f32>,
}

/// A variable that a skill prompt expects
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillVariable {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub required: bool,
    pub default: Option<String>,
}

/// Result of skill resolution
#[derive(Debug, Clone)]
pub struct ResolvedSkill {
    pub skill: Skill,
    pub rendered_prompt: String,
}

/// Registry of available skills
#[derive(Debug, Clone, Default)]
pub struct SkillRegistry {
    skills: HashMap<String, Skill>,
}

impl SkillRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a skill
    pub fn register(&mut self, skill: Skill) {
        self.skills.insert(skill.name.clone(), skill);
    }

    /// Get a skill by name
    pub fn get(&self, name: &str) -> Option<&Skill> {
        self.skills.get(name)
    }

    /// List all registered skills
    pub fn list(&self) -> Vec<&Skill> {
        let mut skills: Vec<_> = self.skills.values().collect();
        skills.sort_by(|a, b| a.name.cmp(&b.name));
        skills
    }

    /// Resolve a skill with provided variables
    pub fn resolve(
        &self,
        name: &str,
        variables: &HashMap<String, String>,
    ) -> anyhow::Result<ResolvedSkill> {
        let skill = self
            .skills
            .get(name)
            .ok_or_else(|| anyhow::anyhow!("Skill '{name}' not found"))?;

        // Check required variables
        for var in &skill.variables {
            if var.required && !variables.contains_key(&var.name) && var.default.is_none() {
                anyhow::bail!(
                    "Skill '{}' requires variable '{}' ({})",
                    name,
                    var.name,
                    var.description
                );
            }
        }

        // Render prompt with variable substitution
        let mut rendered = skill.prompt.clone();
        for (key, value) in variables {
            rendered = rendered.replace(&format!("{{{{{key}}}}}"), value);
        }

        // Fill defaults for missing variables
        for var in &skill.variables {
            if !variables.contains_key(&var.name) {
                if let Some(default) = &var.default {
                    rendered = rendered.replace(&format!("{{{{{}}}}}", var.name), default);
                }
            }
        }

        Ok(ResolvedSkill {
            skill: skill.clone(),
            rendered_prompt: rendered,
        })
    }

    /// Load skills from a directory of YAML/TOML files
    pub fn load_from_dir(dir: &Path) -> anyhow::Result<Self> {
        let mut registry = Self::new();

        if !dir.exists() {
            return Ok(registry);
        }

        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();

            let content = std::fs::read_to_string(&path)?;

            let skill: Skill = match path.extension().and_then(|e| e.to_str()) {
                Some("yaml" | "yml") => serde_yaml::from_str(&content)?,
                Some("toml") => toml::from_str(&content)?,
                Some("json") => serde_json::from_str(&content)?,
                _ => continue,
            };

            registry.register(skill);
        }

        Ok(registry)
    }

    /// Get built-in skills
    pub fn builtin_skills() -> Vec<Skill> {
        vec![
            Skill {
                name: "review".into(),
                description: "Review code for issues and suggest improvements".into(),
                prompt:
                    "Review the following code for bugs, performance issues, and style problems. Provide specific suggestions:\n\n{{code}}"
                        .into(),
                variables: vec![SkillVariable {
                    name: "code".into(),
                    description: "Code to review".into(),
                    required: true,
                    default: None,
                }],
                tools: vec!["read_file".into(), "grep".into()],
                model: None,
                temperature: Some(0.3),
            },
            Skill {
                name: "test".into(),
                description: "Generate tests for the given code".into(),
                prompt:
                    "Write comprehensive tests for the following code. Use table-driven tests where appropriate:\n\n{{code}}"
                        .into(),
                variables: vec![SkillVariable {
                    name: "code".into(),
                    description: "Code to test".into(),
                    required: true,
                    default: None,
                }],
                tools: vec!["read_file".into(), "write_file".into(), "bash".into()],
                model: None,
                temperature: Some(0.2),
            },
            Skill {
                name: "explain".into(),
                description: "Explain code in plain language".into(),
                prompt:
                    "Explain the following code in plain language. Describe what it does, how it works, and any notable patterns:\n\n{{code}}"
                        .into(),
                variables: vec![SkillVariable {
                    name: "code".into(),
                    description: "Code to explain".into(),
                    required: true,
                    default: None,
                }],
                tools: vec!["read_file".into()],
                model: None,
                temperature: Some(0.5),
            },
            Skill {
                name: "fix".into(),
                description: "Fix bugs in the given code".into(),
                prompt:
                    "Fix the bugs in the following code. Explain what was wrong and how you fixed it:\n\n{{code}}\n\nError or issue: {{error}}"
                        .into(),
                variables: vec![
                    SkillVariable {
                        name: "code".into(),
                        description: "Buggy code".into(),
                        required: true,
                        default: None,
                    },
                    SkillVariable {
                        name: "error".into(),
                        description: "Error message or description".into(),
                        required: false,
                        default: Some("unspecified".into()),
                    },
                ],
                tools: vec![
                    "read_file".into(),
                    "write_file".into(),
                    "edit_file".into(),
                    "bash".into(),
                ],
                model: None,
                temperature: Some(0.2),
            },
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_register_and_get() {
        let mut reg = SkillRegistry::new();
        reg.register(Skill {
            name: "hello".into(),
            description: "Say hello".into(),
            prompt: "Hello {{name}}!".into(),
            variables: vec![SkillVariable {
                name: "name".into(),
                description: "Name".into(),
                required: true,
                default: None,
            }],
            tools: vec![],
            model: None,
            temperature: None,
        });

        assert!(reg.get("hello").is_some());
        assert!(reg.get("unknown").is_none());
    }

    #[test]
    fn test_resolve_with_variables() {
        let mut reg = SkillRegistry::new();
        reg.register(Skill {
            name: "greet".into(),
            description: "Greeting".into(),
            prompt: "Hello {{name}}, welcome to {{place}}!".into(),
            variables: vec![
                SkillVariable {
                    name: "name".into(),
                    description: "Name".into(),
                    required: true,
                    default: None,
                },
                SkillVariable {
                    name: "place".into(),
                    description: "Place".into(),
                    required: false,
                    default: Some("the project".into()),
                },
            ],
            tools: vec![],
            model: None,
            temperature: None,
        });

        let mut vars = HashMap::new();
        vars.insert("name".into(), "Alice".into());

        let resolved = reg.resolve("greet", &vars).unwrap();
        assert_eq!(
            resolved.rendered_prompt,
            "Hello Alice, welcome to the project!"
        );
    }

    #[test]
    fn test_resolve_missing_required() {
        let mut reg = SkillRegistry::new();
        reg.register(Skill {
            name: "need_var".into(),
            description: "Needs a var".into(),
            prompt: "{{required_var}}".into(),
            variables: vec![SkillVariable {
                name: "required_var".into(),
                description: "Required".into(),
                required: true,
                default: None,
            }],
            tools: vec![],
            model: None,
            temperature: None,
        });

        let result = reg.resolve("need_var", &HashMap::new());
        assert!(result.is_err());
    }

    #[test]
    fn test_builtin_skills() {
        let skills = SkillRegistry::builtin_skills();
        assert!(!skills.is_empty());
        assert!(skills.iter().any(|s| s.name == "review"));
        assert!(skills.iter().any(|s| s.name == "test"));
    }
}
