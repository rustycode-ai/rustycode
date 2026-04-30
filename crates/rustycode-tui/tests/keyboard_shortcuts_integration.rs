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

//! Integration tests for keyboard shortcuts
//!
//! Tests the keyboard shortcut system with the TUI configuration

#[cfg(test)]
mod tests {
    use rustycode_tui::app::KeyboardShortcutHandler;
    use rustycode_tui::config::BehaviorConfig;

    #[test]
    fn test_keyboard_handler_with_vim_mode_enabled() {
        let mut handler = KeyboardShortcutHandler::new(true);
        assert!(handler.is_vim_enabled());

        // Test that Vim keys work when enabled
        let action = handler.handle_vim_key('j');
        assert_eq!(
            action,
            rustycode_tui::app::KeyboardAction::MoveDown,
            "j should move down in Vim mode"
        );

        let action = handler.handle_vim_key('k');
        assert_eq!(
            action,
            rustycode_tui::app::KeyboardAction::MoveUp,
            "k should move up in Vim mode"
        );
    }

    #[test]
    fn test_keyboard_handler_with_vim_mode_disabled() {
        let handler = KeyboardShortcutHandler::new(false);
        assert!(!handler.is_vim_enabled());

        // Handler can still handle keys, but the event loop will check is_vim_enabled()
        // before calling handle_vim_key
    }

    #[test]
    fn test_behavior_config_vim_setting() {
        let config = BehaviorConfig {
            auto_save_interval_seconds: 30,
            max_history_size: 1000,
            confirm_on_dangerous: true,
            yolo_mode: false,
            auto_scroll: true,
            stream_responses: true,
            mouse_scroll_speed: 3,
            vim_enabled: true,
            reduced_motion: false,
        };

        assert!(config.vim_enabled);
    }

    #[test]
    fn test_vim_chord_detection_in_handler() {
        let mut handler = KeyboardShortcutHandler::new(true);

        // First 'g' should not trigger action
        let action = handler.handle_vim_key('g');
        assert_eq!(action, rustycode_tui::app::KeyboardAction::None);
        assert!(handler.vim_chord_state.pending_g);

        // Second 'g' should trigger jump to start
        let action = handler.handle_vim_key('g');
        assert_eq!(action, rustycode_tui::app::KeyboardAction::JumpToStart);
        assert!(!handler.vim_chord_state.pending_g);
    }

    #[test]
    fn test_handler_reset_clears_state() {
        let mut handler = KeyboardShortcutHandler::new(true);

        // Set up state
        handler.handle_vim_key('g');
        assert!(handler.vim_chord_state.pending_g);

        // Reset should clear state
        handler.reset();
        assert!(!handler.vim_chord_state.pending_g);
    }
}
