//! Example: Integrating prompt templates with existing rustycode-core patterns
//!
//! This demonstrates how to use the prompt template system to enhance
//! the existing system prompt building in rustycode-core/src/lib.rs

use rustycode_protocol::WorkingMode;
use rustycode_tools::{PromptContext, PromptTemplateEngine};

/// Build a system prompt using templates instead of raw string formatting
///
/// This replaces the pattern in rustycode-core/src/lib.rs around line 1270:
/// ```ignore
/// let system_prompt = format!(
///     "{}\n\nAvailable tools:\n{}\n\n## Intent Awareness\n{}",
///     mode_system_prompt,
///     tool_list.iter()
///         .map(|t| format!("- {}: {}", t.name, t.description))
///         .collect::<Vec<_>>()
///         .join("\n"),
///     intent_suffix
/// );
/// ```
pub fn build_system_prompt_with_template(
    mode: WorkingMode,
    tool_list: &[rustycode_tools::ToolInfo],
    cwd: &str,
    open_files: Vec<String>,
    git_branch: Option<String>,
) -> String {
    // Create template engine
    let engine = PromptTemplateEngine::new();

    // Build context from environment and additional data
    let mut ctx = PromptContext::from_environment(cwd);
    ctx = ctx
        .with_open_files(open_files)
        .with_git_branch(git_branch)
        .with_var("mode", mode.to_string())
        .with_var("temperature", mode.temperature().to_string());

    // Get the mode-specific base prompt
    let mode_prompt = mode.system_prompt();

    // Create a template that combines mode prompt with tools
    let template = format!(
        r#"{}{{#if git_branch}}

Current branch: {{git_branch}}{{/if}}{{#if open_files}}

Active files:
{{#each open_files}}  - {{this}}
{{/each}}{{/if}}

## Available Tools

{{#each tools}}- {{name}}: {{description}}
{{/each}}

## Platform & Environment
Project: {{project_name}}
Working directory: {{cwd}}
Platform: {{platform}}
Date: {{current_date}}
Mode: {{mode}} (temperature: {{temperature}})"#,
        mode_prompt
    );

    // Build tools data for template
    #[derive(serde::Serialize)]
    struct ToolRef {
        name: String,
        description: String,
    }

    let tools_data: Vec<ToolRef> = tool_list
        .iter()
        .map(|t| ToolRef {
            name: t.name.clone(),
            description: t.description.clone(),
        })
        .collect();

    // Add tools to context
    ctx.variables.insert(
        "tools".to_string(),
        serde_json::to_string(&tools_data).unwrap_or_default(),
    );

    // Render the template
    match engine.render_string(&template, &ctx) {
        Ok(rendered) => rendered,
        Err(e) => {
            // Fallback to original pattern if template fails
            eprintln!("Template rendering failed: {}", e);
            format!(
                "{}\n\nAvailable tools:\n{}",
                mode_prompt,
                tool_list
                    .iter()
                    .map(|t| format!("- {}: {}", t.name, t.description))
                    .collect::<Vec<_>>()
                    .join("\n")
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustycode_tools::ToolInfo;
    use serde_json::Value;

    #[test]
    fn test_build_system_prompt_code_mode() {
        let tools = vec![ToolInfo {
            name: "read_file".to_string(),
            description: "Read file contents".to_string(),
            parameters_schema: Value::Object(Default::default()),
            permission: rustycode_tools::ToolPermission::Read,
            defer_loading: None,
        }];

        let prompt = build_system_prompt_with_template(
            WorkingMode::Code,
            &tools,
            "/test/project",
            vec!["src/main.rs".to_string()],
            Some("main".to_string()),
        );

        assert!(prompt.contains("coding assistant"));
        assert!(prompt.contains("read_file"));
        assert!(prompt.contains("src/main.rs"));
        assert!(prompt.contains("main"));
        assert!(prompt.contains("Code"));
    }

    #[test]
    fn test_build_system_prompt_debug_mode() {
        let tools = vec![];

        let prompt = build_system_prompt_with_template(
            WorkingMode::Debug,
            &tools,
            "/test/project",
            vec![],
            None,
        );

        assert!(prompt.contains("debugger"));
        assert!(prompt.contains("Debug"));
    }

    #[test]
    fn test_build_system_prompt_with_open_files() {
        let tools = vec![];

        let prompt = build_system_prompt_with_template(
            WorkingMode::Ask,
            &tools,
            "/test/project",
            vec!["file1.rs".to_string(), "file2.rs".to_string()],
            None,
        );

        assert!(prompt.contains("file1.rs"));
        assert!(prompt.contains("file2.rs"));
        assert!(prompt.contains("Active files"));
    }
}

fn main() {
    // Example usage
    let tools = vec![
        rustycode_tools::ToolInfo {
            name: "read_file".to_string(),
            description: "Read file contents".to_string(),
            parameters_schema: serde_json::json!({}),
            permission: rustycode_tools::ToolPermission::Read,
            defer_loading: None,
        },
        rustycode_tools::ToolInfo {
            name: "write_file".to_string(),
            description: "Write file contents".to_string(),
            parameters_schema: serde_json::json!({}),
            permission: rustycode_tools::ToolPermission::Write,
            defer_loading: None,
        },
    ];

    let prompt = build_system_prompt_with_template(
        WorkingMode::Code,
        &tools,
        "/Users/nat/dev/rustycode",
        vec!["src/main.rs".to_string(), "Cargo.toml".to_string()],
        Some("feature-branch".to_string()),
    );

    println!("=== Generated System Prompt ===\n");
    println!("{}", prompt);
}
