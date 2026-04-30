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

// Unit tests for Ctrl+R regenerate response functionality
// Tests the regenerate_last_response() logic in event_loop.rs

#[cfg(test)]
mod regenerate_tests {
    #![cfg(any())]

    use rustycode_tui::app::event_loop::App;
    use rustycode_tui::ui::message::{Message, MessageRole};

    // Helper function to create a test message
    fn create_message(role: MessageRole, content: &str) -> Message {
        match role {
            MessageRole::User => Message::user(content.to_string()),
            MessageRole::Assistant => Message::assistant(content.to_string()),
            MessageRole::System => Message::system(content.to_string()),
        }
    }

    // Helper to count messages by role
    fn count_messages_by_role(messages: &[Message], role: MessageRole) -> usize {
        messages.iter().filter(|m| m.role == role).count()
    }

    #[test]
    fn test_regenerate_with_no_messages() {
        // Test that regenerate handles empty message list gracefully
        let mut app = App::new();
        let initial_count = app.messages.len();

        // Attempt to regenerate with no messages
        let result = app.regenerate_last_response();

        // Should not panic
        assert!(result.is_ok());

        // Should add an error system message
        assert_eq!(app.messages.len(), initial_count + 1);

        // Last message should be a system error
        let last_msg = &app.messages[app.messages.len() - 1];
        assert_eq!(last_msg.role, MessageRole::System);
        assert!(last_msg.content.contains("No AI response"));
    }

    #[test]
    fn test_regenerate_with_only_user_message() {
        // Test that regenerate handles only user message
        let mut app = App::new();
        app.messages
            .push(create_message(MessageRole::User, "hello"));

        let initial_count = app.messages.len();
        let result = app.regenerate_last_response();

        // Should not panic
        assert!(result.is_ok());

        // Should add an error system message
        assert_eq!(app.messages.len(), initial_count + 1);

        // Last message should be a system error
        let last_msg = &app.messages[app.messages.len() - 1];
        assert_eq!(last_msg.role, MessageRole::System);
        assert!(last_msg.content.contains("No AI response"));
    }

    #[test]
    fn test_regenerate_removes_last_assistant_message() {
        // Test that regenerate removes the last assistant message
        let mut app = App::new();

        // Create conversation: user -> assistant -> user -> assistant
        app.messages
            .push(create_message(MessageRole::User, "first"));
        app.messages
            .push(create_message(MessageRole::Assistant, "response 1"));
        app.messages
            .push(create_message(MessageRole::User, "second"));
        app.messages
            .push(create_message(MessageRole::Assistant, "response 2"));

        let assistant_count_before = count_messages_by_role(&app.messages, MessageRole::Assistant);
        assert_eq!(assistant_count_before, 2);

        // Regenerate
        let result = app.regenerate_last_response();
        assert!(result.is_ok());

        // Should have one less assistant message
        let assistant_count_after = count_messages_by_role(&app.messages, MessageRole::Assistant);
        assert_eq!(assistant_count_after, 1);

        // First assistant message should still be there
        let first_asst = &app
            .messages
            .iter()
            .filter(|m| m.role == MessageRole::Assistant)
            .next()
            .unwrap();
        assert_eq!(first_asst.content, "response 1");

        // User messages should be intact
        let user_count = count_messages_by_role(&app.messages, MessageRole::User);
        assert_eq!(user_count, 2);
    }

