//! Tests for the Anthropic provider.

use super::*;
use crate::provider::{CompletionResponse, LLMProvider, ThinkingBlock};
use secrecy::SecretString;

use super::helpers::{
    is_opus_47_or_later, map_anthropic_error, map_anthropic_structured_error,
    normalize_thinking_for_model,
};
use super::types::{
    AnthropicContent, AnthropicRequest, AnthropicRequestContent, AnthropicResponse, AnthropicUsage,
    CacheControl, ContentBlock, SystemContentBlock, SystemPrompt,
};

fn make_config(api_key: Option<&str>) -> ProviderConfig {
    ProviderConfig {
        api_key: api_key.map(|s| SecretString::new(s.to_string().into())),
        base_url: None,
        timeout_seconds: Some(120),
        extra_headers: None,
        retry_config: None,
    }
}

#[test]
fn test_requires_api_key() {
    let config = make_config(None);
    assert!(AnthropicProvider::new(config, "claude-sonnet-4-6".to_string()).is_err());
}

#[test]
fn test_accepts_valid_api_key() {
    let config = make_config(Some("test-key"));
    assert!(AnthropicProvider::new(config, "claude-sonnet-4-6".to_string()).is_ok());
}

#[test]
fn test_rejects_empty_api_key() {
    let config = make_config(Some(""));
    assert!(AnthropicProvider::new(config, "claude-sonnet-4-6".to_string()).is_err());
}

#[test]
fn test_provider_name() {
    let config = make_config(Some("test-key"));
    let provider = AnthropicProvider::new(config, "claude-sonnet-4-6".to_string()).unwrap();
    // Use provider trait to avoid ambiguity with unified trait
    assert_eq!(
        <AnthropicProvider as LLMProvider>::name(&provider),
        "anthropic"
    );
}

#[test]
fn test_anthropic_content_deserializes_citations() {
    // Simulate an Anthropic response with citations in a text block
    let json = r#"{
        "type": "text",
        "text": "According to the docs, Rust is safe.",
        "citations": [
            {
                "type": "web_search_result_location",
                "cited_text": "Rust is memory safe",
                "url": "https://doc.rust-lang.org/book/ch01-01.html",
                "title": "The Rust Programming Language",
                "search_result_index": 0
            }
        ]
    }"#;

    let content: AnthropicContent = serde_json::from_str(json).unwrap();
    assert_eq!(content.content_type, "text");
    assert_eq!(content.text, "According to the docs, Rust is safe.");
    assert!(content.citations.is_some());

    let citations = content.citations.unwrap();
    assert_eq!(citations.len(), 1);
    assert_eq!(citations[0].citation_type, "web_search_result_location");
    assert_eq!(citations[0].cited_text, "Rust is memory safe");
    assert_eq!(
        citations[0].url,
        "https://doc.rust-lang.org/book/ch01-01.html"
    );
    assert_eq!(citations[0].title, "The Rust Programming Language");
    assert_eq!(citations[0].search_result_index, Some(0));
}

#[test]
fn test_anthropic_content_no_citations() {
    let json = r#"{
        "type": "text",
        "text": "Hello world"
    }"#;

    let content: AnthropicContent = serde_json::from_str(json).unwrap();
    assert_eq!(content.text, "Hello world");
    assert!(content.citations.is_none());
}

#[test]
fn test_anthropic_metadata_has_claude4_models() {
    let metadata = AnthropicProvider::metadata();
    let model_ids: Vec<&str> = metadata
        .recommended_models
        .iter()
        .map(|m| m.model_id.as_str())
        .collect();

    // Should have Claude 4.x models
    assert!(
        model_ids
            .iter()
            .any(|id| id.contains("sonnet-4") || id.contains("opus-4")),
        "recommended_models should include Claude 4.x, got: {:?}",
        model_ids
    );
    // Should NOT have Claude 3.x models
    assert!(
        !model_ids.iter().any(|id| id.starts_with("claude-3-")),
        "recommended_models should not include Claude 3.x, got: {:?}",
        model_ids
    );
}

#[test]
fn test_map_anthropic_error_404_model_not_found() {
    let status = reqwest::StatusCode::from_u16(404).unwrap();
    let headers = reqwest::header::HeaderMap::new();
    let error = map_anthropic_error(status, "model does not exist", &headers);
    match error {
        ProviderError::InvalidModel(msg) => {
            assert!(msg.contains("model does not exist"));
        }
        other => panic!("expected InvalidModel, got {:?}", other),
    }
}

#[test]
fn test_map_anthropic_error_502_service_unavailable() {
    let status = reqwest::StatusCode::from_u16(502).unwrap();
    let headers = reqwest::header::HeaderMap::new();
    let error = map_anthropic_error(status, "bad gateway", &headers);
    match error {
        ProviderError::Network(msg) => {
            assert!(msg.contains("service unavailable"));
            assert!(msg.contains("bad gateway"));
        }
        other => panic!("expected Network, got {:?}", other),
    }
}

