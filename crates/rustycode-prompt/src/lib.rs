#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

//! Prompt template system for LLM interactions.
//!
//! This module provides:
//! - Template rendering using Handlebars engine
//! - Built-in system and user prompt templates
//! - Variable interpolation for context injection
//! - Template discovery and loading from project directories
//! - Layered prompt building with context injection
//! - Environment context gathering

pub mod environment;
pub mod layered;
pub mod phase_prompts;

use handlebars::Handlebars;
use serde_json::Value;
use std::collections::HashMap;
use std::path::Path;
use thiserror::Error;

pub use environment::{EnvironmentContext, GitStatus};
pub use layered::{InstructionScanner, ModelProvider, PromptBuilder, PromptLayer};
pub use phase_prompts::{render_phase_prompt, PhasePromptBuilder};

/// Errors that can occur during template rendering
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum TemplateError {
    #[error("Template not found: {0}")]
    TemplateNotFound(String),

    #[error("Failed to parse template: {0}")]
    ParseError(String),

    #[error("Render error: {0}")]
    RenderError(String),

    #[error("Missing required variable: {0}")]
    MissingVariable(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

/// Result type for template operations
pub type Result<T> = std::result::Result<T, TemplateError>;

/// Template context containing variables for rendering
pub type TemplateContext = HashMap<String, Value>;

/// Prompt template manager
#[derive(Debug, Clone)]
pub struct TemplateManager {
    registry: Handlebars<'static>,
}

impl TemplateManager {
    pub fn new() -> Result<Self> {
        let mut registry = Handlebars::new();
        registry.register_escape_fn(handlebars::no_escape);

        // Add built-in templates
        Self::register_built_in_templates(&mut registry)?;

        Ok(Self { registry })
    }

    /// Create a template manager and load templates from a directory
    pub fn from_dir<P: AsRef<Path>>(dir: P) -> Result<Self> {
        let dir = dir.as_ref();
        let mut registry = Handlebars::new();
        registry.register_escape_fn(handlebars::no_escape);

        // Load .tera/.hbs template files from directory
        if dir.exists() {
            let walk = walkdir(dir)?;
            for entry in walk {
                let path = entry.path();
                let ext = path.extension().and_then(|e| e.to_str());
                if ext == Some("tera") || ext == Some("hbs") {
                    if let Some(name) = path.file_stem().and_then(|s| s.to_str()) {
                        let content = std::fs::read_to_string(&path)?;
                        registry
                            .register_template_string(name, &content)
                            .map_err(|e| TemplateError::ParseError(e.to_string()))?;
                    }
                }
            }
        }

        // Add built-in templates as fallback
        Self::register_built_in_templates(&mut registry)?;

        Ok(Self { registry })
    }

    /// Register built-in system templates
    #[allow(clippy::too_many_lines)]
    fn register_built_in_templates(registry: &mut Handlebars<'static>) -> Result<()> {
        // Default coding assistant system prompt
        registry
            .register_template_string(
                "system/coding_assistant",
                r"
You are {{name}}, an AI programming assistant.

## Core Principles

- **Accuracy**: Provide correct, working code solutions
- **Clarity**: Explain your reasoning step-by-step
- **Safety**: Never introduce security vulnerabilities
- **Best Practices**: Follow language-specific idioms and patterns

## Constraints

- Prefer standard library solutions over external dependencies
- Include error handling for production code
- Add comments for non-obvious logic
- Respect the user's coding style and project conventions

## Task Tracking

When working on multi-step tasks (3+ steps), use the `todo_write` and `todo_update` tools to track progress:
- Create a task list with `todo_write` before starting complex work
- Mark each task `in_progress` BEFORE beginning work on it
- Mark tasks `completed` only when FULLY done — not when partially implemented
- Keep only ONE task `in_progress` at a time
- Work through tasks in ID order when multiple are available

{{#if routing_enabled}}
## Task Routing

- Estimated confidence in the current interpretation: {{routing_confidence}}
- Routing threshold: {{routing_confidence_threshold}}
- Selected workflow: {{routing_workflow}}
- Suggested harness: {{routing_harness}}
- Suggested thinking depth: {{routing_thinking}}
- Suggested team: {{routing_team}}
- Suggested agent: {{routing_agent}}
- Next step: {{routing_next_step}}
- Default action: {{routing_action}}
- Routing summary:
{{routing_summary}}
- Harness summary: {{routing_harness_summary}}
- Thinking summary: {{routing_thinking_summary}}
- Execution plan:
{{routing_execution_plan}}

{{#if interactive_mode}}
- If the default action is `clarify`, ask up to {{routing_max_clarifying_questions}} concise clarifying questions before acting.
- Do not guess about missing requirements when a quick clarification would materially reduce risk.
{{else}}
- If the default action is `research`, do targeted research first: inspect the workspace, search relevant docs/tests, and infer the intended workflow from evidence.
- Repeat research up to {{routing_max_research_passes}} passes if important information is still missing.
- If uncertainty remains, report the missing information and choose the safest fallback workflow instead of guessing.
{{/if}}
{{/if}}

{{#if intent_suffix}}
## Intent Guidance

{{intent_suffix}}
{{/if}}

{{#if context}}
## Context

{{context}}
{{/if}}

You will help with code analysis, implementation, debugging, and refactoring tasks.
",
            )
            .map_err(|e| TemplateError::ParseError(e.to_string()))?;

        // Code review system prompt
        registry
            .register_template_string(
                "system/code_review",
                r"
You are a code reviewer focused on:

1. **Correctness**: Logic bugs, edge cases, error handling
2. **Security**: OWASP Top 10, input validation, authentication/authorization
3. **Performance**: Algorithmic complexity, resource usage, caching opportunities
4. **Maintainability**: Code organization, naming, documentation, testing
5. **Best Practices**: Language idioms, design patterns, SOLID principles

{{#if language}}
Focus on {{language}}-specific patterns and idioms.
{{/if}}

Provide constructive feedback with specific examples and suggested improvements.
",
            )
            .map_err(|e| TemplateError::ParseError(e.to_string()))?;

        // Debug assistant system prompt
        registry
            .register_template_string(
                "system/debug",
                r"
You are a debugging assistant. Follow the scientific method:

1. **Observe**: What are the symptoms? What is the expected vs actual behavior?
2. **Hypothesize**: What could be causing this issue?
3. **Test**: How can we verify the hypothesis?
4. **Analyze**: What do the test results tell us?
5. **Solve**: Implement the fix and verify it works

## Debugging Strategy

- Start with the most likely causes
- Use binary search (narrow down the problem space)
- Check recent changes
- Look for common patterns (off-by-one, null handling, race conditions)

{{#if error}}
Current error: {{error}}
{{/if}}
",
            )
            .map_err(|e| TemplateError::ParseError(e.to_string()))?;

        // Headless coding agent system prompt
        registry
            .register_template_string(
                "system/headless_coding_agent",
                r"
{{coding_assistant_base}}

## HEADLESS MODE OPERATING PRINCIPLES
1. **Completion-First**: Iterate autonomously until the task is fully verified. Do not pause for guidance.
2. **Action-Oriented**: Prefer tool execution over text analysis.
3. **Verification**: Verify all changes by running tests or scripts.
4. **No Documentation**: Do not write READMEs, notes, or examples unless critical to the task.

## ROUTING DISCIPLINE
When the routing confidence is low, do not guess. Use the configured research budget to gather more evidence, then either:
- continue with the safest workflow you can justify, or
- use up to {{routing_max_research_passes}} research passes before stopping,
- emit a concise handoff using the structured payload below.

Suggested harness: {{routing_harness}}
Harness summary: {{routing_harness_summary}}
Suggested thinking depth: {{routing_thinking}}
Thinking summary: {{routing_thinking_summary}}
Next step: {{routing_next_step}}
Execution plan:
{{routing_execution_plan}}

Handoff format:
[RUSTYTOOL_HANDOFF]
{{routing_handoff_payload}}
[/RUSTYTOOL_HANDOFF]

{{#if intent_suffix}}
## Intent Guidance

{{intent_suffix}}
{{/if}}

## VERIFICATION GATES
Before finalizing, ensure:
- Changes are tested and verified
- All build artifacts (if applicable) are created and installed
- Task is provably complete
",
            )
            .map_err(|e| TemplateError::ParseError(e.to_string()))?;

        // User prompt for code explanation
        registry
            .register_template_string(
                "user/explain_code",
                r"
Please explain the following code:

```{{language}}
{{code}}
```

{{#if focus}}
Focus your explanation on: {{focus}}
{{/if}}
",
            )
            .map_err(|e| TemplateError::ParseError(e.to_string()))?;

        // User prompt for code generation
        registry
            .register_template_string(
                "user/generate",
                r"
{{task}}

{{#if requirements}}
## Requirements

{{requirements}}
{{/if}}

{{#if constraints}}
## Constraints

{{constraints}}
{{/if}}

{{#if examples}}
## Examples

{{examples}}
{{/if}}

Please implement this following best practices for {{language}}.
",
            )
            .map_err(|e| TemplateError::ParseError(e.to_string()))?;

        // User prompt for refactoring
        registry
            .register_template_string(
                "user/refactor",
                r"
Please refactor the following {{language}} to improve it.

```{{language}}
{{code}}
```

## Goals

{{goals}}

{{#if constraints}}
## Constraints

{{constraints}}
{{/if}}

Provide the refactored code and explain your changes.
",
            )
            .map_err(|e| TemplateError::ParseError(e.to_string()))?;

        Ok(())
    }

    /// Render a template by name with the given context
    pub fn render(&self, name: &str, context: &TemplateContext) -> Result<String> {
        self.registry
            .render(name, context)
            .map_err(|e| TemplateError::RenderError(e.to_string()))
    }

    /// Render a template string (inline template)
    pub fn render_inline(&mut self, template: &str, context: &TemplateContext) -> Result<String> {
        // Create a unique template name for inline rendering
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        template.hash(&mut hasher);
        let template_name = format!("inline_{:x}", hasher.finish());

        // Only register if not already present
        if !self.registry.has_template(&template_name) {
            // Prevent unbounded growth: if too many inline templates, use a
            // temporary registry for one-shot rendering instead.
            const MAX_INLINE_TEMPLATES: usize = 256;
            let inline_count = self
                .registry
                .get_templates()
                .keys()
                .filter(|k| k.starts_with("inline_"))
                .count();

            if inline_count >= MAX_INLINE_TEMPLATES {
                // One-shot render without polluting the main registry
                let mut temp = Handlebars::new();
                temp.register_escape_fn(handlebars::no_escape);
                temp.register_template_string(&template_name, template)
                    .map_err(|e| TemplateError::ParseError(e.to_string()))?;
                return temp
                    .render(&template_name, context)
                    .map_err(|e| TemplateError::RenderError(e.to_string()));
            }

            self.registry
                .register_template_string(&template_name, template)
                .map_err(|e| TemplateError::ParseError(e.to_string()))?;
        }

        self.render(&template_name, context)
    }

    /// Get the coding assistant system prompt
    pub fn coding_assistant_prompt(&self, context: &TemplateContext) -> Result<String> {
        self.render("system/coding_assistant", context)
    }

    /// Get the code review system prompt
    pub fn code_review_prompt(&self, context: &TemplateContext) -> Result<String> {
        self.render("system/code_review", context)
    }

    /// Get the debug system prompt
    pub fn debug_prompt(&self, context: &TemplateContext) -> Result<String> {
        self.render("system/debug", context)
    }

    /// Check if a template exists
    pub fn has_template(&self, name: &str) -> bool {
        self.registry.has_template(name)
    }

    /// List all available template names
    pub fn list_templates(&self) -> Vec<String> {
        self.registry.get_templates().keys().cloned().collect()
    }
}

impl Default for TemplateManager {
    #[allow(clippy::expect_used)]
    fn default() -> Self {
        Self::new().expect("Failed to create default template manager")
    }
}

/// Helper to create a template context with default values for handlebars compatibility.
/// Missing variables in handlebars render as empty string by default, so we inject
/// defaults for known template variables.
#[cfg(test)]
fn context_with_defaults(ctx: &TemplateContext) -> TemplateContext {
    let mut result = ctx.clone();
    let defaults = [
        ("name", serde_json::json!("Claude")),
        ("language", serde_json::json!("text")),
        (
            "goals",
            serde_json::json!("Improve readability, maintainability, and performance"),
        ),
    ];
    for (key, default_val) in &defaults {
        if !result.contains_key(*key) {
            result.insert(key.to_string(), default_val.clone());
        }
    }
    result
}

/// Built-in template names
pub mod templates {
    pub const CODING_ASSISTANT: &str = "system/coding_assistant";
    pub const CODE_REVIEW: &str = "system/code_review";
    pub const DEBUG: &str = "system/debug";
    pub const EXPLAIN_CODE: &str = "user/explain_code";
    pub const GENERATE: &str = "user/generate";
    pub const REFACTOR: &str = "user/refactor";
}

/// Helper to create a template context from key-value pairs
#[macro_export]
macro_rules! context {
    ($($key:expr => $value:expr),* $(,)?) => {
        {
            #[allow(unused_mut)]
            let mut ctx = std::collections::HashMap::new();
            $(
                ctx.insert($key.to_string(), serde_json::to_value(&$value).unwrap_or_else(|e| {
                    let _ = e; // serialization failed, use null
                    serde_json::json!(null)
                }));
            )*
            ctx
        }
    };
}

/// Walk a directory for template files, returning entries
fn walkdir(dir: &Path) -> Result<Vec<std::fs::DirEntry>> {
    let mut entries = Vec::new();
    let walk = std::fs::read_dir(dir).map_err(TemplateError::Io)?;
    for entry in walk {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            let sub = walkdir(&path)?;
            entries.extend(sub);
        } else {
            entries.push(entry);
        }
    }
    Ok(entries)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_template_manager_creation() {
        let manager = TemplateManager::new();
        assert!(manager.is_ok());
    }

    #[test]
    fn test_render_coding_assistant() {
        let manager = TemplateManager::new().unwrap();
        let mut context = context! {
            "name" => "TestBot",
            "context" => "Working on a Rust project"
        };
        context = context_with_defaults(&context);
        let result = manager.coding_assistant_prompt(&context);
        assert!(result.is_ok());
        let output = result.unwrap();
        assert!(output.contains("TestBot"));
        assert!(output.contains("Working on a Rust project"));
    }

    #[test]
    fn test_render_with_missing_variable() {
        let manager = TemplateManager::new().unwrap();
        let context = context_with_defaults(&TemplateContext::new());
        let result = manager.render("system/coding_assistant", &context);
        assert!(result.is_ok()); // handlebars renders missing vars as empty
    }

    #[test]
    fn test_list_templates() {
        let manager = TemplateManager::new().unwrap();
        let templates = manager.list_templates();
        assert!(!templates.is_empty());
        assert!(templates.contains(&"system/coding_assistant".to_string()));
    }

    #[test]
    fn test_context_macro() {
        let ctx = context! {
            "foo" => "bar",
            "number" => 42
        };
        assert_eq!(ctx.get("foo").unwrap().as_str().unwrap(), "bar");
        assert_eq!(ctx.get("number").unwrap().as_i64().unwrap(), 42);
    }

    #[test]
    fn test_explain_code_template() {
        let manager = TemplateManager::new().unwrap();
        let context = context! {
            "code" => "fn main() { println!(\"Hello\"); }",
            "language" => "rust",
            "focus" => "the println macro"
        };
        let result = manager.render("user/explain_code", &context);
        assert!(result.is_ok());
        let output = result.unwrap();
        assert!(output.contains("fn main()"));
        assert!(output.contains("println macro"));
    }

    #[test]
    fn test_inline_template() {
        let mut manager = TemplateManager::new().unwrap();
        let template = "Hello {{name}}!";
        let context = context! { "name" => "World" };
        let result = manager.render_inline(template, &context);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "Hello World!");
    }

    #[test]
    fn test_has_template() {
        let manager = TemplateManager::new().unwrap();
        assert!(manager.has_template("system/coding_assistant"));
        assert!(manager.has_template("system/code_review"));
        assert!(manager.has_template("system/debug"));
        assert!(manager.has_template("user/explain_code"));
        assert!(manager.has_template("user/generate"));
        assert!(manager.has_template("user/refactor"));
        assert!(!manager.has_template("nonexistent/template"));
    }

    #[test]
    fn test_code_review_prompt() {
        let manager = TemplateManager::new().unwrap();
        let context = context! { "language" => "Rust" };
        let result = manager.code_review_prompt(&context);
        assert!(result.is_ok());
        assert!(result.unwrap().contains("Rust"));
    }

    #[test]
    fn test_debug_prompt() {
        let manager = TemplateManager::new().unwrap();
        let context = context! { "error" => "panic at index out of bounds" };
        let result = manager.debug_prompt(&context);
        assert!(result.is_ok());
        let output = result.unwrap();
        assert!(output.contains("panic at index out of bounds"));
        assert!(output.contains("scientific method"));
    }

    #[test]
    fn test_default_impl() {
        let manager = TemplateManager::default();
        assert!(manager.has_template("system/coding_assistant"));
    }

    #[test]
    fn test_template_constants() {
        assert_eq!(templates::CODING_ASSISTANT, "system/coding_assistant");
        assert_eq!(templates::CODE_REVIEW, "system/code_review");
        assert_eq!(templates::DEBUG, "system/debug");
        assert_eq!(templates::EXPLAIN_CODE, "user/explain_code");
        assert_eq!(templates::GENERATE, "user/generate");
        assert_eq!(templates::REFACTOR, "user/refactor");
    }

    #[test]
    fn test_render_nonexistent_template() {
        let manager = TemplateManager::new().unwrap();
        let context = TemplateContext::new();
        let result = manager.render("nonexistent", &context);
        assert!(result.is_err());
    }

    #[test]
    fn test_inline_template_caching() {
        let mut manager = TemplateManager::new().unwrap();
        let template = "Cached: {{x}}";
        let ctx1 = context! { "x" => 1 };
        let ctx2 = context! { "x" => 2 };
        let r1 = manager.render_inline(template, &ctx1).unwrap();
        let r2 = manager.render_inline(template, &ctx2).unwrap();
        assert_eq!(r1, "Cached: 1");
        assert_eq!(r2, "Cached: 2");
    }

    #[test]
    fn test_generate_prompt() {
        let manager = TemplateManager::new().unwrap();
        let context = context! {
            "task" => "Build a REST API",
            "language" => "Rust",
            "requirements" => "Must handle errors gracefully"
        };
        let result = manager.render("user/generate", &context);
        assert!(result.is_ok());
        let output = result.unwrap();
        assert!(output.contains("Build a REST API"));
        assert!(output.contains("Rust"));
        assert!(output.contains("Must handle errors gracefully"));
    }

    // --- Error display tests ---

    #[test]
    fn test_template_error_display_not_found() {
        let err = TemplateError::TemplateNotFound("my_template".to_string());
        let msg = format!("{err}");
        assert!(msg.contains("Template not found"), "got: {msg}");
        assert!(msg.contains("my_template"), "got: {msg}");
    }

    #[test]
    fn test_template_error_display_parse_error() {
        let err = TemplateError::ParseError("bad syntax at line 3".to_string());
        let msg = format!("{err}");
        assert!(msg.contains("Failed to parse template"), "got: {msg}");
        assert!(msg.contains("bad syntax at line 3"), "got: {msg}");
    }

    #[test]
    fn test_template_error_display_render_error() {
        let err = TemplateError::RenderError("variable overflow".to_string());
        let msg = format!("{err}");
        assert!(msg.contains("Render error"), "got: {msg}");
        assert!(msg.contains("variable overflow"), "got: {msg}");
    }

    #[test]
    fn test_template_error_display_missing_variable() {
        let err = TemplateError::MissingVariable("username".to_string());
        let msg = format!("{err}");
        assert!(msg.contains("Missing required variable"), "got: {msg}");
        assert!(msg.contains("username"), "got: {msg}");
    }

    #[test]
    fn test_template_error_display_io() {
        let io_err = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "access denied");
        let err = TemplateError::Io(io_err);
        let msg = format!("{err}");
        assert!(msg.contains("IO error"), "got: {msg}");
        assert!(msg.contains("access denied"), "got: {msg}");
    }

    #[test]
    fn test_template_error_from_io() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file gone");
        let err: TemplateError = io_err.into();
        assert!(matches!(err, TemplateError::Io(_)));
    }

    #[test]
    fn test_render_nonexistent_yields_error() {
        let manager = TemplateManager::new().unwrap();
        let result = manager.render("does/not/exist", &TemplateContext::new());
        let err = result.unwrap_err();
        let msg = format!("{err}");
        assert!(!msg.is_empty());
    }

    #[test]
    fn test_inline_template_bad_syntax() {
        let mut manager = TemplateManager::new().unwrap();
        let bad_template = "Hello {{#if unclosed";
        let result = manager.render_inline(bad_template, &TemplateContext::new());
        assert!(result.is_err());
    }

    #[test]
    fn test_inline_template_empty_string() {
        let mut manager = TemplateManager::new().unwrap();
        let result = manager.render_inline("", &TemplateContext::new());
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "");
    }

    #[test]
    fn test_inline_template_no_variables() {
        let mut manager = TemplateManager::new().unwrap();
        let result = manager.render_inline("static content here", &TemplateContext::new());
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "static content here");
    }

    #[test]
    fn test_context_macro_empty() {
        let ctx: TemplateContext = context! {};
        assert!(ctx.is_empty());
    }

    #[test]
    fn test_context_macro_trailing_comma() {
        let ctx = context! {
            "a" => 1,
            "b" => 2,
        };
        assert_eq!(ctx.len(), 2);
    }

    #[test]
    fn test_context_macro_bool_value() {
        let ctx = context! { "flag" => true };
        assert_eq!(ctx.get("flag").unwrap().as_bool(), Some(true));
    }

    #[test]
    fn test_context_macro_null_value() {
        let ctx = context! { "nothing" => serde_json::Value::Null };
        assert!(ctx.get("nothing").unwrap().is_null());
    }

    #[test]
    fn test_context_macro_nested_object() {
        let nested = serde_json::json!({"inner": "value", "count": 5});
        let ctx = context! { "data" => nested };
        let data = ctx.get("data").unwrap();
        assert_eq!(data["inner"].as_str(), Some("value"));
        assert_eq!(data["count"].as_i64(), Some(5));
    }

    #[test]
    fn test_render_explain_code_without_focus() {
        let manager = TemplateManager::new().unwrap();
        let context = context! {
            "code" => "let x = 1;",
            "language" => "rust"
        };
        let result = manager.render("user/explain_code", &context).unwrap();
        assert!(result.contains("let x = 1;"));
        assert!(result.contains("rust"));
        // The focus section should not appear since variable is not provided
        assert!(!result.contains("Focus your explanation on"));
    }

    #[test]
    fn test_render_explain_code_default_language() {
        let manager = TemplateManager::new().unwrap();
        let context = context! { "code" => "print('hi')" };
        let _result = manager.render("user/explain_code", &context).unwrap();
        // language is empty string when not provided (handlebars default)
    }

    #[test]
    fn test_render_generate_with_constraints_and_examples() {
        let manager = TemplateManager::new().unwrap();
        let context = context! {
            "task" => "Parse CSV files",
            "language" => "Python",
            "constraints" => "No external deps",
            "examples" => "Input: a,b,c"
        };
        let result = manager.render("user/generate", &context).unwrap();
        assert!(result.contains("Parse CSV files"));
        assert!(result.contains("No external deps"));
        assert!(result.contains("Input: a,b,c"));
    }

    #[test]
    fn test_render_generate_minimal() {
        let manager = TemplateManager::new().unwrap();
        let context = context! { "task" => "Hello world" };
        let result = manager.render("user/generate", &context).unwrap();
        assert!(result.contains("Hello world"));
    }

    #[test]
    fn test_render_refactor_template() {
        let manager = TemplateManager::new().unwrap();
        let context = context! {
            "code" => "fn old() {}",
            "language" => "rust",
            "goals" => "Improve naming"
        };
        let result = manager.render("user/refactor", &context).unwrap();
        assert!(result.contains("fn old()"));
        assert!(result.contains("Improve naming"));
    }

    #[test]
    fn test_render_refactor_default_goals() {
        let manager = TemplateManager::new().unwrap();
        let context = context_with_defaults(&context! {
            "code" => "x = 1",
        });
        let result = manager.render("user/refactor", &context).unwrap();
        // Default goals should mention readability, maintainability, and performance
        assert!(result.contains("readability"));
    }

    #[test]
    fn test_debug_prompt_without_error() {
        let manager = TemplateManager::new().unwrap();
        let context = TemplateContext::new();
        let result = manager.debug_prompt(&context).unwrap();
        assert!(result.contains("scientific method"));
        // The error section should not appear
        assert!(!result.contains("Current error:"));
    }

    #[test]
    fn test_coding_assistant_prompt_default_name() {
        let manager = TemplateManager::new().unwrap();
        let context = context_with_defaults(&TemplateContext::new());
        let result = manager.coding_assistant_prompt(&context).unwrap();
        // Default name is "Claude"
        assert!(result.contains("Claude"));
    }

    #[test]
    fn test_coding_assistant_contains_task_tracking() {
        let manager = TemplateManager::new().unwrap();
        let context = context_with_defaults(&TemplateContext::new());
        let result = manager.coding_assistant_prompt(&context).unwrap();
        assert!(
            result.contains("Task Tracking"),
            "System prompt should contain Task Tracking section"
        );
        assert!(
            result.contains("todo_write"),
            "System prompt should mention todo_write tool"
        );
        assert!(
            result.contains("todo_update"),
            "System prompt should mention todo_update tool"
        );
    }

    #[test]
    fn test_code_review_prompt_without_language() {
        let manager = TemplateManager::new().unwrap();
        let context = TemplateContext::new();
        let result = manager.code_review_prompt(&context).unwrap();
        // Should have review criteria but no language-specific section
        assert!(result.contains("Correctness"));
        assert!(result.contains("Security"));
        assert!(!result.contains("Focus on"));
    }

    #[test]
    fn test_list_templates_contains_all_builtins() {
        let manager = TemplateManager::new().unwrap();
        let names = manager.list_templates();
        let expected = [
            templates::CODING_ASSISTANT,
            templates::CODE_REVIEW,
            templates::DEBUG,
            templates::EXPLAIN_CODE,
            templates::GENERATE,
            templates::REFACTOR,
        ];
        for name in &expected {
            assert!(
                names.contains(&name.to_string()),
                "missing template: {name}"
            );
        }
    }

    #[test]
    fn test_template_manager_clone() {
        let manager = TemplateManager::new().unwrap();
        let cloned = manager.clone();
        assert!(cloned.has_template("system/coding_assistant"));
        assert_eq!(
            cloned.list_templates().len(),
            manager.list_templates().len()
        );
    }

    #[test]
    fn test_template_manager_debug() {
        let manager = TemplateManager::new().unwrap();
        let debug_str = format!("{manager:?}");
        assert!(debug_str.contains("TemplateManager"));
    }

    #[test]
    fn test_from_dir_nonexistent_path() {
        let result = TemplateManager::from_dir("/no/such/directory/ever");
        assert!(result.is_ok() || result.is_err());
    }

    #[test]
    fn test_render_with_empty_context_values() {
        let manager = TemplateManager::new().unwrap();
        let context = context! {
            "name" => "",
            "context" => ""
        };
        let result = manager.coding_assistant_prompt(&context).unwrap();
        // Empty strings should not cause errors, just produce empty content
        assert!(!result.is_empty());
    }

    #[test]
    fn test_render_with_special_characters() {
        let mut manager = TemplateManager::new().unwrap();
        let context = context! { "name" => "<script>alert('xss')</script>" };
        let result = manager.render_inline("Hello {{name}}!", &context).unwrap();
        assert!(result.contains("<script>"));
    }

    #[test]
    fn test_render_with_unicode() {
        let mut manager = TemplateManager::new().unwrap();
        let context = context! { "name" => "日本語テスト" };
        let result = manager.render_inline("Hello {{name}}!", &context).unwrap();
        assert!(result.contains("日本語テスト"));
    }

    #[test]
    fn test_inline_template_idempotent() {
        let mut manager = TemplateManager::new().unwrap();
        let template = "Value: {{x}}";
        let ctx = context! { "x" => 42 };
        let r1 = manager.render_inline(template, &ctx).unwrap();
        let r2 = manager.render_inline(template, &ctx).unwrap();
        assert_eq!(r1, r2);
    }

    #[test]
    fn test_inline_template_different_templates_same_context() {
        let mut manager = TemplateManager::new().unwrap();
        let ctx = context! { "val" => "test" };
        let r1 = manager.render_inline("A: {{val}}", &ctx).unwrap();
        let r2 = manager.render_inline("B: {{val}}", &ctx).unwrap();
        assert_eq!(r1, "A: test");
        assert_eq!(r2, "B: test");
    }

    #[test]
    fn test_inline_template_cap_falls_back_to_temp_registry() {
        let mut manager = TemplateManager::new().unwrap();
        let ctx = context! { "val" => "test" };

        // Register more than 256 unique inline templates
        for i in 0..260 {
            let template = format!("template_{i}: {{{{val}}}}");
            let result = manager.render_inline(&template, &ctx);
            assert!(
                result.is_ok(),
                "render_inline should succeed for template {i}"
            );
            assert_eq!(result.unwrap(), format!("template_{i}: test"));
        }
    }
}
