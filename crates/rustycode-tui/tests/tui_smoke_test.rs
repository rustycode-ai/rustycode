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

//! Smoke tests for TUI initialization and core functionality
//!
//! These tests verify the TUI can initialize and basic components work.

use rustycode_tui::{DiffRenderer, MarkdownRenderer, SyntaxHighlighter};

#[test]
fn test_syntax_highlighter_initialization() {
    let highlighter = SyntaxHighlighter::new_with_theme("base16-ocean.dark");
    let code = "fn main() { println!(\"Hello\"); }";
    let result = highlighter.highlight(code, Some("rust"));

    assert!(
        !result.is_empty(),
        "SyntaxHighlighter should produce output"
    );
    assert!(!result.is_empty(), "Should have at least one line");
}

#[test]
fn test_diff_renderer_initialization() {
    let renderer = DiffRenderer::new();
    let old = "line 1\nline 2";
    let new = "line 1\nline 2 modified";

    let result = renderer.render_unified_diff(old, new, "test.txt");

    assert!(!result.is_empty(), "DiffRenderer should produce output");
    assert!(result.len() > 3, "Should have header and content");
}

#[test]
fn test_markdown_renderer_initialization() {
    let renderer = MarkdownRenderer::new();
    let markdown = "# Test\n\nHello world";

    let result = renderer.parse(markdown);

    assert!(!result.is_empty(), "MarkdownRenderer should produce output");
}

#[test]
fn test_all_components_instantiation() {
    // Verify all core components can be instantiated
    let highlighter = SyntaxHighlighter::new_with_theme("base16-ocean.dark");
    let diff_renderer = DiffRenderer::new();
    let markdown_renderer = MarkdownRenderer::new();

    // Basic smoke test - they all work
    let code = "test";
    let hl = highlighter.highlight(code, Some("text"));
    assert!(!hl.is_empty());

    let diff = diff_renderer.render_unified_diff("a", "b", "test.txt");
    assert!(!diff.is_empty());

    let md = markdown_renderer.parse("test");
    assert!(!md.is_empty());
}

#[test]
fn test_multi_language_support() {
    let highlighter = SyntaxHighlighter::new_with_theme("base16-ocean.dark");

    // Test a subset of supported languages
    let languages = vec![
        ("rust", "fn main() {}"),
        ("python", "def main(): pass"),
        ("javascript", "function main() {}"),
        ("go", "func main() {}"),
        ("java", "class Main {}"),
    ];

    for (lang, code) in languages {
        let result = highlighter.highlight(code, Some(lang));
        assert!(!result.is_empty(), "{} should highlight", lang);
    }
}

#[test]
fn test_diff_formats() {
    let renderer = DiffRenderer::new();
    let old = "old content";
    let new = "new content";

    // Test all three diff formats
    let unified = renderer.render_unified_diff(old, new, "test.txt");
    assert!(!unified.is_empty(), "Unified diff should work");

    let side_by_side = renderer.render_side_by_side(old, new);
    assert!(!side_by_side.is_empty(), "Side-by-side diff should work");

    let hunk = renderer.render_hunk_diff(old, new, "test.txt");
    assert!(!hunk.is_empty(), "Hunk diff should work");
}

#[test]
fn test_error_handling_with_empty_strings() {
    let renderer = DiffRenderer::new();

    // Should handle edge cases gracefully
    let result1 = renderer.render_unified_diff("", "", "empty.txt");
    assert!(!result1.is_empty(), "Should handle empty strings");

    let result2 = renderer.render_unified_diff("content", "", "delete.txt");
    assert!(!result2.is_empty(), "Should handle deletions");

    let result3 = renderer.render_unified_diff("", "content", "add.txt");
    assert!(!result3.is_empty(), "Should handle additions");
}

#[test]
fn test_markdown_with_code_blocks() {
    let renderer = MarkdownRenderer::new();

    let markdown = r#"```rust
fn main() {
    println!("Hello");
}
```"#;

    let result = renderer.parse(markdown);
    assert!(!result.is_empty(), "Should handle code blocks");
}

#[test]
fn test_auto_language_detection() {
    let highlighter = SyntaxHighlighter::new_with_theme("base16-ocean.dark");

    // Test auto-detection without file extension
    let rust_code = "fn main() { let x = 42; }";
    let result = highlighter.highlight_auto(rust_code, None);
    assert!(!result.is_empty(), "Auto-detection should work");
}

#[test]
fn test_large_file_handling() {
    let highlighter = SyntaxHighlighter::new_with_theme("base16-ocean.dark");

    // Create a large file (1000 lines)
    let large_code = "fn test() {}\n".repeat(1000);

    let result = highlighter.highlight(&large_code, Some("rust"));
    assert!(!result.is_empty(), "Should handle large files");
    assert!(result.len() > 900, "Should have most lines");
}

#[test]
fn test_special_characters_in_code() {
    let highlighter = SyntaxHighlighter::new();

    // Code with special characters
    let code = r#"fn main() {
    let s = "Hello \"World\"";
    let c = '🚀';
    println!("{}\n", s);
}"#;

    let result = highlighter.highlight(code, Some("rust"));
    assert!(!result.is_empty(), "Should handle special characters");
}