#[test]
fn test_map_anthropic_error_503_service_unavailable() {
    let status = reqwest::StatusCode::from_u16(503).unwrap();
    let headers = reqwest::header::HeaderMap::new();
    let error = map_anthropic_error(status, "service overloaded", &headers);
    match error {
        ProviderError::Network(msg) => {
            assert!(msg.contains("service unavailable"));
        }
        other => panic!("expected Network, got {:?}", other),
    }
}

#[test]
fn test_map_anthropic_error_401_auth() {
    let status = reqwest::StatusCode::from_u16(401).unwrap();
    let headers = reqwest::header::HeaderMap::new();
    let error = map_anthropic_error(status, "invalid key", &headers);
    assert!(matches!(error, ProviderError::Auth(_)));
}

#[test]
fn test_map_anthropic_error_429_rate_limited() {
    let status = reqwest::StatusCode::from_u16(429).unwrap();
    let headers = reqwest::header::HeaderMap::new();
    let error = map_anthropic_error(status, "slow down", &headers);
    assert!(matches!(
        error,
        ProviderError::RateLimited { retry_delay: None }
    ));
}

#[test]
fn test_map_anthropic_structured_error_not_found() {
    let status = reqwest::StatusCode::from_u16(404).unwrap();
    let headers = reqwest::header::HeaderMap::new();
    let error = map_anthropic_structured_error(
        status,
        "not_found_error",
        "model: foo-bar does not exist",
        None,
        &headers,
    );
    match error {
        ProviderError::InvalidModel(msg) => {
            assert!(msg.contains("not_found_error"));
            assert!(msg.contains("foo-bar"));
        }
        other => panic!("expected InvalidModel, got {:?}", other),
    }
}

#[test]
fn test_map_anthropic_structured_error_overloaded() {
    let status = reqwest::StatusCode::from_u16(529).unwrap();
    let headers = reqwest::header::HeaderMap::new();
    let error = map_anthropic_structured_error(
        status,
        "overloaded_error",
        "Anthropic is overloaded",
        None,
        &headers,
    );
    match error {
        ProviderError::Network(msg) => {
            assert!(msg.contains("overloaded"));
        }
        other => panic!("expected Network, got {:?}", other),
    }
}

#[test]
fn test_map_anthropic_structured_error_with_param() {
    let status = reqwest::StatusCode::from_u16(400).unwrap();
    let headers = reqwest::header::HeaderMap::new();
    let error = map_anthropic_structured_error(
        status,
        "invalid_request_error",
        "max_tokens must be positive",
        Some("max_tokens"),
        &headers,
    );
    match error {
        ProviderError::Api(msg) => {
            assert!(msg.contains("parameter: max_tokens"));
        }
        other => panic!("expected Api, got {:?}", other),
    }
}

#[test]
fn test_anthropic_metadata_display_name() {
    let metadata = AnthropicProvider::metadata();
    assert_eq!(metadata.display_name, "Anthropic");
    assert_eq!(metadata.provider_id, "anthropic");
}

#[test]
fn test_anthropic_metadata_tool_calling_supported() {
    let metadata = AnthropicProvider::metadata();
    assert!(metadata.tool_calling.supported);
    assert!(metadata.tool_calling.streaming_support);
}

#[test]
fn test_anthropic_endpoint_default() {
    let config = make_config(Some("test-key"));
    let provider = AnthropicProvider::new(config, "claude-sonnet-4-6".to_string()).unwrap();
    assert_eq!(provider.endpoint(), "https://api.anthropic.com/v1/messages");
}

#[test]
fn test_anthropic_endpoint_custom_base_url() {
    let mut config = make_config(Some("test-key"));
    config.base_url = Some("https://my-proxy.example.com".to_string());
    let provider = AnthropicProvider::new(config, "claude-sonnet-4-6".to_string()).unwrap();
    assert_eq!(
        provider.endpoint(),
        "https://my-proxy.example.com/v1/messages"
    );
}

#[test]
fn test_anthropic_usage_deserialization() {
    let json = r#"{
        "input_tokens": 100,
        "output_tokens": 50,
        "cache_read_input_tokens": 30,
        "cache_creation_input_tokens": 10
    }"#;
    let usage: AnthropicUsage = serde_json::from_str(json).unwrap();
    assert_eq!(usage.input_tokens, 100);
    assert_eq!(usage.output_tokens, 50);
    assert_eq!(usage.cache_read_input_tokens, 30);
    assert_eq!(usage.cache_creation_input_tokens, 10);
}

