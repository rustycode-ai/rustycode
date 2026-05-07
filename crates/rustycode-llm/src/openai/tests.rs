//! Tests for the OpenAI provider.

use super::*;
use crate::provider::{EffortLevel, LLMProvider, SSEEvent};
use secrecy::SecretString;

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
fn test_creates_provider() {
    let config = make_config(Some(
        &std::env::var("OPENAI_API_KEY").unwrap_or_else(|_| "sk-test123".to_string()),
    ));
    let provider = OpenAiProvider::new(config, "gpt-4o".to_string()).unwrap();
    assert_eq!(<OpenAiProvider as LLMProvider>::name(&provider), "openai");
    assert_eq!(provider.endpoint(), "https://api.openai.com/v1");
}

#[test]
fn test_default_endpoint() {
    let p = OpenAiProvider::new(
        make_config(Some(
            &std::env::var("OPENAI_API_KEY").unwrap_or_else(|_| "sk-test".to_string()),
        )),
        "gpt-4o".to_string(),
    )
    .unwrap();
    assert_eq!(p.endpoint(), "https://api.openai.com/v1");
}

#[test]
fn test_custom_endpoint() {
    let mut config = make_config(Some(
        &std::env::var("OPENAI_API_KEY").unwrap_or_else(|_| "sk-test".to_string()),
    ));
    config.base_url = Some("https://proxy.example.com/v1".to_string());
    let p = OpenAiProvider::new(config, "gpt-4o".to_string()).unwrap();
    assert_eq!(p.endpoint(), "https://proxy.example.com/v1");
}

#[test]
fn test_provider_name() {
    let p = OpenAiProvider::new(
        make_config(Some(
            &std::env::var("OPENAI_API_KEY").unwrap_or_else(|_| "sk-test".to_string()),
        )),
        "gpt-4o".to_string(),
    )
    .unwrap();
    assert_eq!(<OpenAiProvider as LLMProvider>::name(&p), "openai");
}

#[test]
fn test_metadata_display_name() {
    let metadata = OpenAiProvider::metadata();
    assert_eq!(metadata.display_name, "OpenAI");
    assert_eq!(metadata.provider_id, "openai");
}

#[test]
fn test_metadata_tool_calling_supported() {
    let metadata = OpenAiProvider::metadata();
    assert!(metadata.tool_calling.supported);
    assert!(metadata.tool_calling.streaming_support);
    assert!(metadata.tool_calling.parallel_calling);
}

#[test]
fn test_metadata_env_mappings() {
    let metadata = OpenAiProvider::metadata();
    assert_eq!(
        metadata.config_schema.env_mappings.get("api_key"),
        Some(&"OPENAI_API_KEY".to_string())
    );
}

#[test]
fn test_metadata_recommended_models() {
    let metadata = OpenAiProvider::metadata();
    let model_ids: Vec<&str> = metadata
        .recommended_models
        .iter()
        .map(|m| m.model_id.as_str())
        .collect();
    assert!(model_ids.iter().any(|id| id.contains("gpt-4o")));
}

#[test]
fn test_openai_request_serialization() {
    let request = types::OpenAiRequest {
        model: "gpt-4o".to_string(),
        messages: vec![types::OpenAiMessage {
            role: "user".to_string(),
            content: Some(serde_json::Value::String("Hello".to_string())),
            tool_calls: None,
            tool_call_id: None,
            name: None,
        }],
        temperature: Some(0.5),
        max_tokens: Some(2048),
        max_completion_tokens: None,
        stream: Some(true),
        tools: None,
        tool_choice: None,
        parallel_tool_calls: None,
        reasoning_effort: None,
        response_format: None,
        thinking: None,
        stream_options: None,
        prompt_cache_key: None,
    };
    let json = serde_json::to_string(&request).unwrap();
    assert!(json.contains("\"model\":\"gpt-4o\""));
    assert!(json.contains("\"temperature\":0.5"));
    assert!(json.contains("\"max_tokens\":2048"));
    assert!(json.contains("\"stream\":true"));
    assert!(!json.contains("\"tools\""));
    assert!(!json.contains("\"max_completion_tokens\""));
    assert!(!json.contains("\"reasoning_effort\""));
    assert!(!json.contains("\"thinking\""));
    assert!(!json.contains("\"tool_choice\""));
    assert!(!json.contains("\"parallel_tool_calls\""));
}

#[test]
fn test_openai_request_serialization_with_tools() {
    let request = types::OpenAiRequest {
        model: "gpt-4o".to_string(),
        messages: vec![],
        temperature: None,
        max_tokens: None,
        max_completion_tokens: None,
        stream: None,
        tools: Some(vec![serde_json::json!({
            "type": "function",
            "function": {"name": "get_weather", "description": "Get weather"}
        })]),
        tool_choice: None,
        parallel_tool_calls: None,
        reasoning_effort: None,
        response_format: None,
        thinking: None,
        stream_options: None,
        prompt_cache_key: None,
    };
    let json = serde_json::to_string(&request).unwrap();
    assert!(json.contains("\"tools\""));
    assert!(json.contains("get_weather"));
    assert!(!json.contains("\"temperature\""));
    assert!(!json.contains("\"max_tokens\""));
    assert!(!json.contains("\"stream\""));
}

#[test]
fn test_openai_request_reasoning_model() {
    let request = types::OpenAiRequest {
        model: "o4-mini".to_string(),
        messages: vec![types::OpenAiMessage {
            role: "user".to_string(),
            content: Some(serde_json::Value::String("Solve this".to_string())),
            tool_calls: None,
            tool_call_id: None,
            name: None,
        }],
        temperature: None,
        max_tokens: None,
        max_completion_tokens: Some(4096),
        stream: Some(false),
        tools: None,
        tool_choice: None,
        parallel_tool_calls: None,
        reasoning_effort: Some("medium".to_string()),
        response_format: None,
        thinking: None,
        stream_options: None,
        prompt_cache_key: None,
    };
    let json = serde_json::to_string(&request).unwrap();
    assert!(json.contains("\"model\":\"o4-mini\""));
    assert!(json.contains("\"max_completion_tokens\":4096"));
    assert!(json.contains("\"reasoning_effort\":\"medium\""));
    assert!(!json.contains("\"max_tokens\":"));
    assert!(!json.contains("\"temperature\""));
}

#[test]
fn test_stream_options_included_when_streaming() {
    let request = types::OpenAiRequest {
        model: "gpt-4o".to_string(),
        messages: vec![],
        temperature: None,
        max_tokens: None,
        max_completion_tokens: None,
        stream: Some(true),
        tools: None,
        tool_choice: None,
        parallel_tool_calls: None,
        reasoning_effort: None,
        response_format: None,
        thinking: None,
        stream_options: Some(serde_json::json!({"include_usage": true})),
        prompt_cache_key: None,
    };
    let json = serde_json::to_string(&request).unwrap();
    assert!(json.contains("\"stream_options\""));
    assert!(json.contains("\"include_usage\":true"));

    let request_no_stream = types::OpenAiRequest {
        stream: Some(false),
        stream_options: None,
        prompt_cache_key: None,
        ..request
    };
    let json_no_stream = serde_json::to_string(&request_no_stream).unwrap();
    assert!(!json_no_stream.contains("\"stream_options\""));
}

