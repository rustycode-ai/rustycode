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

//! Unit tests for Anthropic search_result content block support
//! https://platform.claude.com/docs/en/build-with-claude/search-results

use rustycode_llm::anthropic::{
    CacheControl, CitationMetadata, SearchResultBlock, SearchResultContent,
};

#[test]
fn test_search_result_serialization() {
    // Test that SearchResultBlock serializes to correct JSON format
    let search_result = SearchResultBlock {
        content_type: "search_result",
        source: "https://example.com/article".to_string(),
        title: "Example Article".to_string(),
        content: vec![SearchResultContent {
            content_type: "text",
            text: "This is the content of the search result.".to_string(),
        }],
        citations: None,
        cache_control: None,
    };

    let json =
        serde_json::to_string(&search_result).expect("Failed to serialize SearchResultBlock");
    let parsed: serde_json::Value =
        serde_json::from_str(&json).expect("Failed to parse serialized JSON");

    assert_eq!(parsed["type"], "search_result");
    assert_eq!(parsed["source"], "https://example.com/article");
    assert_eq!(parsed["title"], "Example Article");
    assert!(parsed["citations"].is_null() || parsed["citations"].is_object()); // Optional field
    assert!(parsed["cache_control"].is_null() || parsed["cache_control"].is_object()); // Optional field

    println!("✓ SearchResultBlock serialization test passed");
    println!("  Serialized JSON: {}", json);
}

#[test]
fn test_search_result_with_citations_enabled() {
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

    let json = serde_json::to_string(&search_result).expect("Failed to serialize");
    let parsed: serde_json::Value = serde_json::from_str(&json).expect("Failed to parse");

    assert_eq!(parsed["citations"]["enabled"], true);

    println!("✓ SearchResult with citations enabled test passed");
}

#[test]
fn test_search_result_with_cache_control() {
    let search_result = SearchResultBlock {
        content_type: "search_result",
        source: "https://example.com".to_string(),
        title: "Test".to_string(),
        content: vec![SearchResultContent {
            content_type: "text",
            text: "Content".to_string(),
        }],
        citations: None,
        cache_control: Some(CacheControl {
            cache_type: "ephemeral",
        }),
    };

    let json = serde_json::to_string(&search_result).expect("Failed to serialize");
    let parsed: serde_json::Value = serde_json::from_str(&json).expect("Failed to parse");

    assert_eq!(parsed["cache_control"]["type"], "ephemeral");

    println!("✓ SearchResult with cache control test passed");
}

#[test]
fn test_single_search_result_parsing() {
    let json = r#"{
        "type": "search_result",
        "source": "https://docs.anthropic.com",
        "title": "Anthropic Documentation",
        "content": [
            {
                "type": "text",
                "text": "Claude is a large language model..."
            }
        ]
    }"#;

    let search_result: SearchResultBlock =
        serde_json::from_str(json).expect("Failed to parse single search_result");

    assert_eq!(search_result.content_type, "search_result");
    assert_eq!(search_result.source, "https://docs.anthropic.com");
    assert_eq!(search_result.title, "Anthropic Documentation");
    assert_eq!(search_result.content.len(), 1);
    assert_eq!(
        search_result.content[0].text,
        "Claude is a large language model..."
    );

    println!("✓ Single search_result parsing test passed");
}

#[test]
fn test_mixed_content_array_parsing() {
    // Test parsing an array with both text and search_result blocks
    let json = r#"{
        "type": "message",
        "role": "user",
        "content": [
            {
                "type": "text",
                "text": "Search for information about Rust programming"
            },
            {
                "type": "search_result",
                "source": "https://doc.rust-lang.org",
                "title": "The Rust Programming Language",
                "content": [
                    {
                        "type": "text",
                        "text": "Rust is a systems programming language..."
                    }
                ]
            },
            {
                "type": "text",
                "text": "Now summarize what you found"
            }
        ]
    }"#;

    let parsed: serde_json::Value =
        serde_json::from_str(json).expect("Failed to parse mixed content");
    let content = parsed["content"]
        .as_array()
        .expect("Content should be an array");

    assert_eq!(content.len(), 3);
    assert_eq!(content[0]["type"], "text");
    assert_eq!(
        content[0]["text"],
        "Search for information about Rust programming"
    );
    assert_eq!(content[1]["type"], "search_result");
    assert_eq!(content[1]["title"], "The Rust Programming Language");
    assert_eq!(content[2]["type"], "text");

    println!("✓ Mixed content array parsing test passed");
}

#[test]
fn test_citation_metadata_parsing() {
    let json = r#"{
        "type": "search_result",
        "source": "https://example.com/article",
        "title": "Test Article",
        "content": [
            {"type": "text", "text": "Content here"}
        ],
        "citations": {
            "enabled": true
        }
    }"#;

    let search_result: SearchResultBlock =
        serde_json::from_str(json).expect("Failed to parse search_result with citations");

    assert!(search_result.citations.is_some());
    assert!(search_result.citations.unwrap().enabled);

    println!("✓ Citation metadata parsing test passed");
}

#[test]
fn test_search_result_with_empty_content() {
    let json = r#"{
        "type": "search_result",
        "source": "https://example.com",
        "title": "Empty",
        "content": []
    }"#;

    let search_result: SearchResultBlock =
        serde_json::from_str(json).expect("Failed to parse search_result with empty content");

    assert_eq!(search_result.content.len(), 0);

    println!("✓ SearchResult with empty content test passed");
}

#[test]
fn test_multiple_search_results_in_array() {
    let json = r#"{
        "content": [
            {
                "type": "search_result",
                "source": "https://example1.com",
                "title": "First Result",
                "content": [{"type": "text", "text": "First content"}]
            },
            {
                "type": "search_result",
                "source": "https://example2.com",
                "title": "Second Result",
                "content": [{"type": "text", "text": "Second content"}]
            },
            {
                "type": "search_result",
                "source": "https://example3.com",
                "title": "Third Result",
                "content": [{"type": "text", "text": "Third content"}]
            }
        ]
    }"#;

    let parsed: serde_json::Value = serde_json::from_str(json).expect("Failed to parse");
    let content = parsed["content"]
        .as_array()
        .expect("Content should be an array");

    assert_eq!(content.len(), 3);
    for (i, result) in content.iter().enumerate() {
        assert_eq!(result["type"], "search_result");
        assert_eq!(
            result["title"],
            format!("{} Result", ["First", "Second", "Third"][i])
        );
    }

    println!("✓ Multiple search_results in array test passed");
}

#[test]
fn test_search_result_content_types() {
    // Test that only "text" type is currently supported in content
    let json = r#"{
        "type": "search_result",
        "source": "https://example.com",
        "title": "Test",
        "content": [
            {"type": "text", "text": "Text content"}
        ]
    }"#;

    let search_result: SearchResultBlock =
        serde_json::from_str(json).expect("Failed to parse search_result");

    assert_eq!(search_result.content[0].content_type, "text");
    assert_eq!(search_result.content[0].text, "Text content");

    println!("✓ SearchResult content types test passed");
}