#[test]
fn test_anthropic_usage_defaults_cache_tokens() {
    let json = r#"{
        "input_tokens": 100,
        "output_tokens": 50
    }"#;
    let usage: AnthropicUsage = serde_json::from_str(json).unwrap();
    assert_eq!(usage.cache_read_input_tokens, 0);
    assert_eq!(usage.cache_creation_input_tokens, 0);
}

#[test]
fn test_anthropic_response_deserialization() {
    let json = r#"{
        "content": [
            {"type": "text", "text": "Hello!", "id": "", "name": "", "input": null}
        ],
        "usage": {"input_tokens": 10, "output_tokens": 5, "cache_read_input_tokens": 0, "cache_creation_input_tokens": 0},
        "model": "claude-sonnet-4-20250514"
    }"#;
    let response: AnthropicResponse = serde_json::from_str(json).unwrap();
    assert_eq!(response.model, "claude-sonnet-4-20250514");
    assert_eq!(response.content.len(), 1);
    assert_eq!(response.content[0].text, "Hello!");
}

#[test]
fn test_anthropic_request_serialization() {
    let request = AnthropicRequest {
        model: "claude-sonnet-4-6".to_string(),
        messages: vec![],
        max_tokens: 4096,
        temperature: 0.7,
        system: Some(SystemPrompt::Blocks(vec![SystemContentBlock {
            block_type: "text",
            text: "You are helpful".to_string(),
            cache_control: Some(CacheControl {
                cache_type: "ephemeral",
            }),
        }])),
        stream: Some(false),
        tools: None,
        thinking: None,
        output_config: None,
        container: None,
        tool_choice: None,
        parallel_tool_calls: None,
    };
    let json = serde_json::to_string(&request).unwrap();
    assert!(json.contains("\"model\":\"claude-sonnet-4-6\""));
    assert!(json.contains("\"max_tokens\":4096"));
    assert!(json.contains("\"temperature\":0.7"));
    // stream: false should still be serialized since skip_serializing_if is only for None
    // tools and effort should be absent since they are None
    assert!(!json.contains("\"tools\""));
}

#[test]
fn test_cache_control_on_system_and_tools() {
    // System prompt should be serialized as content block array with cache_control
    let request = AnthropicRequest {
        model: "claude-sonnet-4-6".to_string(),
        messages: vec![],
        max_tokens: 4096,
        temperature: 0.7,
        system: Some(SystemPrompt::Blocks(vec![SystemContentBlock {
            block_type: "text",
            text: "You are a helpful assistant".to_string(),
            cache_control: Some(CacheControl {
                cache_type: "ephemeral",
            }),
        }])),
        stream: None,
        tools: Some(vec![
            serde_json::json!({
                "name": "get_weather",
                "description": "Get weather",
                "input_schema": {"type": "object", "properties": {}}
            }),
            serde_json::json!({
                "name": "Read",
                "description": "Read a file",
                "input_schema": {"type": "object", "properties": {}},
                "cache_control": {"type": "ephemeral"}
            }),
        ]),
        thinking: None,
        output_config: None,
        container: None,
        tool_choice: None,
        parallel_tool_calls: None,
    };
    let json = serde_json::to_string(&request).unwrap();

    // System should be array with cache_control on the block
    assert!(json.contains("\"system\":[{"), "system should be an array");
    assert!(
        json.contains("\"cache_control\":{\"type\":\"ephemeral\"}"),
        "system block should have cache_control"
    );

    // No top-level cache_control
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert!(
        parsed.get("cache_control").is_none(),
        "no top-level cache_control"
    );
}

// -- Protocol-level message roundtrip tests --
//
// Input messages use rustycode_protocol types (ProtoBlock, MessageContent).
// The output uses Anthropic's own ContentBlock (super::ContentBlock).

use rustycode_protocol::{
    ContentBlock as ProtoBlock, ImageSource as ProtoImageSource, MessageContent,
};

fn make_anthropic_provider() -> AnthropicProvider {
    AnthropicProvider::new_without_validation(
        make_config(Some("test-key")),
        "claude-sonnet-4-6".to_string(),
    )
    .unwrap()
}

/// Helper: extract text from AnthropicRequestContent
fn extract_anthropic_text(content: &AnthropicRequestContent) -> Option<String> {
    match content {
        AnthropicRequestContent::Text(t) => Some(t.clone()),
        AnthropicRequestContent::Blocks(blocks) => {
            let texts: Vec<&str> = blocks
                .iter()
                .filter_map(|b| match b {
                    ContentBlock::Text { text, .. } => Some(text.as_str()),
                    _ => None,
                })
                .collect();
            if texts.is_empty() {
                None
            } else {
                Some(texts.join("\n"))
            }
        }
    }
}

