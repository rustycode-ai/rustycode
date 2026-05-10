//! Cross-provider message format consistency tests.
//!
//! Validates that Anthropic, OpenAI, and Ollama providers all correctly handle
//! the same protocol-level ContentBlock types and produce the expected
//! provider-specific wire formats.

#![allow(clippy::cloned_ref_to_slice_refs)] // &[msg.clone()] is clearer than std::slice::from_ref in tests

use crate::anthropic::{AnthropicProvider, AnthropicRequestContent};
use crate::ollama::OllamaProvider;
use crate::openai::OpenAiProvider;
use crate::provider::{ChatMessage, MessageRole};
use rustycode_protocol::{ContentBlock, ImageSource, MessageContent};
use secrecy::SecretString;

// ── Helpers ──────────────────────────────────────────────────────────────────

fn anthropic_config() -> crate::provider::ProviderConfig {
    crate::provider::ProviderConfig {
        api_key: Some(SecretString::new("sk-ant-testkey".into())),
        base_url: None,
        timeout_seconds: Some(30),
        extra_headers: None,
        retry_config: None,
    }
}

#[allow(dead_code)]
fn openai_config() -> crate::provider::ProviderConfig {
    crate::provider::ProviderConfig {
        api_key: Some(SecretString::new("sk-testkey".into())),
        base_url: None,
        timeout_seconds: Some(30),
        extra_headers: None,
        retry_config: None,
    }
}

#[allow(dead_code)]
fn ollama_config() -> crate::provider::ProviderConfig {
    crate::provider::ProviderConfig {
        api_key: None,
        base_url: Some("http://localhost:11434".to_string()),
        timeout_seconds: Some(30),
        extra_headers: None,
        retry_config: None,
    }
}

// ── 1. ContentBlock::Text ────────────────────────────────────────────────────

#[test]
fn cross_provider_text_block() {
    let msg = ChatMessage {
        role: MessageRole::User,
        content: MessageContent::Blocks(vec![ContentBlock::text("Hello, world!")]),
    };

    // Anthropic: text block inside user message
    let anthropic = AnthropicProvider::new(anthropic_config(), "claude-sonnet-4-6".into()).unwrap();
    let a_msgs = anthropic.parse_conversation_messages(&[msg.clone()]);
    assert_eq!(a_msgs.len(), 1);
    assert_eq!(a_msgs[0].role, "user");
    match &a_msgs[0].content {
        AnthropicRequestContent::Blocks(blocks) => {
            assert_eq!(blocks.len(), 1);
            let json = serde_json::to_value(&blocks[0]).unwrap();
            assert_eq!(json["type"], "text");
            assert_eq!(json["text"], "Hello, world!");
        }
        other => panic!("Anthropic: expected Blocks, got {:?}", other),
    }

    // OpenAI: text string content in user message
    let o_msgs = OpenAiProvider::convert_messages(&[msg.clone()]);
    assert_eq!(o_msgs.len(), 1);
    assert_eq!(o_msgs[0].role, "user");
    assert_eq!(
        o_msgs[0].content.as_ref().unwrap().as_str(),
        Some("Hello, world!")
    );
    assert!(o_msgs[0].tool_calls.is_none());

    // Ollama: flattened text
    let ol_msgs = OllamaProvider::convert_messages(vec![msg]);
    assert_eq!(ol_msgs.len(), 1);
    assert_eq!(ol_msgs[0].role, "user");
    assert_eq!(ol_msgs[0].content, "Hello, world!");
}

// ── 2. ContentBlock::ToolUse ────────────────────────────────────────────────

