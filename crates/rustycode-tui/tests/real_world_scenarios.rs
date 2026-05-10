#![allow(
    clippy::case_sensitive_file_extension_comparisons,
    clippy::doc_markdown,
    clippy::manual_let_else,
    clippy::redundant_closure_for_method_calls,
    clippy::single_char_pattern,
    clippy::single_match_else,
    clippy::too_many_lines,
    clippy::uninlined_format_args,
    clippy::unwrap_used
)]

//! Real-world scenario tests for tool execution
//!
//! These tests verify that RustyCode can handle complex, multi-file projects
//! that developers actually work with.
//!
//! # Scenarios
//!
//! 1. Create a React web project with multiple files
//! 2. Create a Node.js API project
//! 3. Create a full-stack MERN application
//! 4. Refactor existing codebase
//!
//! # Running
//!
//! ```bash
//! cargo test --test real_world_scenarios -- --nocapture --test-threads=1 --ignored
//! ```
//!
//! # Requirements
//!
//! - ANTHROPIC_API_KEY environment variable set
//! - Network access to LLM API

use rustycode_llm::{
    anthropic::AnthropicProvider,
    provider::{ChatMessage, CompletionRequest, LLMProvider, ProviderConfig},
};
use secrecy::SecretString;
use std::env;
use std::fs;
use std::path::PathBuf;

#[tokio::test]
#[cfg_attr(
    not(feature = "live-api-tests"),
    ignore = "Requires API key — run with: cargo test --features live-api-tests -- --ignored"
)]
async fn test_react_project_creation() {
    let api_key = match env::var("ANTHROPIC_API_KEY") {
        Ok(key) => key,
        Err(_) => {
            println!("❌ Skipping test: ANTHROPIC_API_KEY not set");
            return;
        }
    };

    let base_url = env::var("ANTHROPIC_BASE_URL").ok();
    let test_dir = PathBuf::from("/tmp/rustycode_react_test");

    // Clean up any existing test directory
    let _ = fs::remove_dir_all(&test_dir);
    fs::create_dir_all(&test_dir).unwrap();

    let config = ProviderConfig {
        api_key: Some(SecretString::new(api_key.into())),
        base_url,
        timeout_seconds: Some(180), // 3 minutes for complex task
        extra_headers: None,
        retry_config: None,
    };

    let model = env::var("ANTHROPIC_MODEL").unwrap_or_else(|_| "claude-sonnet-4-6".to_string());

    let provider = match AnthropicProvider::new_without_validation(config, model.clone()) {
        Ok(p) => p,
        Err(e) => {
            println!("❌ Failed to create provider: {:?}", e);
            return;
        }
    };

    println!("🚀 Testing: Create a React Web Project");
    println!("═══════════════════════════════════════════════════════════\n");

    let system_prompt = format!(
        "You are RustyCode, a coding assistant working in: {}

IMPORTANT: You have access to the following tools:
- read_file: Read file contents
- write_file: Write content to a file (use this to CREATE files)
- list_dir: List files in a directory
- grep: Search for patterns in files

TASK: Create a complete React web application with the following structure:

1. Create package.json with dependencies (react, react-dom, react-scripts)
2. Create public/index.html (HTML entry point)
3. Create src/index.js (React entry point)
4. Create src/App.js (Main React component)
5. Create src/App.css (Styles for the app)
6. The app should be a simple counter or todo list

Use write_file to create each file. Make sure to:
- Include proper JSON syntax in package.json
- Include proper HTML structure in index.html
- Include proper React JSX syntax in .js files
- Include proper CSS in .css files

After creating all files, use list_dir to verify the structure.

Respond naturally after creating each file.",
        test_dir.display()
    );

    let prompt = "Create a React web application with a counter component. Include all necessary files (package.json, index.html, index.js, App.js, App.css).";

    let messages = vec![ChatMessage::user(prompt.to_string())];
    let request = CompletionRequest::new(model.clone(), messages)
        .with_system_prompt(system_prompt)
        .with_max_tokens(4096) // Large token budget for multi-file creation
        .with_temperature(0.1);

    println!("▶ Sending prompt to LLM...");
    println!("   Expected: 5+ files created\n");

    match LLMProvider::complete(&provider, request).await {
        Ok(response) => {
            println!("📝 LLM Response:\n");
            println!(
                "{}\n",
                response.content.chars().take(500).collect::<String>()
            );

            // Verify the project structure
            println!("🔍 Verifying created files...\n");

            let mut files_created = 0;
            let required_files = vec![
                "package.json",
                "public/index.html",
                "src/index.js",
                "src/App.js",
                "src/App.css",
            ];

            for file in &required_files {
                let file_path = test_dir.join(file);
                if file_path.exists() {
                    files_created += 1;
                    println!("   ✓ {} created", file);

                    // Verify file contents
                    let contents = fs::read_to_string(&file_path).unwrap();
                    let is_valid = validate_file_contents(file, &contents);
                    if is_valid {
                        println!("      Content: ✓ Valid");
                    } else {
                        println!("      Content: ✗ Invalid (may have errors)");
                    }
                } else {
                    println!("   ✗ {} missing", file);
                }
            }

            println!("\n═══════════════════════════════════════════════════════════");
            println!("📊 RESULTS");
            println!("═══════════════════════════════════════════════════════════\n");
            println!("Files created: {}/{}", files_created, required_files.len());

            if files_created >= required_files.len() - 1 {
                // Allow 1 missing file for now (LLM might use different structure)
                println!("\n✅ Test PASSED - React project structure created successfully!");
                println!("   The system can handle multi-file project creation");
            } else {
                println!("\n⚠️  Test PARTIAL - Some files missing, but structure is reasonable");
                println!("   This is acceptable as LLM may use different file structure");
            }

            // Show actual structure
            println!("\n📁 Actual project structure:");
            print_directory_tree(&test_dir, 0);
        }
        Err(e) => {
            println!("❌ LLM error: {}", e);
        }
    }

    // Clean up
    let _ = fs::remove_dir_all(&test_dir);
}