#[test]
fn test_roundtrip_simple_text_user_message() {
    let provider = make_anthropic_provider();
    let msgs = vec![ChatMessage::user("Hello, world!")];
    let result = provider.parse_conversation_messages(&msgs);
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].role, "user");
    assert_eq!(
        extract_anthropic_text(&result[0].content).unwrap(),
        "Hello, world!"
    );
}

#[test]
fn test_roundtrip_simple_text_assistant_message() {
    let provider = make_anthropic_provider();
    let msgs = vec![ChatMessage::assistant("Hi there!")];
    let result = provider.parse_conversation_messages(&msgs);
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].role, "assistant");
    assert_eq!(
        extract_anthropic_text(&result[0].content).unwrap(),
        "Hi there!"
    );
}

#[test]
fn test_roundtrip_system_role_maps_to_user() {
    let provider = make_anthropic_provider();
    let msgs = vec![ChatMessage {
        role: MessageRole::System,
        content: MessageContent::simple("System prompt"),
    }];
    let result = provider.parse_conversation_messages(&msgs);
    assert_eq!(result.len(), 1);
    // Anthropic maps System -> "user"
    assert_eq!(result[0].role, "user");
}

#[test]
fn test_roundtrip_text_block() {
    let provider = make_anthropic_provider();
    let msgs = vec![ChatMessage {
        role: MessageRole::User,
        content: MessageContent::Blocks(vec![ProtoBlock::text("Block content")]),
    }];
    let result = provider.parse_conversation_messages(&msgs);
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].role, "user");
    let text = extract_anthropic_text(&result[0].content).unwrap();
    assert_eq!(text, "Block content");
}

#[test]
fn test_roundtrip_tool_use_block_in_assistant_role() {
    let provider = make_anthropic_provider();
    let msgs = vec![ChatMessage {
        role: MessageRole::Assistant,
        content: MessageContent::Blocks(vec![ProtoBlock::tool_use(
            "tu_1",
            "Read",
            serde_json::json!({"path": "main.rs"}),
        )]),
    }];
    let result = provider.parse_conversation_messages(&msgs);
    assert_eq!(result.len(), 1);
    // ToolUse stays in assistant role
    assert_eq!(result[0].role, "assistant");
    match &result[0].content {
        AnthropicRequestContent::Blocks(blocks) => {
            assert_eq!(blocks.len(), 1);
            match &blocks[0] {
                ContentBlock::ToolUse {
                    id, name, input, ..
                } => {
                    assert_eq!(id, "tu_1");
                    assert_eq!(name, "Read");
                    assert_eq!(input["path"], "main.rs");
                }
                other => panic!("expected ToolUse block, got {:?}", other),
            }
        }
        other => panic!("expected Blocks, got {:?}", other),
    }
}

#[test]
fn test_roundtrip_tool_result_block_forced_user_role() {
    let provider = make_anthropic_provider();
    let msgs = vec![ChatMessage {
        role: MessageRole::User,
        content: MessageContent::Blocks(vec![ProtoBlock::tool_result("tu_1", "file content here")]),
    }];
    let result = provider.parse_conversation_messages(&msgs);
    assert_eq!(result.len(), 1);
    // ToolResult forces role to "user" in Anthropic API
    assert_eq!(result[0].role, "user");
    match &result[0].content {
        AnthropicRequestContent::Blocks(blocks) => {
            assert_eq!(blocks.len(), 1);
            match &blocks[0] {
                ContentBlock::ToolResult {
                    tool_use_id,
                    content,
                    ..
                } => {
                    assert_eq!(tool_use_id, "tu_1");
                    assert_eq!(content, "file content here");
                }
                other => panic!("expected ToolResult block, got {:?}", other),
            }
        }
        other => panic!("expected Blocks, got {:?}", other),
    }
}

#[test]
fn test_roundtrip_tool_error_block_sets_is_error() {
    let provider = make_anthropic_provider();
    let msgs = vec![ChatMessage {
        role: MessageRole::User,
        content: MessageContent::Blocks(vec![ProtoBlock::tool_error(
            "tu_err",
            "command failed: No such file",
        )]),
    }];
    let result = provider.parse_conversation_messages(&msgs);
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].role, "user");
    match &result[0].content {
        AnthropicRequestContent::Blocks(blocks) => {
            assert_eq!(blocks.len(), 1);
            match &blocks[0] {
                ContentBlock::ToolResult {
                    tool_use_id,
                    content,
                    is_error,
                    ..
                } => {
                    assert_eq!(tool_use_id, "tu_err");
                    assert_eq!(content, "command failed: No such file");
                    assert_eq!(*is_error, Some(true));
                }
                other => panic!("expected ToolResult block, got {:?}", other),
            }
        }
        other => panic!("expected Blocks, got {:?}", other),
    }
}

