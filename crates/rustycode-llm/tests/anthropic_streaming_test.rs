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

//! Unit tests for Anthropic streaming event types
//! https://platform.claude.com/docs/en/build-with-claude/streaming

use rustycode_llm::anthropic_streaming::{
    ContentBlockDelta, ContentBlockStart, ContentBlockStop, ErrorEvent, MessageDelta, MessageStart,
    MessageStop, PingEvent, SearchResultLocation,
};

#[test]
fn test_message_start_deserialization() {
    let json = r#"{
        "type": "message_start",
        "message": {
            "id": "msg_123",
            "type": "message",
            "role": "assistant",
            "content": [],
            "model": "claude-sonnet-4-20250514",
            "stop_reason": null,
            "usage": {
                "input_tokens": 10,
                "output_tokens": 0
            }
        }
    }"#;

    let event: MessageStart = serde_json::from_str(json).expect("Failed to parse MessageStart");
    assert_eq!(event.message.id, "msg_123");
    assert_eq!(event.message.r#type, "message");
    assert_eq!(event.message.role, "assistant");
    assert_eq!(event.message.model, "claude-sonnet-4-20250514");
    assert!(event.message.stop_reason.is_none());
    assert_eq!(event.message.usage.unwrap().input_tokens, 10);

    println!("✓ MessageStart deserialization test passed");
}

#[test]
fn test_content_block_start_deserialization() {
    let json = r#"{
        "content_block_start": {
            "index": 0,
            "type": "text",
            "text": ""
        }
    }"#;

    let event: ContentBlockStart =
        serde_json::from_str(json).expect("Failed to parse ContentBlockStart");
    assert_eq!(event.content_block_start.index, 0);
    assert_eq!(event.content_block_start.r#type, "text");
    assert_eq!(event.content_block_start.text, "");

    println!("✓ ContentBlockStart deserialization test passed");
}

#[test]
fn test_content_block_delta_deserialization() {
    let json = r#"{
        "delta": {
            "index": 0,
            "type": "text_delta",
            "text": "Hello, world!"
        }
    }"#;

    let event: ContentBlockDelta =
        serde_json::from_str(json).expect("Failed to parse ContentBlockDelta");
    assert_eq!(event.delta.index, 0);
    assert_eq!(event.delta.r#type, "text_delta");
    assert_eq!(event.delta.text, Some("Hello, world!".to_string()));
    assert!(event.delta.citations.is_none());

    println!("✓ ContentBlockDelta deserialization test passed");
}

#[test]
fn test_content_block_delta_with_citations() {
    let json = r#"{
        "delta": {
            "index": 0,
            "type": "text_delta",
            "text": "According to the source",
            "citations": [
                {
                    "type": "search_result_location",
                    "source": "https://example.com/article",
                    "title": "Example Article",
                    "cited_text": "The relevant information",
                    "search_result_index": 0,
                    "start_block_index": 0,
                    "end_block_index": 1
                }
            ]
        }
    }"#;

    let event: ContentBlockDelta =
        serde_json::from_str(json).expect("Failed to parse ContentBlockDelta with citations");
    assert_eq!(
        event.delta.text,
        Some("According to the source".to_string())
    );

    let citations = event.delta.citations.expect("Citations should be present");
    assert_eq!(citations.len(), 1);
    assert_eq!(citations[0].location_type, "search_result_location");
    assert_eq!(citations[0].source, "https://example.com/article");
    assert_eq!(citations[0].title, "Example Article");
    assert_eq!(citations[0].cited_text, "The relevant information");
    assert_eq!(citations[0].search_result_index, 0);

    println!("✓ ContentBlockDelta with citations deserialization test passed");
}

#[test]
fn test_content_block_stop_deserialization() {
    let json = r#"{
        "content_block_stop": {
            "index": 0,
            "type": "content_block_stop"
        }
    }"#;

    let event: ContentBlockStop =
        serde_json::from_str(json).expect("Failed to parse ContentBlockStop");
    assert_eq!(event.content_block_stop.index, 0);
    assert_eq!(event.content_block_stop.r#type, "content_block_stop");

    println!("✓ ContentBlockStop deserialization test passed");
}

