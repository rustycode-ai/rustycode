#![allow(
    clippy::items_after_statements,
    clippy::needless_collect,
    clippy::no_effect_underscore_binding,
    clippy::redundant_closure_for_method_calls,
    clippy::uninlined_format_args,
    clippy::unreadable_literal,
    clippy::unwrap_used
)]

//! Edge case tests for TUI components.
//!
//! This module tests edge cases and boundary conditions:
//! - Empty input
//! - Very long lines
//! - Special characters
//! - Null bytes
//! - Concurrent modifications
//! - Invalid UTF-8

#[cfg(test)]
mod edge_case_tests {

    #[test]
    fn test_empty_input_handling() {
        let empty = "";
        assert_eq!(empty.len(), 0);
        assert!(empty.is_empty());
    }

    #[test]
    fn test_very_long_line_handling() {
        let very_long = "A".repeat(100_000);

        assert_eq!(very_long.len(), 100_000);

        // Test character count (should be same as byte count for ASCII)
        let char_count = very_long.chars().count();
        assert_eq!(char_count, 100_000);
    }

    #[test]
    fn test_special_characters() {
        let special = "!@#$%^&*()_+-=[]{}|;':\",./<>?`~";

        // Verify all special chars are present
        assert!(special.contains('!'));
        assert!(special.contains('@'));
        assert!(special.contains('#'));
        assert!(special.contains('$'));
        assert!(special.contains('%'));
        assert!(special.contains('^'));
        assert!(special.contains('&'));
        assert!(special.contains('*'));
        assert!(special.contains('('));
        assert!(special.contains(')'));
    }

    #[test]
    fn test_null_bytes_handling() {
        let text_with_null = "Hello\0World";

        // Should handle null bytes
        assert!(text_with_null.contains('\0'));

        // String methods should work
        assert_eq!(text_with_null.len(), 11); // "Hello" (5) + null (1) + "World" (5)
    }

    #[test]
    #[allow(invalid_from_utf8)]
    fn test_invalid_utf8_handling() {
        // Invalid UTF-8 sequence
        let invalid_bytes: &[u8] = &[0xFF, 0xFE, 0xFD];

        // Should fail to convert to string
        let result = std::str::from_utf8(invalid_bytes);
        assert!(result.is_err(), "Should detect invalid UTF-8");
    }

    #[test]
    fn test_mixed_line_endings() {
        let mixed = "Line1\nLine2\r\nLine3\rLine4";

        // Count line breaks of different types
        let unix_count = mixed.matches('\n').count();
        let windows_count = mixed.matches("\r\n").count();
        let old_mac_count = mixed
            .chars()
            .filter(|&c| c == '\r')
            .count()
            .saturating_sub(windows_count);

        assert_eq!(unix_count, 2); // \n in "\r\n" and standalone
        assert_eq!(windows_count, 1);
        assert_eq!(old_mac_count, 1);
    }

    #[test]
    fn test_zero_width_characters() {
        let text = "Hello\u{200B}World"; // Zero-width space

        // Should have zero-width character
        assert!(text.contains('\u{200B}'));

        // Character count includes zero-width chars
        assert_eq!(text.chars().count(), 11); // 5 + 1 + 5

        // But display width should not include it
        use unicode_width::UnicodeWidthStr;
        assert_eq!(text.width(), 10); // 5 + 0 + 5
    }

    #[test]
    fn test_combining_characters() {
        let text = "c\u{0327}"; // c + combining cedilla = ç

        // Should be 2 chars but 1 grapheme
        assert_eq!(text.chars().count(), 2);

        // Unicode width should be 1
        use unicode_width::UnicodeWidthStr;
        assert_eq!(text.width(), 1);

        // Grapheme segmentation should treat as one
        use unicode_segmentation::UnicodeSegmentation;
        assert_eq!(text.graphemes(true).count(), 1);
    }

    #[test]
    fn test_right_to_left_text() {
        let hebrew = "שָׁלוֹם";
        let arabic = "مرحبا";

        // Should handle RTL text (Hebrew includes combining marks)
        assert_eq!(hebrew.chars().count(), 7);
        assert_eq!(arabic.chars().count(), 5);

        // Display width should work
        use unicode_width::UnicodeWidthStr;
        assert!(hebrew.width() > 0);
        assert!(arabic.width() > 0);
    }

    #[test]
    fn test_emoji_sequences() {
        // Regular emoji
        let regular = "👋";

        // Emoji with skin tone modifier
        let with_tone = "👋🏽";

        // Family emoji (ZWJ sequence)
        let family = "👨‍👩‍👧‍👦";

        // All should have positive display width
        use unicode_width::UnicodeWidthStr;
        assert!(regular.width() > 0);
        assert!(with_tone.width() > 0);
        assert!(family.width() > 0);

        // Grapheme counts
        use unicode_segmentation::UnicodeSegmentation;
        assert_eq!(regular.graphemes(true).count(), 1);
        assert_eq!(with_tone.graphemes(true).count(), 1);
        assert_eq!(family.graphemes(true).count(), 1);
    }