#[test]
fn test_roundtrip_tool_result_no_error_omits_flag() {
    let provider = make_anthropic_provider();
    let msgs = vec![ChatMessage {
        role: MessageRole::User,
        content: MessageContent::Blocks(vec![ProtoBlock::tool_result("tu_ok", "success output")]),
    }];
    let result = provider.parse_conversation_messages(&msgs);
    assert_eq!(result.len(), 1);
    match &result[0].content {
        AnthropicRequestContent::Blocks(blocks) => {
            assert_eq!(blocks.len(), 1);
            match &blocks[0] {
                ContentBlock::ToolResult { is_error, .. } => {
                    assert_eq!(*is_error, None);
                }
                other => panic!("expected ToolResult block, got {:?}", other),
            }
        }
        other => panic!("expected Blocks, got {:?}", other),
    }
}

#[test]
fn test_roundtrip_image_block_is_preserved() {
    // Image blocks are now properly converted to Anthropic image blocks
    let provider = make_anthropic_provider();
    let msgs = vec![ChatMessage {
        role: MessageRole::User,
        content: MessageContent::Blocks(vec![ProtoBlock::image(ProtoImageSource::base64(
            "image/png",
            "iVBOR",
        ))]),
    }];
    let result = provider.parse_conversation_messages(&msgs);
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].role, "user");
    match &result[0].content {
        AnthropicRequestContent::Blocks(blocks) => {
            assert_eq!(blocks.len(), 1);
            match &blocks[0] {
                ContentBlock::Image { source, .. } => {
                    assert_eq!(source.source_type, "base64");
                    assert_eq!(source.media_type, "image/png");
                    assert_eq!(source.data, "iVBOR");
                }
                other => panic!("expected Image block, got {:?}", other),
            }
        }
        other => panic!("expected Blocks, got {:?}", other),
    }
}

#[test]
fn test_roundtrip_thinking_block_preserved() {
    // Thinking blocks are preserved as thinking blocks (with signature) for multi-turn
    let provider = make_anthropic_provider();
    let msgs = vec![ChatMessage {
        role: MessageRole::Assistant,
        content: MessageContent::Blocks(vec![ProtoBlock::thinking("deep thought", "sig123")]),
    }];
    let result = provider.parse_conversation_messages(&msgs);
    assert_eq!(result.len(), 1);
    match &result[0].content {
        AnthropicRequestContent::Blocks(blocks) => {
            assert_eq!(blocks.len(), 1);
            // Thinking block is preserved with thinking content and signature
            let think_json = serde_json::to_value(&blocks[0]).unwrap();
            assert_eq!(think_json["type"], "thinking");
            assert_eq!(think_json["thinking"], "deep thought");
            assert_eq!(think_json["signature"], "sig123");
        }
        other => panic!("expected Blocks, got {:?}", other),
    }
}

#[test]
fn test_roundtrip_mixed_text_and_tool_use() {
    let provider = make_anthropic_provider();
    let msgs = vec![ChatMessage {
        role: MessageRole::Assistant,
        content: MessageContent::Blocks(vec![
            ProtoBlock::text("I'll read that file."),
            ProtoBlock::tool_use("call_x", "Read", serde_json::json!({"path": "x.rs"})),
        ]),
    }];
    let result = provider.parse_conversation_messages(&msgs);
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].role, "assistant");
    match &result[0].content {
        AnthropicRequestContent::Blocks(blocks) => {
            assert_eq!(blocks.len(), 2);
            assert!(
                matches!(&blocks[0], super::ContentBlock::Text { text, .. } if text == "I'll read that file.")
            );
            assert!(
                matches!(&blocks[1], super::ContentBlock::ToolUse { id, name, .. } if id == "call_x" && name == "Read")
            );
        }
        other => panic!("expected Blocks, got {:?}", other),
    }
}

#[test]
fn test_roundtrip_mixed_text_and_tool_result() {
    let provider = make_anthropic_provider();
    let msgs = vec![ChatMessage {
        role: MessageRole::User,
        content: MessageContent::Blocks(vec![
            ProtoBlock::text("Here is the result:"),
            ProtoBlock::tool_result("call_1", "output data"),
        ]),
    }];
    let result = provider.parse_conversation_messages(&msgs);
    assert_eq!(result.len(), 1);
    // ToolResult forces role to "user"
    assert_eq!(result[0].role, "user");
    match &result[0].content {
        AnthropicRequestContent::Blocks(blocks) => {
            assert_eq!(blocks.len(), 2);
            assert!(
                matches!(&blocks[0], super::ContentBlock::Text { text, .. } if text == "Here is the result:")
            );
            assert!(
                matches!(&blocks[1], super::ContentBlock::ToolResult { tool_use_id, .. } if tool_use_id == "call_1")
            );
        }
        other => panic!("expected Blocks, got {:?}", other),
    }
}

