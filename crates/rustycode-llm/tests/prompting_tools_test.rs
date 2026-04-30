#![allow(
    clippy::cast_lossless,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::cloned_instead_of_copied,
    clippy::doc_markdown,
    clippy::equatable_if_let,
    clippy::expect_used,
    clippy::float_cmp,
    clippy::format_push_string,
    clippy::manual_string_new,
    clippy::match_same_arms,
    clippy::missing_const_for_fn,
    clippy::needless_raw_string_hashes,
    clippy::no_effect_underscore_binding,
    clippy::redundant_clone,
    clippy::redundant_closure_for_method_calls,
    clippy::similar_names,
    clippy::single_char_pattern,
    clippy::too_many_lines,
    clippy::uninlined_format_args,
    clippy::unwrap_used
)]

//! Tests for Claude's prompting tools concepts.
//!
//! This test module covers:
//! - Prompt templates with {{variable}} placeholders
//! - Variable substitution for dynamic content
//! - Prompt improver workflow simulation
//! - XML-tagged prompt structure
//! - Chain-of-thought reasoning instructions

use rustycode_llm::provider::ChatMessage;
use std::collections::HashMap;

/// Simple variable substitution for prompt templates
fn substitute_variables(template: &str, variables: &HashMap<String, String>) -> String {
    let mut result = template.to_string();
    for (key, value) in variables {
        let placeholder = format!("{{{{{}}}}}", key);
        result = result.replace(&placeholder, value);
    }
    result
}

/// Parse variables from a template (finds all {{var}} patterns)
fn extract_variables(template: &str) -> Vec<String> {
    let mut vars = Vec::new();
    let mut chars = template.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '{' {
            if let Some(&'{') = chars.peek() {
                chars.next(); // consume second '{'
                let mut var_name = String::new();
                while let Some(&ch) = chars.peek() {
                    if ch == '}' {
                        chars.next();
                        if let Some(&'}') = chars.peek() {
                            chars.next();
                            break;
                        }
                    }
                    var_name.push(ch);
                    chars.next();
                }
                if !var_name.is_empty() {
                    vars.push(var_name);
                }
            }
        }
    }
    vars
}

#[test]
fn test_simple_variable_substitution() {
    let template = "Translate this text from English to Spanish: {{text}}";
    let mut vars = HashMap::new();
    vars.insert("text".to_string(), "Hello, world!".to_string());

    let result = substitute_variables(template, &vars);
    assert_eq!(
        result,
        "Translate this text from English to Spanish: Hello, world!"
    );
}

#[test]
fn test_multiple_variable_substitution() {
    let template = r#"You are {{role}} working for {{company}}.
Your task is to {{task}} using {{language}}."#;

    let mut vars = HashMap::new();
    vars.insert("role".to_string(), "a senior developer".to_string());
    vars.insert("company".to_string(), "Acme Corp".to_string());
    vars.insert("task".to_string(), "build a REST API".to_string());
    vars.insert("language".to_string(), "Rust and Actix-web".to_string());

    let result = substitute_variables(template, &vars);
    assert!(result.contains("a senior developer"));
    assert!(result.contains("Acme Corp"));
    assert!(result.contains("build a REST API"));
    assert!(result.contains("Rust and Actix-web"));
}

#[test]
fn test_extract_variables_from_template() {
    let template = "Process {{file_path}} with {{operation}}. Output to {{output_dir}}.";
    let vars = extract_variables(template);

    assert_eq!(vars.len(), 3);
    assert!(vars.contains(&"file_path".to_string()));
    assert!(vars.contains(&"operation".to_string()));
    assert!(vars.contains(&"output_dir".to_string()));
}

#[test]
fn test_template_with_code_blocks() {
    let template = r#"Analyze this {{language}} code:
```{{language}}
fn {{function_name}}() {
    {{code_body}}
}
```
Provide feedback on: {{focus_areas}}"#;

    let mut vars = HashMap::new();
    vars.insert("language".to_string(), "Rust".to_string());
    vars.insert("function_name".to_string(), "calculate".to_string());
    vars.insert("code_body".to_string(), "return 42;".to_string());
    vars.insert(
        "focus_areas".to_string(),
        "error handling, performance".to_string(),
    );

    let result = substitute_variables(template, &vars);
    assert!(result.contains("```Rust"));
    assert!(result.contains("fn calculate()"));
    assert!(result.contains("return 42;"));
    assert!(result.contains("error handling, performance"));
}

