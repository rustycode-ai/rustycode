//! Template-based Prompt System
//!
//! Handlebars-powered prompt templates with context injection.
//! Inspired by forgecode's `Template<T>` pattern.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Context available to all prompt templates
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptContext {
    /// Current working directory
    pub cwd: String,
    /// Project name (directory name)
    pub project_name: String,
    /// List of open/modified files
    pub open_files: Vec<String>,
    /// Git branch name
    pub git_branch: Option<String>,
    /// Git status summary
    pub git_status: Option<String>,
    /// OS/platform info
    pub platform: String,
    /// Current date/time
    pub current_date: String,
    /// Custom variables for template expansion
    #[serde(flatten)]
    pub variables: HashMap<String, String>,
}

/// A rendered prompt with metadata
#[derive(Debug, Clone)]
pub struct RenderedPrompt {
    pub content: String,
    pub template_name: String,
    pub token_estimate: usize,
}

/// Template engine for rendering prompts
pub struct PromptTemplateEngine {
    registry: handlebars::Handlebars<'static>,
}

impl PromptTemplateEngine {
    pub fn new() -> Self {
        let mut registry = handlebars::Handlebars::new();
        registry.register_escape_fn(handlebars::no_escape);

        // Register built-in templates
        registry
            .register_template_string(
                "system_default",
                include_str!("templates/system_default.hbs"),
            )
            .expect("Failed to register system_default template");
        registry
            .register_template_string(
                "system_code_review",
                include_str!("templates/system_code_review.hbs"),
            )
            .expect("Failed to register system_code_review template");
        registry
            .register_template_string(
                "system_refactor",
                include_str!("templates/system_refactor.hbs"),
            )
            .expect("Failed to register system_refactor template");
        registry
            .register_template_string("user_task", include_str!("templates/user_task.hbs"))
            .expect("Failed to register user_task template");

        Self { registry }
    }

    /// Register a custom template
    pub fn register_template(&mut self, name: &str, template: &str) -> anyhow::Result<()> {
        self.registry
            .register_template_string(name, template)
            .map_err(|e| anyhow::anyhow!("Failed to register template '{name}': {e}"))?;
        Ok(())
    }

    /// Render a template with context
    pub fn render(
        &self,
        template_name: &str,
        context: &PromptContext,
    ) -> anyhow::Result<RenderedPrompt> {
        let content = self
            .registry
            .render(template_name, context)
            .map_err(|e| anyhow::anyhow!("Failed to render template '{template_name}': {e}"))?;
        let token_estimate = rustycode_protocol::estimate_tokens(&content);
        Ok(RenderedPrompt {
            content,
            template_name: template_name.to_string(),
            token_estimate,
        })
    }

    /// Render a raw template string with context
    pub fn render_string(&self, template: &str, context: &PromptContext) -> anyhow::Result<String> {
        self.registry
            .render_template(template, context)
            .map_err(|e| anyhow::anyhow!("Failed to render template string: {e}"))
    }
}

impl Default for PromptTemplateEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl PromptContext {
    /// Build context from current environment
    pub fn from_environment(cwd: &str) -> Self {
        let project_name = std::path::Path::new(cwd)
            .canonicalize()
            .ok()
            .and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_string()))
            .unwrap_or_default();

        let git_branch = std::process::Command::new("git")
            .args(["branch", "--show-current"])
            .current_dir(cwd)
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .map(|s| s.trim().to_string());

        let git_status = std::process::Command::new("git")
            .args(["status", "--short"])
            .current_dir(cwd)
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .and_then(|s| {
                let status = s.trim();
                if status.is_empty() {
                    None
                } else {
                    Some(status.to_string())
                }
            });

        Self {
            cwd: cwd.to_string(),
            project_name,
            open_files: Vec::new(),
            git_branch,
            git_status,
            platform: std::env::consts::OS.to_string(),
            current_date: chrono::Local::now().format("%Y-%m-%d").to_string(),
            variables: HashMap::new(),
        }
    }

    /// Add a custom variable
    pub fn with_var(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.variables.insert(key.into(), value.into());
        self
    }

    /// Set open files
    pub fn with_open_files(mut self, files: Vec<String>) -> Self {
        self.open_files = files;
        self
    }

    /// Set git branch
    pub fn with_git_branch(mut self, branch: Option<String>) -> Self {
        self.git_branch = branch;
        self
    }

    /// Set git status
    pub fn with_git_status(mut self, status: Option<String>) -> Self {
        self.git_status = status;
        self
    }
}

// Prompt Layers - Composable Prompt Construction

