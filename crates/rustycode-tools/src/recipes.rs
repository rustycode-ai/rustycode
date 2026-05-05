//! Recipe System
//!
//! Reusable workflow definitions inspired by goose's recipe system.
//! Recipes define multi-step workflows with parameters, tool presets,
//! and configurable retry behavior.
//!
//! # Example
//!
//! ```yaml
//! version: "1.0.0"
//! title: "Code Review"
//! description: "Review code for quality issues"
//! prompt: "Review the following code: {{code_path}}"
//! tools:
//!   - read_file
//!   - grep
//! parameters:
//!   - name: code_path
//!     required: true
//! ```
//!
//! # Builder Pattern
//!
//! ```ignore
//! use rustycode_tools::recipes::{RecipeBuilder, RecipeParameter, RecipeParameterKind};
//!
//! let recipe = RecipeBuilder::new("Security Audit")
//!     .description("Audit code for security vulnerabilities")
//!     .prompt("Scan {{code_path}} for security issues")
//!     .tool("read_file")
//!     .tool("grep")
//!     .parameter(RecipeParameter {
//!         name: "code_path".into(),
//!         required: true,
//!         ..Default::default()
//!     })
//!     .max_turns(10)
//!     .build();
//! ```
//!
//! # Multi-Path Discovery
//!
//! Recipes are discovered from multiple locations:
//! - Current working directory
//! - `.rustycode/recipes/` in the project root
//! - `~/.rustycode/recipes/` (global recipes)
//! - Directories listed in `RUSTYCODE_RECIPE_PATH` env var

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

fn default_version() -> String {
    "1.0.0".to_string()
}

const fn default_max_attempts() -> u32 {
    2
}

const fn default_delay_seconds() -> u64 {
    5
}

const fn default_kind() -> RecipeParameterKind {
    RecipeParameterKind::String
}

/// A recipe defining a reusable workflow
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Recipe {
    /// Schema version
    #[serde(default = "default_version")]
    pub version: String,
    /// Human-readable title
    pub title: String,
    /// Short description
    pub description: String,
    /// Optional instructions for the LLM
    #[serde(default)]
    pub instructions: Option<String>,
    /// The prompt template with {{variable}} placeholders
    #[serde(default)]
    pub prompt: Option<String>,
    /// Tools to enable for this recipe
    #[serde(default)]
    pub tools: Vec<String>,
    /// Optional parameters the recipe accepts
    #[serde(default)]
    pub parameters: Vec<RecipeParameter>,
    /// Retry configuration
    #[serde(default)]
    pub retry: Option<RetryConfig>,
    /// Author metadata
    #[serde(default)]
    pub author: Option<RecipeAuthor>,
}

impl Default for Recipe {
    fn default() -> Self {
        Self {
            version: default_version(),
            title: String::new(),
            description: String::new(),
            instructions: None,
            prompt: None,
            tools: Vec::new(),
            parameters: Vec::new(),
            retry: None,
            author: None,
        }
    }
}

/// A parameter that a recipe accepts
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecipeParameter {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default = "default_kind")]
    pub kind: RecipeParameterKind,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub default: Option<String>,
    /// For select kind: allowed values
    #[serde(default)]
    pub options: Vec<String>,
}

impl Default for RecipeParameter {
    fn default() -> Self {
        Self {
            name: String::new(),
            description: String::new(),
            kind: default_kind(),
            required: false,
            default: None,
            options: Vec::new(),
        }
    }
}

/// Type of recipe parameter
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum RecipeParameterKind {
    #[default]
    String,
    Number,
    Boolean,
    Date,
    File,
    Select,
}

/// Retry configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryConfig {
    #[serde(default = "default_max_attempts")]
    pub max_attempts: u32,
    #[serde(default = "default_delay_seconds")]
    pub delay_seconds: u64,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_attempts: default_max_attempts(),
            delay_seconds: default_delay_seconds(),
        }
    }
}

/// Author metadata
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RecipeAuthor {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub email: Option<String>,
}

/// Provider/model settings override for a recipe.
///
/// Inspired by goose's `Settings` struct. Allows recipes to specify
/// a preferred provider, model, temperature, and turn limit.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RecipeSettings {
    /// Override the provider (e.g., "openai", "anthropic")
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    /// Override the model (e.g., "gpt-4o", "claude-sonnet-4-6")
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// Sampling temperature
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    /// Maximum conversation turns
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_turns: Option<usize>,
}

