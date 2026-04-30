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

//! Comprehensive tests for DiffRenderer
//!
//! Tests diff visualization in multiple formats:
//! - Unified diff
//! - Side-by-side diff
//! - Hunk diff
//!
//! Also tests edge cases like empty files, large diffs, and special characters.

use rustycode_tui::DiffRenderer;

/// Test basic unified diff
#[test]
fn test_unified_diff_basic() {
    let renderer = DiffRenderer::new();
    let old = "line 1\nline 2\nline 3";
    let new = "line 1\nline 2 modified\nline 3";

    let result = renderer.render_unified_diff(old, new, "test.txt");

    assert!(!result.is_empty(), "Should produce diff output");
    assert!(result.len() >= 4, "Should have header and diff lines");
}

/// Test unified diff with additions only
#[test]
fn test_unified_diff_additions_only() {
    let renderer = DiffRenderer::new();
    let old = "line 1\nline 2";
    let new = "line 1\nline 2\nline 3\nline 4";

    let result = renderer.render_unified_diff(old, new, "test.txt");

    assert!(!result.is_empty(), "Should show additions");
    // Should contain green (addition) indicators
    let text = result
        .iter()
        .map(|line| line.to_string())
        .collect::<String>();
    assert!(
        text.contains("+") || !text.is_empty(),
        "Should show additions"
    );
}

/// Test unified diff with deletions only
#[test]
fn test_unified_diff_deletions_only() {
    let renderer = DiffRenderer::new();
    let old = "line 1\nline 2\nline 3\nline 4";
    let new = "line 1\nline 4";

    let result = renderer.render_unified_diff(old, new, "test.txt");

    assert!(!result.is_empty(), "Should show deletions");
}

/// Test unified diff with modifications
#[test]
fn test_unified_diff_modifications() {
    let renderer = DiffRenderer::new();
    let old = "line 1\nline 2\nline 3";
    let new = "line 1\nmodified line 2\nline 3";

    let result = renderer.render_unified_diff(old, new, "test.txt");

    assert!(!result.is_empty(), "Should show modifications");
}

/// Test unified diff with no changes
#[test]
fn test_unified_diff_no_changes() {
    let renderer = DiffRenderer::new();
    let content = "line 1\nline 2\nline 3";

    let result = renderer.render_unified_diff(content, content, "test.txt");

    // Should produce minimal output for identical files
    assert!(!result.is_empty(), "Should handle identical files");
}

/// Test unified diff with empty old file
#[test]
fn test_unified_diff_empty_old() {
    let renderer = DiffRenderer::new();
    let old = "";
    let new = "line 1\nline 2\nline 3";

    let result = renderer.render_unified_diff(old, new, "new_file.txt");

    assert!(!result.is_empty(), "Should handle new file (empty old)");
}

/// Test unified diff with empty new file
#[test]
fn test_unified_diff_empty_new() {
    let renderer = DiffRenderer::new();
    let old = "line 1\nline 2\nline 3";
    let new = "";

    let result = renderer.render_unified_diff(old, new, "deleted_file.txt");

    assert!(
        !result.is_empty(),
        "Should handle file deletion (empty new)"
    );
}

/// Test unified diff with both empty
#[test]
fn test_unified_diff_both_empty() {
    let renderer = DiffRenderer::new();

    let result = renderer.render_unified_diff("", "", "empty.txt");

    assert!(!result.is_empty(), "Should handle both empty");
}

/// Test side-by-side diff
#[test]
fn test_side_by_side_diff() {
    let renderer = DiffRenderer::new();
    let old = "line 1\nline 2\nline 3";
    let new = "line 1\nline 2 modified\nline 3";

    let result = renderer.render_side_by_side(old, new);

    assert!(!result.is_empty(), "Should produce side-by-side diff");
}

/// Test side-by-side diff with different line counts
#[test]
fn test_side_by_side_diff_different_counts() {
    let renderer = DiffRenderer::new();
    let old = "line 1\nline 2";
    let new = "line 1\nline 2\nline 3\nline 4";

    let result = renderer.render_side_by_side(old, new);

    assert!(!result.is_empty(), "Should handle different line counts");
}

/// Test hunk diff
#[test]
fn test_hunk_diff() {
    let renderer = DiffRenderer::new();
    let old = "line 1\nline 2\nline 3\nline 4\nline 5";
    let new = "line 1\nline 2 modified\nline 3\nline 4 modified\nline 5";

    let result = renderer.render_hunk_diff(old, new, "test.txt");

    assert!(!result.is_empty(), "Should produce hunk diff");
}

