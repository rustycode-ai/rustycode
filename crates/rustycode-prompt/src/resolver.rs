//! Prompt resolution through a layered chain.
//!
//! Resolution order for a prompt identified by (category, name):
//! 1. User override: `{user_dir}/{category}/{name}.{model}.txt`
//! 2. User override: `{user_dir}/{category}/{name}.txt`
//! 3. Embedded: compile-time `include_str!` prompt file
//! 4. Default: hardcoded const string passed by caller

use std::path::PathBuf;

use handlebars::Handlebars;
use serde_json::Value;

use crate::layered::ModelProvider;
use crate::{Result, TemplateError};

/// Resolves prompts through a layered chain.
///
/// Uses compile-time embedded prompt files as the baseline, with
/// model-specific and user-level overrides checked at runtime.
pub struct PromptResolver {
    user_dir: Option<PathBuf>,
    model: ModelProvider,
}

impl PromptResolver {
    /// Create a resolver for the given model provider.
    #[must_use]
    pub fn new(model: ModelProvider) -> Self {
        Self {
            user_dir: None,
            model,
        }
    }

    /// Set the user override directory (e.g. `~/.rustycode/prompts/`).
    #[must_use]
    pub fn with_user_dir(mut self, dir: PathBuf) -> Self {
        self.user_dir = Some(dir);
        self
    }

    /// Resolve a prompt by category and name through the layering chain.
    pub fn resolve(&self, category: &str, name: &str, default: &str) -> String {
        if let Some(ref dir) = self.user_dir {
            let suffix = model_file_suffix(self.model);
            // 1. User dir, model-specific
            let path = dir.join(category).join(format!("{name}.{suffix}.txt"));
            if let Ok(content) = std::fs::read_to_string(&path) {
                return content;
            }
            // 2. User dir, generic
            let path = dir.join(category).join(format!("{name}.txt"));
            if let Ok(content) = std::fs::read_to_string(&path) {
                return content;
            }
        }

        // 3. Embedded model-specific override
        let suffix = model_file_suffix(self.model);
        if let Some(content) = get_embedded_model_prompt(category, name, suffix) {
            return content.to_string();
        }

        // 4. Embedded generic prompt
        if let Some(content) = get_embedded_prompt(category, name) {
            return content.to_string();
        }

        // 5. Hardcoded default
        default.to_string()
    }

    /// Resolve and render a template with Handlebars variables.
    ///
    /// Registers the `schema_partial` partial for strategy templates.
    pub fn render(
        &self,
        category: &str,
        name: &str,
        default: &str,
        vars: &Value,
    ) -> Result<String> {
        let template = self.resolve(category, name, default);
        let mut hb = Handlebars::new();
        hb.register_escape_fn(handlebars::no_escape);

        #[allow(clippy::expect_used)]
        {
            hb.register_partial(
                "schema_partial",
                include_str!("../prompts/strategies/_schema_partial.txt"),
            )
            .expect("schema_partial is a compile-time constant");
        }

        hb.render_template(&template, vars)
            .map_err(|e| TemplateError::RenderError(e.to_string()))
    }

    /// The model this resolver is configured for.
    #[must_use]
    pub fn model(&self) -> ModelProvider {
        self.model
    }
}

/// Return the filename suffix for a model provider.
const fn model_file_suffix(model: ModelProvider) -> &'static str {
    match model {
        ModelProvider::ClaudeOpus => "claude-opus",
        ModelProvider::ClaudeSonnet => "claude-sonnet",
        ModelProvider::ClaudeHaiku => "claude-haiku",
        ModelProvider::GPT5 => "gpt5",
        ModelProvider::GPT4 => "gpt4",
        ModelProvider::OpenAIReasoning => "o-series",
        ModelProvider::Gemini3 => "gemini3",
        ModelProvider::Gemini2 => "gemini2",
        ModelProvider::Mistral => "mistral",
        ModelProvider::DeepSeek => "deepseek",
        ModelProvider::Llama => "llama",
        ModelProvider::Qwen => "qwen",
        ModelProvider::Cohere => "cohere",
        ModelProvider::Generic => "generic",
    }
}