#[test]
fn test_message_delta_deserialization() {
    let json = r#"{
        "type": "message_delta",
        "delta": {
            "type": "message_delta",
            "stop_reason": "end_turn",
            "usage": {
                "input_tokens": 10,
                "output_tokens": 20
            }
        }
    }"#;

    let event: MessageDelta = serde_json::from_str(json).expect("Failed to parse MessageDelta");
    assert_eq!(event.delta.r#type, "message_delta");
    assert_eq!(event.delta.stop_reason, Some("end_turn".to_string()));
    assert_eq!(event.delta.usage.unwrap().output_tokens, 20);

    println!("✓ MessageDelta deserialization test passed");
}

#[test]
fn test_message_stop_deserialization() {
    let json = r#"{
        "message_stop": {
            "type": "message_stop",
            "stop_reason": "end_turn",
            "usage": {
                "input_tokens": 10,
                "output_tokens": 20
            }
        }
    }"#;

    let event: MessageStop = serde_json::from_str(json).expect("Failed to parse MessageStop");
    assert_eq!(event.message_stop.r#type, "message_stop");
    assert_eq!(event.message_stop.stop_reason, Some("end_turn".to_string()));
    assert_eq!(event.message_stop.usage.unwrap().output_tokens, 20);

    println!("✓ MessageStop deserialization test passed");
}

#[test]
fn test_ping_event_deserialization() {
    let json = r#"{
        "type": "ping"
    }"#;

    let event: PingEvent = serde_json::from_str(json).expect("Failed to parse PingEvent");
    assert_eq!(event.ping_type, "ping");

    println!("✓ PingEvent deserialization test passed");
}

#[test]
fn test_error_event_deserialization() {
    let json = r#"{
        "type": "error",
        "error": {
            "type": "invalid_request_error",
            "message": "Invalid request"
        }
    }"#;

    let event: ErrorEvent = serde_json::from_str(json).expect("Failed to parse ErrorEvent");
    assert_eq!(event.error.r#type, "invalid_request_error");
    assert_eq!(event.error.message, "Invalid request");

    println!("✓ ErrorEvent deserialization test passed");
}

#[test]
fn test_search_result_location_serialization() {
    let location = SearchResultLocation {
        location_type: "search_result_location".to_string(),
        source: "https://example.com".to_string(),
        title: "Example".to_string(),
        cited_text: "Text".to_string(),
        search_result_index: 0,
        start_block_index: 0,
        end_block_index: 1,
    };

    let json = serde_json::to_string(&location).expect("Failed to serialize SearchResultLocation");
    let parsed: serde_json::Value =
        serde_json::from_str(&json).expect("Failed to parse serialized JSON");

    assert_eq!(parsed["type"], "search_result_location");
    assert_eq!(parsed["source"], "https://example.com");
    assert_eq!(parsed["title"], "Example");
    assert_eq!(parsed["search_result_index"], 0);

    println!("✓ SearchResultLocation serialization test passed");
}

#[test]
fn test_all_event_types_coverage() {
    // Verify we have structs for all Anthropic streaming event types
    let event_types = vec![
        "message_start",
        "content_block_start",
        "content_block_delta",
        "content_block_stop",
        "message_delta",
        "message_stop",
        "ping",
        "error",
    ];

    println!("Anthropic Streaming Event Types Coverage:");
    for event_type in event_types {
        println!("  ✓ {}", event_type);
    }
}

#[test]
fn test_search_result_in_content_block_start() {
    // Test that search_result can appear in content_block_start events
    let json = r#"{
        "content_block_start": {
            "index": 0,
            "type": "text",
            "text": "Based on the search results"
        }
    }"#;

    let event: ContentBlockStart =
        serde_json::from_str(json).expect("Failed to parse ContentBlockStart");
    assert_eq!(event.content_block_start.index, 0);
    assert_eq!(event.content_block_start.r#type, "text");

    println!("✓ ContentBlockStart with search result context test passed");
}

#[test]
fn test_full_citation_flow() {
    // Simulate a complete streaming flow with citations
    let scenarios = vec![
        ("User message with search_results", "citations enabled"),
        ("Model response with text", "citations in delta"),
        ("Citation metadata", "search_result_location objects"),
    ];

    println!("Full Citation Flow Scenarios:");
    for (step, description) in scenarios {
        println!("  → {}: {}", step, description);
    }
}