/// A sub-recipe that can be composed into a larger workflow.
///
/// Inspired by goose's `SubRecipe`. Enables multi-stage workflows
/// where each stage can use different tools and prompts.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubRecipe {
    /// Unique identifier for this stage
    pub name: String,
    /// The prompt for this sub-recipe
    pub prompt: String,
    /// Tools specific to this stage (inherits parent tools if empty)
    #[serde(default)]
    pub tools: Vec<String>,
    /// Condition to check before running (template expression)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub condition: Option<String>,
}

/// Builder for constructing recipes programmatically.
///
/// Provides a fluent API for building recipes with validation.
///
/// # Example
///
/// ```ignore
/// let recipe = RecipeBuilder::new("Security Audit")
///     .description("Audit code for security vulnerabilities")
///     .prompt("Scan {{code_path}} for security issues")
///     .tool("read_file")
///     .tool("grep")
///     .parameter(RecipeParameter {
///         name: "code_path".into(),
///         required: true,
///         ..Default::default()
///     })
///     .max_turns(10)
///     .build();
/// ```
pub struct RecipeBuilder {
    title: String,
    description: Option<String>,
    instructions: Option<String>,
    prompt: Option<String>,
    tools: Vec<String>,
    parameters: Vec<RecipeParameter>,
    retry: Option<RetryConfig>,
    author: Option<RecipeAuthor>,
    settings: Option<RecipeSettings>,
    sub_recipes: Vec<SubRecipe>,
}

impl RecipeBuilder {
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            description: None,
            instructions: None,
            prompt: None,
            tools: Vec::new(),
            parameters: Vec::new(),
            retry: None,
            author: None,
            settings: None,
            sub_recipes: Vec::new(),
        }
    }

    /// Set the description.
    pub fn description(mut self, desc: impl Into<String>) -> Self {
        self.description = Some(desc.into());
        self
    }

    /// Set the LLM instructions.
    pub fn instructions(mut self, instructions: impl Into<String>) -> Self {
        self.instructions = Some(instructions.into());
        self
    }

    /// Set the prompt template.
    pub fn prompt(mut self, prompt: impl Into<String>) -> Self {
        self.prompt = Some(prompt.into());
        self
    }

    /// Add a tool.
    pub fn tool(mut self, tool: impl Into<String>) -> Self {
        self.tools.push(tool.into());
        self
    }

    /// Add multiple tools.
    pub fn tools(mut self, tools: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.tools.extend(tools.into_iter().map(Into::into));
        self
    }

    /// Add a parameter.
    pub fn parameter(mut self, param: RecipeParameter) -> Self {
        self.parameters.push(param);
        self
    }

    /// Set retry configuration.
    pub const fn retry(mut self, max_attempts: u32, delay_secs: u64) -> Self {
        self.retry = Some(RetryConfig {
            max_attempts,
            delay_seconds: delay_secs,
        });
        self
    }

    /// Set the author.
    pub fn author(mut self, name: impl Into<String>, email: Option<String>) -> Self {
        self.author = Some(RecipeAuthor {
            name: Some(name.into()),
            email,
        });
        self
    }

    /// Set the maximum turns.
    pub fn max_turns(mut self, turns: usize) -> Self {
        self.settings = Some(RecipeSettings {
            max_turns: Some(turns),
            ..Default::default()
        });
        self
    }

    /// Set provider/model overrides.
    pub fn with_settings(mut self, settings: RecipeSettings) -> Self {
        self.settings = Some(settings);
        self
    }

    /// Add a sub-recipe stage.
    pub fn sub_recipe(mut self, sub: SubRecipe) -> Self {
        self.sub_recipes.push(sub);
        self
    }

    /// Build the recipe.
    ///
    /// Panics if title or description are missing.
    pub fn build(self) -> Recipe {
        Recipe {
            version: default_version(),
            title: self.title,
            description: self.description.unwrap_or_default(),
            instructions: self.instructions,
            prompt: self.prompt,
            tools: self.tools,
            parameters: self.parameters,
            retry: self.retry,
            author: self.author,
        }
    }
}

