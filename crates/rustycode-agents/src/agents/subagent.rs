//! Subagent implementation for specialized task delegation
//!
//! Subagents are specialized agents with specific expertise, tools, and
//! instructions. They can be invoked by an orchestrator to handle
//! particular tasks.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

/// Configuration for a subagent
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubagentConfig {
    /// Unique identifier for this subagent
    pub id: String,

    /// Human-readable name
    pub name: String,

    /// Description of what this subagent does
    pub description: String,

    /// System prompt for this subagent
    pub system_prompt: String,

    /// Model to use for this subagent
    pub model: String,

    /// Tools this subagent has access to
    pub allowed_tools: Vec<String>,

    /// Maximum tokens for responses
    pub max_tokens: Option<u32>,

    /// Temperature for responses
    pub temperature: Option<f32>,
}

impl SubagentConfig {
    /// Create a new subagent configuration
    pub fn new(id: String, name: String, description: String) -> Self {
        Self {
            id,
            name,
            description,
            system_prompt: String::new(),
            model: "claude-sonnet-4-6".to_string(),
            allowed_tools: Vec::new(),
            max_tokens: Some(4096),
            temperature: Some(0.7),
        }
    }

    /// Set the system prompt
    pub fn with_system_prompt(mut self, prompt: String) -> Self {
        self.system_prompt = prompt;
        self
    }

    /// Set the model
    pub fn with_model(mut self, model: String) -> Self {
        self.model = model;
        self
    }

    /// Add allowed tools
    pub fn with_tools(mut self, tools: Vec<String>) -> Self {
        self.allowed_tools = tools;
        self
    }

    /// Set max tokens
    pub fn with_max_tokens(mut self, max_tokens: u32) -> Self {
        self.max_tokens = Some(max_tokens);
        self
    }

    /// Set temperature
    pub fn with_temperature(mut self, temperature: f32) -> Self {
        self.temperature = Some(temperature);
        self
    }

    /// Load subagent configuration from a markdown file
    ///
    /// Expects markdown files with YAML frontmatter:
    /// ```markdown
    /// ---
    /// name: Coder
    /// description: Expert at writing and refactoring code
    /// model: claude-sonnet-4-6
    /// ---
    ///
    /// You are an expert coder...
    /// ```
    pub fn from_markdown_file(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read subagent file: {:?}", path))?;

        Self::from_markdown(&content)
    }

    /// Parse subagent configuration from markdown content
    pub fn from_markdown(content: &str) -> Result<Self> {
        // Extract YAML frontmatter
        let frontmatter = content
            .strip_prefix("---")
            .and_then(|s| s.split("---").next())
            .ok_or_else(|| anyhow::anyhow!("Missing YAML frontmatter"))?;

        // Parse YAML
        let yaml: serde_yaml::Value = serde_yaml::from_str(frontmatter)
            .with_context(|| "Failed to parse YAML frontmatter")?;

        let name = yaml["name"]
            .as_str()
            .or_else(|| yaml["title"].as_str())
            .unwrap_or("Unnamed")
            .to_string();

        let description = yaml["description"].as_str().unwrap_or("").to_string();

        let id = yaml["id"]
            .as_str()
            .map(|s| s.to_string())
            .unwrap_or_else(|| name.to_lowercase().replace(' ', "-"));

        let model = yaml["model"]
            .as_str()
            .unwrap_or("claude-sonnet-4-6")
            .to_string();

        // Extract system prompt (content after frontmatter)
        let system_prompt = content.split("---").nth(2).unwrap_or("").trim().to_string();

        // Parse allowed_tools if present
        let allowed_tools = if let Some(tools) = yaml["allowed_tools"].as_sequence() {
            tools
                .iter()
                .filter_map(|v| v.as_str())
                .map(|s| s.to_string())
                .collect()
        } else {
            Vec::new()
        };

        // Parse max_tokens
        let max_tokens = yaml["max_tokens"].as_u64().map(|v| v as u32);

        // Parse temperature
        let temperature = yaml["temperature"].as_f64().map(|v| v as f32);

        Ok(Self {
            id,
            name,
            description,
            system_prompt,
            model,
            allowed_tools,
            max_tokens,
            temperature,
        })
    }
}

/// A specialized subagent
#[derive(Debug, Clone)]
pub struct Subagent {
    config: SubagentConfig,
}

impl Subagent {
    /// Create a new subagent from configuration
    pub fn new(config: SubagentConfig) -> Self {
        Self { config }
    }

