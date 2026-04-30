#![allow(
    clippy::bool_to_int_with_if,
    clippy::branches_sharing_code,
    clippy::case_sensitive_file_extension_comparisons,
    clippy::cast_lossless,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::collapsible_else_if,
    clippy::collection_is_never_read,
    clippy::default_trait_access,
    clippy::doc_markdown,
    clippy::equatable_if_let,
    clippy::expect_used,
    clippy::explicit_iter_loop,
    clippy::float_cmp,
    clippy::if_not_else,
    clippy::ignore_without_reason,
    clippy::ignored_unit_patterns,
    clippy::implicit_clone,
    clippy::imprecise_flops,
    clippy::items_after_statements,
    clippy::iter_on_single_items,
    clippy::literal_string_with_formatting_args,
    clippy::manual_assert,
    clippy::manual_let_else,
    clippy::manual_string_new,
    clippy::map_unwrap_or,
    clippy::match_same_arms,
    clippy::missing_const_for_fn,
    clippy::needless_collect,
    clippy::needless_continue,
    clippy::needless_pass_by_value,
    clippy::needless_raw_string_hashes,
    clippy::no_effect_underscore_binding,
    clippy::option_if_let_else,
    clippy::range_plus_one,
    clippy::redundant_clone,
    clippy::redundant_closure_for_method_calls,
    clippy::redundant_else,
    clippy::ref_option,
    clippy::search_is_some,
    clippy::semicolon_if_nothing_returned,
    clippy::significant_drop_in_scrutinee,
    clippy::significant_drop_tightening,
    clippy::similar_names,
    clippy::single_char_pattern,
    clippy::single_match_else,
    clippy::struct_excessive_bools,
    clippy::struct_field_names,
    clippy::suboptimal_flops,
    clippy::too_many_lines,
    clippy::trivially_copy_pass_by_ref,
    clippy::unchecked_time_subtraction,
    clippy::uninlined_format_args,
    clippy::unnecessary_debug_formatting,
    clippy::unnecessary_literal_bound,
    clippy::unnecessary_wraps,
    clippy::unreadable_literal,
    clippy::unused_async,
    clippy::unused_peekable,
    clippy::unused_self,
    clippy::unwrap_used,
    clippy::use_self,
    clippy::used_underscore_binding,
    clippy::useless_let_if_seq
)]

//! Ollama integration tests.
//!
//! This module provides testing for Ollama provider:
//! - Connection testing
//! - Streaming responses
//! - Model listing
//! - Error handling

#[cfg(test)]
#[cfg(feature = "ollama-tests")]
mod ollama_tests {
    use futures::StreamExt;
    use rustycode_llm::{
        ChatMessage, CompletionRequest, LLMProvider, OllamaProvider, ProviderConfig,
    };
    use std::time::Duration;
    use tokio::time::timeout;

    /// Helper to check if Ollama is running
    async fn ollama_running() -> bool {
        use tokio::net::TcpStream;
        timeout(
            Duration::from_secs(1),
            TcpStream::connect("127.0.0.1:11434"),
        )
        .await
        .map(|r| r.is_ok())
        .unwrap_or(false)
    }

    /// Helper to get Ollama base URL
    fn get_ollama_url() -> String {
        std::env::var("OLLAMA_BASE_URL").unwrap_or_else(|_| "http://localhost:11434".to_string())
    }

    /// Helper to get test model name
    fn get_test_model() -> String {
        std::env::var("OLLAMA_TEST_MODEL").unwrap_or_else(|_| "llama3".to_string())
    }

    #[tokio::test]
    async fn test_ollama_connection() {
        if !ollama_running().await {
            println!("Skipping Ollama test - not running");
            return;
        }

        let config = ProviderConfig {
            base_url: Some(get_ollama_url()),
            ..Default::default()
        };
        let provider = OllamaProvider::new(config).expect("failed to create OllamaProvider");

        // Try to list models (this tests connection)
        let result = timeout(Duration::from_secs(5), LLMProvider::list_models(&provider)).await;

        assert!(result.is_ok(), "Connection timeout");
        let models = result.unwrap().unwrap();

        // Should have at least one model
        assert!(!models.is_empty(), "No models found");
    }

