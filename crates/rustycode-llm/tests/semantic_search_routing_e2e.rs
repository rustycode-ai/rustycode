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

//! End-to-End test for semantic search auto-routing
//!
//! This test verifies that user queries are correctly routed to the
//! appropriate search tool (semantic_search, grep, glob, lsp_*) based
//! on intent analysis.
//!
//! Only compiled when the `vector-memory` feature is enabled.

#![cfg(feature = "vector-memory")]

use rustycode_llm::provider::{ChatMessage, MessageRole};
use rustycode_llm::tool_selection_helper::ToolSelectionState;
use rustycode_tools_api::{route_query, SearchStrategy};
use rustycode_tools_api::{ToolInfo, ToolMetadataProvider, ToolPermission};
use std::sync::Arc;

// Dummy provider used for tests to satisfy the ToolSelectionState dependency.
struct DummyProvider;
impl ToolMetadataProvider for DummyProvider {
    fn list_tools(&self) -> Vec<ToolInfo> {
        Vec::new()
    }
    fn tool_info(&self, name: &str) -> Option<ToolInfo> {
        Some(ToolInfo {
            name: name.to_string(),
            description: format!("{} (dummy)", name),
            parameters_schema: serde_json::json!({}),
            permission: ToolPermission::Read,
            defer_loading: None,
        })
    }
}

#[test]
fn test_end_to_end_semantic_routing() {
    // Create tool selection state with dummy provider
    let provider = Arc::new(DummyProvider) as Arc<dyn ToolMetadataProvider>;
    let state = ToolSelectionState::new(provider);

    // Simulate a user asking about authentication implementation
    let messages = vec![ChatMessage {
        role: MessageRole::User,
        content: "how is authentication implemented in this project?".into(),
    }];

    // Get the tools that would be sent to the LLM
    let tools = state.select_tools_for_prompt(&messages, &|tools| {
        tools
            .iter()
            .map(|t| serde_json::json!(t.name.clone()))
            .collect()
    });

    // Verify semantic_search is the only search tool sent
    assert!(tools.is_some(), "Should return tools for user message");
    let tool_names = tools.unwrap();

    let tool_strings: Vec<String> = tool_names
        .iter()
        .filter_map(|v| v.as_str().map(String::from))
        .collect();

    assert!(
        tool_strings.contains(&"semantic_search".to_string()),
        "semantic_search should be sent for 'how is authentication implemented' query. Got: {:?}",
        tool_strings
    );
}

#[test]
fn test_end_to_end_lsp_routing() {
    let provider = Arc::new(DummyProvider) as Arc<dyn ToolMetadataProvider>;
    let state = ToolSelectionState::new(provider);

    let messages = vec![ChatMessage {
        role: MessageRole::User,
        content: "where is `validate_jwt` defined?".into(),
    }];

    let tools = state.select_tools_for_prompt(&messages, &|tools| {
        tools
            .iter()
            .map(|t| serde_json::json!(t.name.clone()))
            .collect()
    });

    assert!(tools.is_some());
    let tool_names = tools.unwrap();

    let tool_strings: Vec<String> = tool_names
        .iter()
        .filter_map(|v| v.as_str().map(String::from))
        .collect();

    // LSP tools should be sent for exact symbol lookup
    assert!(
        tool_strings.iter().any(|t| t.starts_with("lsp_")),
        "LSP tools should be sent for exact symbol query. Got: {:?}",
        tool_strings
    );
}

#[test]
fn test_end_to_end_grep_routing() {
    let provider = Arc::new(DummyProvider) as Arc<dyn ToolMetadataProvider>;
    let state = ToolSelectionState::new(provider);

    let messages = vec![ChatMessage {
        role: MessageRole::User,
        content: "search for \"Unauthorized\"".into(),
    }];

    let tools = state.select_tools_for_prompt(&messages, &|tools| {
        tools
            .iter()
            .map(|t| serde_json::json!(t.name.clone()))
            .collect()
    });

    assert!(tools.is_some());
    let tool_names = tools.unwrap();

    let tool_strings: Vec<String> = tool_names
        .iter()
        .filter_map(|v| v.as_str().map(String::from))
        .collect();

    // grep should be sent for exact string search
    assert!(
        tool_strings.contains(&"grep".to_string()),
        "grep should be sent for exact string query. Got: {:?}",
        tool_strings
    );
}

#[test]
fn test_route_query_directly() {
    // Test direct route_query function
    assert_eq!(
        route_query("how do we validate JWT tokens?"),
        SearchStrategy::Semantic
    );
    assert_eq!(route_query("`validate_jwt`"), SearchStrategy::Lsp);
    assert_eq!(route_query("auth::middleware"), SearchStrategy::Lsp);
    assert_eq!(route_query("\"Unauthorized\""), SearchStrategy::Grep);
    assert_eq!(route_query("src/**/*.rs"), SearchStrategy::Glob);
}

#[test]
fn test_full_pipeline_semantic() {
    // Full pipeline: user query -> route_query -> tool filter
    let query = "find auth validation logic";

    // Step 1: Route query
    let strategy = route_query(query);
    assert_eq!(
        strategy,
        SearchStrategy::Semantic,
        "Should detect semantic intent"
    );

    // Step 2: Verify tool filtering would select semantic_search
    let available_tools = vec![
        "grep".to_string(),
        "glob".to_string(),
        "semantic_search".to_string(),
        "read_file".to_string(),
    ];

    let filtered = ToolSelectionState::apply_auto_routing(&available_tools, query);
    assert_eq!(
        filtered,
        vec!["semantic_search".to_string()],
        "Should filter to only semantic_search for semantic queries"
    );
}