#[test]
fn test_is_reasoning_model() {
    assert!(OpenAiProvider::is_reasoning_model("o1"));
    assert!(OpenAiProvider::is_reasoning_model("o1-mini"));
    assert!(OpenAiProvider::is_reasoning_model("o3"));
    assert!(OpenAiProvider::is_reasoning_model("o3-mini"));
    assert!(OpenAiProvider::is_reasoning_model("o4-mini"));
    assert!(OpenAiProvider::is_reasoning_model("glm-5.1"));
    assert!(OpenAiProvider::is_reasoning_model("glm-5"));
    assert!(OpenAiProvider::is_reasoning_model("gpt-5"));
    assert!(OpenAiProvider::is_reasoning_model("gpt-5.2"));
    assert!(OpenAiProvider::is_reasoning_model("gpt-5.1-mini"));
    assert!(!OpenAiProvider::is_reasoning_model("gpt-4o"));
    assert!(!OpenAiProvider::is_reasoning_model("optimum"));
    assert!(!OpenAiProvider::is_reasoning_model("glm-4"));
}

#[test]
fn test_build_request_body_standard_model() {
    let provider =
        OpenAiProvider::new(make_config(Some("sk-test")), "gpt-4o".to_string()).unwrap();
    let body = provider.build_request_body(
        "gpt-4o".to_string(),
        vec![],
        vec![],
        Some(2048),
        Some(0.7),
        None,
        Some(false),
        None,
        None,
        None,
        None,
        None,
    );
    assert_eq!(body.max_tokens, Some(2048));
    assert_eq!(body.max_completion_tokens, None);
    assert_eq!(body.reasoning_effort, None);
}

#[test]
fn test_build_request_body_reasoning_model() {
    let provider =
        OpenAiProvider::new(make_config(Some("sk-test")), "gpt-4o".to_string()).unwrap();
    let body = provider.build_request_body(
        "o4-mini".to_string(),
        vec![],
        vec![],
        Some(4096),
        None,
        Some(&EffortLevel::High),
        None,
        None,
        None,
        None,
        None,
        None,
    );
    assert_eq!(body.max_tokens, None);
    assert_eq!(body.max_completion_tokens, Some(4096));
    assert_eq!(body.reasoning_effort, Some("high".to_string()));
}

#[test]
fn test_build_request_body_glm5_thinking() {
    let provider =
        OpenAiProvider::new(make_config(Some("sk-test")), "gpt-4o".to_string()).unwrap();
    let body = provider.build_request_body(
        "glm-5.1".to_string(),
        vec![],
        vec![],
        Some(8192),
        None,
        None,
        Some(true),
        None,
        None,
        None,
        None,
        None,
    );
    assert_eq!(body.max_tokens, None);
    assert_eq!(body.max_completion_tokens, Some(8192));
    let thinking = body
        .thinking
        .as_ref()
        .expect("GLM-5.x should have thinking enabled");
    assert_eq!(thinking["type"], "enabled");
    let json = serde_json::to_string(&body).unwrap();
    assert!(json.contains("\"thinking\":{\"type\":\"enabled\"}"));
}

#[test]
fn test_build_request_body_standard_model_no_thinking() {
    let provider =
        OpenAiProvider::new(make_config(Some("sk-test")), "gpt-4o".to_string()).unwrap();
    let body = provider.build_request_body(
        "gpt-4o".to_string(),
        vec![],
        vec![],
        Some(2048),
        None,
        None,
        Some(true),
        None,
        None,
        None,
        None,
        None,
    );
    assert!(body.thinking.is_none());
    let json = serde_json::to_string(&body).unwrap();
    assert!(!json.contains("\"thinking\""));
}

#[test]
fn test_openai_response_deserialization() {
    let json = r#"{
        "choices": [
            {
                "message": {"role": "assistant", "content": "Hello! How can I help?"},
                "finish_reason": "stop"
            }
        ],
        "usage": {
            "prompt_tokens": 10,
            "completion_tokens": 8,
            "total_tokens": 18,
            "prompt_tokens_details": {"cached_tokens": 5}
        },
        "model": "gpt-4o"
    }"#;
    let response: types::OpenAiResponse = serde_json::from_str(json).unwrap();
    assert_eq!(response.model, "gpt-4o");
    assert_eq!(response.choices.len(), 1);
    assert_eq!(
        response.choices[0].message.content.as_deref(),
        Some("Hello! How can I help?")
    );
    assert!(response.usage.is_some());
    let usage = response.usage.as_ref().unwrap();
    assert_eq!(usage.total_tokens, 18);
    assert_eq!(
        usage.prompt_tokens_details.as_ref().unwrap().cached_tokens,
        5
    );
}

#[test]
fn test_openai_response_deserialization_no_cached_tokens() {
    let json = r#"{
        "choices": [
            {
                "message": {"role": "assistant", "content": "Hi"},
                "finish_reason": "stop"
            }
        ],
        "usage": {"prompt_tokens": 10, "completion_tokens": 5, "total_tokens": 15},
        "model": "gpt-4o"
    }"#;
    let response: types::OpenAiResponse = serde_json::from_str(json).unwrap();
    let usage = response.usage.as_ref().unwrap();
    assert!(usage.prompt_tokens_details.is_none());
}

#[test]
fn test_openai_response_usage_optional() {
    let json = r#"{
        "choices": [
            {
                "message": {"role": "assistant", "content": "Hi"},
                "finish_reason": null
            }
        ],
        "usage": null,
        "model": "gpt-4o"
    }"#;
    let response: types::OpenAiResponse = serde_json::from_str(json).unwrap();
    assert!(response.usage.is_none());
    assert!(response.choices[0].finish_reason.is_none());
}

#[test]
fn test_openai_response_with_tool_calls() {
    let json = r#"{
        "choices": [
            {
                "message": {
                    "role": "assistant",
                    "content": null,
                    "tool_calls": [
                        {
                            "id": "call_abc123",
                            "type": "function",
                            "function": {
                                "name": "get_weather",
                                "arguments": "{\"location\": \"San Francisco\"}"
                            }
                        }
                    ]
                },
                "finish_reason": "tool_calls"
            }
        ],
        "usage": {"prompt_tokens": 50, "completion_tokens": 20, "total_tokens": 70},
        "model": "gpt-4o"
    }"#;
    let response: types::OpenAiResponse = serde_json::from_str(json).unwrap();
    assert_eq!(response.choices.len(), 1);
    let msg = &response.choices[0].message;
    assert!(msg.content.is_none());
    assert!(msg.tool_calls.is_some());
    let tool_calls = msg.tool_calls.as_ref().unwrap();
    assert_eq!(tool_calls.len(), 1);
    assert_eq!(tool_calls[0].function.name, "get_weather");
    assert_eq!(
        tool_calls[0].function.arguments,
        "{\"location\": \"San Francisco\"}"
    );
    assert_eq!(
        response.choices[0].finish_reason.as_deref(),
        Some("tool_calls")
    );
}

#[test]
fn test_openai_response_with_text_and_tool_calls() {
    let json = r#"{
        "choices": [
            {
                "message": {
                    "role": "assistant",
                    "content": "I'll check the weather for you.",
                    "tool_calls": [
                        {
                            "id": "call_xyz",
                            "type": "function",
                            "function": {
                                "name": "read_file",
                                "arguments": "{\"path\": \"src/main.rs\"}"
                            }
                        }
                    ]
                },
                "finish_reason": "stop"
            }
        ],
        "usage": {"prompt_tokens": 30, "completion_tokens": 15, "total_tokens": 45},
        "model": "gpt-4o"
    }"#;
    let response: types::OpenAiResponse = serde_json::from_str(json).unwrap();
    let msg = &response.choices[0].message;
    assert_eq!(
        msg.content.as_deref(),
        Some("I'll check the weather for you.")
    );
    assert!(msg.tool_calls.is_some());
}

