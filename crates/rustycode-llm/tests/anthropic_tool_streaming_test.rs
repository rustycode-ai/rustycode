#![allow(
    clippy::cast_lossless,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::cloned_instead_of_copied,
    clippy::doc_markdown,
    clippy::expect_used,
    clippy::float_cmp,
    clippy::manual_string_new,
    clippy::match_same_arms,
    clippy::missing_const_for_fn,
    clippy::redundant_clone,
    clippy::similar_names,
    clippy::single_char_pattern,
    clippy::too_many_lines,
    clippy::uninlined_format_args,
    clippy::unwrap_used
)]

//! Tests for Anthropic fine-grained tool streaming support
//! https://platform.claude.com/docs/en/agents-and-tools/tool-use/fine-grained-tool-streaming

use rustycode_llm::tools::{to_anthropic_tools, ToolDefinition};
use serde_json::json;

#[test]
fn test_tool_with_eager_streaming_enabled() {
    let tool = ToolDefinition::new(
        "Bash",
        "Execute bash commands",
        json!({
            "type": "object",
            "properties": {
                "command": {"type": "string"}
            }
        }),
    )
    .with_eager_streaming();

    assert_eq!(tool.name, "Bash");
    assert_eq!(tool.eager_input_streaming, Some(true));
}

#[test]
fn test_tool_without_eager_streaming() {
    let tool = ToolDefinition::new(
        "Read",
        "Read file contents",
        json!({
            "type": "object",
            "properties": {
                "path": {"type": "string"}
            }
        }),
    );

    assert_eq!(tool.name, "Read");
    assert_eq!(tool.eager_input_streaming, None);
}

#[test]
fn test_anthropic_tool_conversion_with_eager_streaming() {
    let tools = vec![ToolDefinition::new(
        "Bash",
        "Execute bash commands",
        json!({
            "type": "object",
            "properties": {
                "command": {"type": "string"}
            }
        }),
    )
    .with_eager_streaming()];

    let anthropic_tools = to_anthropic_tools(&tools);
    assert_eq!(anthropic_tools.len(), 1);

    let bash_tool = &anthropic_tools[0];
    assert_eq!(bash_tool["name"], "Bash");
    assert_eq!(bash_tool["eager_input_streaming"], true);
}

#[test]
fn test_anthropic_tool_conversion_without_eager_streaming() {
    let tools = vec![ToolDefinition::new(
        "Read",
        "Read file contents",
        json!({
            "type": "object",
            "properties": {
                "path": {"type": "string"}
            }
        }),
    )];

    let anthropic_tools = to_anthropic_tools(&tools);
    assert_eq!(anthropic_tools.len(), 1);

    let read_file_tool = &anthropic_tools[0];
    assert_eq!(read_file_tool["name"], "Read");
    // eager_input_streaming should not be present
    assert!(read_file_tool.get("eager_input_streaming").is_none());
}

#[test]
fn test_mixed_tools_with_and_without_eager_streaming() {
    let tools = vec![
        ToolDefinition::new(
            "Bash",
            "Execute bash commands",
            json!({
                "type": "object",
                "properties": {
                    "command": {"type": "string"}
                }
            }),
        )
        .with_eager_streaming(),
        ToolDefinition::new(
            "Read",
            "Read file contents",
            json!({
                "type": "object",
                "properties": {
                    "path": {"type": "string"}
                }
            }),
        ),
        ToolDefinition::new(
            "WebFetch",
            "Fetch web content",
            json!({
                "type": "object",
                "properties": {
                    "url": {"type": "string"}
                }
            }),
        )
        .with_eager_streaming(),
    ];

    let anthropic_tools = to_anthropic_tools(&tools);
    assert_eq!(anthropic_tools.len(), 3);

    // bash should have eager_input_streaming
    assert_eq!(anthropic_tools[0]["eager_input_streaming"], true);

    // read_file should not have eager_input_streaming
    assert!(anthropic_tools[1].get("eager_input_streaming").is_none());

    // web_fetch should have eager_input_streaming
    assert_eq!(anthropic_tools[2]["eager_input_streaming"], true);
}