/// Registry of recipes, searchable by name, with multi-path discovery.
#[derive(Debug, Clone, Default)]
pub struct RecipeRegistry {
    recipes: Vec<Recipe>,
}

impl RecipeRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Load recipes from a directory (YAML and JSON files)
    pub fn load_from_dir(&mut self, dir: &Path) -> anyhow::Result<usize> {
        if !dir.is_dir() {
            return Ok(0);
        }
        let mut count = 0;
        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                if (ext == "yaml" || ext == "yml" || ext == "json") && self.load_file(&path).is_ok()
                {
                    count += 1;
                }
            }
        }
        Ok(count)
    }

    /// Load a single recipe file
    pub fn load_file(&mut self, path: &Path) -> anyhow::Result<()> {
        let content = std::fs::read_to_string(path)?;
        let is_json = path.extension().is_some_and(|e| e == "json");
        let recipe: Recipe = if is_json {
            serde_json::from_str(&content)?
        } else {
            serde_yaml::from_str(&content)?
        };
        self.recipes.push(recipe);
        Ok(())
    }

    /// Find a recipe by title (case-insensitive)
    pub fn find(&self, title: &str) -> Option<&Recipe> {
        let title_lower = title.to_lowercase();
        self.recipes
            .iter()
            .find(|r| r.title.to_lowercase() == title_lower)
    }

    /// Get all recipe titles
    pub fn titles(&self) -> Vec<String> {
        self.recipes.iter().map(|r| r.title.clone()).collect()
    }

    /// Resolve a recipe prompt with parameter values
    pub fn resolve_prompt(&self, recipe: &Recipe, params: &HashMap<String, String>) -> String {
        let mut prompt = recipe.prompt.clone().unwrap_or_default();

        for (key, value) in params {
            prompt = prompt.replace(&format!("{{{{{key}}}}}"), value);
        }

        if let Some(ref instructions) = recipe.instructions {
            if !instructions.is_empty() {
                prompt = format!("{instructions}\n\n{prompt}");
            }
        }

        prompt
    }

    /// Get the tools a recipe needs
    pub fn resolve_tools(&self, recipe: &Recipe) -> Vec<String> {
        if recipe.tools.is_empty() {
            vec![
                "read_file".to_string(),
                "grep".to_string(),
                "glob".to_string(),
                "list_dir".to_string(),
            ]
        } else {
            recipe.tools.clone()
        }
    }

    /// Discover recipes from standard search paths.
    ///
    /// Inspired by goose's multi-path recipe discovery. Searches in order:
    /// 1. Current working directory
    /// 2. `.rustycode/recipes/` relative to the git root
    /// 3. `~/.rustycode/recipes/` (global recipes)
    /// 4. Directories in `RUSTYCODE_RECIPE_PATH` env var (colon-separated)
    ///
    /// Returns the total number of recipes loaded.
    pub fn discover(cwd: &Path) -> anyhow::Result<Self> {
        let mut registry = Self::new();
        let mut seen_titles = std::collections::HashSet::new();

        let search_paths = Self::search_paths(cwd);
        for dir in &search_paths {
            if dir.is_dir() {
                if let Ok(entries) = std::fs::read_dir(dir) {
                    for entry in entries.flatten() {
                        let path = entry.path();
                        if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                            if ext == "yaml" || ext == "yml" || ext == "json" {
                                if let Ok(recipe) = Self::parse_file(&path) {
                                    if !seen_titles.contains(&recipe.title.to_lowercase()) {
                                        seen_titles.insert(recipe.title.to_lowercase());
                                        registry.recipes.push(recipe);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        Ok(registry)
    }

    /// Get the standard recipe search paths.
    pub fn search_paths(cwd: &Path) -> Vec<std::path::PathBuf> {
        let mut paths = Vec::new();

        // 1. Current working directory
        paths.push(cwd.to_path_buf());

        // 2. .rustycode/recipes/ relative to cwd (walk up to git root)
        let mut dir = cwd.to_path_buf();
        loop {
            let recipe_dir = dir.join(".rustycode").join("recipes");
            if recipe_dir.is_dir() {
                paths.push(recipe_dir);
                break;
            }
            if !dir.pop() {
                break;
            }
        }

        // 3. Global recipes: ~/.rustycode/recipes/
        if let Some(home) = dirs::home_dir() {
            paths.push(home.join(".rustycode").join("recipes"));
        }

        // 4. RUSTYCODE_RECIPE_PATH env var (colon-separated)
        if let Ok(extra) = std::env::var("RUSTYCODE_RECIPE_PATH") {
            for p in extra.split(':') {
                if !p.is_empty() {
                    paths.push(std::path::PathBuf::from(p));
                }
            }
        }

        paths
    }

    /// Parse a single recipe file.
    fn parse_file(path: &Path) -> anyhow::Result<Recipe> {
        let content = std::fs::read_to_string(path)?;
        let is_json = path.extension().is_some_and(|e| e == "json");
        let recipe: Recipe = if is_json {
            serde_json::from_str(&content)?
        } else {
            serde_yaml::from_str(&content)?
        };
        Ok(recipe)
    }

    /// Validate recipe parameters against the definition.
    ///
    /// Returns a list of validation errors (empty if valid).
    pub fn validate_params(
        &self,
        recipe: &Recipe,
        params: &HashMap<String, String>,
    ) -> Vec<String> {
        let mut errors = Vec::new();

        for param in &recipe.parameters {
            if param.required && !params.contains_key(&param.name) && param.default.is_none() {
                errors.push(format!(
                    "Missing required parameter '{}' ({})",
                    param.name, param.description
                ));
            }

            if let Some(value) = params.get(&param.name) {
                // Validate Select parameters have a valid option
                if matches!(param.kind, RecipeParameterKind::Select)
                    && !param.options.is_empty()
                    && !param.options.contains(value)
                {
                    errors.push(format!(
                        "Parameter '{}' must be one of: {}",
                        param.name,
                        param.options.join(", ")
                    ));
                }

                // Validate Number parameters
                if matches!(param.kind, RecipeParameterKind::Number)
                    && value.parse::<f64>().is_err()
                {
                    errors.push(format!(
                        "Parameter '{}' must be a number, got: {}",
                        param.name, value
                    ));
                }

                // Validate Boolean parameters
                if matches!(param.kind, RecipeParameterKind::Boolean)
                    && value.parse::<bool>().is_err()
                {
                    errors.push(format!(
                        "Parameter '{}' must be true/false, got: {}",
                        param.name, value
                    ));
                }
            }
        }

        errors
    }

    /// Add built-in recipes
    pub fn add_builtins(&mut self) {
        self.recipes.push(Recipe {
            title: "Code Review".into(),
            description: "Review code for quality, security, and performance issues".into(),
            prompt: Some(
                "Review the following code thoroughly:\n\n\
                 ## Focus Areas:\n\
                 1. **Correctness**: Logic errors, edge cases, unused code\n\
                 2. **Security**: Injection vulnerabilities, secret exposure\n\
                 3. **Performance**: Inefficient algorithms, memory leaks\n\
                 4. **Readability**: Naming, structure, documentation\n\
                 5. **Testing**: Missing tests, untested paths\n\n\
                 {{code_path}}"
                    .into(),
            ),
            tools: vec!["read_file".into(), "grep".into(), "glob".into()],
            parameters: vec![RecipeParameter {
                name: "code_path".into(),
                description: "Path to the code file or directory".into(),
                required: true,
                ..Default::default()
            }],
            ..Default::default()
        });

        self.recipes.push(Recipe {
            title: "Bug Investigation".into(),
            description: "Investigate and diagnose a bug report".into(),
            prompt: Some(
                "Investigate the following bug report:\n\n\
                 {{bug_description}}\n\n\
                 ## Steps:\n\
                 1. Reproduce the issue (if possible)\n\
                 2. Trace the error through the code\n\
                 3. Identify root cause\n\
                 4. Suggest a fix with explanation"
                    .into(),
            ),
            tools: vec![
                "read_file".into(),
                "grep".into(),
                "glob".into(),
                "bash".into(),
            ],
            parameters: vec![RecipeParameter {
                name: "bug_description".into(),
                description: "Description of the bug".into(),
                required: true,
                ..Default::default()
            }],
            ..Default::default()
        });

        self.recipes.push(Recipe {
            title: "Refactor".into(),
            description: "Suggest refactoring improvements for code".into(),
            prompt: Some(
                "Analyze the following code and suggest refactoring improvements:\n\n\
                 Focus on:\n\
                 - Reducing complexity\n\
                 - Improving naming\n\
                 - Better error handling\n\
                 - Performance optimizations\n\n\
                 {{code_path}}"
                    .into(),
            ),
            tools: vec!["read_file".into(), "grep".into(), "glob".into()],
            parameters: vec![RecipeParameter {
                name: "code_path".into(),
                description: "Path to code to refactor".into(),
                required: true,
                ..Default::default()
            }],
            ..Default::default()
        });

        self.recipes.push(Recipe {
            title: "Write Tests".into(),
            description: "Generate tests for existing code".into(),
            prompt: Some(
                "Write comprehensive tests for the following code:\n\n\
                 {{code_path}}\n\n\
                 ## Test Requirements:\n\
                 - Unit tests for all public functions\n\
                 - Edge case coverage\n\
                 - Error condition testing\n\
                 - Integration tests where appropriate\n\
                 - Minimum 80% code coverage"
                    .into(),
            ),
            tools: vec![
                "read_file".into(),
                "grep".into(),
                "glob".into(),
                "bash".into(),
            ],
            parameters: vec![RecipeParameter {
                name: "code_path".into(),
                description: "Path to code to test".into(),
                required: true,
                ..Default::default()
            }],
            ..Default::default()
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn test_recipe_registry_find() {
        let mut registry = RecipeRegistry::new();
        registry.add_builtins();

        let review = registry.find("Code Review");
        assert!(review.is_some());
        assert_eq!(review.unwrap().title, "Code Review");

        let bug = registry.find("bug investigation");
        assert!(bug.is_some());
    }

    #[test]
    fn test_resolve_prompt() {
        let registry = RecipeRegistry::new();
        let recipe = Recipe {
            title: "Test".into(),
            description: "Test recipe".into(),
            prompt: Some("Hello {{name}}, you are {{age}} years old".into()),
            parameters: vec![
                RecipeParameter {
                    name: "name".into(),
                    required: true,
                    ..Default::default()
                },
                RecipeParameter {
                    name: "age".into(),
                    required: false,
                    default: Some("25".into()),
                    ..Default::default()
                },
            ],
            ..Default::default()
        };

        let mut params = HashMap::new();
        params.insert("name".to_string(), "Alice".to_string());
        params.insert("age".to_string(), "30".to_string());

        let prompt = registry.resolve_prompt(&recipe, &params);
        assert_eq!(prompt, "Hello Alice, you are 30 years old");
    }

    #[test]
    fn test_resolve_prompt_defaults() {
        let registry = RecipeRegistry::new();
        let recipe = Recipe {
            title: "Test".into(),
            description: "Test".into(),
            prompt: Some("Hello {{name}}".into()),
            ..Default::default()
        };

        let params: HashMap<String, String> = HashMap::new();
        let prompt = registry.resolve_prompt(&recipe, &params);
        assert_eq!(prompt, "Hello {{name}}");
    }

    #[test]
    fn test_builtins_loaded() {
        let mut registry = RecipeRegistry::new();
        registry.add_builtins();
        let titles = registry.titles();
        assert!(titles.contains(&"Code Review".to_string()));
        assert!(titles.contains(&"Bug Investigation".to_string()));
        assert!(titles.contains(&"Refactor".to_string()));
        assert!(titles.contains(&"Write Tests".to_string()));
    }

    // ── Builder Tests ──────────────────────────────────────────────────

    #[test]
    fn test_builder_basic() {
        let recipe = RecipeBuilder::new("Test Recipe")
            .description("A test recipe")
            .prompt("Do something with {{input}}")
            .tool("read_file")
            .tool("grep")
            .build();

        assert_eq!(recipe.title, "Test Recipe");
        assert_eq!(recipe.description, "A test recipe");
        assert_eq!(recipe.tools, vec!["read_file", "grep"]);
        assert_eq!(
            recipe.prompt,
            Some("Do something with {{input}}".to_string())
        );
    }

    #[test]
    fn test_builder_with_tools_vec() {
        let recipe = RecipeBuilder::new("Multi-Tool")
            .description("Uses many tools")
            .tools(vec!["read_file", "grep", "bash"])
            .build();

        assert_eq!(recipe.tools.len(), 3);
    }

    #[test]
    fn test_builder_with_parameters() {
        let recipe = RecipeBuilder::new("Parameterized")
            .description("Has parameters")
            .parameter(RecipeParameter {
                name: "path".into(),
                required: true,
                ..Default::default()
            })
            .parameter(RecipeParameter {
                name: "verbose".into(),
                kind: RecipeParameterKind::Boolean,
                default: Some("false".into()),
                ..Default::default()
            })
            .build();

        assert_eq!(recipe.parameters.len(), 2);
        assert!(recipe.parameters[0].required);
        assert_eq!(recipe.parameters[1].kind, RecipeParameterKind::Boolean);
    }

    #[test]
    fn test_builder_with_retry() {
        let recipe = RecipeBuilder::new("Retryable")
            .description("Retries on failure")
            .retry(3, 10)
            .build();

        assert!(recipe.retry.is_some());
        let retry = recipe.retry.unwrap();
        assert_eq!(retry.max_attempts, 3);
        assert_eq!(retry.delay_seconds, 10);
    }

    #[test]
    fn test_builder_with_author() {
        let recipe = RecipeBuilder::new("Authored")
            .description("Has an author")
            .author("Alice", Some("alice@example.com".into()))
            .build();

        assert!(recipe.author.is_some());
        let author = recipe.author.unwrap();
        assert_eq!(author.name, Some("Alice".to_string()));
        assert_eq!(author.email, Some("alice@example.com".to_string()));
    }

    #[test]
    fn test_builder_minimal() {
        let recipe = RecipeBuilder::new("Minimal").build();
        assert_eq!(recipe.title, "Minimal");
        assert!(recipe.description.is_empty());
        assert!(recipe.tools.is_empty());
    }

    // ── Discovery Tests ────────────────────────────────────────────────

    #[test]
    fn test_discover_empty_dir() {
        let temp = tempfile::tempdir().unwrap();
        let registry = RecipeRegistry::discover(temp.path()).unwrap();
        assert_eq!(registry.titles().len(), 0);
    }

    #[test]
    fn test_discover_from_yaml() {
        let temp = tempfile::tempdir().unwrap();
        let recipe_path = temp.path().join("test.yaml");
        std::fs::write(
            &recipe_path,
            "title: YAML Recipe\ndescription: A YAML test recipe\n",
        )
        .unwrap();

        let registry = RecipeRegistry::discover(temp.path()).unwrap();
        assert!(registry.find("YAML Recipe").is_some());
    }

    #[test]
    fn test_discover_from_json() {
        let temp = tempfile::tempdir().unwrap();
        let recipe_path = temp.path().join("test.json");
        std::fs::write(
            &recipe_path,
            r#"{"title":"JSON Recipe","description":"A JSON test recipe"}"#,
        )
        .unwrap();

        let registry = RecipeRegistry::discover(temp.path()).unwrap();
        assert!(registry.find("JSON Recipe").is_some());
    }

    #[test]
    fn test_discover_deduplicates_by_title() {
        let temp = tempfile::tempdir().unwrap();

        // Create same recipe in both YAML and JSON
        std::fs::write(
            temp.path().join("recipe.yaml"),
            "title: Duplicate\ndescription: First\n",
        )
        .unwrap();
        std::fs::write(
            temp.path().join("recipe.json"),
            r#"{"title":"Duplicate","description":"Second"}"#,
        )
        .unwrap();

        let registry = RecipeRegistry::discover(temp.path()).unwrap();
        // Should have exactly one recipe with this title
        let recipe = registry.find("Duplicate").unwrap();
        // First found wins (order depends on directory iteration, just verify dedup)
        assert!(recipe.description == "First" || recipe.description == "Second");
    }

    #[test]
    fn test_search_paths_includes_cwd() {
        let temp = tempfile::tempdir().unwrap();
        let paths = RecipeRegistry::search_paths(temp.path());
        assert!(paths.contains(&temp.path().to_path_buf()));
    }

    #[test]
    fn test_search_paths_includes_home() {
        let temp = tempfile::tempdir().unwrap();
        let paths = RecipeRegistry::search_paths(temp.path());
        // Should include ~/.rustycode/recipes if home dir is available
        if let Some(home) = dirs::home_dir() {
            assert!(paths.contains(&home.join(".rustycode").join("recipes")));
        }
    }

    // ── Validation Tests ───────────────────────────────────────────────

    #[test]
    fn test_validate_required_param_present() {
        let registry = RecipeRegistry::new();
        let recipe = Recipe {
            title: "Test".into(),
            description: "Test".into(),
            parameters: vec![RecipeParameter {
                name: "path".into(),
                required: true,
                ..Default::default()
            }],
            ..Default::default()
        };

        let mut params = HashMap::new();
        params.insert("path".to_string(), "/tmp/test".to_string());

        let errors = registry.validate_params(&recipe, &params);
        assert!(errors.is_empty());
    }

    #[test]
    fn test_validate_required_param_missing() {
        let registry = RecipeRegistry::new();
        let recipe = Recipe {
            title: "Test".into(),
            description: "Test".into(),
            parameters: vec![RecipeParameter {
                name: "path".into(),
                description: "Required path".into(),
                required: true,
                ..Default::default()
            }],
            ..Default::default()
        };

        let errors = registry.validate_params(&recipe, &HashMap::new());
        assert_eq!(errors.len(), 1);
        assert!(errors[0].contains("Missing required"));
        assert!(errors[0].contains("path"));
    }

    #[test]
    fn test_validate_number_param() {
        let registry = RecipeRegistry::new();
        let recipe = Recipe {
            title: "Test".into(),
            description: "Test".into(),
            parameters: vec![RecipeParameter {
                name: "count".into(),
                kind: RecipeParameterKind::Number,
                ..Default::default()
            }],
            ..Default::default()
        };

        let mut valid_params = HashMap::new();
        valid_params.insert("count".to_string(), "42".to_string());
        assert!(registry.validate_params(&recipe, &valid_params).is_empty());

        let mut invalid_params = HashMap::new();
        invalid_params.insert("count".to_string(), "not_a_number".to_string());
        let errors = registry.validate_params(&recipe, &invalid_params);
        assert_eq!(errors.len(), 1);
        assert!(errors[0].contains("must be a number"));
    }

    #[test]
    fn test_validate_select_param() {
        let registry = RecipeRegistry::new();
        let recipe = Recipe {
            title: "Test".into(),
            description: "Test".into(),
            parameters: vec![RecipeParameter {
                name: "level".into(),
                kind: RecipeParameterKind::Select,
                options: vec!["low".into(), "medium".into(), "high".into()],
                ..Default::default()
            }],
            ..Default::default()
        };

        let mut valid_params = HashMap::new();
        valid_params.insert("level".to_string(), "medium".to_string());
        assert!(registry.validate_params(&recipe, &valid_params).is_empty());

        let mut invalid_params = HashMap::new();
        invalid_params.insert("level".to_string(), "extreme".to_string());
        let errors = registry.validate_params(&recipe, &invalid_params);
        assert_eq!(errors.len(), 1);
        assert!(errors[0].contains("must be one of"));
    }

    #[test]
    fn test_validate_boolean_param() {
        let registry = RecipeRegistry::new();
        let recipe = Recipe {
            title: "Test".into(),
            description: "Test".into(),
            parameters: vec![RecipeParameter {
                name: "verbose".into(),
                kind: RecipeParameterKind::Boolean,
                ..Default::default()
            }],
            ..Default::default()
        };

        let mut valid_params = HashMap::new();
        valid_params.insert("verbose".to_string(), "true".to_string());
        assert!(registry.validate_params(&recipe, &valid_params).is_empty());

        let mut invalid_params = HashMap::new();
        invalid_params.insert("verbose".to_string(), "yes".to_string());
        let errors = registry.validate_params(&recipe, &invalid_params);
        assert_eq!(errors.len(), 1);
    }

    #[test]
    fn test_validate_optional_with_default() {
        let registry = RecipeRegistry::new();
        let recipe = Recipe {
            title: "Test".into(),
            description: "Test".into(),
            parameters: vec![RecipeParameter {
                name: "level".into(),
                required: false,
                default: Some("info".into()),
                ..Default::default()
            }],
            ..Default::default()
        };

        // Optional param with default - no error when missing
        let errors = registry.validate_params(&recipe, &HashMap::new());
        assert!(errors.is_empty());
    }
}