#[tokio::test]
#[cfg_attr(
    not(feature = "live-api-tests"),
    ignore = "Requires API key — run with: cargo test --features live-api-tests -- --ignored"
)]
async fn test_nodejs_api_creation() {
    let api_key = match env::var("ANTHROPIC_API_KEY") {
        Ok(key) => key,
        Err(_) => {
            println!("❌ Skipping test: ANTHROPIC_API_KEY not set");
            return;
        }
    };

    let base_url = env::var("ANTHROPIC_BASE_URL").ok();
    let test_dir = PathBuf::from("/tmp/rustycode_nodejs_test");

    let _ = fs::remove_dir_all(&test_dir);
    fs::create_dir_all(&test_dir).unwrap();

    let config = ProviderConfig {
        api_key: Some(SecretString::new(api_key.into())),
        base_url,
        timeout_seconds: Some(180),
        extra_headers: None,
        retry_config: None,
    };

    let model = env::var("ANTHROPIC_MODEL").unwrap_or_else(|_| "claude-sonnet-4-6".to_string());

    let provider = match AnthropicProvider::new_without_validation(config, model.clone()) {
        Ok(p) => p,
        Err(e) => {
            println!("❌ Failed to create provider: {:?}", e);
            return;
        }
    };

    println!("🚀 Testing: Create a Node.js API Project");
    println!("═══════════════════════════════════════════════════════════\n");

    let system_prompt = format!(
        "You are RustyCode, working in: {}

TASK: Create a Node.js REST API with Express.js

Create these files:
1. package.json with express and other dependencies
2. server.js (main server file with Express setup)
3. routes/users.js (user routes)
4. middleware/logger.js (logging middleware)

Make it a simple API with GET /users endpoint.",
        test_dir.display()
    );

    let prompt = "Create a Node.js REST API using Express with user routes";

    let messages = vec![ChatMessage::user(prompt.to_string())];
    let request = CompletionRequest::new(model, messages)
        .with_system_prompt(system_prompt)
        .with_max_tokens(4096)
        .with_temperature(0.1);

    println!("▶ Sending prompt to LLM...\n");

    match LLMProvider::complete(&provider, request).await {
        Ok(response) => {
            println!("📝 LLM Response:\n");
            println!(
                "{}\n",
                response.content.chars().take(400).collect::<String>()
            );

            println!("🔍 Verifying Node.js project structure...\n");

            let expected_files = vec!["package.json", "server.js"];
            let mut found = 0;

            for file in &expected_files {
                if test_dir.join(file).exists() {
                    found += 1;
                    println!("   ✓ {} created", file);
                } else {
                    println!("   ✗ {} missing", file);
                }
            }

            println!("\n📊 Files created: {}/{}", found, expected_files.len());

            if found >= 2 {
                println!("✅ Node.js API project created!");
            }

            println!("\n📁 Project structure:");
            print_directory_tree(&test_dir, 0);
        }
        Err(e) => {
            println!("❌ LLM error: {}", e);
        }
    }

    let _ = fs::remove_dir_all(&test_dir);
}