#[test]
fn test_roundtrip_tool_role_maps_to_user() {
    let provider = make_anthropic_provider();
    let msgs = vec![ChatMessage {
        role: MessageRole::Tool("call_id".to_string()),
        content: MessageContent::simple("Tool output"),
    }];
    let result = provider.parse_conversation_messages(&msgs);
    assert_eq!(result.len(), 1);
    // Tool role maps to "user" in Anthropic
    assert_eq!(result[0].role, "user");
}

#[test]
fn test_json_tool_result_with_error_sets_is_error() {
    let provider = make_anthropic_provider();
    let msgs = vec![ChatMessage::tool_result_with_error(
        "command not found".to_string(),
        "tu_json_err".to_string(),
        true,
    )];
    let result = provider.parse_conversation_messages(&msgs);
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].role, "user");
    match &result[0].content {
        AnthropicRequestContent::Blocks(blocks) => {
            assert_eq!(blocks.len(), 1);
            match &blocks[0] {
                ContentBlock::ToolResult {
                    tool_use_id,
                    content,
                    is_error,
                    ..
                } => {
                    assert_eq!(tool_use_id, "tu_json_err");
                    assert_eq!(content, "command not found");
                    assert_eq!(*is_error, Some(true));
                }
                other => panic!("expected ToolResult block, got {:?}", other),
            }
        }
        other => panic!("expected Blocks, got {:?}", other),
    }
}

#[test]
fn test_roundtrip_empty_message() {
    let provider = make_anthropic_provider();
    let msgs = vec![ChatMessage::user("")];
    let result = provider.parse_conversation_messages(&msgs);
    assert_eq!(result.len(), 1);
    // parse_conversation_messages faithfully converts empty content
    // (complete_internal later filters it out before sending)
    let text = extract_anthropic_text(&result[0].content).unwrap();
    assert_eq!(text, "");
}

#[test]
fn test_roundtrip_whitespace_only_message() {
    let provider = make_anthropic_provider();
    let msgs = vec![ChatMessage::user("   \n\t  ")];
    let result = provider.parse_conversation_messages(&msgs);
    assert_eq!(result.len(), 1);
    let text = extract_anthropic_text(&result[0].content).unwrap();
    assert_eq!(text, "   \n\t  ");
}

#[test]
fn test_roundtrip_very_long_text_content() {
    let provider = make_anthropic_provider();
    let long_text = "B".repeat(12_000);
    let msgs = vec![ChatMessage::user(long_text.clone())];
    let result = provider.parse_conversation_messages(&msgs);
    assert_eq!(result.len(), 1);
    let text = extract_anthropic_text(&result[0].content).unwrap();
    assert_eq!(text, long_text);
}

// -- Stop reason and refusal handling tests --

#[test]
fn test_anthropic_response_stop_reason_tool_use() {
    let json = r#"{
        "content": [
            {"type": "tool_use", "text": "", "id": "tu_1", "name": "Read", "input": {"path": "main.rs"}}
        ],
        "usage": {"input_tokens": 10, "output_tokens": 5, "cache_read_input_tokens": 0, "cache_creation_input_tokens": 0},
        "model": "claude-sonnet-4-6",
        "stop_reason": "tool_use"
    }"#;
    let response: AnthropicResponse = serde_json::from_str(json).unwrap();
    assert_eq!(response.stop_reason.as_deref(), Some("tool_use"));
}

#[test]
fn test_anthropic_response_stop_reason_refusal() {
    let json = r#"{
        "content": [
            {"type": "text", "text": "I cannot", "id": "", "name": "", "input": null}
        ],
        "usage": {"input_tokens": 10, "output_tokens": 5, "cache_read_input_tokens": 0, "cache_creation_input_tokens": 0},
        "model": "claude-sonnet-4-6",
        "stop_reason": "refusal"
    }"#;
    let response: AnthropicResponse = serde_json::from_str(json).unwrap();
    assert_eq!(response.stop_reason.as_deref(), Some("refusal"));
}

#[test]
fn test_anthropic_response_stop_reason_max_tokens() {
    let json = r#"{
        "content": [
            {"type": "text", "text": "The answer is...", "id": "", "name": "", "input": null}
        ],
        "usage": {"input_tokens": 10, "output_tokens": 4096, "cache_read_input_tokens": 0, "cache_creation_input_tokens": 0},
        "model": "claude-sonnet-4-6",
        "stop_reason": "max_tokens"
    }"#;
    let response: AnthropicResponse = serde_json::from_str(json).unwrap();
    assert_eq!(response.stop_reason.as_deref(), Some("max_tokens"));
}