/// A composable layer in a prompt stack.
///
/// Inspired by goose's layered prompt construction. Each layer adds
/// a section to the final prompt. Layers are rendered in order and
/// combined into a single system prompt.
///
/// # Example
///
/// ```ignore
/// let layers = vec![
///     PromptLayer::text("You are a coding assistant."),
///     PromptLayer::template("system_code_review", &context),
///     PromptLayer::conditional(has_tools, tools_section),
/// ];
/// let prompt = PromptComposer::new(layers).compose(&context);
/// ```
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum PromptLayer {
    /// Static text that's included verbatim
    Static(String),
    /// A Handlebars template rendered with context
    Template { name: String, template: String },
    /// Conditional layer: included only if predicate is true
    Conditional { predicate: bool, content: Box<Self> },
    /// Context-derived section (e.g., tool descriptions from registry)
    ContextSection {
        key: String,
        /// Function to extract and format the section from context
        formatter: fn(&PromptContext) -> Option<String>,
    },
}

impl PromptLayer {
    /// Create a static text layer.
    pub fn text(content: impl Into<String>) -> Self {
        Self::Static(content.into())
    }

    /// Create a template layer from a registered template name.
    pub fn template(template_name: impl Into<String>) -> Self {
        Self::Template {
            name: template_name.into(),
            template: String::new(),
        }
    }

    /// Create a template layer with inline template content.
    pub fn template_inline(template: impl Into<String>) -> Self {
        Self::Template {
            name: String::new(),
            template: template.into(),
        }
    }

    /// Create a conditional layer.
    pub fn conditional(predicate: bool, content: Self) -> Self {
        Self::Conditional {
            predicate,
            content: Box::new(content),
        }
    }

    /// Create a newline separator layer.
    pub fn separator() -> Self {
        Self::Static("\n\n".to_string())
    }
}

/// Composes multiple prompt layers into a single rendered prompt.
///
/// Layers are rendered in order, with conditional layers skipped
/// when their predicate is false. Template layers are rendered
/// using the provided context.
pub struct PromptComposer {
    layers: Vec<PromptLayer>,
}

impl PromptComposer {
    pub const fn new(layers: Vec<PromptLayer>) -> Self {
        Self { layers }
    }

    /// Create an empty composer.
    pub const fn empty() -> Self {
        Self::new(Vec::new())
    }

    /// Add a layer to the stack.
    pub fn layer(mut self, layer: PromptLayer) -> Self {
        self.layers.push(layer);
        self
    }

    /// Add a static text layer.
    pub fn static_text(mut self, text: impl Into<String>) -> Self {
        self.layers.push(PromptLayer::text(text));
        self
    }

    /// Add a separator layer.
    pub fn separator(mut self) -> Self {
        self.layers.push(PromptLayer::separator());
        self
    }

    /// Compose all layers into a single prompt string.
    ///
    /// Renders each layer in order:
    /// - Static: included verbatim
    /// - Template: rendered through Handlebars engine
    /// - Conditional: included only if predicate is true
    /// - `ContextSection`: extracted from context using formatter
    pub fn compose(&self, context: &PromptContext) -> String {
        let engine = PromptTemplateEngine::new();
        let mut sections = Vec::new();

        for layer in &self.layers {
            match layer {
                PromptLayer::Static(text) => {
                    sections.push(text.clone());
                }
                PromptLayer::Template { name, template } => {
                    let rendered = if template.is_empty() {
                        // Named template - look up registered template
                        engine.render(name, context).ok().map(|r| r.content)
                    } else {
                        // Inline template - render directly
                        engine.render_string(template, context).ok()
                    };
                    if let Some(text) = rendered {
                        sections.push(text);
                    }
                }
                PromptLayer::Conditional { predicate, content } => {
                    if *predicate {
                        // Recursively compose the inner content
                        let inner = Self::new(vec![*content.clone()]);
                        let text = inner.compose(context);
                        if !text.is_empty() {
                            sections.push(text);
                        }
                    }
                }
                PromptLayer::ContextSection { formatter, .. } => {
                    if let Some(text) = formatter(context) {
                        sections.push(text);
                    }
                }
            }
        }

        sections.join("\n\n")
    }