    #[tokio::test]
    async fn test_ollama_simple_completion() {
        if !ollama_running().await {
            println!("Skipping Ollama test - not running");
            return;
        }

        let config = ProviderConfig {
            base_url: Some(get_ollama_url()),
            ..Default::default()
        };
        let provider = OllamaProvider::new(config).expect("failed to create OllamaProvider");
        let request = CompletionRequest::new(
            get_test_model(),
            vec![ChatMessage::user("Say 'Hello, Ollama!'")],
        );

        let result = timeout(
            Duration::from_secs(30),
            LLMProvider::complete(&provider, request),
        )
        .await;

        assert!(result.is_ok(), "Request timeout");
        let completion = result.unwrap().unwrap();

        assert!(!completion.content.is_empty(), "Empty response");

        // Should contain "Hello"
        assert!(
            completion.content.to_lowercase().contains("hello"),
            "Unexpected response: {}",
            completion.content
        );
    }

    #[tokio::test]
    async fn test_ollama_streaming() {
        if !ollama_running().await {
            println!("Skipping Ollama test - not running");
            return;
        }

        let config = ProviderConfig {
            base_url: Some(get_ollama_url()),
            ..Default::default()
        };
        let provider = OllamaProvider::new(config).expect("failed to create OllamaProvider");
        let request = CompletionRequest::new(
            get_test_model(),
            vec![ChatMessage::user("Count from 1 to 5")],
        );

        let stream_result = timeout(
            Duration::from_secs(30),
            LLMProvider::complete_stream(&provider, request),
        )
        .await;

        assert!(stream_result.is_ok(), "Stream creation timeout");
        let mut stream = stream_result.unwrap().unwrap();

        let mut full_response = String::new();
        let mut chunk_count = 0;

        // Collect stream chunks
        loop {
            match timeout(Duration::from_secs(5), stream.next()).await {
                Ok(Some(Ok(event))) => {
                    if let Some(t) = event.as_text() {
                        full_response.push_str(&t);
                        chunk_count += 1;
                    }
                }
                Ok(Some(Err(e))) => {
                    panic!("Stream error: {:?}", e);
                }
                Ok(None) => break, // Stream finished
                Err(_) => {
                    panic!("Stream chunk timeout");
                }
            }

            // Safety limit
            if chunk_count > 1000 {
                panic!("Too many chunks - possible infinite stream");
            }
        }

        assert!(!full_response.is_empty(), "Empty streaming response");
        assert!(chunk_count > 0, "No chunks received");

        // Should contain numbers
        assert!(
            full_response.contains("1") || full_response.contains("one"),
            "Streaming failed: {}",
            full_response
        );

        println!("Received {} chunks in streaming response", chunk_count);
    }

    #[tokio::test]
    async fn test_ollama_multi_turn_conversation() {
        if !ollama_running().await {
            println!("Skipping Ollama test - not running");
            return;
        }

        let config = ProviderConfig {
            base_url: Some(get_ollama_url()),
            ..Default::default()
        };
        let provider = OllamaProvider::new(config).expect("failed to create OllamaProvider");

        let messages = vec![
            ChatMessage::user("My name is Alice"),
            ChatMessage::assistant("Hello Alice! Nice to meet you."),
            ChatMessage::user("What's my name?"),
        ];

        let request = CompletionRequest::new(get_test_model(), messages);

        let result = timeout(
            Duration::from_secs(30),
            LLMProvider::complete(&provider, request),
        )
        .await;

        assert!(result.is_ok(), "Request timeout");
        let completion = result.unwrap().unwrap();

        // Should remember the name
        assert!(
            completion.content.to_lowercase().contains("alice"),
            "Model didn't remember context: {}",
            completion.content
        );
    }