#[test]
fn cross_provider_tool_use_block() {
    let msg = ChatMessage {
        role: MessageRole::Assistant,
        content: MessageContent::Blocks(vec![
            ContentBlock::text("I'll read that file."),
            ContentBlock::tool_use(
                "toolu_01",
                "Read",
                serde_json::json!({"path": "src/main.rs"}),
            ),
        ]),
    };

    // Anthropic: tool_use block inside assistant message
    let anthropic = AnthropicProvider::new(anthropic_config(), "claude-sonnet-4-6".into()).unwrap();
    let a_msgs = anthropic.parse_conversation_messages(&[msg.clone()]);
    assert_eq!(a_msgs.len(), 1);
    assert_eq!(a_msgs[0].role, "assistant");
    match &a_msgs[0].content {
        AnthropicRequestContent::Blocks(blocks) => {
            assert_eq!(blocks.len(), 2);
            // Second block should be tool_use
            let json = serde_json::to_value(&blocks[1]).unwrap();
            assert_eq!(json["type"], "tool_use");
            assert_eq!(json["id"], "toolu_01");
            assert_eq!(json["name"], "Read");
        }
        other => panic!("Anthropic: expected Blocks, got {:?}", other),
    }

    // OpenAI: tool_calls array on assistant message
    let o_msgs = OpenAiProvider::convert_messages(&[msg.clone()]);
    assert_eq!(o_msgs.len(), 1);
    assert_eq!(o_msgs[0].role, "assistant");
    let tool_calls = o_msgs[0].tool_calls.as_ref().expect("expected tool_calls");
    assert_eq!(tool_calls.len(), 1);
    assert_eq!(tool_calls[0].id, "toolu_01");
    assert_eq!(tool_calls[0].function.name, "Read");

    // Ollama: flattened to text
    let ol_msgs = OllamaProvider::convert_messages(vec![msg]);
    assert_eq!(ol_msgs.len(), 1);
    assert_eq!(ol_msgs[0].role, "assistant");
    assert!(ol_msgs[0].content.contains("I'll read that file."));
    assert!(ol_msgs[0].content.contains("[Tool use: Read]"));
}

// ── 3. ContentBlock::ToolResult ──────────────────────────────────────────────

#[test]
fn cross_provider_tool_result_block() {
    let msg = ChatMessage {
        role: MessageRole::User,
        content: MessageContent::Blocks(vec![ContentBlock::tool_result(
            "toolu_01",
            "File contents here",
        )]),
    };

    // Anthropic: tool_result block in user role
    let anthropic = AnthropicProvider::new(anthropic_config(), "claude-sonnet-4-6".into()).unwrap();
    let a_msgs = anthropic.parse_conversation_messages(&[msg.clone()]);
    assert_eq!(a_msgs.len(), 1);
    assert_eq!(a_msgs[0].role, "user"); // Anthropic requires tool_result in user message
    match &a_msgs[0].content {
        AnthropicRequestContent::Blocks(blocks) => {
            assert_eq!(blocks.len(), 1);
            let json = serde_json::to_value(&blocks[0]).unwrap();
            assert_eq!(json["type"], "tool_result");
            assert_eq!(json["tool_use_id"], "toolu_01");
            assert_eq!(json["content"], "File contents here");
        }
        other => panic!("Anthropic: expected Blocks, got {:?}", other),
    }

    // OpenAI: role="tool" with tool_call_id
    let o_msgs = OpenAiProvider::convert_messages(&[msg.clone()]);
    assert_eq!(o_msgs.len(), 1);
    assert_eq!(o_msgs[0].role, "tool");
    assert_eq!(o_msgs[0].tool_call_id.as_deref(), Some("toolu_01"));
    assert_eq!(
        o_msgs[0].content.as_ref().unwrap().as_str(),
        Some("File contents here")
    );
    assert!(o_msgs[0].tool_calls.is_none());

    // Ollama: flattened text, tool results included inline
    let ol_msgs = OllamaProvider::convert_messages(vec![msg]);
    assert_eq!(ol_msgs.len(), 1);
    assert_eq!(ol_msgs[0].role, "user");
    assert_eq!(ol_msgs[0].content, "File contents here");
}

#[test]
fn cross_provider_tool_result_error_block() {
    let msg = ChatMessage {
        role: MessageRole::User,
        content: MessageContent::Blocks(vec![ContentBlock::tool_error(
            "toolu_02",
            "Permission denied",
        )]),
    };

    // Anthropic: error tool_result in user role
    let anthropic = AnthropicProvider::new(anthropic_config(), "claude-sonnet-4-6".into()).unwrap();
    let a_msgs = anthropic.parse_conversation_messages(&[msg.clone()]);
    assert_eq!(a_msgs.len(), 1);
    assert_eq!(a_msgs[0].role, "user");
    match &a_msgs[0].content {
        AnthropicRequestContent::Blocks(blocks) => {
            let json = serde_json::to_value(&blocks[0]).unwrap();
            assert_eq!(json["type"], "tool_result");
            assert_eq!(json["content"], "Permission denied");
        }
        other => panic!("Anthropic: expected Blocks, got {:?}", other),
    }

    // OpenAI: same tool role, error text prefixed with "Error: "
    let o_msgs = OpenAiProvider::convert_messages(&[msg.clone()]);
    assert_eq!(o_msgs.len(), 1);
    assert_eq!(o_msgs[0].role, "tool");
    assert_eq!(o_msgs[0].tool_call_id.as_deref(), Some("toolu_02"));
    assert_eq!(
        o_msgs[0].content.as_ref().unwrap().as_str(),
        Some("Error: Permission denied")
    );

    // Ollama: error text flattened
    let ol_msgs = OllamaProvider::convert_messages(vec![msg]);
    assert_eq!(ol_msgs.len(), 1);
    assert_eq!(ol_msgs[0].content, "Permission denied");
}

