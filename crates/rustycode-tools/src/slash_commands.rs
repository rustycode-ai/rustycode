//! Slash Command System
//!
//! Maps `/command` strings to recipe workflows. Provides both built-in
//! commands and user-defined command overrides.
//!
//! Inspired by goose's `slash_commands` module.
//!
//! # Built-in Commands
//!
//! - `/review` — Code Review recipe
//! - `/bug` — Bug Investigation recipe
//! - `/refactor` — Refactor recipe
//! - `/test` — Write Tests recipe
//!
//! # Example
//!
//! ```ignore
//! use rustycode_tools::slash_commands::SlashCommandRegistry;
//! use rustycode_tools::recipes::RecipeRegistry;
//!
//! let recipes = RecipeRegistry::new();
//! let mut cmds = SlashCommandRegistry::new(recipes);
//! cmds.add_builtins();
//!
//! if let Some(cmd) = cmds.resolve("/review") {
//!     println!("Found command: {} -> {}", cmd.command, cmd.description);
//! }
//! ```

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

use crate::recipes::{Recipe, RecipeRegistry};

/// A resolved slash command with its associated recipe
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlashCommand {
    /// The command string (without leading /)
    pub command: String,
    /// Human-readable description
    pub description: String,
    /// The recipe this command maps to
    pub recipe_title: String,
    /// Optional short alias
    pub alias: Option<String>,
}

/// Registry of slash commands
#[derive(Debug, Clone)]
pub struct SlashCommandRegistry {
    commands: HashMap<String, SlashCommand>,
    recipes: RecipeRegistry,
}

impl SlashCommandRegistry {
    pub fn new(recipes: RecipeRegistry) -> Self {
        Self {
            commands: HashMap::new(),
            recipes,
        }
    }

    /// Create a registry with built-in commands and recipes
    pub fn with_builtins() -> Self {
        let mut recipes = RecipeRegistry::new();
        recipes.add_builtins();

        let mut registry = Self::new(recipes);
        registry.add_builtins();
        registry
    }

    /// Add built-in slash commands
    pub fn add_builtins(&mut self) {
        let builtins = vec![
            SlashCommand {
                command: "review".into(),
                description: "Review code for quality issues".into(),
                recipe_title: "Code Review".into(),
                alias: Some("cr".into()),
            },
            SlashCommand {
                command: "bug".into(),
                description: "Investigate and diagnose a bug".into(),
                recipe_title: "Bug Investigation".into(),
                alias: Some("bi".into()),
            },
            SlashCommand {
                command: "refactor".into(),
                description: "Suggest refactoring improvements".into(),
                recipe_title: "Refactor".into(),
                alias: Some("rf".into()),
            },
            SlashCommand {
                command: "test".into(),
                description: "Generate tests for code".into(),
                recipe_title: "Write Tests".into(),
                alias: Some("wt".into()),
            },
        ];

        for cmd in builtins {
            self.register(cmd);
        }
    }

    /// Register a new slash command
    pub fn register(&mut self, command: SlashCommand) {
        self.commands
            .insert(command.command.clone(), command.clone());
        // Also register alias if present
        if let Some(ref alias) = command.alias {
            let mut aliased = command.clone();
            aliased.alias = None; // Prevent infinite alias chains
            self.commands.insert(alias.clone(), aliased);
        }
    }

    /// Resolve a command string to its `SlashCommand`
    ///
    /// Accepts commands with or without leading `/`
    pub fn resolve(&self, input: &str) -> Option<&SlashCommand> {
        let normalized = input.trim_start_matches('/').to_lowercase();
        self.commands.get(&normalized)
    }

    /// Resolve a command and return the associated recipe
    pub fn resolve_recipe(&self, input: &str) -> Option<&Recipe> {
        let cmd = self.resolve(input)?;
        self.recipes.find(&cmd.recipe_title)
    }

    /// List all registered commands
    pub fn list_commands(&self) -> Vec<&SlashCommand> {
        let mut cmds: Vec<_> = self
            .commands
            .values()
            .filter(|c| c.alias.is_none()) // Don't list aliases separately
            .collect();
        cmds.sort_by(|a, b| a.command.cmp(&b.command));
        cmds
    }

