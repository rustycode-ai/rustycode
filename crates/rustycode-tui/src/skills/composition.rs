//! Skill composition and advanced features
//!
//! This module provides advanced skill system features including:
//! - Skill aliases and shortcuts
//! - Skill composition (chaining skills together)
//! - Skill execution history
//! - Skill templates

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

/// Alias for a skill or skill chain
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillAlias {
    /// The alias name
    pub name: String,
    /// The target skill(s) this alias refers to
    pub target: AliasTarget,
    /// Description of what this alias does
    pub description: String,
    /// Whether this is a user-defined alias
    pub user_defined: bool,
}

/// Target of an alias - can be a single skill or a chain
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub enum AliasTarget {
    /// Single skill
    Single(String),
    /// Chain of skills to execute in sequence
    Chain(Vec<String>),
    /// Parallel execution of multiple skills
    Parallel(Vec<String>),
}

/// A composition of multiple skills
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillComposition {
    /// Unique identifier for this composition
    pub id: String,
    /// Display name
    pub name: String,
    /// Description
    pub description: String,
    /// The skills in this composition
    pub steps: Vec<CompositionStep>,
    /// Execution mode
    pub mode: ExecutionMode,
    /// Whether to stop on first error
    pub stop_on_error: bool,
}

/// A single step in a composition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompositionStep {
    /// The skill to execute
    pub skill_id: String,
    /// Parameters to pass to the skill
    pub parameters: HashMap<String, serde_json::Value>,
    /// Condition for executing this step
    pub condition: Option<StepCondition>,
}

/// Condition for executing a step
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub enum StepCondition {
    /// Execute only if previous step succeeded
    OnSuccess,
    /// Execute only if previous step failed
    OnFailure,
    /// Execute always (default)
    Always,
    /// Execute if a specific parameter matches a value
    ParameterEquals {
        param: String,
        value: serde_json::Value,
    },
}

/// How to execute the composition
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub enum ExecutionMode {
    /// Execute steps sequentially
    Sequential,
    /// Execute steps in parallel
    Parallel,
    /// Execute with dependency resolution
    DependencyResolved,
}

/// History entry for skill execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillExecutionEntry {
    /// Unique ID for this execution
    pub id: String,
    /// Skill that was executed
    pub skill_id: String,
    /// When it was executed
    pub timestamp: chrono::DateTime<chrono::Utc>,
    /// Duration of execution
    pub duration_ms: u64,
    /// Whether it succeeded
    pub success: bool,
    /// Output/error message
    pub output: String,
    /// Parameters used
    pub parameters: HashMap<String, serde_json::Value>,
}

/// Skill template for scaffolding new skills
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillTemplate {
    /// Template identifier
    pub id: String,
    /// Display name
    pub name: String,
    /// Description
    pub description: String,
    /// Category of skill this template creates
    pub category: String,
    /// Template file contents
    pub files: HashMap<String, String>,
    /// Required parameters
    pub parameters: Vec<TemplateParameter>,
}

/// Parameter for template instantiation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemplateParameter {
    /// Parameter name
    pub name: String,
    /// Description
    pub description: String,
    /// Default value (optional)
    pub default: Option<String>,
    /// Whether this parameter is required
    pub required: bool,
}

/// Manager for skill composition and advanced features
pub struct CompositionManager {
    /// Known aliases
    aliases: HashMap<String, SkillAlias>,
    /// Known compositions
    compositions: HashMap<String, SkillComposition>,
    /// Execution history
    history: Vec<SkillExecutionEntry>,
    /// Available templates
    templates: Vec<SkillTemplate>,
    /// Storage path for persistence
    storage_path: PathBuf,
}

impl CompositionManager {
    /// Create a new composition manager
    pub fn new(storage_path: PathBuf) -> Result<Self> {
        let mut manager = Self {
            aliases: HashMap::new(),
            compositions: HashMap::new(),
            history: Vec::new(),
            templates: builtin_templates(),
            storage_path,
        };

        manager.load_state()?;
        Ok(manager)
    }

    /// Add an alias
    pub fn add_alias(&mut self, alias: SkillAlias) -> Result<()> {
        self.aliases.insert(alias.name.clone(), alias);
        self.save_state()?;
        Ok(())
    }

    /// Remove an alias
    pub fn remove_alias(&mut self, name: &str) -> Result<()> {
        self.aliases.remove(name);
        self.save_state()?;
        Ok(())
    }