// ── 4. ContentBlock::Image ──────────────────────────────────────────────────

#[test]
fn cross_provider_image_block_url() {
    let msg = ChatMessage {
        role: MessageRole::User,
        content: MessageContent::Blocks(vec![
            ContentBlock::text("What's in this image?"),
            ContentBlock::image(ImageSource::url(
                "https://example.com/photo.jpg",
                "image/jpeg",
            )),
        ]),
    };

    // Anthropic: image block (currently falls to unsupported — documented issue)
    let anthropic = AnthropicProvider::new(anthropic_config(), "claude-sonnet-4-6".into()).unwrap();
    let a_msgs = anthropic.parse_conversation_messages(&[msg.clone()]);
    assert_eq!(a_msgs.len(), 1);
    assert_eq!(a_msgs[0].role, "user");
    match &a_msgs[0].content {
        AnthropicRequestContent::Blocks(blocks) => {
            assert_eq!(blocks.len(), 2);
            // First is text
            let text_json = serde_json::to_value(&blocks[0]).unwrap();
            assert_eq!(text_json["type"], "text");
            // Second block: Image is now properly converted to an Anthropic image block
            let img_json = serde_json::to_value(&blocks[1]).unwrap();
            assert_eq!(img_json["type"], "image");
            assert_eq!(img_json["source"]["type"], "url");
        }
        other => panic!("Anthropic: expected Blocks, got {:?}", other),
    }

    // OpenAI: image_url content part
    let o_msgs = OpenAiProvider::convert_messages(&[msg.clone()]);
    assert_eq!(o_msgs.len(), 1);
    assert_eq!(o_msgs[0].role, "user");
    // With 2 parts, it should be serialized as array
    let content = o_msgs[0].content.as_ref().unwrap();
    assert!(content.is_array());
    let parts = content.as_array().unwrap();
    assert_eq!(parts.len(), 2);
    assert_eq!(parts[0]["type"], "text");
    assert_eq!(parts[1]["type"], "image_url");
    assert_eq!(
        parts[1]["image_url"]["url"],
        "https://example.com/photo.jpg"
    );

    // Ollama: base64 images extracted, url images not extracted as images
    let ol_msgs = OllamaProvider::convert_messages(vec![msg]);
    assert_eq!(ol_msgs.len(), 1);
    assert_eq!(ol_msgs[0].role, "user");
    assert!(ol_msgs[0].content.contains("What's in this image?"));
    assert!(ol_msgs[0].content.contains("[Image]"));
    // URL source -> no images array (only base64 goes into images)
    assert!(ol_msgs[0].images.is_none());
}

#[test]
fn cross_provider_image_block_base64() {
    let msg = ChatMessage {
        role: MessageRole::User,
        content: MessageContent::Blocks(vec![ContentBlock::image(ImageSource::base64(
            "image/png",
            "iVBORw0KGgo=",
        ))]),
    };

    // OpenAI: base64 -> data URI
    let o_msgs = OpenAiProvider::convert_messages(&[msg.clone()]);
    assert_eq!(o_msgs.len(), 1);
    let content = o_msgs[0].content.as_ref().unwrap();
    let parts = content.as_array().unwrap();
    assert_eq!(parts[0]["type"], "image_url");
    assert_eq!(
        parts[0]["image_url"]["url"],
        "data:image/png;base64,iVBORw0KGgo="
    );

    // Ollama: base64 images go into the images array
    let ol_msgs = OllamaProvider::convert_messages(vec![msg]);
    assert_eq!(ol_msgs.len(), 1);
    assert!(ol_msgs[0].images.is_some());
    let imgs = ol_msgs[0].images.as_ref().unwrap();
    assert_eq!(imgs.len(), 1);
    assert_eq!(imgs[0], "iVBORw0KGgo=");
}

// ── 5. ContentBlock::Thinking ────────────────────────────────────────────────

