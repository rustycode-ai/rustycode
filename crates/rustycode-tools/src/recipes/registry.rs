use super::{Recipe, RecipeParameter, RecipeParameterKind};
use rustycode_protocol::tool_names as tn;
use std::collections::HashMap;
use std::path::Path;

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
                tn::READ.to_string(),
                tn::GREP.to_string(),
                tn::GLOB.to_string(),
                tn::LIST_DIR.to_string(),
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
            tools: vec!["Read".into(), "Grep".into(), "Glob".into()],
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
            tools: vec!["Read".into(), "Grep".into(), "Glob".into(), "Bash".into()],
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
            tools: vec!["Read".into(), "Grep".into(), "Glob".into()],
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
            tools: vec!["Read".into(), "Grep".into(), "Glob".into(), "Bash".into()],
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