#[tokio::test]
async fn test_list_models_returns_known_models() {
    let api_key = std::env::var("OPENAI_API_KEY").unwrap_or_else(|_| "sk-test".to_string());
    let p = OpenAiProvider::new(make_config(Some(&api_key)), "gpt-4o".to_string()).unwrap();
    let models = <OpenAiProvider as LLMProvider>::list_models(&p)
        .await
        .unwrap();
    assert!(models.iter().any(|m| m == "gpt-5.2"));
    assert!(models.iter().any(|m| m == "o4-mini"));
    assert!(models.iter().any(|m| m == "gpt-4o"));
    assert!(models.iter().any(|m| m == "o3"));
}

#[test]
fn test_new_without_validation() {
    let config = make_config(Some(
        &std::env::var("OPENAI_API_KEY").unwrap_or_else(|_| "sk-test".to_string()),
    ));
    let provider =
        OpenAiProvider::new_without_validation(config, "gpt-4o".to_string()).unwrap();
    assert_eq!(<OpenAiProvider as LLMProvider>::name(&provider), "openai");
}

#[test]
fn test_endpoint_trims_trailing_slash() {
    let mut config = make_config(Some(
        &std::env::var("OPENAI_API_KEY").unwrap_or_else(|_| "sk-test".to_string()),
    ));
    config.base_url = Some("https://proxy.example.com/v1/".to_string());
    let p = OpenAiProvider::new(config, "gpt-4o".to_string()).unwrap();
    assert_eq!(p.endpoint(), "https://proxy.example.com/v1");
}

#[tokio::test]
async fn test_is_available_with_key() {
    let p = OpenAiProvider::new(make_config(Some("sk-test")), "gpt-4o".to_string()).unwrap();
    assert!(<OpenAiProvider as LLMProvider>::is_available(&p).await);
}

#[tokio::test]
async fn test_is_available_without_key() {
    let p = OpenAiProvider::new_without_validation(make_config(None), "gpt-4o".to_string())
        .unwrap();
    assert!(!<OpenAiProvider as LLMProvider>::is_available(&p).await);
}

#[test]
fn test_convert_messages_simple_text() {
    let messages = vec![ChatMessage::user("Hello")];
    let result = OpenAiProvider::convert_messages(&messages);
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].role, "user");
    assert_eq!(result[0].content.as_ref().unwrap().as_str(), Some("Hello"));
    assert!(result[0].tool_calls.is_none());
    assert!(result[0].tool_call_id.is_none());
}

#[test]
fn test_convert_messages_tool_result() {
    use rustycode_protocol::{ContentBlock, MessageContent};
    let messages = vec![ChatMessage {
        role: MessageRole::User,
        content: MessageContent::Blocks(vec![ContentBlock::tool_result(
            "call_abc123",
            "File contents here",
        )]),
    }];
    let result = OpenAiProvider::convert_messages(&messages);
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].role, "tool");
    assert_eq!(result[0].tool_call_id.as_deref(), Some("call_abc123"));
    assert_eq!(
        result[0].content.as_ref().unwrap().as_str(),
        Some("File contents here")
    );
}

#[test]
fn test_convert_messages_assistant_with_tool_use() {
    use rustycode_protocol::{ContentBlock, MessageContent};
    let messages = vec![ChatMessage {
        role: MessageRole::Assistant,
        content: MessageContent::Blocks(vec![
            ContentBlock::text("I'll read that file."),
            ContentBlock::tool_use(
                "call_xyz",
                "read_file",
                serde_json::json!({"path": "test.rs"}),
            ),
        ]),
    }];
    let result = OpenAiProvider::convert_messages(&messages);
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].role, "assistant");
    assert!(result[0].tool_calls.is_some());
    let tool_calls = result[0].tool_calls.as_ref().unwrap();
    assert_eq!(tool_calls.len(), 1);
    assert_eq!(tool_calls[0].id, "call_xyz");
    assert_eq!(tool_calls[0].function.name, "read_file");
}

#[test]
fn test_convert_messages_mixed_tool_result_and_text() {
    use rustycode_protocol::{ContentBlock, MessageContent};
    let messages = vec![ChatMessage {
        role: MessageRole::User,
        content: MessageContent::Blocks(vec![
            ContentBlock::text("Here's the result:"),
            ContentBlock::tool_result("call_1", "output data"),
            ContentBlock::tool_result("call_2", "more data"),
        ]),
    }];
    let result = OpenAiProvider::convert_messages(&messages);
    assert_eq!(result.len(), 3);
    assert_eq!(result[0].role, "user");
    assert_eq!(result[1].role, "tool");
    assert_eq!(result[1].tool_call_id.as_deref(), Some("call_1"));
    assert_eq!(result[2].role, "tool");
    assert_eq!(result[2].tool_call_id.as_deref(), Some("call_2"));
}

#[test]
fn test_openai_message_tool_result_serialization() {
    let msg = types::OpenAiMessage {
        role: "tool".to_string(),
        content: Some(serde_json::Value::String("result data".to_string())),
        tool_calls: None,
        tool_call_id: Some("call_abc".to_string()),
        name: None,
    };
    let json = serde_json::to_string(&msg).unwrap();
    assert!(json.contains("\"role\":\"tool\""));
    assert!(json.contains("\"tool_call_id\":\"call_abc\""));
    assert!(json.contains("\"content\":\"result data\""));
    assert!(!json.contains("\"tool_calls\""));
    assert!(!json.contains("\"name\""));
}

#[test]
fn test_openai_request_with_tool_choice() {
    let request = types::OpenAiRequest {
        model: "gpt-4o".to_string(),
        messages: vec![],
        temperature: None,
        max_tokens: None,
        max_completion_tokens: None,
        stream: None,
        tools: Some(vec![serde_json::json!({
            "type": "function",
            "function": {"name": "test", "description": "test"}
        })]),
        tool_choice: Some(serde_json::json!("auto")),
        parallel_tool_calls: Some(false),
        reasoning_effort: None,
        response_format: None,
        thinking: None,
        stream_options: None,
        prompt_cache_key: None,
    };
    let json = serde_json::to_string(&request).unwrap();
    assert!(json.contains("\"tool_choice\":\"auto\""));
    assert!(json.contains("\"parallel_tool_calls\":false"));
}

// -- SSE Streaming Edge Case Tests ------------------------------------------------

/// Empty content in delta should not emit a text event.
#[test]
fn test_sse_empty_content_delta_not_emitted() {
    let lines = r#"data: {"choices":[{"delta":{"content":""},"finish_reason":null}]}"#;
    let events = streaming::parse_sse_lines(lines);
    let text_events: Vec<_> = events
        .iter()
        .filter(|e| {
            matches!(
                e,
                Ok(SSEEvent::ContentBlockDelta {
                    delta: crate::provider::ContentDelta::Text { .. },
                    ..
                })
            )
        })
        .collect();
    assert!(
        text_events.is_empty(),
        "empty content delta should not produce a text event"
    );
}