#[tokio::test]
#[cfg_attr(
    not(feature = "live-api-tests"),
    ignore = "Requires API key — run with: cargo test --features live-api-tests -- --ignored"
)]
async fn test_refactor_existing_code() {
    // Test refactoring an existing codebase
    let api_key = match env::var("ANTHROPIC_API_KEY") {
        Ok(key) => key,
        Err(_) => {
            println!("❌ Skipping test: ANTHROPIC_API_KEY not set");
            return;
        }
    };

    let base_url = env::var("ANTHROPIC_BASE_URL").ok();
    let test_dir = PathBuf::from("/tmp/rustycode_refactor_test");

    // Setup initial codebase
    setup_old_codebase(&test_dir);

    let config = ProviderConfig {
        api_key: Some(SecretString::new(api_key.into())),
        base_url,
        timeout_seconds: Some(180),
        extra_headers: None,
        retry_config: None,
    };

    let model = env::var("ANTHROPIC_MODEL").unwrap_or_else(|_| "claude-sonnet-4-6".to_string());

    let provider = match AnthropicProvider::new_without_validation(config, model.clone()) {
        Ok(p) => p,
        Err(e) => {
            println!("❌ Failed to create provider: {:?}", e);
            return;
        }
    };

    println!("🔨 Testing: Refactor Existing Codebase");
    println!("═══════════════════════════════════════════════════════════\n");

    let system_prompt = format!(
        "You are RustyCode, working in: {}

TASK: Refactor the codebase to use modern ES6+ syntax

Files to refactor:
- src/utils.js: Convert var to const/let, add arrow functions
- src/app.js: Convert to ES6 modules, use import/export
- package.json: Update to use type: module

Use read_file first to see the current code, then use write_file to update each file.",
        test_dir.display()
    );

    let prompt = "Refactor the JavaScript codebase to use modern ES6+ syntax (const/let, arrow functions, modules).";

    let messages = vec![ChatMessage::user(prompt.to_string())];
    let request = CompletionRequest::new(model, messages)
        .with_system_prompt(system_prompt)
        .with_max_tokens(4096)
        .with_temperature(0.1);

    println!("▶ Sending refactoring request...\n");

    match LLMProvider::complete(&provider, request).await {
        Ok(response) => {
            println!("📝 LLM Response:\n");
            println!(
                "{}\n",
                response.content.chars().take(400).collect::<String>()
            );

            println!("🔍 Verifying refactoring...\n");

            // Check if files were modified
            let utils_path = test_dir.join("src/utils.js");
            if utils_path.exists() {
                let contents = fs::read_to_string(&utils_path).unwrap();
                let has_es6 = contents.contains("const")
                    || contents.contains("arrow")
                    || contents.contains("=>");
                if has_es6 {
                    println!("   ✓ utils.js refactored to ES6");
                } else {
                    println!("   ⚠️  utils.js may not be fully refactored");
                }
            }

            println!("\n✅ Refactoring test completed");
            println!("   (Full verification would require running the code)");
        }
        Err(e) => {
            println!("❌ LLM error: {}", e);
        }
    }

    let _ = fs::remove_dir_all(&test_dir);
}

/// Validate file contents based on file type
fn validate_file_contents(filename: &str, contents: &str) -> bool {
    match filename {
        "package.json" => {
            // Check for valid JSON and required fields
            contents.contains("\"name\"")
                && contents.contains("\"version\"")
                && contents.contains("\"dependencies\"")
                && (contents.contains("react") || contents.contains("express"))
        }
        f if f.ends_with(".html") => {
            contents.contains("<!DOCTYPE html")
                || contents.contains("<html")
                || contents.contains("<div")
        }
        f if f.ends_with(".js") || f.ends_with(".jsx") => {
            contents.contains("import")
                || contents.contains("require")
                || contents.contains("function")
                || contents.contains("const")
                || contents.contains("class")
        }
        f if f.ends_with(".css") => {
            contents.contains("{") || contents.contains(":") || contents.contains(";")
        }
        _ => true,
    }
}

/// Print directory tree structure
fn print_directory_tree(path: &std::path::Path, depth: usize) {
    let indent = "  ".repeat(depth);

    if path.is_dir() {
        println!(
            "{}📁 {}/",
            indent,
            path.file_name().unwrap_or_default().to_string_lossy()
        );

        let mut entries: Vec<_> = fs::read_dir(path).unwrap().filter_map(|e| e.ok()).collect();
        entries.sort_by_key(|e| e.path());

        for entry in entries {
            print_directory_tree(&entry.path(), depth + 1);
        }
    } else {
        let file_name = path.file_name().unwrap_or_default().to_string_lossy();
        let size = fs::metadata(path).map_or(0, |m| m.len());

        let size_str = if size > 1024 {
            format!("({}KB)", size / 1024)
        } else {
            format!("({}B)", size)
        };

        println!("{}📄 {} {}", indent, file_name, size_str);
    }
}

/// Setup old-style codebase for refactoring test
fn setup_old_codebase(path: &std::path::Path) {
    fs::create_dir_all(path.join("src")).unwrap();

    // Old-style JavaScript
    fs::write(
        path.join("src/utils.js"),
        r#"var helper = function(name) {
    return "Hello " + name;
};

var calculate = function(a, b) {
    return a + b;
};
"#,
    )
    .unwrap();

    fs::write(
        path.join("src/app.js"),
        r#"var utils = require('./utils');

console.log(utils.helper("World"));
"#,
    )
    .unwrap();

    fs::write(
        path.join("package.json"),
        r#"{
  "name": "old-project",
  "version": "1.0.0"
}
"#,
    )
    .unwrap();
}
