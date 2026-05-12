//! Recipe System
//!
//! Reusable workflow definitions inspired by goose's recipe system.

pub mod builder;
pub mod registry;

#[cfg(test)]
mod tests;

pub use builder::RecipeBuilder;
pub use registry::RecipeRegistry;

use serde::{Deserialize, Serialize};

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