    /// Get an alias
    pub fn get_alias(&self, name: &str) -> Option<&SkillAlias> {
        self.aliases.get(name)
    }

    /// Resolve an alias to its target skill(s)
    pub fn resolve_alias(&self, name: &str) -> Option<Vec<String>> {
        self.aliases.get(name).map(|alias| match &alias.target {
            AliasTarget::Single(skill) => vec![skill.clone()],
            AliasTarget::Chain(skills) => skills.clone(),
            AliasTarget::Parallel(skills) => skills.clone(),
        })
    }

    /// List all aliases
    pub fn list_aliases(&self) -> Vec<&SkillAlias> {
        self.aliases.values().collect()
    }

    /// Create a new composition
    pub fn create_composition(&mut self, composition: SkillComposition) -> Result<()> {
        self.compositions
            .insert(composition.id.clone(), composition);
        self.save_state()?;
        Ok(())
    }

    /// Get a composition
    pub fn get_composition(&self, id: &str) -> Option<&SkillComposition> {
        self.compositions.get(id)
    }

    /// List all compositions
    pub fn list_compositions(&self) -> Vec<&SkillComposition> {
        self.compositions.values().collect()
    }

    /// Remove a composition
    pub fn remove_composition(&mut self, id: &str) -> Result<()> {
        self.compositions.remove(id);
        self.save_state()?;
        Ok(())
    }

    /// Add an execution entry to history
    pub fn record_execution(&mut self, entry: SkillExecutionEntry) -> Result<()> {
        self.history.push(entry);
        self.trim_history();
        self.save_state()?;
        Ok(())
    }

    /// Get execution history for a skill
    pub fn get_history(&self, skill_id: &str) -> Vec<&SkillExecutionEntry> {
        self.history
            .iter()
            .filter(|e| e.skill_id == skill_id)
            .collect()
    }

    /// Get all execution history
    pub fn get_all_history(&self) -> &[SkillExecutionEntry] {
        &self.history
    }

    /// Clear execution history
    pub fn clear_history(&mut self) -> Result<()> {
        self.history.clear();
        self.save_state()?;
        Ok(())
    }

    /// Get available templates
    pub fn get_templates(&self) -> &[SkillTemplate] {
        &self.templates
    }

    /// Get a specific template
    pub fn get_template(&self, id: &str) -> Option<&SkillTemplate> {
        self.templates.iter().find(|t| t.id == id)
    }

    /// Instantiate a template with parameters
    pub fn instantiate_template(
        &self,
        template_id: &str,
        params: &HashMap<String, String>,
    ) -> Result<HashMap<String, String>> {
        let template = self
            .get_template(template_id)
            .ok_or_else(|| anyhow::anyhow!("Template '{}' not found", template_id))?;

        let mut result = HashMap::new();

        for (file_path, content) in &template.files {
            let mut instantiated = content.clone();

            for param in &template.parameters {
                let value = params
                    .get(&param.name)
                    .or(param.default.as_ref())
                    .ok_or_else(|| {
                        anyhow::anyhow!("Required parameter '{}' not provided", param.name)
                    })?;

                // Replace both {{name}} and {{ name }} formats
                instantiated = instantiated.replace(&format!("{{{{{}}}", param.name), value);
                instantiated = instantiated.replace(&format!("{{{} }}", param.name), value);
            }

            result.insert(file_path.clone(), instantiated);
        }

        Ok(result)
    }

    /// Keep history at a reasonable size
    fn trim_history(&mut self) {
        const MAX_HISTORY: usize = 1000;
        if self.history.len() > MAX_HISTORY {
            self.history = self.history.split_off(self.history.len() - MAX_HISTORY);
        }
    }

    /// Save state to disk
    fn save_state(&self) -> Result<()> {
        if let Some(parent) = self.storage_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let state = CompositionState {
            aliases: self.aliases.clone(),
            compositions: self.compositions.clone(),
            history: self.history.clone(),
        };

        let json = serde_json::to_string_pretty(&state)?;
        std::fs::write(&self.storage_path, json)?;
        Ok(())
    }

    /// Load state from disk
    fn load_state(&mut self) -> Result<()> {
        if !self.storage_path.exists() {
            return Ok(());
        }

        let content = std::fs::read_to_string(&self.storage_path)?;
        let state: CompositionState = serde_json::from_str(&content)?;

        self.aliases = state.aliases;
        self.compositions = state.compositions;
        self.history = state.history;

        Ok(())
    }
}