    #[test]
    fn test_regenerate_preserves_earlier_messages() {
        // Test that regenerate only affects the last exchange
        let mut app = App::new();

        // Create multi-turn conversation
        for i in 1..=5 {
            app.messages
                .push(create_message(MessageRole::User, &format!("user {}", i)));
            app.messages.push(create_message(
                MessageRole::Assistant,
                &format!("response {}", i),
            ));
        }

        let total_before = app.messages.len();
        assert_eq!(total_before, 10);

        // Regenerate
        let result = app.regenerate_last_response();
        assert!(result.is_ok());

        // Should have: 10 original - 1 last assistant + 1 system message
        let total_after = app.messages.len();
        assert_eq!(total_after, 10);

        // First 4 exchanges should be intact
        for i in 1..=4 {
            let user_msg = &app.messages[(i - 1) * 2];
            assert_eq!(user_msg.content, format!("user {}", i));

            let asst_msg = &app.messages[(i - 1) * 2 + 1];
            assert_eq!(asst_msg.content, format!("response {}", i));
        }

        // 5th user message should still be there
        let fifth_user = &app.messages[8];
        assert_eq!(fifth_user.content, "user 5");

        // 5th assistant message should be removed (replaced by system message)
        let last_msg = &app.messages[app.messages.len() - 1];
        assert_eq!(last_msg.role, MessageRole::System);
        assert!(last_msg.content.contains("Regenerating"));
    }

    #[test]
    fn test_regenerate_during_streaming() {
        // Test that regenerate is blocked during streaming
        let mut app = App::new();

        // Set up streaming state
        app.is_streaming = true;
        app.messages.push(create_message(MessageRole::User, "test"));
        app.messages
            .push(create_message(MessageRole::Assistant, "response"));

        let result = app.regenerate_last_response();
        assert!(result.is_ok());

        // Should add error message
        let last_msg = &app.messages[app.messages.len() - 1];
        assert_eq!(last_msg.role, MessageRole::System);
        assert!(last_msg
            .content
            .contains("Cannot regenerate while streaming"));

        // Original messages should be unchanged
        assert_eq!(app.messages.len(), 3);
        assert_eq!(app.messages[1].content, "response");
    }

    #[test]
    fn test_regenerate_multiple_times() {
        // Test multiple sequential regenerations
        let mut app = App::new();

        app.messages
            .push(create_message(MessageRole::User, "question"));
        app.messages
            .push(create_message(MessageRole::Assistant, "answer 1"));

        // First regeneration
        app.regenerate_last_response().unwrap();
        assert_eq!(app.messages.len(), 3); // user + asst + system

        // Add new response (simulating LLM)
        app.messages.pop(); // Remove system message
        app.messages
            .push(create_message(MessageRole::Assistant, "answer 2"));

        // Second regeneration
        app.regenerate_last_response().unwrap();
        assert_eq!(app.messages.len(), 3);

        // Should still only have one assistant message
        let assistant_count = count_messages_by_role(&app.messages, MessageRole::Assistant);
        assert_eq!(assistant_count, 1);
    }

    #[test]
    fn test_regenerate_system_message_format() {
        // Test that the system message has correct format
        let mut app = App::new();

        app.messages.push(create_message(MessageRole::User, "test"));
        app.messages
            .push(create_message(MessageRole::Assistant, "response"));

        app.regenerate_last_response().unwrap();

        let system_msg = &app.messages[app.messages.len() - 1];
        assert_eq!(system_msg.role, MessageRole::System);

        // Check for emoji
        assert!(system_msg.content.contains("🔄"));

        // Check for key text
        assert!(system_msg.content.contains("Regenerating"));
    }

    #[test]
    fn test_regenerate_dirty_flag() {
        // Test that regenerate sets the dirty flag
        let mut app = App::new();

        app.messages.push(create_message(MessageRole::User, "test"));
        app.messages
            .push(create_message(MessageRole::Assistant, "response"));

        app.dirty = false;
        app.regenerate_last_response().unwrap();

        // Dirty flag should be set
        assert!(app.dirty);
    }

    #[test]
    fn test_regenerate_updates_scroll_offset() {
        // Test that regeneration updates scroll position
        let mut app = App::new();

        app.messages.push(create_message(MessageRole::User, "test"));
        app.messages
            .push(create_message(MessageRole::Assistant, "response"));

        app.viewport_height = 10;
        app.selected_message = 1;
        app.scroll_offset = 0;

        app.regenerate_last_response().unwrap();

        // Should scroll to latest message
        assert_eq!(app.selected_message, app.messages.len() - 1);
    }
}
