use super::{
    default_version, Recipe, RecipeAuthor, RecipeParameter, RecipeSettings, RetryConfig, SubRecipe,
};

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
///     .tool("Read")
///     .tool("Grep")
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