#[test]
fn test_eager_streaming_with_examples() {
    let tool = ToolDefinition::new(
        "Bash",
        "Execute bash commands",
        json!({
            "type": "object",
            "properties": {
                "command": {"type": "string"}
            }
        }),
    )
    .with_examples(vec![
        json!({"command": "ls -la"}),
        json!({"command": "cargo test"}),
    ])
    .with_eager_streaming();

    assert!(tool.examples.is_some());
    assert_eq!(tool.examples.as_ref().unwrap().len(), 2);
    assert_eq!(tool.eager_input_streaming, Some(true));

    let anthropic_tools = to_anthropic_tools(&[tool]);
    let bash_tool = &anthropic_tools[0];

    assert_eq!(bash_tool["eager_input_streaming"], true);
    assert!(bash_tool.get("examples").is_some());
    assert_eq!(bash_tool["examples"].as_array().unwrap().len(), 2);
}

#[test]
fn test_server_tool_with_eager_streaming() {
    // Server tools shouldn't have eager_input_streaming in the output
    // since they're not sent in the tool definitions
    let tool = ToolDefinition::new(
        "WebSearch",
        "Search the web",
        json!({
            "type": "object",
            "properties": {
                "query": {"type": "string"}
            }
        }),
    )
    .server_tool()
    .with_anthropic_type("web_search_20260209")
    .with_eager_streaming();

    assert!(tool.is_server_tool);
    assert_eq!(tool.eager_input_streaming, Some(true));

    // Server tools are included but converted with type field, not eager_input_streaming
    let anthropic_tools = to_anthropic_tools(&[tool]);
    assert_eq!(anthropic_tools.len(), 1);
    assert_eq!(anthropic_tools[0]["name"], "WebSearch");
    assert_eq!(anthropic_tools[0]["type"], "web_search_20260209");
    assert!(anthropic_tools[0].get("eager_input_streaming").is_none());
}

#[test]
fn test_tool_builder_chaining() {
    // Test that builder methods can be chained in any order
    let tool = ToolDefinition::new(
        "Bash",
        "Execute bash commands",
        json!({
            "type": "object",
            "properties": {
                "command": {"type": "string"}
            }
        }),
    )
    .with_examples(vec![json!({"command": "ls"})])
    .with_eager_streaming();

    assert_eq!(tool.name, "Bash");
    assert!(tool.examples.is_some());
    assert_eq!(tool.eager_input_streaming, Some(true));

    // Test reverse order
    let tool2 = ToolDefinition::new(
        "Bash",
        "Execute bash commands",
        json!({
            "type": "object",
            "properties": {
                "command": {"type": "string"}
            }
        }),
    )
    .with_eager_streaming()
    .with_examples(vec![json!({"command": "ls"})]);

    assert_eq!(tool2.name, "Bash");
    assert!(tool2.examples.is_some());
    assert_eq!(tool2.eager_input_streaming, Some(true));
}

#[test]
fn test_production_tools_have_eager_streaming() {
    let tools = rustycode_llm::tools::tui_tools();
    let anthropic_tools = to_anthropic_tools(&tools);

    let eager_tools = ["Bash", "Read", "Write", "WebFetch"];
    for name in &eager_tools {
        let tool = anthropic_tools
            .iter()
            .find(|t| t["name"] == *name)
            .unwrap_or_else(|| panic!("Tool '{}' not found", name));
        assert_eq!(
            tool["eager_input_streaming"], true,
            "Tool '{}' should have eager_input_streaming enabled",
            name
        );
    }
}

#[test]
fn test_lsp_tools_do_not_have_eager_streaming() {
    let tools = rustycode_llm::tools::tui_tools();
    let anthropic_tools = to_anthropic_tools(&tools);

    let non_eager_tools = [
        "LspDiagnostics",
        "LspHover",
        "LspDefinition",
        "LspCompletion",
    ];
    for name in &non_eager_tools {
        let tool = anthropic_tools
            .iter()
            .find(|t| t["name"] == *name)
            .unwrap_or_else(|| panic!("Tool '{}' not found", name));
        assert!(
            tool.get("eager_input_streaming").is_none(),
            "Tool '{}' should NOT have eager_input_streaming",
            name
        );
    }
}
