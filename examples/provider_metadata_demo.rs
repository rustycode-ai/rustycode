//! Example demonstrating provider metadata usage for dynamic configuration and prompt optimization.
//!
//! This example shows how to:
//! 1. Get metadata for a provider (for dynamic UI generation)
//! 2. Generate provider-specific system prompts
//! 3. Format tools according to provider capabilities

use rustycode_llm::{ToolSchema, get_metadata};

fn main() -> anyhow::Result<()> {
    // Example 1: Get provider metadata for dynamic UI generation
    println!("=== Provider Metadata for Configuration UI ===\n");

    if let Some(anthropic_metadata) = get_metadata("anthropic") {
        println!("Provider: {}", anthropic_metadata.display_name);
        println!("Description: {}", anthropic_metadata.description);

        println!("\nRequired Fields:");
        for field in &anthropic_metadata.config_schema.required_fields {
            println!("  - {} ({})", field.label, field.name);
            println!("    Type: {:?}", field.field_type);
            println!(
                "    Placeholder: {}",
                field.placeholder.as_ref().unwrap_or(&"N/A".to_string())
            );
            println!("    Required: Yes");
        }

        println!("\nOptional Fields:");
        for field in &anthropic_metadata.config_schema.optional_fields {
            println!("  - {} ({})", field.label, field.name);
            println!("    Type: {:?}", field.field_type);
            println!(
                "    Default: {}",
                field.default.as_ref().unwrap_or(&"N/A".to_string())
            );
            println!("    Required: No");
        }

        println!("\nEnvironment Variables:");
        for (field, env_var) in &anthropic_metadata.config_schema.env_mappings {
            println!("  - {}: {}", field, env_var);
        }
    }

    // Example 2: Generate provider-specific system prompts (no tools in prompt!)
    println!("\n\n=== Provider-Specific System Prompt Generation ===\n");

    if let Some(openai_metadata) = get_metadata("openai") {
        let system_prompt = openai_metadata
            .generate_system_prompt("You are helping a developer debug a complex issue.");

        println!("OpenAI System Prompt:");
        println!("{}\n", system_prompt);
    }

    // Example 2b: Show how tools are handled separately in request JSON
    println!("=== Tool Definitions (Request JSON) ===\n");

    let tools = vec![
        ToolSchema {
            name: "web_search".to_string(),
            description: "Search the web for current information".to_string(),
            parameters: r#"{"query": "string", "num_results": "number"}"#.to_string(),
        },
        ToolSchema {
            name: "code_execute".to_string(),
            description: "Execute code in a safe environment".to_string(),
            parameters: r#"{"language": "string", "code": "string"}"#.to_string(),
        },
    ];

    // Show how Anthropic formats tools for request JSON
    if let Some(anthropic_metadata) = get_metadata("anthropic") {
        let tool_definitions = anthropic_metadata.generate_tool_definitions(&tools);
        println!("Anthropic Request JSON Tools field:");
        println!(
            "{}",
            serde_json::to_string_pretty(&tool_definitions).unwrap()
        );
        println!();

        let tool_instructions = anthropic_metadata.generate_tool_instructions();
        println!("Anthropic Tool Instructions (for system prompt):");
        println!("{}\n", tool_instructions);
    }

    // Example 3: Compare system prompts across providers
    println!("=== System Prompt Comparison Across Providers ===\n");

    let context = "Help the user write efficient Rust code.";

    for provider_id in &["anthropic", "openai", "gemini"] {
        if let Some(metadata) = get_metadata(provider_id) {
            println!("--- {} ({}) ---", metadata.display_name, provider_id);
            let prompt = metadata.generate_system_prompt(context);
            println!("{}\n", prompt);
        }
    }

    // Example 3b: Compare tool request JSON format across providers
    println!("=== Tool Request JSON Format Comparison ===\n");

    let tools = vec![ToolSchema {
        name: "file_read".to_string(),
        description: "Read file contents".to_string(),
        parameters: r#"{"path": "string"}"#.to_string(),
    }];

    for provider_id in &["anthropic", "openai", "gemini"] {
        if let Some(metadata) = get_metadata(provider_id) {
            println!("--- {} ({}) ---", metadata.display_name, provider_id);
            let tool_definitions = metadata.generate_tool_definitions(&tools);
            println!(
                "{}\n",
                serde_json::to_string_pretty(&tool_definitions).unwrap()
            );
        }
    }

    // Example 4: Tool calling capabilities
    println!("=== Tool Calling Capabilities ===\n");

    for provider_id in &["anthropic", "openai", "gemini", "together"] {
        if let Some(metadata) = get_metadata(provider_id) {
            println!("{}:", metadata.display_name);
            println!("  - Supported: {}", metadata.tool_calling.supported);
            println!(
                "  - Parallel Calling: {}",
                metadata.tool_calling.parallel_calling
            );
            println!("  - Streaming: {}", metadata.tool_calling.streaming_support);
            if let Some(max_tools) = metadata.tool_calling.max_tools_per_call {
                println!("  - Max Tools Per Call: {}", max_tools);
            }
            println!();
        }
    }

    // Example 5: Model recommendations
    println!("=== Model Recommendations ===\n");

    if let Some(anthropic_metadata) = get_metadata("anthropic") {
        println!(
            "Recommended models for {}:",
            anthropic_metadata.display_name
        );
        for model in &anthropic_metadata.recommended_models {
            println!(
                "  - {} (Cost Tier: {})",
                model.display_name, model.cost_tier
            );
            println!("    Context Window: {}", model.context_window);
            println!(
                "    Tools: {}",
                if model.supports_tools { "✓" } else { "✗" }
            );
            println!("    Use Cases: {}", model.use_cases.join(", "));
            println!();
        }
    }

    // Example 6: Prompt optimization strategies
    println!("=== Prompt Optimization Strategies ===\n");

    for provider_id in &["anthropic", "openai", "together"] {
        if let Some(metadata) = get_metadata(provider_id) {
            println!("{}:", metadata.display_name);
            println!(
                "  - XML Structure: {}",
                metadata.prompt_template.optimizations.prefer_xml_structure
            );
            println!(
                "  - Include Examples: {}",
                metadata.prompt_template.optimizations.include_examples
            );
            println!(
                "  - Prompt Length: {:?}",
                metadata
                    .prompt_template
                    .optimizations
                    .preferred_prompt_length
            );
            println!("  - Special Instructions:");
            for instruction in &metadata.prompt_template.optimizations.special_instructions {
                println!("    • {}", instruction);
            }
            println!();
        }
    }

    Ok(())
}
