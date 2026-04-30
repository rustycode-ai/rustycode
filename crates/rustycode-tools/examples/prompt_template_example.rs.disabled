//! Example demonstrating the prompt template system
//!
//! Run with: cargo run --example prompt_template_example

use rustycode_tools::{PromptContext, PromptTemplateEngine};

fn main() -> anyhow::Result<()> {
    // Create a template engine
    let engine = PromptTemplateEngine::new();

    // Build context from current directory
    let mut ctx = PromptContext::from_environment(".");

    // Add custom variables
    ctx = ctx
        .with_var("mode", "code")
        .with_var("focus", "testing")
        .with_open_files(vec!["src/main.rs".to_string(), "src/lib.rs".to_string()]);

    // Render the default system prompt
    let rendered = engine.render("system_default", &ctx)?;
    println!("=== System Default Template ===\n");
    println!("{}", rendered.content);
    println!("\nEstimated tokens: {}\n", rendered.token_estimate);

    // Render code review template
    let rendered_review = engine.render("system_code_review", &ctx)?;
    println!("=== Code Review Template ===\n");
    println!("{}", rendered_review.content);
    println!("\nEstimated tokens: {}\n", rendered_review.token_estimate);

    // Render a custom template string
    let custom_template = r#"
Project: {{project_name}}
Mode: {{mode}}
Focus: {{focus}}
Files:
{{#each open_files}}  - {{this}}
{{/each}}
    "#;

    let rendered_custom = engine.render_string(custom_template, &ctx)?;
    println!("=== Custom Template ===\n");
    println!("{}", rendered_custom);

    Ok(())
}