/// Test prompt structure following Claude's best practices
#[test]
fn test_xml_tagged_prompt_structure() {
    let template = r#"<task>
Describe the {{item_type}} in detail.
</task>

<context>
This is for {{audience}} with {{background}} background.
</context>

<instructions>
1. Start with a brief summary
2. Provide {{detail_level}} details
3. Include examples if relevant
</instructions>

<output_format>
- Use {{format}} format
- Keep response under {{max_length}} words
</output_format>"#;

    let mut vars = HashMap::new();
    vars.insert("item_type".to_string(), "Rust async function".to_string());
    vars.insert(
        "audience".to_string(),
        "intermediate developers".to_string(),
    );
    vars.insert("background".to_string(), "some JavaScript".to_string());
    vars.insert("detail_level".to_string(), "comprehensive".to_string());
    vars.insert("format".to_string(), "markdown".to_string());
    vars.insert("max_length".to_string(), "500".to_string());

    let result = substitute_variables(template, &vars);

    // Verify all sections are present
    assert!(result.contains("<task>"));
    assert!(result.contains("</task>"));
    assert!(result.contains("<context>"));
    assert!(result.contains("</context>"));
    assert!(result.contains("<instructions>"));
    assert!(result.contains("</instructions>"));
    assert!(result.contains("<output_format>"));
    assert!(result.contains("</output_format>"));

    // Verify substitutions worked
    assert!(result.contains("Rust async function"));
    assert!(result.contains("intermediate developers"));
    assert!(result.contains("markdown format"));
}

/// Test prompt improver workflow simulation
#[test]
fn test_prompt_improver_workflow() {
    // Step 1: Example identification (simulated)
    let _examples = [
        ("I love this product!", "positive"),
        ("This is terrible.", "negative"),
        ("It's okay, nothing special.", "neutral"),
    ];

    // Step 2 & 3: Initial draft with chain-of-thought
    let improved_template = r#"Classify the sentiment of the following text.

<text>
{{text}}
</text>

<reasoning>
1. Read the text carefully and identify key emotional words
2. Consider the overall tone and context
3. Determine if the sentiment is positive, negative, or neutral
4. Provide your classification with a brief explanation
</reasoning>

<output_format>
Respond in JSON format:
{
  "sentiment": "positive|negative|neutral",
  "confidence": "high|medium|low",
  "explanation": "brief reasoning"
}
</output_format>"#;

    let mut vars = HashMap::new();
    vars.insert(
        "text".to_string(),
        "This exceeded my expectations!".to_string(),
    );

    let result = substitute_variables(improved_template, &vars);

    // Verify structure
    assert!(result.contains("<text>"));
    assert!(result.contains("<reasoning>"));
    assert!(result.contains("<output_format>"));
    assert!(result.contains("This exceeded my expectations!"));
}

/// Test multi-turn conversation with templates
#[test]
fn test_multiturn_template_conversation() {
    // System prompt template (fixed content)
    let system_template = "You are a {{role}} helping users with {{domain}}.";

    // User message template
    let user_template = "Help me {{request}}.";

    // Assistant response template
    let response_template = "I'll help you {{action}}. Let's start by {{first_step}}.";

    let mut vars = HashMap::new();
    vars.insert("role".to_string(), "senior Rust developer".to_string());
    vars.insert("domain".to_string(), "async programming".to_string());
    vars.insert("request".to_string(), "understand async Rust".to_string());
    vars.insert("action".to_string(), "understand async Rust".to_string());
    vars.insert(
        "first_step".to_string(),
        "covering async/await basics".to_string(),
    );

    let system_msg = substitute_variables(system_template, &vars);
    let user_msg = substitute_variables(user_template, &vars);
    let response_msg = substitute_variables(response_template, &vars);

    assert!(system_msg.contains("senior Rust developer"));
    assert!(system_msg.contains("async programming"));
    assert!(user_msg.contains("understand async Rust"));
    assert!(response_msg.contains("covering async/await basics"));

    // Build conversation
    let conversation = [
        ChatMessage::system(system_msg),
        ChatMessage::user(user_msg),
        ChatMessage::assistant(response_msg),
    ];

    assert_eq!(conversation.len(), 3);
}