#[test]
fn cross_provider_thinking_block() {
    let msg = ChatMessage {
        role: MessageRole::Assistant,
        content: MessageContent::Blocks(vec![
            ContentBlock::thinking("Let me reason about this...", "sig_abc123"),
            ContentBlock::text("Here is my answer."),
        ]),
    };

    // Anthropic: thinking block is preserved with signature for multi-turn round-tripping
    let anthropic = AnthropicProvider::new(anthropic_config(), "claude-sonnet-4-6".into()).unwrap();
    let a_msgs = anthropic.parse_conversation_messages(&[msg.clone()]);
    assert_eq!(a_msgs.len(), 1);
    assert_eq!(a_msgs[0].role, "assistant");
    match &a_msgs[0].content {
        AnthropicRequestContent::Blocks(blocks) => {
            assert_eq!(blocks.len(), 2);
            let think_json = serde_json::to_value(&blocks[0]).unwrap();
            assert_eq!(think_json["type"], "thinking");
            assert!(think_json["thinking"]
                .as_str()
                .unwrap()
                .contains("Let me reason about this"));
            assert_eq!(think_json["signature"], "sig_abc123");
            let text_json = serde_json::to_value(&blocks[1]).unwrap();
            assert_eq!(text_json["type"], "text");
            assert_eq!(text_json["text"], "Here is my answer.");
        }
        other => panic!("Anthropic: expected Blocks, got {:?}", other),
    }

    // OpenAI: thinking goes to reasoning_content, text stays in content
    let o_msgs = OpenAiProvider::convert_messages(&[msg.clone()]);
    assert_eq!(o_msgs.len(), 1);
    assert_eq!(o_msgs[0].role, "assistant");
    // Text content is in the content field
    let content_str = o_msgs[0]
        .content
        .as_ref()
        .expect("content should exist")
        .to_string();
    assert!(
        content_str.contains("Here is my answer."),
        "expected original text in OpenAI content"
    );
    // Thinking goes to reasoning_content field
    let rc = o_msgs[0]
        .reasoning_content
        .as_ref()
        .expect("reasoning_content should be set");
    assert!(
        rc.contains("Let me reason about this"),
        "expected thinking in reasoning_content"
    );

    // Ollama: thinking flattened to text
    let ol_msgs = OllamaProvider::convert_messages(vec![msg]);
    assert_eq!(ol_msgs.len(), 1);
    assert_eq!(ol_msgs[0].role, "assistant");
    assert!(ol_msgs[0].content.contains("Let me reason about this..."));
    assert!(ol_msgs[0].content.contains("Here is my answer."));
}

// ── 6. Role Mapping Consistency ──────────────────────────────────────────────

#[test]
fn cross_provider_role_mapping_user() {
    let msg = ChatMessage::user("Hello");

    let anthropic = AnthropicProvider::new(anthropic_config(), "claude-sonnet-4-6".into()).unwrap();
    let a = anthropic.parse_conversation_messages(&[msg.clone()]);
    assert_eq!(a[0].role, "user");

    let o = OpenAiProvider::convert_messages(&[msg.clone()]);
    assert_eq!(o[0].role, "user");

    let ol = OllamaProvider::convert_messages(vec![msg]);
    assert_eq!(ol[0].role, "user");
}

#[test]
fn cross_provider_role_mapping_assistant() {
    let msg = ChatMessage::assistant("Hi there");

    let anthropic = AnthropicProvider::new(anthropic_config(), "claude-sonnet-4-6".into()).unwrap();
    let a = anthropic.parse_conversation_messages(&[msg.clone()]);
    assert_eq!(a[0].role, "assistant");

    let o = OpenAiProvider::convert_messages(&[msg.clone()]);
    assert_eq!(o[0].role, "assistant");

    let ol = OllamaProvider::convert_messages(vec![msg]);
    assert_eq!(ol[0].role, "assistant");
}

#[test]
fn cross_provider_role_mapping_system() {
    let msg = ChatMessage::system("You are helpful.");

    // Anthropic maps System -> "user"
    let anthropic = AnthropicProvider::new(anthropic_config(), "claude-sonnet-4-6".into()).unwrap();
    let a = anthropic.parse_conversation_messages(&[msg.clone()]);
    assert_eq!(a[0].role, "user");

    // OpenAI maps System -> "system"
    let o = OpenAiProvider::convert_messages(&[msg.clone()]);
    assert_eq!(o[0].role, "system");

    // Ollama maps System -> "system"
    let ol = OllamaProvider::convert_messages(vec![msg]);
    assert_eq!(ol[0].role, "system");
}

