#![allow(
    clippy::manual_let_else,
    clippy::needless_raw_string_hashes,
    clippy::redundant_clone,
    clippy::single_match_else,
    clippy::uninlined_format_args,
    clippy::unwrap_used
)]

//! Test multiturn conversation functionality with system prompts
use rustycode_llm::{
    anthropic::AnthropicProvider,
    provider::{ChatMessage, CompletionRequest, LLMProvider, ProviderConfig},
};
use secrecy::SecretString;
use std::env;

#[test]
fn test_multiturn_conversation_with_system_prompt() {
    // Skip test if API key not set
    let api_key = match env::var("ANTHROPIC_API_KEY") {
        Ok(key) => key,
        Err(_) => {
            println!("Skipping test: ANTHROPIC_API_KEY not set");
            return;
        }
    };

    let base_url = env::var("ANTHROPIC_BASE_URL").ok();

    let config = ProviderConfig {
        api_key: Some(SecretString::new(api_key.into())),
        base_url,
        timeout_seconds: Some(300),
        extra_headers: None,
        retry_config: None,
    };

    let model = env::var("ANTHROPIC_MODEL").unwrap_or_else(|_| "claude-3-haiku".to_string());

    let provider = match AnthropicProvider::new_without_validation(config, model.clone()) {
        Ok(p) => p,
        Err(e) => {
            println!("Skipping test: Failed to create provider: {:?}", e);
            return;
        }
    };

    // Create system prompt with tool descriptions
    let system_prompt = create_system_prompt();

    // First message: Tell the AI my name
    let mut messages = vec![ChatMessage::user(
        "My name is Alice. Remember that.".to_string(),
    )];

    let request = CompletionRequest::new(model.clone(), messages.clone())
        .with_system_prompt(system_prompt.clone());

    let rt = tokio::runtime::Runtime::new().unwrap();
    let result1 = rt.block_on(async { provider.complete(request).await });

    match result1 {
        Ok(response1) => {
            println!("Response 1: {}", response1.content);

            // Second message: Ask if it remembers my name
            messages.push(ChatMessage::assistant(response1.content.clone()));
            messages.push(ChatMessage::user("What is my name?".to_string()));

            let request2 = CompletionRequest::new(model.clone(), messages.clone())
                .with_system_prompt(system_prompt.clone());

            let result2 = rt.block_on(async { provider.complete(request2).await });

            match result2 {
                Ok(response2) => {
                    println!("Response 2: {}", response2.content);

                    // Check if AI remembers the name
                    assert!(
                        response2.content.to_lowercase().contains("alice"),
                        "AI should remember the name 'Alice'"
                    );
                    println!("✓ Multiturn test passed: AI remembered the name");
                }
                Err(e) => {
                    println!("Second request failed: {:?}", e);
                    panic!("Second request failed");
                }
            }
        }
        Err(e) => {
            println!("First request failed: {:?}", e);
            panic!("First request failed");
        }
    }
}

fn create_system_prompt() -> String {
    r#"You are RustyCode, an AI coding assistant optimized for:

**Core Principles:**
1. **Correctness** - Ensure code works as intended and handles edge cases
2. **Fast iteration** - Make incremental progress with rapid feedback loops
3. **Production-safe** - Write code that's maintainable, testable, and deployable

You excel at understanding codebases, implementing features, fixing bugs, and navigating complex code. You communicate clearly and take decisive action when the path forward is clear.

## Available Tools

**read_file** - Examine file contents
- Use to read source code, configuration files, documentation
- Parameters: path (required)

**bash** - Execute shell commands
- Use to run tests, build projects, search code
- Parameters: command (required)

**write_file** - Create or modify files
- Use to create new files or update existing ones
- Parameters: path (required), content (required)

**grep** - Search for patterns in files
- Use to search file contents
- Parameters: pattern (required), path (optional)

**glob** - Find files by pattern
- Use to find files matching a pattern
- Parameters: pattern (required)

Remember to use tools when needed to complete tasks!"#.to_string()
}