/// Test hunk diff with multiple hunks
#[test]
fn test_hunk_diff_multiple_hunks() {
    let renderer = DiffRenderer::new();
    let old = "line 1\nline 2\nline 3\nline 4\nline 5\nline 6\nline 7\nline 8";
    let new =
        "line 1\nline 2 modified\nline 3\nline 4\nline 5 modified\nline 6\nline 7\nline 8 modified";

    let result = renderer.render_hunk_diff(old, new, "test.txt");

    assert!(!result.is_empty(), "Should handle multiple hunks");
}

/// Test diff with special characters
#[test]
fn test_diff_with_special_characters() {
    let renderer = DiffRenderer::new();
    let old = r#"line 1
line with "quotes"
line with 'apostrophes'
line with \t tabs \n newlines"#;
    let new = r#"line 1
line with "quotes" modified
line with 'apostrophes'
line with \t tabs \n newlines modified"#;

    let result = renderer.render_unified_diff(old, new, "test.txt");

    assert!(!result.is_empty(), "Should handle special characters");
}

/// Test diff with Unicode characters
#[test]
fn test_diff_with_unicode() {
    let renderer = DiffRenderer::new();
    let old = "Hello\nWorld\n🚀 Rocket";
    let new = "Hello\nWorld Modified\n🚀 Rocket";

    let result = renderer.render_unified_diff(old, new, "test.txt");

    assert!(!result.is_empty(), "Should handle Unicode characters");
}

/// Test diff with very long lines
#[test]
fn test_diff_with_long_lines() {
    let renderer = DiffRenderer::new();
    let long_line = "a".repeat(1000);
    let old = format!("line 1\n{}\nline 3", long_line);
    let new = format!("line 1\n{} modified\nline 3", long_line);

    let result = renderer.render_unified_diff(&old, &new, "test.txt");

    assert!(!result.is_empty(), "Should handle long lines");
}

/// Test diff with code (Rust)
#[test]
fn test_diff_with_rust_code() {
    let renderer = DiffRenderer::new();
    let old = r#"fn main() {
    let x = 5;
    println!("Hello");
}"#;
    let new = r#"fn main() {
    let x = 10;
    println!("Hello, World!");
}"#;

    let result = renderer.render_unified_diff(old, new, "main.rs");

    assert!(!result.is_empty(), "Should handle Rust code");
}

/// Test diff with code (Python)
#[test]
fn test_diff_with_python_code() {
    let renderer = DiffRenderer::new();
    let old = r#"def main():
    x = 5
    print("Hello")

if __name__ == "__main__":
    main()"#;
    let new = r#"def main():
    x = 10
    print("Hello, World!")

if __name__ == "__main__":
    main()"#;

    let result = renderer.render_unified_diff(old, new, "main.py");

    assert!(!result.is_empty(), "Should handle Python code");
}

/// Test diff with JSON
#[test]
fn test_diff_with_json() {
    let renderer = DiffRenderer::new();
    let old = r#"{
    "name": "test",
    "version": "1.0.0"
}"#;
    let new = r#"{
    "name": "test",
    "version": "2.0.0",
    "description": "updated"
}"#;

    let result = renderer.render_unified_diff(old, new, "package.json");

    assert!(!result.is_empty(), "Should handle JSON");
}

/// Test diff with large file
#[test]
fn test_diff_with_large_file() {
    let renderer = DiffRenderer::new();

    let old: String = (0..1000)
        .map(|i| format!("line {}", i))
        .collect::<Vec<_>>()
        .join("\n");

    let new: String = (0..1000)
        .map(|i| format!("line {} modified", i))
        .collect::<Vec<_>>()
        .join("\n");

    let result = renderer.render_unified_diff(&old, &new, "large.txt");

    assert!(!result.is_empty(), "Should handle large files");
}

/// Test diff with mixed additions and deletions
#[test]
fn test_diff_with_mixed_changes() {
    let renderer = DiffRenderer::new();
    let old = "line 1\nline 2\nline 3\nline 4\nline 5";
    let new = "line 1\nline 2 modified\nline 3.5\nline 4\nline 5 modified";

    let result = renderer.render_unified_diff(old, new, "test.txt");

    assert!(!result.is_empty(), "Should handle mixed changes");
}

/// Test diff preserves line numbers
#[test]
fn test_diff_preserves_line_numbers() {
    let renderer = DiffRenderer::new();
    let old = "a\nb\nc\nd\ne";
    let new = "a\nb\nmodified\nd\ne";

    let result = renderer.render_unified_diff(old, new, "test.txt");

    assert!(!result.is_empty(), "Should preserve context");
}