/// Return the compile-time embedded prompt for a known category/name pair.
fn get_embedded_prompt(category: &str, name: &str) -> Option<&'static str> {
    match (category, name) {
        // Roles
        ("roles", "explore") => Some(include_str!("../prompts/roles/explore.txt")),
        ("roles", "research") => Some(include_str!("../prompts/roles/research.txt")),
        ("roles", "code") => Some(include_str!("../prompts/roles/code.txt")),
        ("roles", "review") => Some(include_str!("../prompts/roles/review.txt")),
        ("roles", "verify") => Some(include_str!("../prompts/roles/verify.txt")),
        ("roles", "plan") => Some(include_str!("../prompts/roles/plan.txt")),
        ("roles", "debug") => Some(include_str!("../prompts/roles/debug.txt")),

        // Strategies
        ("strategies", "sequential") => Some(include_str!("../prompts/strategies/sequential.txt")),
        ("strategies", "dialectic") => Some(include_str!("../prompts/strategies/dialectic.txt")),
        ("strategies", "parallel") => Some(include_str!("../prompts/strategies/parallel.txt")),
        ("strategies", "analogical") => Some(include_str!("../prompts/strategies/analogical.txt")),
        ("strategies", "abductive") => Some(include_str!("../prompts/strategies/abductive.txt")),
        ("strategies", "implementation") => {
            Some(include_str!("../prompts/strategies/implementation.txt"))
        }

        // AST
        ("ast", "system") => Some(include_str!("../prompts/ast/system.txt")),
        ("ast/phases", "classify") => Some(include_str!("../prompts/ast/phases/classify.txt")),
        ("ast/phases", "research") => Some(include_str!("../prompts/ast/phases/research.txt")),
        ("ast/phases", "skeleton") => Some(include_str!("../prompts/ast/phases/skeleton.txt")),
        ("ast/phases", "expand") => Some(include_str!("../prompts/ast/phases/expand.txt")),
        ("ast/phases", "execute") => Some(include_str!("../prompts/ast/phases/execute.txt")),
        ("ast/phases", "verify") => Some(include_str!("../prompts/ast/phases/verify.txt")),

        // Tools
        ("tools", "structured_thinking") => {
            Some(include_str!("../prompts/tools/structured_thinking.txt"))
        }

        // Tasks
        ("tasks", "milestone_decompose") => {
            Some(include_str!("../prompts/tasks/milestone_decompose.txt"))
        }

        _ => None,
    }
}