#[test]
fn test_anthropic_response_stop_reason_end_turn() {
    let json = r#"{
        "content": [
            {"type": "text", "text": "Done!", "id": "", "name": "", "input": null}
        ],
        "usage": {"input_tokens": 10, "output_tokens": 5, "cache_read_input_tokens": 0, "cache_creation_input_tokens": 0},
        "model": "claude-sonnet-4-6",
        "stop_reason": "end_turn"
    }"#;
    let response: AnthropicResponse = serde_json::from_str(json).unwrap();
    assert_eq!(response.stop_reason.as_deref(), Some("end_turn"));
}

#[test]
fn test_anthropic_request_serializes_output_config() {
    let request = AnthropicRequest {
        model: "claude-sonnet-4-6".to_string(),
        messages: vec![],
        max_tokens: 4096,
        temperature: 0.7,
        system: None,
        stream: Some(false),
        tools: None,
        thinking: None,
        output_config: Some(crate::provider::OutputConfig::with_effort(
            crate::provider::EffortLevel::High,
        )),
        container: None,
        tool_choice: None,
        parallel_tool_calls: None,
    };
    let json = serde_json::to_string(&request).unwrap();
    assert!(json.contains("\"output_config\""));
    assert!(json.contains("\"effort\":\"high\""));
}

#[test]
fn test_anthropic_request_serializes_json_schema_output() {
    let schema = serde_json::json!({
        "type": "object",
        "properties": {
            "answer": {"type": "string"}
        },
        "required": ["answer"]
    });
    let request = AnthropicRequest {
        model: "claude-sonnet-4-6".to_string(),
        messages: vec![],
        max_tokens: 4096,
        temperature: 0.7,
        system: None,
        stream: Some(false),
        tools: None,
        thinking: None,
        output_config: Some(crate::provider::OutputConfig::with_json_schema(schema)),
        container: None,
        tool_choice: None,
        parallel_tool_calls: None,
    };
    let json = serde_json::to_string(&request).unwrap();
    assert!(json.contains("\"output_config\""));
    assert!(json.contains("\"schema\""));
}

#[test]
fn test_anthropic_content_thinking_block_deserialization() {
    let json = r#"{
        "type": "thinking",
        "thinking": "Let me analyze this step by step...",
        "signature": "WaUjzkypQ2mUEVM36O2TxuC06KN8xyfbJwyem2dw3URve..."
    }"#;
    let block: AnthropicContent = serde_json::from_str(json).unwrap();
    assert_eq!(block.content_type, "thinking");
    assert_eq!(block.thinking, "Let me analyze this step by step...");
    assert_eq!(
        block.signature,
        "WaUjzkypQ2mUEVM36O2TxuC06KN8xyfbJwyem2dw3URve..."
    );
}

#[test]
fn test_anthropic_content_redacted_thinking_block_deserialization() {
    let json = r#"{
        "type": "redacted_thinking",
        "data": "ErwDkUYICxIMMb3LzNrMu..."
    }"#;
    let block: AnthropicContent = serde_json::from_str(json).unwrap();
    assert_eq!(block.content_type, "redacted_thinking");
    assert_eq!(block.data, "ErwDkUYICxIMMb3LzNrMu...");
}

#[test]
fn test_thinking_block_roundtrip_preservation() {
    let thinking = ThinkingBlock {
        block_type: "thinking".to_string(),
        thinking: "I need to think about this...".to_string(),
        signature: "sig_abc123".to_string(),
        data: String::new(),
        display: None,
    };
    let json = serde_json::to_string(&thinking).unwrap();
    let back: ThinkingBlock = serde_json::from_str(&json).unwrap();
    assert_eq!(back.block_type, "thinking");
    assert_eq!(back.thinking, "I need to think about this...");
    assert_eq!(back.signature, "sig_abc123");
}

#[test]
fn test_completion_response_with_thinking_blocks() {
    let response = CompletionResponse {
        content: "The answer is 42.".to_string(),
        model: "claude-sonnet-4-6".to_string(),
        usage: None,
        stop_reason: Some("end_turn".to_string()),
        citations: None,
        thinking_blocks: Some(vec![ThinkingBlock {
            block_type: "thinking".to_string(),
            thinking: "Deep analysis...".to_string(),
            signature: "sig_xyz".to_string(),
            data: String::new(),
            display: None,
        }]),
        structured_output: None,
    };
    let json = serde_json::to_string(&response).unwrap();
    assert!(json.contains("thinking_blocks"));
    let back: CompletionResponse = serde_json::from_str(&json).unwrap();
    assert!(back.thinking_blocks.is_some());
    let blocks = back.thinking_blocks.unwrap();
    assert_eq!(blocks.len(), 1);
    assert_eq!(blocks[0].block_type, "thinking");
}