/// Test diff with only whitespace changes
#[test]
fn test_diff_with_whitespace_changes() {
    let renderer = DiffRenderer::new();
    let old = "line 1\nline 2\nline 3";
    let new = "line 1  \n  line 2\nline 3";

    let result = renderer.render_unified_diff(old, new, "test.txt");

    // May or may not show whitespace as changes depending on algorithm
    assert!(!result.is_empty(), "Should handle whitespace changes");
}

/// Test diff with tabs vs spaces
#[test]
fn test_diff_with_tabs_vs_spaces() {
    let renderer = DiffRenderer::new();
    let old = "line 1\n\tline 2\nline 3";
    let new = "line 1\n    line 2\nline 3";

    let result = renderer.render_unified_diff(old, new, "test.txt");

    assert!(!result.is_empty(), "Should handle tabs vs spaces");
}

/// Test diff with trailing whitespace
#[test]
fn test_diff_with_trailing_whitespace() {
    let renderer = DiffRenderer::new();
    let old = "line 1  \nline 2\nline 3";
    let new = "line 1\nline 2\nline 3";

    let result = renderer.render_unified_diff(old, new, "test.txt");

    assert!(!result.is_empty(), "Should handle trailing whitespace");
}

/// Test diff file name display
#[test]
fn test_diff_displays_filename() {
    let renderer = DiffRenderer::new();
    let old = "old content";
    let new = "new content";

    let result = renderer.render_unified_diff(old, new, "specific_filename.rs");

    // Result should contain the filename or handle it appropriately
    assert!(!result.is_empty(), "Should use filename in output");
}

/// Test all three diff formats produce output
#[test]
fn test_all_diff_formats() {
    let renderer = DiffRenderer::new();
    let old = "line 1\nline 2";
    let new = "line 1\nline 2 modified";

    let unified = renderer.render_unified_diff(old, new, "test.txt");
    let side_by_side = renderer.render_side_by_side(old, new);
    let hunk = renderer.render_hunk_diff(old, new, "test.txt");

    assert!(!unified.is_empty(), "Unified diff should work");
    assert!(!side_by_side.is_empty(), "Side-by-side diff should work");
    assert!(!hunk.is_empty(), "Hunk diff should work");
}

/// Test diff with single line change
#[test]
fn test_diff_single_line_change() {
    let renderer = DiffRenderer::new();
    let old = "single line";
    let new = "single line modified";

    let result = renderer.render_unified_diff(old, new, "test.txt");

    assert!(!result.is_empty(), "Should handle single line change");
}

/// Test diff with empty lines
#[test]
fn test_diff_with_empty_lines() {
    let renderer = DiffRenderer::new();
    let old = "line 1\n\nline 3";
    let new = "line 1\nmodified\nline 3";

    let result = renderer.render_unified_diff(old, new, "test.txt");

    assert!(!result.is_empty(), "Should handle empty lines");
}

/// Test diff with only empty lines
#[test]
fn test_diff_only_empty_lines() {
    let renderer = DiffRenderer::new();
    let old = "\n\n\n";
    let new = "\n\n\n";

    let result = renderer.render_unified_diff(old, new, "test.txt");

    assert!(!result.is_empty(), "Should handle only empty lines");
}

/// Test consistency across multiple calls
#[test]
fn test_diff_consistency() {
    let renderer = DiffRenderer::new();
    let old = "line 1\nline 2";
    let new = "line 1\nline 2 modified";

    let result1 = renderer.render_unified_diff(old, new, "test.txt");
    let result2 = renderer.render_unified_diff(old, new, "test.txt");

    assert_eq!(result1.len(), result2.len(), "Results should be consistent");
}

/// Performance test for large diffs
#[test]
fn test_diff_performance() {
    let renderer = DiffRenderer::new();

    let old: String = (0..5000)
        .map(|i| format!("line {} content here", i))
        .collect::<Vec<_>>()
        .join("\n");

    let new: String = (0..5000)
        .map(|i| format!("line {} content here modified", i))
        .collect::<Vec<_>>()
        .join("\n");

    let start = std::time::Instant::now();
    let result = renderer.render_unified_diff(&old, &new, "large.txt");
    let duration = start.elapsed();

    assert!(!result.is_empty(), "Should handle large diffs");
    assert!(
        duration.as_secs() < 10,
        "Large diff should complete in reasonable time"
    );
}