    /// Load custom commands from a directory of YAML/JSON files
    pub fn load_from_dir(&mut self, dir: &Path) -> anyhow::Result<usize> {
        let count = self.recipes.load_from_dir(dir)?;
        // Recipes loaded — any matching title can be used as a command
        Ok(count)
    }

    /// Register a custom command mapping
    pub fn register_custom(
        &mut self,
        command: &str,
        recipe_title: &str,
        description: &str,
    ) -> anyhow::Result<()> {
        if self.recipes.find(recipe_title).is_none() {
            anyhow::bail!("Recipe '{recipe_title}' not found in registry");
        }

        let normalized = command.trim_start_matches('/').to_lowercase();
        self.register(SlashCommand {
            command: normalized,
            description: description.to_string(),
            recipe_title: recipe_title.to_string(),
            alias: None,
        });

        Ok(())
    }

    /// Get the underlying recipe registry
    pub const fn recipes(&self) -> &RecipeRegistry {
        &self.recipes
    }

    /// Get a mutable reference to the recipe registry
    pub const fn recipes_mut(&mut self) -> &mut RecipeRegistry {
        &mut self.recipes
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_builtin_commands() {
        let registry = SlashCommandRegistry::with_builtins();

        assert!(registry.resolve("/review").is_some());
        assert!(registry.resolve("/bug").is_some());
        assert!(registry.resolve("/refactor").is_some());
        assert!(registry.resolve("/test").is_some());
    }

    #[test]
    fn test_resolve_without_slash() {
        let registry = SlashCommandRegistry::with_builtins();

        assert!(registry.resolve("review").is_some());
        assert!(registry.resolve("bug").is_some());
    }

    #[test]
    fn test_resolve_case_insensitive() {
        let registry = SlashCommandRegistry::with_builtins();

        assert!(registry.resolve("/Review").is_some());
        assert!(registry.resolve("/REVIEW").is_some());
        assert!(registry.resolve("Review").is_some());
    }

    #[test]
    fn test_aliases() {
        let registry = SlashCommandRegistry::with_builtins();

        assert!(registry.resolve("/cr").is_some());
        assert!(registry.resolve("/bi").is_some());
        assert!(registry.resolve("/rf").is_some());
        assert!(registry.resolve("/wt").is_some());
    }

    #[test]
    fn test_resolve_recipe() {
        let registry = SlashCommandRegistry::with_builtins();

        let recipe = registry.resolve_recipe("/review");
        assert!(recipe.is_some());
        assert_eq!(recipe.unwrap().title, "Code Review");
    }

    #[test]
    fn test_list_commands() {
        let registry = SlashCommandRegistry::with_builtins();
        let cmds = registry.list_commands();

        assert_eq!(cmds.len(), 4);
        assert_eq!(cmds[0].command, "bug");
        assert_eq!(cmds[1].command, "refactor");
        assert_eq!(cmds[2].command, "review");
        assert_eq!(cmds[3].command, "test");
    }

    #[test]
    fn test_unknown_command() {
        let registry = SlashCommandRegistry::with_builtins();

        assert!(registry.resolve("/unknown").is_none());
        assert!(registry.resolve_recipe("/unknown").is_none());
    }

    #[test]
    fn test_register_custom() {
        let mut registry = SlashCommandRegistry::with_builtins();

        // Register a custom command that maps to an existing recipe
        registry
            .register_custom("quick-review", "Code Review", "Quick code review")
            .unwrap();

        assert!(registry.resolve("/quick-review").is_some());
        let cmd = registry.resolve("/quick-review").unwrap();
        assert_eq!(cmd.recipe_title, "Code Review");
    }

    #[test]
    fn test_register_custom_missing_recipe() {
        let registry = SlashCommandRegistry::with_builtins();

        let result = registry
            .clone()
            .register_custom("foo", "Nonexistent Recipe", "Should fail");
        assert!(result.is_err());
    }
}