#[test]
fn test_completion_response_with_structured_output() {
    let response = CompletionResponse {
        content: r#"{"answer": "42"}"#.to_string(),
        model: "claude-sonnet-4-6".to_string(),
        usage: None,
        stop_reason: Some("end_turn".to_string()),
        citations: None,
        thinking_blocks: None,
        structured_output: Some(serde_json::json!({"answer": "42"})),
    };
    assert!(response.structured_output.is_some());
    let output = response.structured_output.unwrap();
    assert_eq!(output["answer"], "42");
}

#[test]
fn test_completion_request_with_json_schema() {
    let schema = serde_json::json!({
        "type": "object",
        "properties": {
            "items": {
                "type": "array",
                "items": { "type": "string" }
            }
        },
        "required": ["items"]
    });
    let request = crate::provider::CompletionRequest::new("claude-sonnet-4-6".to_string(), vec![])
        .with_json_schema(schema.clone());
    assert!(request.output_config.is_some());
    let oc = request.output_config.unwrap();
    assert!(oc.format.is_some());
    let fmt = oc.format.unwrap();
    assert_eq!(
        fmt.format_type,
        crate::provider::OutputFormatType::JsonSchema
    );
    assert_eq!(fmt.json_schema.unwrap(), schema);
}

#[test]
fn test_normalize_thinking_downgrades_enabled_for_opus_47() {
    let config = crate::provider::ThinkingConfig::enabled(10000);
    let value = normalize_thinking_for_model(Some(config), None, "claude-opus-4-7");
    let value = value.expect("should return Some");
    assert_eq!(value["type"], "adaptive");
    assert_eq!(value["budget_tokens"], 10000);
}

#[test]
fn test_normalize_thinking_keeps_enabled_for_older_models() {
    let config = crate::provider::ThinkingConfig::enabled(10000);
    let value = normalize_thinking_for_model(Some(config), None, "claude-opus-4-6");
    let value = value.expect("should return Some");
    assert_eq!(value["type"], "enabled");
}

#[test]
fn test_normalize_thinking_handles_date_suffixed_model() {
    let config = crate::provider::ThinkingConfig::enabled(10000);
    let value = normalize_thinking_for_model(Some(config), None, "claude-opus-4-7-20250515");
    let value = value.expect("should return Some");
    assert_eq!(value["type"], "adaptive");
}

#[test]
fn test_normalize_thinking_adaptive_unchanged() {
    let config = crate::provider::ThinkingConfig::adaptive();
    let value = normalize_thinking_for_model(Some(config), None, "claude-opus-4-7");
    let value = value.expect("should return Some");
    assert_eq!(value["type"], "adaptive");
}

#[test]
fn test_is_opus_47_or_later() {
    assert!(is_opus_47_or_later("claude-opus-4-7"));
    assert!(is_opus_47_or_later("claude-opus-4-7-20250515"));
    assert!(is_opus_47_or_later("claude-opus-4.7-20250515"));
    assert!(is_opus_47_or_later("claude-opus-5"));
    assert!(!is_opus_47_or_later("claude-opus-4-6"));
    assert!(!is_opus_47_or_later("claude-sonnet-4-6"));
}

#[test]
fn test_tool_use_id_preserved_in_serialized_content() {
    // Simulate an API response with a tool_use block containing an id
    let json = r#"{
        "content": [
            {"type": "text", "text": "I'll run a command.", "id": "", "name": "", "input": null},
            {"type": "tool_use", "text": "", "id": "call_abc123def456", "name": "Bash", "input": {"command": "ls /app/"}}
        ],
        "usage": {"input_tokens": 10, "output_tokens": 5, "cache_read_input_tokens": 0, "cache_creation_input_tokens": 0},
        "model": "glm-5.1",
        "stop_reason": "tool_use"
    }"#;
    let response: AnthropicResponse = serde_json::from_str(json).unwrap();

    // Replicate the serialization logic from complete_internal
    let mut tool_calls: Vec<serde_json::Value> = Vec::new();
    for block in &response.content {
        if block.content_type == "tool_use" {
            let tool_call = serde_json::json!({
                "id": block.id,
                "name": block.name,
                "arguments": block.input
            });
            tool_calls.push(tool_call);
        }
    }

    assert_eq!(tool_calls.len(), 1);
    assert_eq!(tool_calls[0]["id"], "call_abc123def456");
    assert_eq!(tool_calls[0]["name"], "Bash");

    // Verify the serialized JSON contains the id for downstream consumers
    let serialized = serde_json::to_string(&tool_calls).unwrap();
    assert!(
        serialized.contains("call_abc123def456"),
        "id must survive serialization"
    );
}