/// Test RAG-style template with retrieved content
#[test]
fn test_rag_template_with_retrieved_content() {
    let template = r#"Answer the user's question based on the following context.

<context>
{{retrieved_context}}
</context>

<question>
{{user_question}}
</question>

If the context doesn't contain enough information to answer the question, respond with "I don't have enough information to answer this question.""#;

    let mut vars = HashMap::new();
    vars.insert(
        "retrieved_context".to_string(),
        "RustyCode is a Rust-based AI coding assistant. It uses Claude API for completions."
            .to_string(),
    );
    vars.insert(
        "user_question".to_string(),
        "What is RustyCode?".to_string(),
    );

    let result = substitute_variables(template, &vars);

    assert!(result.contains("RustyCode is a Rust-based AI coding assistant"));
    assert!(result.contains("What is RustyCode?"));
    assert!(result.contains("<context>"));
    assert!(result.contains("<question>"));
}

/// Test edge cases in variable substitution
#[test]
fn test_variable_substitution_edge_cases() {
    // Empty template
    let result = substitute_variables("", &HashMap::new());
    assert_eq!(result, "");

    // Template with no variables
    let result = substitute_variables("Hello world", &HashMap::new());
    assert_eq!(result, "Hello world");

    // Variable not in template (should remain unchanged)
    let template = "Hello {{name}}";
    let mut vars = HashMap::new();
    vars.insert("role".to_string(), "developer".to_string());
    let result = substitute_variables(template, &vars);
    assert_eq!(result, "Hello {{name}}"); // Placeholder not replaced

    // Empty variable value
    let template = "Value: {{data}}";
    let mut vars = HashMap::new();
    vars.insert("data".to_string(), String::new());
    let result = substitute_variables(template, &vars);
    assert_eq!(result, "Value: ");

    // Multiple occurrences of same variable
    let template = "{{x}} + {{x}} = {{result}}";
    let mut vars = HashMap::new();
    vars.insert("x".to_string(), "5".to_string());
    vars.insert("result".to_string(), "10".to_string());
    let result = substitute_variables(template, &vars);
    assert_eq!(result, "5 + 5 = 10");
}

/// Test prompt with structured examples
#[test]
fn test_prompt_with_structured_examples() {
    let template = r#"Given a {{problem_type}}, provide a solution.

<examples>
Input: {{example1_input}}
Output: {{example1_output}}

Input: {{example2_input}}
Output: {{example2_output}}
</examples>

Now solve: {{target_input}}"#;

    let mut vars = HashMap::new();
    vars.insert("problem_type".to_string(), "math word problem".to_string());
    vars.insert("example1_input".to_string(), "2 + 2".to_string());
    vars.insert("example1_output".to_string(), "4".to_string());
    vars.insert("example2_input".to_string(), "5 * 3".to_string());
    vars.insert("example2_output".to_string(), "15".to_string());
    vars.insert("target_input".to_string(), "10 / 2".to_string());

    let result = substitute_variables(template, &vars);

    assert!(result.contains("math word problem"));
    assert!(result.contains("2 + 2"));
    assert!(result.contains("5 * 3"));
    assert!(result.contains("10 / 2"));
}

/// Simulate prompt improver's chain-of-thought refinement
#[test]
fn test_chain_of_thought_refinement() {
    // After chain-of-thought refinement
    let refined = r#"Solve the following problem step by step.

<problem>
{{problem}}
</problem>

<solution_steps>
1. Understand what the problem is asking
2. Identify the key information and constraints
3. Break down the problem into smaller parts
4. Solve each part systematically
5. Verify your answer
</solution_steps>

Provide your final answer in the following format:
<answer>
Your solution here.
</answer>"#;

    let mut vars = HashMap::new();
    vars.insert(
        "problem".to_string(),
        "If x + 3 = 10, what is x?".to_string(),
    );

    let result = substitute_variables(refined, &vars);

    assert!(result.contains("If x + 3 = 10, what is x?"));
    assert!(result.contains("<problem>"));
    assert!(result.contains("<solution_steps>"));
    assert!(result.contains("<answer>"));
}

/// Test variable substitution with special characters
#[test]
fn test_variables_with_special_characters() {
    let template = r#"Path: {{path}}
File: {{file}}
Command: {{command}}"#;

    let mut vars = HashMap::new();
    vars.insert("path".to_string(), "/home/user/projects".to_string());
    vars.insert("file".to_string(), "main.rs".to_string());
    vars.insert("command".to_string(), "cargo build --release".to_string());

    let result = substitute_variables(template, &vars);

    assert!(result.contains("/home/user/projects"));
    assert!(result.contains("main.rs"));
    assert!(result.contains("cargo build --release"));
}