/// Return a compile-time embedded model-specific prompt override.
fn get_embedded_model_prompt(
    category: &str,
    name: &str,
    model_suffix: &str,
) -> Option<&'static str> {
    match (category, name, model_suffix) {
        // Roles — Claude Opus overrides
        ("roles", "explore", "claude-opus") => {
            Some(include_str!("../prompts/roles/explore.claude-opus.txt"))
        }
        ("roles", "code", "claude-opus") => {
            Some(include_str!("../prompts/roles/code.claude-opus.txt"))
        }
        _ => None,
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn resolve_returns_default_for_unknown_prompt() {
        let resolver = PromptResolver::new(ModelProvider::Generic);
        let result = resolver.resolve("unknown", "nonexistent", "hardcoded default");
        assert_eq!(result, "hardcoded default");
    }

    #[test]
    fn resolve_returns_embedded_for_known_prompt() {
        let resolver = PromptResolver::new(ModelProvider::Generic);
        let result = resolver.resolve("roles", "explore", "default");
        assert!(result.contains("exploration agent"));
        assert!(result.contains("Do NOT modify"));
    }

    #[test]
    fn resolve_returns_embedded_role_code() {
        let resolver = PromptResolver::new(ModelProvider::Generic);
        let result = resolver.resolve("roles", "code", "default");
        assert!(result.contains("coding agent"));
    }

    #[test]
    fn resolve_all_roles_return_content() {
        let resolver = PromptResolver::new(ModelProvider::Generic);
        for name in &[
            "explore", "research", "code", "review", "verify", "plan", "debug",
        ] {
            let result = resolver.resolve("roles", name, "default");
            assert!(
                !result.contains("default"),
                "role '{name}' should resolve to embedded content"
            );
        }
    }

    #[test]
    fn resolve_all_strategies_return_content() {
        let resolver = PromptResolver::new(ModelProvider::Generic);
        for name in &[
            "sequential",
            "dialectic",
            "parallel",
            "analogical",
            "abductive",
            "implementation",
        ] {
            let result = resolver.resolve("strategies", name, "default");
            assert!(
                !result.contains("default"),
                "strategy '{name}' should resolve to embedded content"
            );
        }
    }

    #[test]
    fn resolve_ast_system_returns_content() {
        let resolver = PromptResolver::new(ModelProvider::Generic);
        let result = resolver.resolve("ast", "system", "default");
        assert!(result.contains("Adaptive Structured Thinking"));
    }

    #[test]
    fn resolve_ast_phases_return_content() {
        let resolver = PromptResolver::new(ModelProvider::Generic);
        for name in &[
            "classify", "research", "skeleton", "expand", "execute", "verify",
        ] {
            let result = resolver.resolve("ast/phases", name, "default");
            assert!(
                !result.contains("default"),
                "ast phase '{name}' should resolve to embedded content"
            );
        }
    }

    #[test]
    fn resolve_tools_structured_thinking() {
        let resolver = PromptResolver::new(ModelProvider::Generic);
        let result = resolver.resolve("tools", "structured_thinking", "default");
        assert!(result.contains("structured_thinking tool"));
    }

    #[test]
    fn resolve_tasks_milestone() {
        let resolver = PromptResolver::new(ModelProvider::Generic);
        let result = resolver.resolve("tasks", "milestone_decompose", "default");
        assert!(result.contains("decomposing"));
    }

    #[test]
    fn resolve_user_dir_generic_override() {
        let dir = tempfile::tempdir().unwrap();
        let roles_dir = dir.path().join("roles");
        std::fs::create_dir_all(&roles_dir).unwrap();
        std::fs::write(roles_dir.join("code.txt"), "custom code prompt").unwrap();

        let resolver =
            PromptResolver::new(ModelProvider::Generic).with_user_dir(dir.path().to_path_buf());
        let result = resolver.resolve("roles", "code", "default");
        assert_eq!(result, "custom code prompt");
    }

    #[test]
    fn resolve_user_dir_model_specific_takes_priority() {
        let dir = tempfile::tempdir().unwrap();
        let roles_dir = dir.path().join("roles");
        std::fs::create_dir_all(&roles_dir).unwrap();
        std::fs::write(roles_dir.join("code.txt"), "generic override").unwrap();
        std::fs::write(roles_dir.join("code.claude-opus.txt"), "opus override").unwrap();

        let resolver =
            PromptResolver::new(ModelProvider::ClaudeOpus).with_user_dir(dir.path().to_path_buf());
        let result = resolver.resolve("roles", "code", "default");
        assert_eq!(result, "opus override");
    }

    #[test]
    fn resolve_user_dir_model_specific_only_matches_correct_model() {
        let dir = tempfile::tempdir().unwrap();
        let roles_dir = dir.path().join("roles");
        std::fs::create_dir_all(&roles_dir).unwrap();
        std::fs::write(roles_dir.join("code.claude-opus.txt"), "opus override").unwrap();

        // Sonnet should NOT match the opus override, should fall through to embedded
        let resolver = PromptResolver::new(ModelProvider::ClaudeSonnet)
            .with_user_dir(dir.path().to_path_buf());
        let result = resolver.resolve("roles", "code", "default");
        assert!(
            result.contains("coding agent"),
            "Sonnet should get embedded, got: {result}"
        );
    }

    #[test]
    fn render_strategy_with_variables() {
        let resolver = PromptResolver::new(ModelProvider::Generic);
        let vars = serde_json::json!({
            "problem": "Solve the maze",
            "thoughts_summary": "no prior thoughts",
            "depth": 1,
            "iteration": 1
        });
        let result = resolver
            .render("strategies", "sequential", "default", &vars)
            .unwrap();
        assert!(result.contains("Solve the maze"));
        assert!(result.contains("step-by-step"));
        assert!(result.contains("JSON"));
    }

    #[test]
    fn render_uses_schema_partial() {
        let resolver = PromptResolver::new(ModelProvider::Generic);
        let vars = serde_json::json!({
            "problem": "test problem",
            "thoughts_summary": "",
            "depth": 1,
            "iteration": 1
        });
        let result = resolver
            .render("strategies", "dialectic", "default", &vars)
            .unwrap();
        assert!(result.contains("Analysis|Hypothesis"));
    }

    #[test]
    fn render_plain_text_template() {
        let resolver = PromptResolver::new(ModelProvider::Generic);
        let vars = serde_json::json!({});
        let result = resolver
            .render("roles", "explore", "default", &vars)
            .unwrap();
        assert!(result.contains("exploration agent"));
    }

    #[test]
    fn model_returns_configured_model() {
        let resolver = PromptResolver::new(ModelProvider::ClaudeOpus);
        assert_eq!(resolver.model(), ModelProvider::ClaudeOpus);
    }

    #[test]
    fn model_file_suffix_all_variants() {
        assert_eq!(model_file_suffix(ModelProvider::ClaudeOpus), "claude-opus");
        assert_eq!(
            model_file_suffix(ModelProvider::ClaudeSonnet),
            "claude-sonnet"
        );
        assert_eq!(
            model_file_suffix(ModelProvider::ClaudeHaiku),
            "claude-haiku"
        );
        assert_eq!(model_file_suffix(ModelProvider::GPT5), "gpt5");
        assert_eq!(model_file_suffix(ModelProvider::GPT4), "gpt4");
        assert_eq!(
            model_file_suffix(ModelProvider::OpenAIReasoning),
            "o-series"
        );
        assert_eq!(model_file_suffix(ModelProvider::Gemini3), "gemini3");
        assert_eq!(model_file_suffix(ModelProvider::Gemini2), "gemini2");
        assert_eq!(model_file_suffix(ModelProvider::Mistral), "mistral");
        assert_eq!(model_file_suffix(ModelProvider::DeepSeek), "deepseek");
        assert_eq!(model_file_suffix(ModelProvider::Llama), "llama");
        assert_eq!(model_file_suffix(ModelProvider::Qwen), "qwen");
        assert_eq!(model_file_suffix(ModelProvider::Cohere), "cohere");
        assert_eq!(model_file_suffix(ModelProvider::Generic), "generic");
    }
}