/// Multiple tool calls in a single SSE response should produce separate ContentBlockStart events.
#[test]
fn test_sse_multiple_tool_calls_get_separate_starts() {
    let lines = "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"call_aaa\",\"type\":\"function\",\"function\":{\"name\":\"read_file\",\"arguments\":\"\"}},{\"index\":1,\"id\":\"call_bbb\",\"type\":\"function\",\"function\":{\"name\":\"write_file\",\"arguments\":\"\"}}]}}]}\n";
    let events = streaming::parse_sse_lines(lines);

    let starts: Vec<_> = events
        .iter()
        .filter_map(|e| match e {
            Ok(SSEEvent::ContentBlockStart {
                index,
                content_block,
            }) => Some((*index, content_block.clone())),
            _ => None,
        })
        .collect();

    assert_eq!(
        starts.len(),
        2,
        "should have 2 ContentBlockStart events for 2 tool calls"
    );
    assert_eq!(starts[0].0, 0);
    assert_eq!(starts[1].0, 1);

    match &starts[0].1 {
        crate::provider::ContentBlockType::ToolUse { name, .. } => {
            assert_eq!(name, "read_file");
        }
        other => panic!("expected ToolUse, got {:?}", other),
    }
    match &starts[1].1 {
        crate::provider::ContentBlockType::ToolUse { name, .. } => {
            assert_eq!(name, "write_file");
        }
        other => panic!("expected ToolUse, got {:?}", other),
    }
}

/// Rapid alternating text and tool deltas -- state tracking must remain correct.
#[test]
fn test_sse_alternating_text_and_tool_deltas() {
    let lines = "\
data: {\"choices\":[{\"delta\":{\"content\":\"Hello\"},\"finish_reason\":null}]}
data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"arguments\":\"{\\\"path\\\":\"}}]},\"finish_reason\":null}]}
data: {\"choices\":[{\"delta\":{\"content\":\" world\"},\"finish_reason\":null}]}
data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"arguments\":\"\\\"test.rs\\\"}\"}}]},\"finish_reason\":null}]}";
    let events = streaming::parse_sse_lines(lines);

    let text_deltas: Vec<String> = events
        .iter()
        .filter_map(|e| match e {
            Ok(SSEEvent::ContentBlockDelta {
                delta: crate::provider::ContentDelta::Text { text },
                ..
            }) => Some(text.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(text_deltas, vec!["Hello", " world"]);

    let json_deltas: Vec<String> = events
        .iter()
        .filter_map(|e| match e {
            Ok(SSEEvent::ContentBlockDelta {
                delta: crate::provider::ContentDelta::PartialJson { partial_json },
                ..
            }) => Some(partial_json.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(json_deltas.len(), 2);
    assert_eq!(json_deltas[0], "{\"path\":");
    assert_eq!(json_deltas[1], "\"test.rs\"}");
}

/// Both "tool_calls" and "stop" finish reasons should produce ContentBlockStop + MessageDelta.
#[test]
fn test_sse_finish_reason_tool_calls_and_stop() {
    let lines_tool = r#"data: {"choices":[{"delta":{},"finish_reason":"tool_calls"}]}"#;
    let events_tool = streaming::parse_sse_lines(lines_tool);
    let has_stop = events_tool
        .iter()
        .any(|e| matches!(e, Ok(SSEEvent::ContentBlockStop { .. })));
    let has_msg_delta = events_tool.iter().any(|e| matches!(
        e,
        Ok(SSEEvent::MessageDelta { stop_reason: Some(ref s), .. }) if s == "tool_use"
    ));
    assert!(
        has_stop,
        "tool_calls finish should produce ContentBlockStop"
    );
    assert!(
        has_msg_delta,
        "tool_calls finish should produce MessageDelta"
    );

    let lines_stop = r#"data: {"choices":[{"delta":{},"finish_reason":"stop"}]}"#;
    let events_stop = streaming::parse_sse_lines(lines_stop);
    let has_stop2 = events_stop
        .iter()
        .any(|e| matches!(e, Ok(SSEEvent::ContentBlockStop { .. })));
    let has_msg_delta2 = events_stop.iter().any(|e| matches!(
        e,
        Ok(SSEEvent::MessageDelta { stop_reason: Some(ref s), .. }) if s == "end_turn"
    ));
    assert!(has_stop2, "stop finish should produce ContentBlockStop");
    assert!(has_msg_delta2, "stop finish should produce MessageDelta");
}

/// SSE chunks from OpenAI typically don't include a model field -- parser should handle it.
#[test]
fn test_sse_missing_model_field_still_parses() {
    let lines = r#"data: {"choices":[{"delta":{"content":"Hi"},"finish_reason":null}]}"#;
    let events = streaming::parse_sse_lines(lines);
    let text_events: Vec<_> = events
        .iter()
        .filter_map(|e| match e {
            Ok(SSEEvent::ContentBlockDelta {
                delta: crate::provider::ContentDelta::Text { text },
                ..
            }) => Some(text.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(text_events, vec!["Hi".to_string()]);
}

/// Response with null content should not emit a text delta event.
#[test]
fn test_sse_null_content_produces_no_text_event() {
    let lines = r#"data: {"choices":[{"delta":{"content":null},"finish_reason":null}]}"#;
    let events = streaming::parse_sse_lines(lines);
    let text_events: Vec<_> = events
        .iter()
        .filter(|e| {
            matches!(
                e,
                Ok(SSEEvent::ContentBlockDelta {
                    delta: crate::provider::ContentDelta::Text { .. },
                    ..
                })
            )
        })
        .collect();
    assert!(
        text_events.is_empty(),
        "null content should not produce a text delta event"
    );
}

// -- Protocol-level message roundtrip tests ----------------------------------------

use rustycode_protocol::{ContentBlock, ImageSource, MessageContent};

#[test]
fn test_roundtrip_simple_text_message() {
    let msgs = vec![ChatMessage::user("Hello, world!")];
    let result = OpenAiProvider::convert_messages(&msgs);
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].role, "user");
    assert_eq!(
        result[0].content.as_ref().unwrap().as_str(),
        Some("Hello, world!")
    );
    assert!(result[0].tool_calls.is_none());
    assert!(result[0].tool_call_id.is_none());
}

#[test]
fn test_roundtrip_text_block() {
    let msgs = vec![ChatMessage {
        role: MessageRole::User,
        content: MessageContent::Blocks(vec![ContentBlock::text("Block text")]),
    }];
    let result = OpenAiProvider::convert_messages(&msgs);
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].role, "user");
    assert_eq!(
        result[0].content.as_ref().unwrap().as_str(),
        Some("Block text")
    );
}

#[test]
fn test_roundtrip_tool_use_block() {
    let msgs = vec![ChatMessage {
        role: MessageRole::Assistant,
        content: MessageContent::Blocks(vec![ContentBlock::tool_use(
            "call_123",
            "read_file",
            serde_json::json!({"path": "a.rs"}),
        )]),
    }];
    let result = OpenAiProvider::convert_messages(&msgs);
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].role, "assistant");
    let tcs = result[0]
        .tool_calls
        .as_ref()
        .expect("should have tool_calls");
    assert_eq!(tcs.len(), 1);
    assert_eq!(tcs[0].id, "call_123");
    assert_eq!(tcs[0].function.name, "read_file");
    assert_eq!(tcs[0].r#type, "function");
    let args: serde_json::Value = serde_json::from_str(&tcs[0].function.arguments).unwrap();
    assert_eq!(args["path"], "a.rs");
}

#[test]
fn test_roundtrip_tool_result_block() {
    let msgs = vec![ChatMessage {
        role: MessageRole::User,
        content: MessageContent::Blocks(vec![ContentBlock::tool_result(
            "call_abc",
            "file contents here",
        )]),
    }];
    let result = OpenAiProvider::convert_messages(&msgs);
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].role, "tool");
    assert_eq!(result[0].tool_call_id.as_deref(), Some("call_abc"));
    assert_eq!(
        result[0].content.as_ref().unwrap().as_str(),
        Some("file contents here")
    );
}

#[test]
fn test_roundtrip_tool_error_block_prefixed() {
    let msgs = vec![ChatMessage {
        role: MessageRole::User,
        content: MessageContent::Blocks(vec![ContentBlock::tool_error(
            "call_err",
            "command not found: foo",
        )]),
    }];
    let result = OpenAiProvider::convert_messages(&msgs);
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].role, "tool");
    assert_eq!(result[0].tool_call_id.as_deref(), Some("call_err"));
    assert_eq!(
        result[0].content.as_ref().unwrap().as_str(),
        Some("Error: command not found: foo")
    );
}

#[test]
fn test_roundtrip_image_block() {
    let msgs = vec![ChatMessage {
        role: MessageRole::User,
        content: MessageContent::Blocks(vec![ContentBlock::image(ImageSource::base64(
            "image/png",
            "iVBOR",
        ))]),
    }];
    let result = OpenAiProvider::convert_messages(&msgs);
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].role, "user");
    let content = result[0].content.as_ref().unwrap();
    assert!(content.is_object() || content.is_array());
}

