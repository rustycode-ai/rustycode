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
    clippy::useless_let_if_seq,
    dead_code
)]
// Legacy test file for the old `App` architecture.
// The current TUI uses `app::TUI` + modular handlers, so these tests are kept as
// documentation-only placeholders and not compiled as executable tests.

#[cfg(any())]
#[test]
fn test_empty_model_list_no_panic() {
    let cwd = std::env::current_dir().unwrap();
    let mut app = App::new(cwd).unwrap();

    // Even with models available, test that invalid indices don't panic
    // We can't clear the models as they're private, but we can test invalid access

    // Test key combinations that would trigger model selection
    // These should not panic even if models list is empty or index is invalid
    app.handle_key(KeyCode::Char('1'), KeyModifiers::CONTROL);
    app.handle_key(KeyCode::Char('2'), KeyModifiers::CONTROL);
    app.handle_key(KeyCode::Char('3'), KeyModifiers::CONTROL);
    app.handle_key(KeyCode::Char('4'), KeyModifiers::CONTROL);

    // Should not have crashed
    assert!(!app.should_quit);
}

#[cfg(any())]
#[test]
fn test_command_history_navigation_no_panic() {
    let cwd = std::env::current_dir().unwrap();
    let mut app = App::new(cwd).unwrap();

    // Test navigation with empty history (new app)
    app.handle_key(KeyCode::Up, KeyModifiers::CONTROL);
    app.handle_key(KeyCode::Down, KeyModifiers::CONTROL);

    // Add some input
    app.input = "test command 1".to_string();
    app.handle_key(KeyCode::Enter, KeyModifiers::empty());

    app.input = "test command 2".to_string();
    app.handle_key(KeyCode::Enter, KeyModifiers::empty());

    // Navigate through history
    app.handle_key(KeyCode::Up, KeyModifiers::CONTROL);
    app.handle_key(KeyCode::Up, KeyModifiers::CONTROL);
    app.handle_key(KeyCode::Up, KeyModifiers::CONTROL); // Should not panic past start
    app.handle_key(KeyCode::Down, KeyModifiers::CONTROL);
    app.handle_key(KeyCode::Down, KeyModifiers::CONTROL);
    app.handle_key(KeyCode::Down, KeyModifiers::CONTROL); // Should not panic past end

    // Should not have crashed
    assert!(!app.should_quit);
}

#[cfg(any())]
#[test]
fn test_file_finder_limit_enforced() {
    let cwd = std::env::current_dir().unwrap();
    let mut app = App::new(cwd).unwrap();

    // Trigger file finder
    app.handle_key(KeyCode::Char('f'), KeyModifiers::CONTROL);
    app.show_file_finder = true;

    // Perform search (results are limited to 100)
    app.search_files();

    // Results should be bounded by the limit
    // (actual count depends on directory size, but should never exceed 100)
    assert!(
        app.file_finder_results.len() <= 100,
        "File finder results should be limited to 100, got {}",
        app.file_finder_results.len()
    );
}

#[cfg(any())]
#[test]
fn test_rapid_model_switching_no_panic() {
    let cwd = std::env::current_dir().unwrap();
    let mut app = App::new(cwd).unwrap();

    // Rapidly switch models (simulates user mashing Ctrl+1-4)
    for _ in 0..100 {
        app.handle_key(KeyCode::Char('1'), KeyModifiers::CONTROL);
        app.handle_key(KeyCode::Char('2'), KeyModifiers::CONTROL);
        app.handle_key(KeyCode::Char('3'), KeyModifiers::CONTROL);
        app.handle_key(KeyCode::Char('4'), KeyModifiers::CONTROL);
    }

    // Should not have crashed
    assert!(!app.should_quit);
}

#[cfg(any())]
#[test]
fn test_esc_during_streaming_no_panic() {
    let cwd = std::env::current_dir().unwrap();
    let mut app = App::new(cwd).unwrap();

    // Simulate streaming state
    app.is_streaming = true;

    // Press Esc to stop streaming (tests mutex recovery)
    app.handle_key(KeyCode::Esc, KeyModifiers::empty());

    // Should have stopped streaming without panic
    assert!(!app.is_streaming);
    assert!(!app.should_quit);
}

#[cfg(any())]
#[test]
fn test_large_message_set_no_panic() {
    let cwd = std::env::current_dir().unwrap();
    let mut app = App::new(cwd).unwrap();

    // Add many messages (tests memory limits)
    for i in 0..200 {
        app.input = format!("Test message {}", i);
        app.handle_key(KeyCode::Enter, KeyModifiers::empty());
    }

    // Should handle large message set gracefully
    // Messages are limited to CONVERSATION_MAX_MESSAGES (50)
    assert!(!app.should_quit);
}

#[cfg(any())]
#[test]
fn test_special_key_sequences_no_panic() {
    let cwd = std::env::current_dir().unwrap();
    let mut app = App::new(cwd).unwrap();

    // Test various key combinations that might access arrays
    let keys = vec![
        KeyCode::Up,
        KeyCode::Down,
        KeyCode::Left,
        KeyCode::Right,
        KeyCode::PageUp,
        KeyCode::PageDown,
        KeyCode::Home,
        KeyCode::End,
        KeyCode::Tab,
        KeyCode::BackTab,
        KeyCode::Delete,
        KeyCode::Insert,
        KeyCode::F(1),
        KeyCode::F(10),
    ];

    for key in keys {
        app.handle_key(key, KeyModifiers::empty());
        app.handle_key(key, KeyModifiers::CONTROL);
        app.handle_key(key, KeyModifiers::SHIFT);
        app.handle_key(key, KeyModifiers::ALT);
    }

    // Should not panic on any key sequence
    assert!(!app.should_quit);
}
