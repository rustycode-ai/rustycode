//! Integration tests for prompt template system

use rustycode_tools::{PromptContext, PromptTemplateEngine};

#[test]
fn test_template_engine_creation() {
    let engine = PromptTemplateEngine::new();
    // Should not panic
    let _ = &engine;
}

#[test]
fn test_context_from_environment() {
    let ctx = PromptContext::from_environment(".");
    assert!(
        !ctx.project_name.is_empty(),
        "project_name should be populated"
    );
    assert!(!ctx.current_date.is_empty());
    assert!(!ctx.platform.is_empty());
}

#[test]
fn test_render_system_default() {
    let engine = PromptTemplateEngine::new();
    let ctx = PromptContext::from_environment(".")
        .with_git_branch(Some("main".into()))
        .with_open_files(vec!["src/lib.rs".into()]);

    let result = engine.render("system_default", &ctx);
    assert!(result.is_ok());
    let rendered = result.unwrap();
    assert!(!rendered.content.is_empty());
    assert!(!ctx.project_name.is_empty());
    assert!(rendered.content.contains("main"));
    assert!(rendered.token_estimate > 0);
}

#[test]
fn test_render_system_code_review() {
    let engine = PromptTemplateEngine::new();
    let ctx = PromptContext::from_environment(".").with_git_status(Some("M src/lib.rs".into()));

    let result = engine.render("system_code_review", &ctx);
    assert!(result.is_ok());
    let rendered = result.unwrap();
    assert!(rendered.content.contains("code reviewer"));
    assert!(rendered.content.contains("M src/lib.rs"));
}

#[test]
fn test_render_system_refactor() {
    let engine = PromptTemplateEngine::new();
    let ctx = PromptContext::from_environment(".");

    let result = engine.render("system_refactor", &ctx);
    assert!(result.is_ok());
    let rendered = result.unwrap();
    assert!(rendered.content.contains("refactoring"));
    assert!(rendered.content.contains("Refactoring Principles"));
}

#[test]
fn test_render_user_task() {
    let engine = PromptTemplateEngine::new();
    let mut ctx = PromptContext::from_environment(".");
    ctx = ctx.with_var("task", "Fix the bug in authentication");

    let result = engine.render("user_task", &ctx);
    assert!(result.is_ok());
    let rendered = result.unwrap();
    assert!(rendered.content.contains("Fix the bug in authentication"));
}

#[test]
fn test_custom_template_registration() {
    let mut engine = PromptTemplateEngine::new();
    let template = "Hello {{name}}!";
    let result = engine.register_template("custom", template);
    assert!(result.is_ok());

    let mut ctx = PromptContext::from_environment(".");
    ctx = ctx.with_var("name", "World");

    let rendered = engine.render("custom", &ctx);
    assert!(rendered.is_ok());
    assert_eq!(rendered.unwrap().content, "Hello World!");
}

#[test]
fn test_render_string_with_conditionals() {
    let engine = PromptTemplateEngine::new();
    let ctx = PromptContext::from_environment(".")
        .with_var("show_message", "true")
        .with_var("message", "Hello");

    let template = "{{#if show_message}}{{message}}{{/if}}";
    let result = engine.render_string(template, &ctx);
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), "Hello");
}

#[test]
fn test_render_string_with_each() {
    let engine = PromptTemplateEngine::new();
    let ctx = PromptContext::from_environment(".").with_open_files(vec![
        "file1.rs".into(),
        "file2.rs".into(),
        "file3.rs".into(),
    ]);

    let template = "{{#each open_files}}{{this}} {{/each}}";
    let result = engine.render_string(template, &ctx);
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), "file1.rs file2.rs file3.rs ");
}

#[test]
fn test_render_string_with_nested_context() {
    let engine = PromptTemplateEngine::new();
    let ctx = PromptContext::from_environment(".")
        .with_var("level1", "value1")
        .with_var("level2", "value2");

    let template = "{{level1}}-{{level2}}";
    let result = engine.render_string(template, &ctx);
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), "value1-value2");
}

#[test]
fn test_token_estimate() {
    let engine = PromptTemplateEngine::new();
    let ctx = PromptContext::from_environment(".");

    let template = "a".repeat(100); // 100 characters
    let result = engine.render_string(&template, &ctx).unwrap();
    assert_eq!(result.len(), 100);

    // For a rendered prompt, token_estimate should be roughly len / 4
    let rendered = engine.render("system_default", &ctx).unwrap();
    assert!(rendered.token_estimate > 0);
    assert!(rendered.token_estimate < rendered.content.len());
}

#[test]
fn test_context_builder_pattern() {
    let ctx = PromptContext::from_environment(".")
        .with_var("key1", "value1")
        .with_var("key2", "value2")
        .with_open_files(vec!["test.rs".into()])
        .with_git_branch(Some("feature".into()))
        .with_git_status(Some("M test.rs".into()));

    assert_eq!(ctx.variables.get("key1"), Some(&"value1".to_string()));
    assert_eq!(ctx.variables.get("key2"), Some(&"value2".to_string()));
    assert_eq!(ctx.open_files.len(), 1);
    assert_eq!(ctx.git_branch, Some("feature".to_string()));
    assert_eq!(ctx.git_status, Some("M test.rs".to_string()));
}

#[test]
fn test_invalid_template_name() {
    let engine = PromptTemplateEngine::new();
    let ctx = PromptContext::from_environment(".");

    let result = engine.render("nonexistent_template", &ctx);
    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("nonexistent_template"));
}

#[test]
fn test_template_with_missing_variable() {
    let engine = PromptTemplateEngine::new();
    let ctx = PromptContext::from_environment("."); // No 'missing_var' set

    let template = "Value: {{missing_var}}";
    let result = engine.render_string(template, &ctx);
    // Handlebars should handle missing vars gracefully
    assert!(result.is_ok());
}