    /// Get the number of layers.
    pub const fn layer_count(&self) -> usize {
        self.layers.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_render_string_basic() {
        let engine = PromptTemplateEngine::new();
        let ctx = PromptContext {
            cwd: "/home/user/project".into(),
            project_name: "project".into(),
            open_files: vec!["src/main.rs".into()],
            git_branch: Some("feature".into()),
            git_status: Some("M src/lib.rs".into()),
            platform: "macos".into(),
            current_date: "2026-04-03".into(),
            variables: HashMap::new(),
        };

        let result = engine
            .render_string("Working on {{project_name}} in {{cwd}}", &ctx)
            .unwrap();
        assert_eq!(result, "Working on project in /home/user/project");
    }

    #[test]
    fn test_context_from_environment() {
        // Use "." as cwd so canonicalize succeeds on any machine;
        // project_name and platform are environment-dependent, so only
        // assert properties that hold universally.
        let ctx = PromptContext::from_environment(".");
        assert!(!ctx.cwd.is_empty());
        assert!(!ctx.current_date.is_empty());
        assert!(!ctx.platform.is_empty());
    }

    #[test]
    fn test_custom_variable() {
        let engine = PromptTemplateEngine::new();
        let ctx = PromptContext::from_environment(".").with_var("mode", "refactor");

        let result = engine.render_string("Mode: {{mode}}", &ctx).unwrap();
        assert_eq!(result, "Mode: refactor");
    }

    #[test]
    fn test_conditional_template() {
        let engine = PromptTemplateEngine::new();
        let ctx = PromptContext::from_environment(".")
            .with_var("has_tests", "true")
            .with_open_files(vec!["src/main.rs".into()]);

        let template = r#"{{#if has_tests}}Tests available{{else}}No tests{{/if}}"#;
        let result = engine.render_string(template, &ctx).unwrap();
        assert_eq!(result, "Tests available");
    }

    #[test]
    fn test_git_branch_conditional() {
        let engine = PromptTemplateEngine::new();
        let ctx_with_branch =
            PromptContext::from_environment(".").with_git_branch(Some("main".into()));
        let ctx_no_branch = PromptContext::from_environment(".").with_git_branch(None);

        let template = r#"Branch: {{#if git_branch}}{{git_branch}}{{else}}no-git{{/if}}"#;
        let result_with = engine.render_string(template, &ctx_with_branch).unwrap();
        let result_without = engine.render_string(template, &ctx_no_branch).unwrap();

        assert_eq!(result_with, "Branch: main");
        assert_eq!(result_without, "Branch: no-git");
    }

    #[test]
    fn test_list_iteration() {
        let engine = PromptTemplateEngine::new();
        let ctx = PromptContext::from_environment(".").with_open_files(vec![
            "src/main.rs".into(),
            "src/lib.rs".into(),
            "Cargo.toml".into(),
        ]);

        let template = r#"Files:{{#each open_files}} {{this}}{{/each}}"#;
        let result = engine.render_string(template, &ctx).unwrap();
        assert_eq!(result, "Files: src/main.rs src/lib.rs Cargo.toml");
    }

    // ── Prompt Layer Tests ──────────────────────────────────────────

    #[test]
    fn test_static_layer() {
        let ctx = PromptContext::from_environment(".");
        let composer =
            PromptComposer::new(vec![PromptLayer::text("Hello"), PromptLayer::text("World")]);
        let result = composer.compose(&ctx);
        assert_eq!(result, "Hello\n\nWorld");
    }

    #[test]
    fn test_conditional_layer_included() {
        let ctx = PromptContext::from_environment(".");
        let composer = PromptComposer::new(vec![
            PromptLayer::text("Base"),
            PromptLayer::conditional(true, PromptLayer::text("Extra")),
        ]);
        let result = composer.compose(&ctx);
        assert!(result.contains("Base"));
        assert!(result.contains("Extra"));
    }

    #[test]
    fn test_conditional_layer_excluded() {
        let ctx = PromptContext::from_environment(".");
        let composer = PromptComposer::new(vec![
            PromptLayer::text("Base"),
            PromptLayer::conditional(false, PromptLayer::text("Hidden")),
        ]);
        let result = composer.compose(&ctx);
        assert!(result.contains("Base"));
        assert!(!result.contains("Hidden"));
    }

    #[test]
    fn test_composer_builder_pattern() {
        let ctx = PromptContext::from_environment(".");
        let composer = PromptComposer::empty()
            .static_text("System prompt")
            .layer(PromptLayer::separator())
            .static_text("User context");

        let result = composer.compose(&ctx);
        assert!(result.contains("System prompt"));
        assert!(result.contains("User context"));
    }

    #[test]
    fn test_composer_layer_count() {
        let composer = PromptComposer::empty()
            .static_text("A")
            .static_text("B")
            .static_text("C");
        assert_eq!(composer.layer_count(), 3);
    }

    #[test]
    fn test_template_layer_inline() {
        let ctx = PromptContext::from_environment(".").with_var("name", "World");
        let composer = PromptComposer::new(vec![PromptLayer::template_inline("Hello {{name}}!")]);
        let result = composer.compose(&ctx);
        assert_eq!(result, "Hello World!");
    }

    #[test]
    fn test_nested_conditionals() {
        let ctx = PromptContext::from_environment(".");
        let composer = PromptComposer::new(vec![PromptLayer::conditional(
            true,
            PromptLayer::conditional(true, PromptLayer::text("Deep")),
        )]);
        let result = composer.compose(&ctx);
        assert!(result.contains("Deep"));
    }
}
