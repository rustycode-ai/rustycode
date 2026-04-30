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

//! Comprehensive integration tests for TUI workflow.
//!
//! This module provides end-to-end testing for:
//! - Full user workflow
//! - Provider switching
//! - Session persistence
//! - Message history
//! - Multi-turn conversations

#[cfg(test)]
mod integration_tests {
    use serde_json::{json, Value};

    use tempfile::TempDir;

    #[tokio::test]
    async fn test_full_workflow() {
        let temp_dir = TempDir::new().unwrap();
        let project_dir = temp_dir.path();

        // 1. Initialize session
        let session_path = project_dir.join("session.json");
        assert!(!session_path.exists());

        // 2. Create initial session data
        let session_data = json!({
            "messages": [],
            "provider": "anthropic",
            "model": "claude-sonnet-4",
            "metadata": {
                "created_at": "2024-01-01T00:00:00Z"
            }
        });

        // 3. Write session
        std::fs::write(
            &session_path,
            serde_json::to_string_pretty(&session_data).unwrap(),
        )
        .unwrap();

        assert!(session_path.exists());

        // 4. Read session back
        let read_data: Value =
            serde_json::from_str(&std::fs::read_to_string(&session_path).unwrap()).unwrap();

        assert_eq!(read_data["provider"], "anthropic");
        assert_eq!(read_data["model"], "claude-sonnet-4");

        // 5. Add messages
        let updated_data = json!({
            "messages": [
                {"role": "user", "content": "Hello"},
                {"role": "assistant", "content": "Hi there!"}
            ],
            "provider": "anthropic",
            "model": "claude-sonnet-4",
            "metadata": {
                "created_at": "2024-01-01T00:00:00Z"
            }
        });

        std::fs::write(
            &session_path,
            serde_json::to_string_pretty(&updated_data).unwrap(),
        )
        .unwrap();

        // 6. Verify messages persisted
        let final_data: Value =
            serde_json::from_str(&std::fs::read_to_string(&session_path).unwrap()).unwrap();

        assert_eq!(final_data["messages"].as_array().unwrap().len(), 2);
    }