#[test]
fn test_roundtrip_thinking_block_converted_to_text() {
    let msgs = vec![ChatMessage {
        role: MessageRole::Assistant,
        content: MessageContent::Blocks(vec![ContentBlock::thinking(
            "internal reasoning",
            "sig123",
        )]),
    }];
    let result = OpenAiProvider::convert_messages(&msgs);
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].role, "assistant");
    let content = result[0].content.as_ref().unwrap().as_str().unwrap();
    assert!(content.contains("[prior-reasoning]"));
    assert!(content.contains("internal reasoning"));
}

#[test]
fn test_roundtrip_empty_thinking_block_skipped() {
    let msgs = vec![ChatMessage {
        role: MessageRole::Assistant,
        content: MessageContent::Blocks(vec![ContentBlock::thinking("", "sig123")]),
    }];
    let result = OpenAiProvider::convert_messages(&msgs);
    assert!(result.is_empty());
}

#[test]
fn test_roundtrip_mixed_text_and_tool_use() {
    let msgs = vec![ChatMessage {
        role: MessageRole::Assistant,
        content: MessageContent::Blocks(vec![
            ContentBlock::text("Let me read that file."),
            ContentBlock::tool_use("call_x", "read_file", serde_json::json!({"path": "x.rs"})),
        ]),
    }];
    let result = OpenAiProvider::convert_messages(&msgs);
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].role, "assistant");
    assert!(result[0].content.is_some());
    let tcs = result[0]
        .tool_calls
        .as_ref()
        .expect("should have tool_calls");
    assert_eq!(tcs.len(), 1);
    assert_eq!(tcs[0].id, "call_x");
}

#[test]
fn test_roundtrip_mixed_text_and_tool_result() {
    let msgs = vec![ChatMessage {
        role: MessageRole::User,
        content: MessageContent::Blocks(vec![
            ContentBlock::text("Here's the result:"),
            ContentBlock::tool_result("call_1", "output data"),
            ContentBlock::tool_result("call_2", "more output"),
        ]),
    }];
    let result = OpenAiProvider::convert_messages(&msgs);
    assert_eq!(result.len(), 3);
    assert_eq!(result[0].role, "user");
    assert_eq!(result[1].role, "tool");
    assert_eq!(result[1].tool_call_id.as_deref(), Some("call_1"));
    assert_eq!(result[2].role, "tool");
    assert_eq!(result[2].tool_call_id.as_deref(), Some("call_2"));
}

#[test]
fn test_roundtrip_empty_message_content() {
    let msgs = vec![ChatMessage::user("")];
    let result = OpenAiProvider::convert_messages(&msgs);
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].content.as_ref().unwrap().as_str(), Some(""));
}

#[test]
fn test_roundtrip_whitespace_only_content() {
    let msgs = vec![ChatMessage::user("   \n\t  ")];
    let result = OpenAiProvider::convert_messages(&msgs);
    assert_eq!(result.len(), 1);
    assert_eq!(
        result[0].content.as_ref().unwrap().as_str(),
        Some("   \n\t  ")
    );
}

#[test]
fn test_roundtrip_very_long_text_content() {
    let long_text = "A".repeat(12_000);
    let msgs = vec![ChatMessage::user(long_text.clone())];
    let result = OpenAiProvider::convert_messages(&msgs);
    assert_eq!(result.len(), 1);
    assert_eq!(
        result[0].content.as_ref().unwrap().as_str(),
        Some(long_text.as_str())
    );
}

#[test]
fn test_roundtrip_empty_blocks_array() {
    let msgs = vec![ChatMessage {
        role: MessageRole::User,
        content: MessageContent::Blocks(vec![]),
    }];
    let result = OpenAiProvider::convert_messages(&msgs);
    assert!(result.is_empty());
}

#[test]
fn test_roundtrip_system_role_mapping() {
    let msgs = vec![ChatMessage {
        role: MessageRole::System,
        content: MessageContent::simple("System prompt"),
    }];
    let result = OpenAiProvider::convert_messages(&msgs);
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].role, "system");
}

#[test]
fn test_roundtrip_tool_role_mapping() {
    let msgs = vec![ChatMessage {
        role: MessageRole::Tool("call_id".to_string()),
        content: MessageContent::simple("Tool output"),
    }];
    let result = OpenAiProvider::convert_messages(&msgs);
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].role, "tool");
}

// ============================================================
// New tests: SSE parsing, tool call streaming, and request
// serialization for standard vs reasoning models
// ============================================================

// --- SSE Event Parsing ---

#[test]
fn test_sse_parse_single_content_chunk() {
    let input = "data: {\"id\":\"chatcmpl-1\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"Hello\"},\"finish_reason\":null}]}\n";
    let events = streaming::parse_sse_lines(input);
    assert_eq!(events.len(), 1);
    match &events[0] {
        Ok(SSEEvent::ContentBlockDelta { index, delta }) => {
            assert_eq!(*index, 0);
            match delta {
                crate::provider::ContentDelta::Text { text } => {
                    assert_eq!(text, "Hello");
                }
                _ => panic!("expected Text delta"),
            }
        }
        _ => panic!("expected ContentBlockDelta, got {:?}", events[0]),
    }
}