#[test]
fn cross_provider_role_mapping_tool() {
    let msg = ChatMessage {
        role: MessageRole::Tool("Read".to_string()),
        content: MessageContent::simple("tool output"),
    };

    // Anthropic: Tool role -> "user"
    let anthropic = AnthropicProvider::new(anthropic_config(), "claude-sonnet-4-6".into()).unwrap();
    let a = anthropic.parse_conversation_messages(&[msg.clone()]);
    assert_eq!(a[0].role, "user");

    // OpenAI: Tool role -> "tool"
    let o = OpenAiProvider::convert_messages(&[msg.clone()]);
    assert_eq!(o[0].role, "tool");

    // Ollama: Tool role is filtered out entirely (contains "tool")
    let ol = OllamaProvider::convert_messages(vec![msg]);
    assert_eq!(ol.len(), 0, "Ollama should filter out tool-role messages");
}

// ── 7. Mixed Content Scenarios ───────────────────────────────────────────────

#[test]
fn cross_provider_mixed_text_and_tool_result() {
    let msg = ChatMessage {
        role: MessageRole::User,
        content: MessageContent::Blocks(vec![
            ContentBlock::text("Here's the result:"),
            ContentBlock::tool_result("call_1", "output data"),
            ContentBlock::tool_result("call_2", "more data"),
        ]),
    };

    // Anthropic: all blocks in one user message
    let anthropic = AnthropicProvider::new(anthropic_config(), "claude-sonnet-4-6".into()).unwrap();
    let a = anthropic.parse_conversation_messages(&[msg.clone()]);
    assert_eq!(a.len(), 1);
    assert_eq!(a[0].role, "user");
    match &a[0].content {
        AnthropicRequestContent::Blocks(blocks) => {
            assert_eq!(blocks.len(), 3);
        }
        other => panic!("expected Blocks, got {:?}", other),
    }

    // OpenAI: text in user message + separate tool messages
    let o = OpenAiProvider::convert_messages(&[msg.clone()]);
    assert_eq!(o.len(), 3, "text + 2 tool results = 3 messages");
    assert_eq!(o[0].role, "user");
    assert_eq!(o[1].role, "tool");
    assert_eq!(o[1].tool_call_id.as_deref(), Some("call_1"));
    assert_eq!(o[2].role, "tool");
    assert_eq!(o[2].tool_call_id.as_deref(), Some("call_2"));

    // Ollama: all flattened into one message
    let ol = OllamaProvider::convert_messages(vec![msg]);
    assert_eq!(ol.len(), 1);
    assert_eq!(ol[0].role, "user");
    assert!(ol[0].content.contains("Here's the result:"));
    assert!(ol[0].content.contains("output data"));
    assert!(ol[0].content.contains("more data"));
}

#[test]
fn cross_provider_multi_turn_conversation() {
    let messages = vec![
        ChatMessage::user("Read the file src/main.rs"),
        ChatMessage {
            role: MessageRole::Assistant,
            content: MessageContent::Blocks(vec![
                ContentBlock::text("I'll read that file."),
                ContentBlock::tool_use(
                    "toolu_01",
                    "Read",
                    serde_json::json!({"path": "src/main.rs"}),
                ),
            ]),
        },
        ChatMessage {
            role: MessageRole::User,
            content: MessageContent::Blocks(vec![ContentBlock::tool_result(
                "toolu_01",
                "fn main() { println!(\"hello\"); }",
            )]),
        },
    ];

    // Anthropic: 3 messages, tool_result in user role
    let anthropic = AnthropicProvider::new(anthropic_config(), "claude-sonnet-4-6".into()).unwrap();
    let a = anthropic.parse_conversation_messages(&messages);
    assert_eq!(a.len(), 3);
    assert_eq!(a[0].role, "user"); // user message
    assert_eq!(a[1].role, "assistant"); // assistant + tool_use
    assert_eq!(a[2].role, "user"); // tool_result -> user

    // OpenAI: user + assistant(tool_calls) + tool(result)
    let o = OpenAiProvider::convert_messages(&messages);
    assert_eq!(o.len(), 3);
    assert_eq!(o[0].role, "user");
    assert_eq!(o[1].role, "assistant");
    assert!(o[1].tool_calls.is_some());
    assert_eq!(o[2].role, "tool");
    assert_eq!(o[2].tool_call_id.as_deref(), Some("toolu_01"));

    // Ollama: 3 messages (user + assistant flattened + user flattened)
    let ol = OllamaProvider::convert_messages(messages);
    assert_eq!(ol.len(), 3);
    assert_eq!(ol[0].role, "user");
    assert_eq!(ol[1].role, "assistant");
    assert_eq!(ol[2].role, "user");
    assert!(ol[2].content.contains("fn main()"));
}

