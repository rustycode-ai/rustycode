#![allow(
    clippy::doc_markdown,
    clippy::expect_used,
    clippy::uninlined_format_args,
    clippy::unwrap_used
)]

//! Example: Using search_result content blocks with Anthropic API
//!
//! This example demonstrates how to use search_result content blocks
//! for Retrieval-Augmented Generation (RAG) applications with citations.

use rustycode_llm::{
    anthropic::{AnthropicProvider, CitationMetadata, SearchResultBlock, SearchResultContent},
    provider::{ChatMessage, CompletionRequest, ProviderConfig},
};
use secrecy::SecretString;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize the Anthropic provider
    let config = ProviderConfig {
        api_key: Some(SecretString::new(
            std::env::var("ANTHROPIC_API_KEY")
                .expect("ANTHROPIC_API_KEY environment variable not set")
                .into(),
        )),
        base_url: Some("https://api.anthropic.com".to_string()),
        timeout_seconds: Some(120),
        extra_headers: None,
        retry_config: None,
    };

    let _provider = AnthropicProvider::new(config, "claude-sonnet-4-6".to_string())?;

    println!("=== Anthropic Search Result Example ===\n");

    // Example 1: Create a single search result
    println!("1. Creating a single search_result block:");
    let search_result = SearchResultBlock {
        content_type: "search_result",
        source: "https://docs.anthropic.com".to_string(),
        title: "Claude API Documentation".to_string(),
        content: vec![SearchResultContent {
            content_type: "text",
            text: "Claude is a next-generation AI assistant...".to_string(),
        }],
        citations: Some(CitationMetadata { enabled: true }),
        cache_control: None,
    };

    let search_json = serde_json::to_string_pretty(&search_result)?;
    println!("{}\n", search_json);

    // Example 2: Multiple search results (typical RAG scenario)
    println!("2. Creating multiple search_result blocks:");
    let search_results = vec![
        SearchResultBlock {
            content_type: "search_result",
            source: "https://doc.rust-lang.org/book".to_string(),
            title: "The Rust Programming Language".to_string(),
            content: vec![SearchResultContent {
                content_type: "text",
                text: "Rust is a systems programming language...".to_string(),
            }],
            citations: Some(CitationMetadata { enabled: true }),
            cache_control: None,
        },
        SearchResultBlock {
            content_type: "search_result",
            source: "https://docs.rs".to_string(),
            title: "Rust Crate Documentation".to_string(),
            content: vec![SearchResultContent {
                content_type: "text",
                text: "This crate provides...".to_string(),
            }],
            citations: Some(CitationMetadata { enabled: true }),
            cache_control: None,
        },
    ];

    let results_json = serde_json::to_string_pretty(&search_results)?;
    println!("{}\n", results_json);

    // Example 3: Using search results in a completion request
    println!("3. Building a completion request:");
    let messages = vec![ChatMessage::user(
        "What is Rust programming language based on these search results?".to_string(),
    )];

    let request = CompletionRequest::new("claude-sonnet-4-6".to_string(), messages);

    println!("Request model: {}", request.model);
    println!("Number of messages: {}\n", request.messages.len());

    // Example 4: Simulated API call (would require valid API key in production)
    println!("4. API call (simulated - requires valid ANTHROPIC_API_KEY):");
    println!("   In production, the search results would be included");
    println!("   in the request content, and Claude's response would");
    println!("   include citations referencing the sources.\n");

    // Example 5: Expected citation format in response
    println!("5. Expected citation format in response:");
    println!("   When citations are enabled, Claude's response includes:");
    println!("   {{");
    println!("     \"type\": \"search_result_location\",");
    println!("     \"source\": \"https://doc.rust-lang.org/book\",");
    println!("     \"title\": \"The Rust Programming Language\",");
    println!("     \"cited_text\": \"Rust is a systems programming language...\",");
    println!("     \"search_result_index\": 0,");
    println!("     \"start_block_index\": 0,");
    println!("     \"end_block_index\": 1");
    println!("   }}\n");

    println!("=== Example Complete ===");
    println!("\nKey Features:");
    println!("  ✓ search_result content blocks for RAG applications");
    println!("  ✓ Automatic citation generation when enabled");
    println!("  ✓ Source attribution with quoted text");
    println!("  ✓ Support for multiple search results");
    println!("  ✓ Optional cache control for performance");

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_search_result_creation() {
        let search_result = SearchResultBlock {
            content_type: "search_result",
            source: "https://example.com".to_string(),
            title: "Test".to_string(),
            content: vec![SearchResultContent {
                content_type: "text",
                text: "Content".to_string(),
            }],
            citations: Some(CitationMetadata { enabled: true }),
            cache_control: None,
        };

        assert_eq!(search_result.content_type, "search_result");
        assert_eq!(search_result.source, "https://example.com");
        assert!(search_result.citations.unwrap().enabled);
    }

    #[test]
    fn test_search_result_serialization() {
        let search_result = SearchResultBlock {
            content_type: "search_result",
            source: "https://example.com".to_string(),
            title: "Test".to_string(),
            content: vec![SearchResultContent {
                content_type: "text",
                text: "Content".to_string(),
            }],
            citations: None,
            cache_control: None,
        };

        let json = serde_json::to_string(&search_result).unwrap();
        assert!(json.contains("\"type\":\"search_result\""));
        assert!(json.contains("\"source\":\"https://example.com\""));
    }
}