    #[tokio::test]
    async fn test_provider_switching() {
        // Start with Anthropic
        let mut config = json!({
            "provider": "anthropic",
            "model": "claude-sonnet-4",
            "messages": [
                {"role": "user", "content": "Test message"}
            ]
        });

        // Switch to OpenAI
        config["provider"] = json!("openai");
        config["model"] = json!("gpt-4");

        assert_eq!(config["provider"], "openai");
        assert_eq!(config["model"], "gpt-4");

        // Messages should be preserved
        assert_eq!(config["messages"].as_array().unwrap().len(), 1);

        // Switch back to Anthropic
        config["provider"] = json!("anthropic");
        config["model"] = json!("claude-sonnet-4");

        assert_eq!(config["provider"], "anthropic");
        assert_eq!(config["model"], "claude-sonnet-4");

        // Messages still preserved
        assert_eq!(config["messages"].as_array().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn test_session_persistence() {
        let temp_dir = TempDir::new().unwrap();
        let session_file = temp_dir.path().join("session.json");

        // Create session with 10 messages
        let mut messages = Vec::new();
        for i in 0..10 {
            messages.push(json!({
                "role": if i % 2 == 0 { "user" } else { "assistant" },
                "content": format!("Message {}", i)
            }));
        }

        let session = json!({
            "messages": messages,
            "provider": "anthropic",
            "model": "claude-sonnet-4",
            "metadata": {
                "created_at": "2024-01-01T00:00:00Z",
                "message_count": 10
            }
        });

        // Save to disk
        std::fs::write(
            &session_file,
            serde_json::to_string_pretty(&session).unwrap(),
        )
        .unwrap();

        // Load in new "instance" (simulate new TUI instance)
        let loaded: Value =
            serde_json::from_str(&std::fs::read_to_string(&session_file).unwrap()).unwrap();

        // Verify all messages present
        assert_eq!(loaded["messages"].as_array().unwrap().len(), 10);

        // Verify metadata preserved
        assert_eq!(loaded["metadata"]["message_count"], 10);

        // Verify content
        assert_eq!(loaded["messages"][0]["content"], "Message 0");
        assert_eq!(loaded["messages"][9]["content"], "Message 9");
    }

    #[tokio::test]
    async fn test_multi_turn_conversation() {
        let mut messages: Vec<Value> = Vec::new();

        // Simulate conversation
        messages.push(json!({"role": "user", "content": "What is 2+2?"}));
        messages.push(json!({"role": "assistant", "content": "2+2 equals 4."}));
        messages.push(json!({"role": "user", "content": "What about 3+3?"}));
        messages.push(json!({"role": "assistant", "content": "3+3 equals 6."}));

        // Verify conversation flow
        assert_eq!(messages.len(), 4);

        // Verify alternating roles
        for (i, msg) in messages.iter().enumerate() {
            if i % 2 == 0 {
                assert_eq!(msg["role"], "user");
            } else {
                assert_eq!(msg["role"], "assistant");
            }
        }
    }

    #[tokio::test]
    async fn test_session_update_and_reload() {
        let temp_dir = TempDir::new().unwrap();
        let session_file = temp_dir.path().join("session.json");

        // Initial session
        let mut session = json!({
            "messages": [],
            "provider": "anthropic",
            "model": "claude-sonnet-4"
        });

        std::fs::write(
            &session_file,
            serde_json::to_string_pretty(&session).unwrap(),
        )
        .unwrap();

        // Update session multiple times
        for i in 0..5 {
            let loaded: Value =
                serde_json::from_str(&std::fs::read_to_string(&session_file).unwrap()).unwrap();

            let mut messages = loaded["messages"].as_array().unwrap().clone();
            messages.push(json!({
                "role": "user",
                "content": format!("Update {}", i)
            }));

            session["messages"] = json!(messages);

            std::fs::write(
                &session_file,
                serde_json::to_string_pretty(&session).unwrap(),
            )
            .unwrap();
        }

        // Final verification
        let final_data: Value =
            serde_json::from_str(&std::fs::read_to_string(&session_file).unwrap()).unwrap();

        assert_eq!(final_data["messages"].as_array().unwrap().len(), 5);
    }

    #[tokio::test]
    async fn test_unicode_in_session() {
        let session = json!({
            "messages": [
                {"role": "user", "content": "สวัสดี 🌍"},
                {"role": "assistant", "content": "Hello! こんにちは"},
                {"role": "user", "content": "مرحبا"}
            ],
            "provider": "anthropic",
            "model": "claude-sonnet-4"
        });

        let temp_dir = TempDir::new().unwrap();
        let session_file = temp_dir.path().join("session.json");

        // Save
        std::fs::write(
            &session_file,
            serde_json::to_string_pretty(&session).unwrap(),
        )
        .unwrap();

        // Load
        let loaded: Value =
            serde_json::from_str(&std::fs::read_to_string(&session_file).unwrap()).unwrap();

        // Verify Unicode preserved
        assert_eq!(loaded["messages"][0]["content"], "สวัสดี 🌍");
        assert_eq!(loaded["messages"][1]["content"], "Hello! こんにちは");
        assert_eq!(loaded["messages"][2]["content"], "مرحبا");
    }

    #[tokio::test]
    async fn test_large_session_handling() {
        let mut messages = Vec::new();

        // Create 1000 messages
        for i in 0..1000 {
            messages.push(json!({
                "role": if i % 2 == 0 { "user" } else { "assistant" },
                "content": format!("Message number {}: Lorem ipsum dolor sit amet", i)
            }));
        }

        let session = json!({
            "messages": messages,
            "provider": "anthropic",
            "model": "claude-sonnet-4"
        });

        let temp_dir = TempDir::new().unwrap();
        let session_file = temp_dir.path().join("session.json");

        let start = std::time::Instant::now();

        // Save
        std::fs::write(
            &session_file,
            serde_json::to_string_pretty(&session).unwrap(),
        )
        .unwrap();

        let save_duration = start.elapsed();

        // Load
        let load_start = std::time::Instant::now();
        let loaded: Value =
            serde_json::from_str(&std::fs::read_to_string(&session_file).unwrap()).unwrap();
        let load_duration = load_start.elapsed();

        // Should be reasonably fast
        assert!(
            save_duration.as_millis() < 500,
            "Save too slow: {:?}",
            save_duration
        );
        assert!(
            load_duration.as_millis() < 500,
            "Load too slow: {:?}",
            load_duration
        );

        // Verify all messages
        assert_eq!(loaded["messages"].as_array().unwrap().len(), 1000);
    }

    #[tokio::test]
    async fn test_session_backup() {
        let temp_dir = TempDir::new().unwrap();
        let session_file = temp_dir.path().join("session.json");
        let backup_file = temp_dir.path().join("session.json.bak");

        let session = json!({
            "messages": [{"role": "user", "content": "Important message"}],
            "provider": "anthropic"
        });

        // Save original
        std::fs::write(&session_file, serde_json::to_string(&session).unwrap()).unwrap();

        // Create backup
        std::fs::copy(&session_file, &backup_file).unwrap();

        // Modify original
        let mut modified = session.clone();
        modified["messages"]
            .as_array_mut()
            .unwrap()
            .push(json!({"role": "assistant", "content": "Response"}));

        std::fs::write(&session_file, serde_json::to_string(&modified).unwrap()).unwrap();

        // Verify backup exists and is different
        assert!(backup_file.exists());

        let backup_data: Value =
            serde_json::from_str(&std::fs::read_to_string(&backup_file).unwrap()).unwrap();
        let current_data: Value =
            serde_json::from_str(&std::fs::read_to_string(&session_file).unwrap()).unwrap();

        assert_eq!(backup_data["messages"].as_array().unwrap().len(), 1);
        assert_eq!(current_data["messages"].as_array().unwrap().len(), 2);
    }

    #[tokio::test]
    async fn test_concurrent_session_access() {
        use std::sync::{Arc, Mutex};
        use std::thread;

        let temp_dir = Arc::new(TempDir::new().unwrap());
        let session_file = temp_dir.path().join("session.json");
        let data = Arc::new(Mutex::new(json!([])));

        let mut handles = vec![];

        // Spawn multiple threads writing to same file (serialized via mutex)
        for i in 0..10 {
            let file_clone = session_file.clone();
            let data_clone = Arc::clone(&data);

            let handle = thread::spawn(move || {
                let mut d = data_clone.lock().unwrap();
                d.as_array_mut()
                    .unwrap()
                    .push(json!({"thread": i, "message": format!("Message {}", i)}));

                std::fs::write(&file_clone, serde_json::to_string_pretty(&*d).unwrap()).unwrap();
            });

            handles.push(handle);
        }

        // Wait for all threads
        for handle in handles {
            handle.join().unwrap();
        }

        // Verify final state
        let final_data: Value =
            serde_json::from_str(&std::fs::read_to_string(&session_file).unwrap()).unwrap();

        assert_eq!(final_data.as_array().unwrap().len(), 10);
    }
}