// ── 8. Simple String Content ────────────────────────────────────────────────

#[test]
fn cross_provider_simple_string_content() {
    let msg = ChatMessage::user("Just a simple string");

    let anthropic = AnthropicProvider::new(anthropic_config(), "claude-sonnet-4-6".into()).unwrap();
    let a = anthropic.parse_conversation_messages(&[msg.clone()]);
    assert_eq!(a.len(), 1);
    match &a[0].content {
        AnthropicRequestContent::Text(t) => assert_eq!(t, "Just a simple string"),
        other => panic!("expected Text, got {:?}", other),
    }

    let o = OpenAiProvider::convert_messages(&[msg.clone()]);
    assert_eq!(
        o[0].content.as_ref().unwrap().as_str(),
        Some("Just a simple string")
    );

    let ol = OllamaProvider::convert_messages(vec![msg]);
    assert_eq!(ol[0].content, "Just a simple string");
}

// ── 9. Empty Blocks Array ───────────────────────────────────────────────────

#[test]
fn cross_provider_empty_blocks() {
    let msg = ChatMessage {
        role: MessageRole::User,
        content: MessageContent::Blocks(vec![]),
    };

    // Anthropic: empty blocks falls through to text path -> Text("")
    let anthropic = AnthropicProvider::new(anthropic_config(), "claude-sonnet-4-6".into()).unwrap();
    let a = anthropic.parse_conversation_messages(&[msg.clone()]);
    assert_eq!(a.len(), 1);
    // Empty blocks array doesn't match the Blocks branch, falls through to plain text
    match &a[0].content {
        AnthropicRequestContent::Text(t) => assert!(t.is_empty()),
        other => panic!("expected empty Text, got {:?}", other),
    }

    // OpenAI: empty blocks -> message with no content parts -> content is None
    let o = OpenAiProvider::convert_messages(&[msg.clone()]);
    assert_eq!(
        o.len(),
        0,
        "OpenAI should skip messages with no content parts and no tool calls"
    );

    // Ollama: empty blocks -> empty content string
    let ol = OllamaProvider::convert_messages(vec![msg]);
    assert_eq!(ol.len(), 1);
    assert_eq!(ol[0].content, "");
}

// ── 10. Tool Use ID Preservation ─────────────────────────────────────────────

#[test]
fn cross_provider_tool_use_id_roundtrip() {
    // Verify tool_use_id survives the roundtrip through tool_result
    let tool_use_id = "toolu_abc123xyz";

    let assistant_msg = ChatMessage {
        role: MessageRole::Assistant,
        content: MessageContent::Blocks(vec![ContentBlock::tool_use(
            tool_use_id,
            "Bash",
            serde_json::json!({"command": "ls"}),
        )]),
    };
    let result_msg = ChatMessage {
        role: MessageRole::User,
        content: MessageContent::Blocks(vec![ContentBlock::tool_result(
            tool_use_id,
            "file1.txt\nfile2.txt",
        )]),
    };

    // Anthropic: IDs preserved in both directions
    let anthropic = AnthropicProvider::new(anthropic_config(), "claude-sonnet-4-6".into()).unwrap();
    let a = anthropic.parse_conversation_messages(&[assistant_msg.clone(), result_msg.clone()]);
    assert_eq!(a.len(), 2);
    // assistant tool_use
    match &a[0].content {
        AnthropicRequestContent::Blocks(blocks) => {
            let json = serde_json::to_value(&blocks[0]).unwrap();
            assert_eq!(json["id"], tool_use_id);
        }
        other => panic!("expected Blocks, got {:?}", other),
    }
    // user tool_result
    match &a[1].content {
        AnthropicRequestContent::Blocks(blocks) => {
            let json = serde_json::to_value(&blocks[0]).unwrap();
            assert_eq!(json["tool_use_id"], tool_use_id);
        }
        other => panic!("expected Blocks, got {:?}", other),
    }

    // OpenAI: IDs preserved in both directions
    let o = OpenAiProvider::convert_messages(&[assistant_msg, result_msg]);
    assert_eq!(o.len(), 2);
    // assistant tool_call
    let tc = o[0].tool_calls.as_ref().unwrap();
    assert_eq!(tc[0].id, tool_use_id);
    // tool result
    assert_eq!(o[1].tool_call_id.as_deref(), Some(tool_use_id));
}