    /// Get the subagent's configuration
    pub fn config(&self) -> &SubagentConfig {
        &self.config
    }

    /// Get the subagent's ID
    pub fn id(&self) -> &str {
        &self.config.id
    }

    /// Get the subagent's name
    pub fn name(&self) -> &str {
        &self.config.name
    }

    /// Create a subagent from a markdown file
    pub fn from_markdown_file(path: &Path) -> Result<Self> {
        let config = SubagentConfig::from_markdown_file(path)?;
        Ok(Self::new(config))
    }
}

/// Registry of available subagents
#[derive(Debug, Clone)]
pub struct SubagentRegistry {
    subagents: HashMap<String, Subagent>,
}

impl SubagentRegistry {
    /// Create a new empty registry
    pub fn new() -> Self {
        Self {
            subagents: HashMap::new(),
        }
    }

    /// Register a subagent
    pub fn register(&mut self, subagent: Subagent) -> Result<()> {
        let id = subagent.id().to_string();
        if self.subagents.contains_key(&id) {
            anyhow::bail!("Subagent with id '{}' already registered", id);
        }
        self.subagents.insert(id, subagent);
        Ok(())
    }

    /// Get a subagent by ID
    pub fn get(&self, id: &str) -> Option<&Subagent> {
        self.subagents.get(id)
    }

    /// Get a mutable reference to a subagent
    pub fn get_mut(&mut self, id: &str) -> Option<&mut Subagent> {
        self.subagents.get_mut(id)
    }

    /// List all registered subagent IDs
    pub fn list_ids(&self) -> Vec<String> {
        self.subagents.keys().cloned().collect()
    }

    /// Load subagents from a directory
    ///
    /// Expects a directory containing markdown files with subagent definitions
    pub fn load_from_directory(&mut self, dir: &Path) -> Result<usize> {
        if !dir.exists() {
            return Ok(0); // Directory doesn't exist, nothing to load
        }

        let entries = std::fs::read_dir(dir)
            .with_context(|| format!("Failed to read subagent directory: {:?}", dir))?;

        let mut loaded = 0;
        for entry in entries {
            let entry = entry?;
            let path = entry.path();

            // Skip directories and hidden files
            if path.is_dir()
                || path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .map(|n| n.starts_with('.'))
                    .unwrap_or(true)
            {
                continue;
            }

            // Only process .md files
            if path.extension().and_then(|s| s.to_str()) != Some("md") {
                continue;
            }

            // Load the subagent
            match Subagent::from_markdown_file(&path) {
                Ok(subagent) => {
                    let id = subagent.id().to_string();
                    self.subagents.insert(id, subagent);
                    loaded += 1;
                }
                Err(e) => {
                    tracing::warn!("Failed to load subagent from {:?}: {}", path, e);
                }
            }
        }

        Ok(loaded)
    }

    /// Create a default registry with built-in subagents
    pub fn with_defaults() -> Self {
        let mut registry = Self::new();

        // Coder subagent
        registry
            .register(Subagent::new(
                SubagentConfig::new(
                    "coder".to_string(),
                    "Coder".to_string(),
                    "Expert at writing and refactoring code".to_string(),
                )
                .with_system_prompt(
                    "You are an expert software engineer specializing in writing clean, \
                idiomatic, well-documented code. You follow best practices for the \
                language you're working in and always consider edge cases and error handling."
                        .to_string(),
                )
                .with_model("claude-sonnet-4-6".to_string()),
            ))
            .unwrap_or_else(|e| tracing::warn!("Failed to register coder subagent: {}", e));

        // Debugger subagent
        registry
            .register(Subagent::new(
                SubagentConfig::new(
                    "debugger".to_string(),
                    "Debugger".to_string(),
                    "Expert at diagnosing and fixing bugs".to_string(),
                )
                .with_system_prompt(
                    "You are an expert debugger. You systematically analyze problems, \
                identify root causes, and propose fixes. You always verify that fixes \
                address the actual problem, not just symptoms."
                        .to_string(),
                )
                .with_model("claude-sonnet-4-6".to_string()),
            ))
            .unwrap_or_else(|e| tracing::warn!("Failed to register debugger subagent: {}", e));

        // Reviewer subagent
        registry
            .register(Subagent::new(
                SubagentConfig::new(
                    "reviewer".to_string(),
                    "Reviewer".to_string(),
                    "Expert at reviewing code for quality and issues".to_string(),
                )
                .with_system_prompt(
                    "You are an expert code reviewer. You identify potential bugs, \
                security issues, performance problems, and maintainability concerns. \
                You provide constructive feedback with specific suggestions for improvement."
                        .to_string(),
                )
                .with_model("claude-opus-4-6".to_string()), // Use Opus for more thorough reviews
            ))
            .unwrap_or_else(|e| tracing::warn!("Failed to register reviewer subagent: {}", e));

        registry
    }
}