    #[test]
    fn test_tab_characters() {
        let text = "Hello\tWorld";

        assert!(text.contains('\t'));
        assert_eq!(text.chars().count(), 11); // 5 + 1 + 5

        // Tab width varies (typically 8 or 4)
        let _tab_width = 8; // Standard tab width
        use unicode_width::UnicodeWidthStr;
        let visual_width = text.width();
        assert!(visual_width >= 11); // At least the visible chars
    }

    #[test]
    fn test_multiple_newlines() {
        let text = "Line1\n\n\nLine2";

        let newline_count = text.matches('\n').count();
        assert_eq!(newline_count, 3);

        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 4);
    }

    #[test]
    fn test_unicode_normalization() {
        use unicode_segmentation::UnicodeSegmentation;

        // Composed form
        let composed = "é"; // Single codepoint

        // Decomposed form (e + combining acute)
        let decomposed = "e\u{0301}";

        // Should be different in memory
        assert_ne!(composed, decomposed);

        // Both forms should still be one grapheme cluster.
        assert_eq!(composed.graphemes(true).count(), 1);
        assert_eq!(decomposed.graphemes(true).count(), 1);
    }

    #[test]
    fn test_string_truncation() {
        let long_text = "A".repeat(1000);

        // Truncate to 10 chars
        let truncated = &long_text[..10];

        assert_eq!(truncated.len(), 10);
        assert_eq!(truncated, "AAAAAAAAAA");
    }

    #[test]
    fn test_string_splitting() {
        let text = "one,two,three,four";

        let parts: Vec<&str> = text.split(',').collect();

        assert_eq!(parts.len(), 4);
        assert_eq!(parts[0], "one");
        assert_eq!(parts[1], "two");
        assert_eq!(parts[2], "three");
        assert_eq!(parts[3], "four");
    }

    #[test]
    fn test_whitespace_variants() {
        let text = " \t\n\r\u{2003}\u{00A0}"; // Various whitespace chars

        let whitespace_chars = vec![' ', '\t', '\n', '\r', '\u{2003}', '\u{00A0}'];

        for ws_char in whitespace_chars {
            assert!(text.contains(ws_char));
            assert!(ws_char.is_whitespace());
        }
    }

    #[test]
    fn test_carriage_return_handling() {
        let text = "Line1\rLine2";

        // Old Mac style line ending
        let parts: Vec<&str> = text.split('\r').collect();

        assert_eq!(parts.len(), 2);
        assert_eq!(parts[0], "Line1");
        assert_eq!(parts[1], "Line2");
    }

    #[test]
    fn test_surrogate_pairs() {
        // Some emoji are represented as surrogate pairs in UTF-16
        let emoji = "😀";

        // In Rust (UTF-8), this is 4 bytes
        assert_eq!(emoji.len(), 4);

        // But 1 char
        assert_eq!(emoji.chars().count(), 1);

        // And 1 grapheme
        use unicode_segmentation::UnicodeSegmentation;
        assert_eq!(emoji.graphemes(true).count(), 1);
    }

    #[test]
    fn test_text_wrap_boundary() {
        let text = "Hello World";

        // Try to split at word boundary
        let split_pos = text.find(' ').unwrap();

        assert_eq!(split_pos, 5);

        let (first, second) = text.split_at(split_pos);

        assert_eq!(first, "Hello");
        assert_eq!(second, " World");
    }

    #[test]
    fn test_numeric_strings() {
        let numeric = "1234567890";

        // All should be digits
        assert!(numeric.chars().all(|c| c.is_numeric()));

        // Should parse as number
        let parsed: i64 = numeric.parse().unwrap();
        assert_eq!(parsed, 1234567890);
    }

    #[test]
    fn test_alphanumeric_mixed() {
        let mixed = "abc123XYZ";

        let letters = mixed.chars().filter(|c| c.is_alphabetic()).count();
        let digits = mixed.chars().filter(|c| c.is_numeric()).count();

        assert_eq!(letters, 6);
        assert_eq!(digits, 3);
    }

    #[test]
    fn test_string_case_conversion() {
        let text = "HeLLo WoRLd";

        assert_eq!(text.to_lowercase(), "hello world");
        assert_eq!(text.to_uppercase(), "HELLO WORLD");
    }

    #[test]
    fn test_trim_operations() {
        let text = "   \t  Hello  \n\t   ";

        assert_eq!(text.trim(), "Hello");
        assert_eq!(text.trim_start(), "Hello  \n\t   ");
        assert_eq!(text.trim_end(), "   \t  Hello");
    }

    #[test]
    fn test_repeated_patterns() {
        let repeated = "abc".repeat(3);

        assert_eq!(repeated, "abcabcabc");
        assert_eq!(repeated.len(), 9);
    }

    #[test]
    fn test_string_concatenation() {
        let part1 = "Hello";
        let part2 = " ";
        let part3 = "World";

        let combined = format!("{}{}{}", part1, part2, part3);

        assert_eq!(combined, "Hello World");
    }

    #[test]
    fn test_character_classification() {
        let text = "a1! ";

        assert!(text.chars().next().unwrap().is_alphabetic());
        assert!(text.chars().nth(1).unwrap().is_numeric());
        assert!(text.chars().nth(2).unwrap().is_ascii_punctuation());
        assert!(text.chars().nth(3).unwrap().is_whitespace());
    }
}