/// Persistent state for composition manager
#[derive(Debug, Clone, Serialize, Deserialize)]
struct CompositionState {
    aliases: HashMap<String, SkillAlias>,
    compositions: HashMap<String, SkillComposition>,
    history: Vec<SkillExecutionEntry>,
}

/// Create built-in templates
fn builtin_templates() -> Vec<SkillTemplate> {
    vec![
        SkillTemplate {
            id: "basic-agent".to_string(),
            name: "Basic Agent Skill".to_string(),
            description: "A simple agent-type skill template".to_string(),
            category: "Agent".to_string(),
            files: {
                let mut map = HashMap::new();
                map.insert(
                    "skill.md".to_string(),
                    r#"# {{name}}

> {{description}}

## When to Use

Use this skill when you need to {{use_case}}.

## Instructions

1. First, analyze the context to understand {{analysis_target}}.
2. Then, execute the following steps:
   - Step 1: {{step1}}
   - Step 2: {{step2}}
3. Finally, verify that {{verification_criteria}}.

## Examples

Input: "{{example_input}}"
Output: "{{example_output}}"
"#
                    .to_string(),
                );
                map
            },
            parameters: vec![
                TemplateParameter {
                    name: "name".to_string(),
                    description: "Name of the skill".to_string(),
                    default: Some("My Skill".to_string()),
                    required: false,
                },
                TemplateParameter {
                    name: "description".to_string(),
                    description: "What the skill does".to_string(),
                    default: None,
                    required: true,
                },
                TemplateParameter {
                    name: "use_case".to_string(),
                    description: "When to use this skill".to_string(),
                    default: None,
                    required: true,
                },
                TemplateParameter {
                    name: "analysis_target".to_string(),
                    description: "What to analyze".to_string(),
                    default: Some("the problem".to_string()),
                    required: false,
                },
                TemplateParameter {
                    name: "step1".to_string(),
                    description: "First step".to_string(),
                    default: None,
                    required: true,
                },
                TemplateParameter {
                    name: "step2".to_string(),
                    description: "Second step".to_string(),
                    default: None,
                    required: true,
                },
                TemplateParameter {
                    name: "verification_criteria".to_string(),
                    description: "Success criteria".to_string(),
                    default: Some("the result is correct".to_string()),
                    required: false,
                },
                TemplateParameter {
                    name: "example_input".to_string(),
                    description: "Example input".to_string(),
                    default: None,
                    required: true,
                },
                TemplateParameter {
                    name: "example_output".to_string(),
                    description: "Example output".to_string(),
                    default: None,
                    required: true,
                },
            ],
        },
        SkillTemplate {
            id: "code-reviewer".to_string(),
            name: "Code Reviewer Skill".to_string(),
            description: "A skill template for reviewing code".to_string(),
            category: "Review".to_string(),
            files: {
                let mut map = HashMap::new();
                map.insert(
                    "skill.md".to_string(),
                    r#"# {{name}} - {{language}} Code Review

> {{description}}

## Review Focus Areas

This skill focuses on:

1. **Correctness**: Logic bugs, edge cases, error handling
2. **{{language}} Best Practices**: Idiomatic code, patterns
3. **Performance**: Efficiency, algorithms, data structures
4. **Security**: {{security_focus}}
5. **Maintainability**: Code organization, naming, documentation

## Review Process

1. **Understand the Goal**: Identify what the code is trying to achieve
2. **Check for Issues**: Look for bugs, anti-patterns, and security issues
3. **Suggest Improvements**: Recommend better approaches
4. **Verify Changes**: Ensure fixes don't introduce new issues

## Common Issues to Check

{{common_issues}}

## Output Format

Provide feedback in this format:
- ✅ **Correct**: What's working well
- ⚠️ **Concerns**: Issues found (with line references)
- 💡 **Suggestions**: Improvement opportunities
"#
                    .to_string(),
                );
                map
            },
            parameters: vec![
                TemplateParameter {
                    name: "name".to_string(),
                    description: "Skill name".to_string(),
                    default: Some("Code Reviewer".to_string()),
                    required: false,
                },
                TemplateParameter {
                    name: "language".to_string(),
                    description: "Programming language".to_string(),
                    default: Some("Rust".to_string()),
                    required: false,
                },
                TemplateParameter {
                    name: "description".to_string(),
                    description: "What this reviewer focuses on".to_string(),
                    default: Some(
                        "Reviews code for quality, security, and best practices".to_string(),
                    ),
                    required: false,
                },
                TemplateParameter {
                    name: "security_focus".to_string(),
                    description: "Security concerns to highlight".to_string(),
                    default: Some("Input validation, authentication, authorization".to_string()),
                    required: false,
                },
                TemplateParameter {
                    name: "common_issues".to_string(),
                    description: "Common issues to check for".to_string(),
                    default: Some(
                        "- Memory leaks\n- Race conditions\n- Error handling".to_string(),
                    ),
                    required: false,
                },
            ],
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_temp_dir() -> tempfile::TempDir {
        tempfile::tempdir().unwrap()
    }

    #[test]
    fn test_alias_resolution() {
        let mut manager =
            CompositionManager::new(make_temp_dir().path().join("state.json")).unwrap();

        let alias = SkillAlias {
            name: "cr".to_string(),
            target: AliasTarget::Single("code-review".to_string()),
            description: "Quick code review".to_string(),
            user_defined: true,
        };

        manager.add_alias(alias).unwrap();

        let resolved = manager.resolve_alias("cr");
        assert_eq!(resolved, Some(vec!["code-review".to_string()]));
    }

    #[test]
    fn test_chain_alias() {
        let mut manager =
            CompositionManager::new(make_temp_dir().path().join("state.json")).unwrap();

        let alias = SkillAlias {
            name: "full-review".to_string(),
            target: AliasTarget::Chain(vec![
                "code-review".to_string(),
                "security-review".to_string(),
            ]),
            description: "Full review with security".to_string(),
            user_defined: true,
        };

        manager.add_alias(alias).unwrap();

        let resolved = manager.resolve_alias("full-review");
        assert_eq!(
            resolved,
            Some(vec![
                "code-review".to_string(),
                "security-review".to_string()
            ])
        );
    }

    #[test]
    fn test_composition_creation() {
        let mut manager =
            CompositionManager::new(make_temp_dir().path().join("state.json")).unwrap();

        let composition = SkillComposition {
            id: "test-comp".to_string(),
            name: "Test Composition".to_string(),
            description: "A test composition".to_string(),
            steps: vec![CompositionStep {
                skill_id: "skill1".to_string(),
                parameters: HashMap::new(),
                condition: None,
            }],
            mode: ExecutionMode::Sequential,
            stop_on_error: true,
        };

        manager.create_composition(composition).unwrap();

        let retrieved = manager.get_composition("test-comp");
        assert!(retrieved.is_some());
    }

    #[test]
    fn test_execution_history() {
        let mut manager =
            CompositionManager::new(make_temp_dir().path().join("state.json")).unwrap();

        let entry = SkillExecutionEntry {
            id: "exec1".to_string(),
            skill_id: "test-skill".to_string(),
            timestamp: chrono::Utc::now(),
            duration_ms: 100,
            success: true,
            output: "Success".to_string(),
            parameters: HashMap::new(),
        };

        manager.record_execution(entry).unwrap();

        let history = manager.get_history("test-skill");
        assert_eq!(history.len(), 1);
    }

    #[test]
    fn test_template_instantiation() {
        let manager = CompositionManager::new(make_temp_dir().path().join("state.json")).unwrap();

        let mut params = HashMap::new();
        params.insert("name".to_string(), "Test Skill".to_string());
        params.insert("description".to_string(), "A test skill".to_string());
        params.insert("use_case".to_string(), "testing things".to_string());
        params.insert("step1".to_string(), "do this".to_string());
        params.insert("step2".to_string(), "do that".to_string());
        params.insert("example_input".to_string(), "input".to_string());
        params.insert("example_output".to_string(), "output".to_string());

        let result = manager.instantiate_template("basic-agent", &params);

        assert!(result.is_ok());
        let files = result.unwrap();
        let skill_md = files.get("skill.md").unwrap();
        assert!(skill_md.contains("Test Skill"));
        assert!(skill_md.contains("A test skill"));
    }

    #[test]
    fn test_history_trimming() {
        let mut manager =
            CompositionManager::new(make_temp_dir().path().join("state.json")).unwrap();

        // Add more than MAX_HISTORY entries
        for i in 0..1100 {
            let entry = SkillExecutionEntry {
                id: format!("exec{}", i),
                skill_id: "test-skill".to_string(),
                timestamp: chrono::Utc::now(),
                duration_ms: 100,
                success: true,
                output: "Success".to_string(),
                parameters: HashMap::new(),
            };
            manager.history.push(entry);
        }

        manager.trim_history();

        assert!(manager.history.len() <= 1000);
    }
}