impl Default for SubagentRegistry {
    fn default() -> Self {
        Self::with_defaults()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_subagent_config_builder() {
        let config = SubagentConfig::new(
            "test".to_string(),
            "Test Agent".to_string(),
            "A test agent".to_string(),
        )
        .with_system_prompt("You are a test agent.".to_string())
        .with_model("claude-haiku-4-5".to_string())
        .with_temperature(0.5);

        assert_eq!(config.id, "test");
        assert_eq!(config.name, "Test Agent");
        assert_eq!(config.model, "claude-haiku-4-5");
        assert_eq!(config.temperature, Some(0.5));
    }

    #[test]
    fn test_subagent_from_markdown() {
        let markdown = r#"---
name: Test Agent
description: A test agent
model: claude-haiku-4-5
max_tokens: 2048
temperature: 0.5
---

You are a test agent.

Your job is to help with testing.
"#;

        let config = SubagentConfig::from_markdown(markdown).unwrap();
        assert_eq!(config.name, "Test Agent");
        assert_eq!(config.description, "A test agent");
        assert_eq!(config.model, "claude-haiku-4-5");
        assert_eq!(config.max_tokens, Some(2048));
        assert_eq!(config.temperature, Some(0.5));
        assert!(config.system_prompt.contains("test agent"));
    }

    #[test]
    fn test_subagent_registry() {
        let mut registry = SubagentRegistry::new();

        let subagent = Subagent::new(SubagentConfig::new(
            "test".to_string(),
            "Test".to_string(),
            "Test agent".to_string(),
        ));

        registry.register(subagent).unwrap();
        assert!(registry.get("test").is_some());
        assert_eq!(registry.list_ids().len(), 1);
    }

    #[test]
    fn test_default_registry() {
        let registry = SubagentRegistry::with_defaults();
        assert!(registry.get("coder").is_some());
        assert!(registry.get("debugger").is_some());
        assert!(registry.get("reviewer").is_some());
    }

    #[test]
    fn test_with_defaults_populates_exactly_three_subagents() {
        let registry = SubagentRegistry::with_defaults();

        // Verify all three built-in subagents are present
        let coder = registry
            .get("coder")
            .expect("coder subagent should be registered");
        assert_eq!(coder.name(), "Coder");
        assert!(
            coder.config().system_prompt.contains("software engineer"),
            "coder should have its system prompt"
        );
        assert_eq!(coder.config().model, "claude-sonnet-4-6");

        let debugger = registry
            .get("debugger")
            .expect("debugger subagent should be registered");
        assert_eq!(debugger.name(), "Debugger");
        assert!(
            debugger.config().system_prompt.contains("debugger"),
            "debugger should have its system prompt"
        );

        let reviewer = registry
            .get("reviewer")
            .expect("reviewer subagent should be registered");
        assert_eq!(reviewer.name(), "Reviewer");
        assert!(
            reviewer.config().system_prompt.contains("code reviewer"),
            "reviewer should have its system prompt"
        );

        // Verify exactly three subagents
        let ids = registry.list_ids();
        assert_eq!(
            ids.len(),
            3,
            "with_defaults should register exactly 3 subagents, got: {:?}",
            ids
        );

        // Verify no unknown subagents
        assert!(
            ids.contains(&"coder".to_string()),
            "ids should contain coder"
        );
        assert!(
            ids.contains(&"debugger".to_string()),
            "ids should contain debugger"
        );
        assert!(
            ids.contains(&"reviewer".to_string()),
            "ids should contain reviewer"
        );
    }

    #[test]
    fn test_default_trait_impl_matches_with_defaults() {
        let from_trait = SubagentRegistry::default();
        let from_method = SubagentRegistry::with_defaults();

        // Both should have the same set of IDs
        let mut trait_ids = from_trait.list_ids();
        let mut method_ids = from_method.list_ids();
        trait_ids.sort();
        method_ids.sort();
        assert_eq!(
            trait_ids, method_ids,
            "Default trait should produce same subagents as with_defaults()"
        );
    }
}