#[test]
fn test_sse_parse_multiple_content_chunks() {
    let input = "\
data: {\"id\":\"chatcmpl-1\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"Hello\"},\"finish_reason\":null}]}\n\
data: {\"id\":\"chatcmpl-1\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\" world\"},\"finish_reason\":null}]}\n\
data: {\"id\":\"chatcmpl-1\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"!\"},\"finish_reason\":null}]}\n";
    let events = streaming::parse_sse_lines(input);
    assert_eq!(events.len(), 3);
    let texts: Vec<String> = events
        .iter()
        .filter_map(|e| match e {
            Ok(SSEEvent::ContentBlockDelta {
                delta: crate::provider::ContentDelta::Text { text },
                ..
            }) => Some(text.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(texts, vec!["Hello", " world", "!"]);
}

#[test]
fn test_sse_parse_empty_content_skipped() {
    let input = "data: {\"id\":\"chatcmpl-1\",\"choices\":[{\"index\":0,\"delta\":{\"role\":\"assistant\",\"content\":\"\"},\"finish_reason\":null}]}\n";
    let events = streaming::parse_sse_lines(input);
    assert_eq!(events.len(), 1);
    match &events[0] {
        Ok(SSEEvent::Text { text }) => {
            assert!(text.is_empty());
        }
        _ => panic!(
            "expected fallback Text event for empty input, got {:?}",
            events[0]
        ),
    }
}

#[test]
fn test_sse_parse_done_event() {
    let input = "data: [DONE]\n";
    let events = streaming::parse_sse_lines(input);
    assert_eq!(events.len(), 1);
    match &events[0] {
        Ok(SSEEvent::MessageStop) => {}
        _ => panic!("expected MessageStop, got {:?}", events[0]),
    }
}

#[test]
fn test_sse_parse_finish_reason_stop() {
    let input = "data: {\"id\":\"chatcmpl-1\",\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"stop\"}]}\n";
    let events = streaming::parse_sse_lines(input);
    assert_eq!(events.len(), 2);
    match &events[0] {
        Ok(SSEEvent::ContentBlockStop { index }) => {
            assert_eq!(*index, 0);
        }
        _ => panic!("expected ContentBlockStop, got {:?}", events[0]),
    }
    match &events[1] {
        Ok(SSEEvent::MessageDelta { stop_reason, .. }) => {
            assert_eq!(stop_reason.as_deref(), Some("end_turn"));
        }
        _ => panic!("expected MessageDelta, got {:?}", events[1]),
    }
}

#[test]
fn test_sse_parse_finish_reason_tool_calls() {
    let input = "data: {\"id\":\"chatcmpl-1\",\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"tool_calls\"}]}\n";
    let events = streaming::parse_sse_lines(input);
    assert_eq!(events.len(), 2);
    match &events[1] {
        Ok(SSEEvent::MessageDelta { stop_reason, .. }) => {
            assert_eq!(stop_reason.as_deref(), Some("tool_use"));
        }
        _ => panic!("expected MessageDelta with tool_calls"),
    }
}

#[test]
fn test_sse_parse_finish_reason_length_not_stop() {
    let input = "data: {\"id\":\"chatcmpl-1\",\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"length\"}]}\n";
    let events = streaming::parse_sse_lines(input);
    assert_eq!(events.len(), 1);
    match &events[0] {
        Ok(SSEEvent::MessageDelta { stop_reason, .. }) => {
            assert_eq!(stop_reason.as_deref(), Some("max_tokens"));
        }
        _ => panic!("expected MessageDelta with length, got {:?}", events[0]),
    }
}

#[test]
fn test_sse_parse_ignores_non_data_lines() {
    let input = "\
: comment line\n\
event: ping\n\
id: 42\n\
data: {\"id\":\"chatcmpl-1\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"hi\"},\"finish_reason\":null}]}\n";
    let events = streaming::parse_sse_lines(input);
    assert_eq!(events.len(), 1);
    match &events[0] {
        Ok(SSEEvent::ContentBlockDelta { delta, .. }) => match delta {
            crate::provider::ContentDelta::Text { text } => assert_eq!(text, "hi"),
            _ => panic!("expected Text delta"),
        },
        _ => panic!("expected ContentBlockDelta"),
    }
}

#[test]
fn test_sse_parse_malformed_json_ignored() {
    let input = "data: {not valid json}\n";
    let events = streaming::parse_sse_lines(input);
    assert_eq!(events.len(), 1);
    match &events[0] {
        Ok(SSEEvent::Text { text }) => assert!(text.is_empty()),
        _ => panic!("expected fallback Text event"),
    }
}

#[test]
fn test_sse_parse_empty_input() {
    let events = streaming::parse_sse_lines("");
    assert_eq!(events.len(), 1);
    match &events[0] {
        Ok(SSEEvent::Text { text }) => assert!(text.is_empty()),
        _ => panic!("expected fallback Text event for empty input"),
    }
}

#[test]
fn test_sse_parse_content_with_special_characters() {
    let input = format!(
        "{}\n",
        r#"data: {"id":"chatcmpl-1","choices":[{"index":0,"delta":{"content":"He said \"hello\" and left\nGoodbye 👋"},"finish_reason":null}]}"#
    );
    let events = streaming::parse_sse_lines(&input);
    assert_eq!(events.len(), 1);
    match &events[0] {
        Ok(SSEEvent::ContentBlockDelta { delta, .. }) => match delta {
            crate::provider::ContentDelta::Text { text } => {
                assert!(text.contains("\"hello\""));
                assert!(text.contains('\n'));
                assert!(text.contains('\u{1F44B}'));
            }
            _ => panic!("expected Text delta"),
        },
        _ => panic!("expected ContentBlockDelta"),
    }
}

// --- Tool Call Streaming ---

#[test]
fn test_sse_parse_tool_call_start() {
    let input = "data: {\"id\":\"chatcmpl-1\",\"choices\":[{\"index\":0,\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"call_abc123\",\"type\":\"function\",\"function\":{\"name\":\"get_weather\",\"arguments\":\"\"}}]},\"finish_reason\":null}]}\n";
    let events = streaming::parse_sse_lines(input);
    assert_eq!(events.len(), 1);
    match &events[0] {
        Ok(SSEEvent::ContentBlockStart {
            index,
            content_block:
                crate::provider::ContentBlockType::ToolUse {
                    id,
                    name,
                    input: tool_input,
                },
        }) => {
            assert_eq!(*index, 0);
            assert_eq!(id, "call_abc123");
            assert_eq!(name, "get_weather");
            assert!(tool_input.is_none());
        }
        _ => panic!(
            "expected ContentBlockStart with ToolUse, got {:?}",
            events[0]
        ),
    }
}

#[test]
fn test_sse_parse_tool_call_argument_deltas() {
    let input = "\
data: {\"id\":\"chatcmpl-1\",\"choices\":[{\"index\":0,\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"arguments\":\"{\\\"lo\"}}]},\"finish_reason\":null}]}\n\
data: {\"id\":\"chatcmpl-1\",\"choices\":[{\"index\":0,\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"arguments\":\"cation\\\": \\\"SF\\\"}\"}}]},\"finish_reason\":null}]}\n";
    let events = streaming::parse_sse_lines(input);
    assert_eq!(events.len(), 2);

    match &events[0] {
        Ok(SSEEvent::ContentBlockDelta { index, delta }) => {
            assert_eq!(*index, 0);
            match delta {
                crate::provider::ContentDelta::PartialJson { partial_json } => {
                    assert_eq!(partial_json, "{\"lo");
                }
                _ => panic!("expected PartialJson delta"),
            }
        }
        _ => panic!("expected ContentBlockDelta with PartialJson"),
    }

    match &events[1] {
        Ok(SSEEvent::ContentBlockDelta { index, delta }) => {
            assert_eq!(*index, 0);
            match delta {
                crate::provider::ContentDelta::PartialJson { partial_json } => {
                    assert_eq!(partial_json, "cation\": \"SF\"}");
                }
                _ => panic!("expected PartialJson delta"),
            }
        }
        _ => panic!("expected ContentBlockDelta with PartialJson"),
    }
}

#[test]
fn test_sse_parse_tool_call_full_flow() {
    let input = "\
data: {\"choices\":[{\"index\":0,\"delta\":{\"role\":\"assistant\",\"content\":null,\"tool_calls\":[{\"index\":0,\"id\":\"call_xyz\",\"type\":\"function\",\"function\":{\"name\":\"read_file\",\"arguments\":\"\"}}]},\"finish_reason\":null}]}\n\
data: {\"choices\":[{\"index\":0,\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"arguments\":\"{\\\"path\\\":\"}}]},\"finish_reason\":null}]}\n\
data: {\"choices\":[{\"index\":0,\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"arguments\":\" \\\"main.rs\\\"}\"}}]},\"finish_reason\":null}]}\n\
data: {\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"tool_calls\"}]}\n\
data: [DONE]\n";
    let events = streaming::parse_sse_lines(input);
    assert_eq!(events.len(), 6);

    match &events[0] {
        Ok(SSEEvent::ContentBlockStart {
            content_block: crate::provider::ContentBlockType::ToolUse { id, name, .. },
            ..
        }) => {
            assert_eq!(id, "call_xyz");
            assert_eq!(name, "read_file");
        }
        _ => panic!("expected ContentBlockStart at index 0, got {:?}", events[0]),
    }

    match &events[1] {
        Ok(SSEEvent::ContentBlockDelta {
            delta: crate::provider::ContentDelta::PartialJson { partial_json },
            ..
        }) => {
            assert_eq!(partial_json, "{\"path\":");
        }
        _ => panic!("expected PartialJson at index 1"),
    }

    match &events[2] {
        Ok(SSEEvent::ContentBlockDelta {
            delta: crate::provider::ContentDelta::PartialJson { partial_json },
            ..
        }) => {
            assert_eq!(partial_json, " \"main.rs\"}");
        }
        _ => panic!("expected PartialJson at index 2"),
    }

    match &events[3] {
        Ok(SSEEvent::ContentBlockStop { index }) => {
            assert_eq!(*index, 0);
        }
        _ => panic!("expected ContentBlockStop at index 3"),
    }

    match &events[4] {
        Ok(SSEEvent::MessageDelta { stop_reason, .. }) => {
            assert_eq!(stop_reason.as_deref(), Some("tool_use"));
        }
        _ => panic!("expected MessageDelta at index 4"),
    }

    match &events[5] {
        Ok(SSEEvent::MessageStop) => {}
        _ => panic!("expected MessageStop at index 5"),
    }
}

#[test]
fn test_sse_parse_parallel_tool_calls() {
    let input = "\
data: {\"choices\":[{\"index\":0,\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"call_a\",\"type\":\"function\",\"function\":{\"name\":\"fn_a\",\"arguments\":\"\"}},{\"index\":1,\"id\":\"call_b\",\"type\":\"function\",\"function\":{\"name\":\"fn_b\",\"arguments\":\"\"}}]},\"finish_reason\":null}]}\n\
data: {\"choices\":[{\"index\":0,\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"arguments\":\"{\\\"x\\\":1}\"}},{\"index\":1,\"function\":{\"arguments\":\"{\\\"y\\\":2}\"}}]},\"finish_reason\":null}]}\n";
    let events = streaming::parse_sse_lines(input);
    assert_eq!(events.len(), 4);

    match &events[0] {
        Ok(SSEEvent::ContentBlockStart {
            index,
            content_block,
        }) => {
            assert_eq!(*index, 0);
            match content_block {
                crate::provider::ContentBlockType::ToolUse { id, name, .. } => {
                    assert_eq!(id, "call_a");
                    assert_eq!(name, "fn_a");
                }
                _ => panic!("expected ToolUse"),
            }
        }
        _ => panic!("expected ContentBlockStart for tool 0"),
    }

    match &events[1] {
        Ok(SSEEvent::ContentBlockStart {
            index,
            content_block,
        }) => {
            assert_eq!(*index, 1);
            match content_block {
                crate::provider::ContentBlockType::ToolUse { id, name, .. } => {
                    assert_eq!(id, "call_b");
                    assert_eq!(name, "fn_b");
                }
                _ => panic!("expected ToolUse"),
            }
        }
        _ => panic!("expected ContentBlockStart for tool 1"),
    }

    match &events[2] {
        Ok(SSEEvent::ContentBlockDelta { index, delta }) => {
            assert_eq!(*index, 0);
            match delta {
                crate::provider::ContentDelta::PartialJson { partial_json } => {
                    assert_eq!(partial_json, "{\"x\":1}");
                }
                _ => panic!("expected PartialJson"),
            }
        }
        _ => panic!("expected PartialJson delta for tool 0"),
    }
    match &events[3] {
        Ok(SSEEvent::ContentBlockDelta { index, delta }) => {
            assert_eq!(*index, 1);
            match delta {
                crate::provider::ContentDelta::PartialJson { partial_json } => {
                    assert_eq!(partial_json, "{\"y\":2}");
                }
                _ => panic!("expected PartialJson"),
            }
        }
        _ => panic!("expected PartialJson delta for tool 1"),
    }
}

#[test]
fn test_sse_parse_tool_call_empty_arguments_skipped() {
    let input = "data: {\"choices\":[{\"index\":0,\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"call_1\",\"type\":\"function\",\"function\":{\"name\":\"test\",\"arguments\":\"\"}}]},\"finish_reason\":null}]}\n";
    let events = streaming::parse_sse_lines(input);
    assert_eq!(events.len(), 1);
    match &events[0] {
        Ok(SSEEvent::ContentBlockStart { .. }) => {}
        _ => panic!("expected only ContentBlockStart (no PartialJson for empty args)"),
    }
}

#[test]
fn test_sse_parse_tool_call_no_id_no_name_no_args() {
    let input = "data: {\"choices\":[{\"index\":0,\"delta\":{\"tool_calls\":[{\"index\":0}]},\"finish_reason\":null}]}\n";
    let events = streaming::parse_sse_lines(input);
    assert_eq!(events.len(), 1);
    match &events[0] {
        Ok(SSEEvent::Text { text }) => assert!(text.is_empty()),
        _ => panic!("expected fallback Text event"),
    }
}

// --- Request Serialization: Standard vs Reasoning Models ---

#[test]
fn test_build_request_body_standard_model_uses_max_tokens() {
    let provider =
        OpenAiProvider::new(make_config(Some("sk-test")), "gpt-4o".to_string()).unwrap();
    let body = provider.build_request_body(
        "gpt-4o".to_string(),
        vec![types::OpenAiMessage {
            role: "user".to_string(),
            content: Some(serde_json::Value::String("test".to_string())),
            tool_calls: None,
            tool_call_id: None,
            name: None,
        }],
        vec![],
        Some(1024),
        Some(0.3),
        None,
        Some(true),
        None,
        None,
        None,
        None,
        None,
    );
    assert_eq!(body.max_tokens, Some(1024));
    assert_eq!(body.max_completion_tokens, None);
    assert_eq!(body.reasoning_effort, None);
    assert_eq!(body.stream, Some(true));
    assert_eq!(body.model, "gpt-4o");

    let json = serde_json::to_string(&body).unwrap();
    assert!(json.contains("\"max_tokens\":1024"));
    assert!(!json.contains("max_completion_tokens"));
    assert!(!json.contains("reasoning_effort"));
}

#[test]
fn test_build_request_body_reasoning_model_uses_max_completion_tokens() {
    let provider = OpenAiProvider::new(make_config(Some("sk-test")), "o3".to_string()).unwrap();
    let body = provider.build_request_body(
        "o3".to_string(),
        vec![types::OpenAiMessage {
            role: "user".to_string(),
            content: Some(serde_json::Value::String("solve it".to_string())),
            tool_calls: None,
            tool_call_id: None,
            name: None,
        }],
        vec![],
        Some(8192),
        None,
        Some(&EffortLevel::Max),
        Some(false),
        None,
        None,
        None,
        None,
        None,
    );
    assert_eq!(body.max_tokens, None);
    assert_eq!(body.max_completion_tokens, Some(8192));
    assert_eq!(body.reasoning_effort, Some("xhigh".to_string()));

    let json = serde_json::to_string(&body).unwrap();
    assert!(json.contains("\"max_completion_tokens\":8192"));
    assert!(json.contains("\"reasoning_effort\":\"xhigh\""));
    assert!(!json.contains("\"max_tokens\":"));
}

#[test]
fn test_build_request_body_effort_levels() {
    let provider =
        OpenAiProvider::new(make_config(Some("sk-test")), "o4-mini".to_string()).unwrap();

    let cases = vec![
        (EffortLevel::Low, "low"),
        (EffortLevel::Medium, "medium"),
        (EffortLevel::High, "high"),
        (EffortLevel::Xhigh, "xhigh"),
        (EffortLevel::Max, "xhigh"),
    ];

    for (effort, expected_str) in cases {
        let body = provider.build_request_body(
            "o4-mini".to_string(),
            vec![],
            vec![],
            Some(4096),
            None,
            Some(&effort),
            None,
            None,
            None,
            None,
            None,
            None,
        );
        assert_eq!(
            body.reasoning_effort,
            Some(expected_str.to_string()),
            "EffortLevel::{:?} should map to {}",
            effort,
            expected_str
        );
    }
}

#[test]
fn test_build_request_body_standard_model_no_effort() {
    let provider =
        OpenAiProvider::new(make_config(Some("sk-test")), "gpt-4o".to_string()).unwrap();
    let body = provider.build_request_body(
        "gpt-4o".to_string(),
        vec![],
        vec![],
        Some(2048),
        Some(0.5),
        Some(&EffortLevel::High),
        None,
        None,
        None,
        None,
        None,
        None,
    );
    assert_eq!(body.max_tokens, Some(2048));
    assert_eq!(body.max_completion_tokens, None);
    assert_eq!(body.reasoning_effort, None);
}

#[test]
fn test_build_request_body_with_tools() {
    let provider =
        OpenAiProvider::new(make_config(Some("sk-test")), "gpt-4o".to_string()).unwrap();
    let tools = vec![
        serde_json::json!({
            "type": "function",
            "function": {
                "name": "get_weather",
                "description": "Get current weather",
                "parameters": {"type": "object", "properties": {"location": {"type": "string"}}}
            }
        }),
        serde_json::json!({
            "type": "function",
            "function": {
                "name": "read_file",
                "description": "Read a file",
                "parameters": {"type": "object", "properties": {"path": {"type": "string"}}}
            }
        }),
    ];
    let body = provider.build_request_body(
        "gpt-4o".to_string(),
        vec![],
        tools,
        Some(4096),
        None,
        None,
        Some(true),
        None,
        None,
        None,
        None,
        None,
    );
    assert!(body.tools.is_some());
    let tools_val = body.tools.as_ref().unwrap();
    assert_eq!(tools_val.len(), 2);
    let json = serde_json::to_string(&body).unwrap();
    assert!(json.contains("get_weather"));
    assert!(json.contains("read_file"));
}

#[test]
fn test_build_request_body_empty_tools_omits_field() {
    let provider =
        OpenAiProvider::new(make_config(Some("sk-test")), "gpt-4o".to_string()).unwrap();
    let body = provider.build_request_body(
        "gpt-4o".to_string(),
        vec![],
        vec![],
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
    );
    assert!(body.tools.is_none());
    let json = serde_json::to_string(&body).unwrap();
    assert!(!json.contains("\"tools\""));
}

#[test]
fn test_build_request_body_no_max_tokens_no_temperature() {
    let provider =
        OpenAiProvider::new(make_config(Some("sk-test")), "gpt-4o".to_string()).unwrap();
    let body = provider.build_request_body(
        "gpt-4o".to_string(),
        vec![types::OpenAiMessage {
            role: "user".to_string(),
            content: Some(serde_json::Value::String("hi".to_string())),
            tool_calls: None,
            tool_call_id: None,
            name: None,
        }],
        vec![],
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
    );
    assert_eq!(body.max_tokens, None);
    assert_eq!(body.temperature, None);
    let json = serde_json::to_string(&body).unwrap();
    assert!(!json.contains("\"max_tokens\""));
    assert!(!json.contains("\"temperature\""));
}

// --- SSE Parse: realistic full streaming conversation ---

#[test]
fn test_sse_parse_realistic_text_stream() {
    let input = "\
data: {\"id\":\"chatcmpl-abc\",\"object\":\"chat.completion.chunk\",\"created\":1234567890,\"model\":\"gpt-4o\",\"choices\":[{\"index\":0,\"delta\":{\"role\":\"assistant\",\"content\":\"\"},\"finish_reason\":null}]}\n\
\n\
data: {\"id\":\"chatcmpl-abc\",\"object\":\"chat.completion.chunk\",\"created\":1234567890,\"model\":\"gpt-4o\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"Rust\"},\"finish_reason\":null}]}\n\
\n\
data: {\"id\":\"chatcmpl-abc\",\"object\":\"chat.completion.chunk\",\"created\":1234567890,\"model\":\"gpt-4o\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\" is\"},\"finish_reason\":null}]}\n\
\n\
data: {\"id\":\"chatcmpl-abc\",\"object\":\"chat.completion.chunk\",\"created\":1234567890,\"model\":\"gpt-4o\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\" fast.\"},\"finish_reason\":null}]}\n\
\n\
data: {\"id\":\"chatcmpl-abc\",\"object\":\"chat.completion.chunk\",\"created\":1234567890,\"model\":\"gpt-4o\",\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"stop\"}]}\n\
\n\
data: [DONE]\n";
    let events = streaming::parse_sse_lines(input);

    let content_deltas: Vec<String> = events
        .iter()
        .filter_map(|e| match e {
            Ok(SSEEvent::ContentBlockDelta {
                delta: crate::provider::ContentDelta::Text { text },
                ..
            }) => Some(text.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(content_deltas, vec!["Rust", " is", " fast."]);

    let has_block_stop = events
        .iter()
        .any(|e| matches!(e, Ok(SSEEvent::ContentBlockStop { .. })));
    let has_msg_delta = events.iter().any(|e| {
        matches!(
            e,
            Ok(SSEEvent::MessageDelta {
                stop_reason: Some(s),
                ..
            }) if s == "end_turn"
        )
    });
    let has_msg_stop = events
        .iter()
        .any(|e| matches!(e, Ok(SSEEvent::MessageStop)));
    assert!(has_block_stop, "should have ContentBlockStop");
    assert!(has_msg_delta, "should have MessageDelta with stop");
    assert!(has_msg_stop, "should have MessageStop");
}