    #[tokio::test]
    async fn test_ollama_error_handling() {
        if !ollama_running().await {
            println!("Skipping Ollama test - not running");
            return;
        }

        let config = ProviderConfig {
            base_url: Some(get_ollama_url()),
            ..Default::default()
        };
        let provider = OllamaProvider::new(config).expect("failed to create OllamaProvider");

        // Test with invalid model
        let request = CompletionRequest::new(
            "nonexistent-model-xyz".to_string(),
            vec![ChatMessage::user("Test")],
        );

        let result = timeout(
            Duration::from_secs(10),
            LLMProvider::complete(&provider, request),
        )
        .await;

        assert!(result.is_ok(), "Request timeout");

        // Should get an error
        match result.unwrap() {
            Ok(_) => {
                panic!("Expected error for invalid model");
            }
            Err(e) => {
                println!("Correctly got error for invalid model: {:?}", e);
                // This is expected
            }
        }
    }

    #[tokio::test]
    async fn test_ollama_unicode_support() {
        if !ollama_running().await {
            println!("Skipping Ollama test - not running");
            return;
        }

        let config = ProviderConfig {
            base_url: Some(get_ollama_url()),
            ..Default::default()
        };
        let provider = OllamaProvider::new(config).expect("failed to create OllamaProvider");

        let unicode_tests = vec![
            "สวัสดี",       // Thai
            "こんにちは", // Japanese
            "مرحبا",      // Arabic
            "Hello 🌍",   // Emoji
        ];

        for test_text in unicode_tests {
            let request = CompletionRequest::new(
                get_test_model(),
                vec![ChatMessage::user(format!("Repeat this: {}", test_text))],
            );

            let result = timeout(
                Duration::from_secs(30),
                LLMProvider::complete(&provider, request),
            )
            .await;

            assert!(result.is_ok(), "Request timeout for: {}", test_text);
            let completion = result.unwrap().unwrap();

            assert!(
                !completion.content.is_empty(),
                "Empty response for: {}",
                test_text
            );

            println!(
                "Unicode test '{}' - Response: {}",
                test_text, completion.content
            );
        }
    }

    #[tokio::test]
    async fn test_ollama_long_response() {
        if !ollama_running().await {
            println!("Skipping Ollama test - not running");
            return;
        }

        let config = ProviderConfig {
            base_url: Some(get_ollama_url()),
            ..Default::default()
        };
        let provider = OllamaProvider::new(config).expect("failed to create OllamaProvider");

        let request = CompletionRequest::new(
            get_test_model(),
            vec![ChatMessage::user(
                "Tell me a short story about a robot learning to love",
            )],
        );

        let result = timeout(
            Duration::from_secs(60),
            LLMProvider::complete(&provider, request),
        )
        .await;

        assert!(result.is_ok(), "Request timeout");
        let completion = result.unwrap().unwrap();

        // Should be a longer response
        assert!(
            completion.content.len() > 100,
            "Response too short: {}",
            completion.content
        );

        println!("Long response length: {} chars", completion.content.len());
    }

    #[tokio::test]
    async fn test_ollama_system_prompt() {
        if !ollama_running().await {
            println!("Skipping Ollama test - not running");
            return;
        }

        let config = ProviderConfig {
            base_url: Some(get_ollama_url()),
            ..Default::default()
        };
        let provider = OllamaProvider::new(config).expect("failed to create OllamaProvider");

        let messages = vec![
            ChatMessage::system("You are a helpful assistant who always responds in rhyme."),
            ChatMessage::user("Tell me about Rust"),
        ];

        let request = CompletionRequest::new(get_test_model(), messages);

        let result = timeout(
            Duration::from_secs(30),
            LLMProvider::complete(&provider, request),
        )
        .await;

        assert!(result.is_ok(), "Request timeout");
        let completion = result.unwrap().unwrap();

        assert!(!completion.content.is_empty(), "Empty response");
        println!("Rhyming response: {}", completion.content);
    }
}

// Note: To enable these tests, run:
// cargo test -p rustycode-tui --test ollama_integration_tests --features ollama-tests
//
// Make sure Ollama is running: ollama serve
// And you have a model pulled: ollama pull llama3
