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

//! Integration tests for RustyCode TUI
//!
//! These tests verify the core TUI functionality without requiring an interactive terminal.

use rustycode_tui::{DiffRenderer, MarkdownRenderer, SyntaxHighlighter};

#[allow(dead_code)]
fn create_highlighter() -> SyntaxHighlighter {
    SyntaxHighlighter::new_with_theme("base16-ocean.dark")
}

#[test]
fn test_syntax_highlighting_rust() {
    let highlighter = SyntaxHighlighter::new_with_theme("base16-ocean.dark");
    let code = r#"
fn main() {
    let x = 42;
    println!("Hello, world!");
}
"#;

    let lines = highlighter.highlight(code, Some("rust"));
    assert!(!lines.is_empty(), "Should produce highlighted lines");
    assert!(lines.len() > 3, "Should have multiple lines");
}

#[test]
fn test_syntax_highlighting_python() {
    let highlighter = SyntaxHighlighter::new();
    let code = r#"
def hello():
    x = 42
    print(f"Hello, world!")
"#;

    let lines = highlighter.highlight(code, Some("python"));
    assert!(!lines.is_empty(), "Should produce highlighted lines");
    assert!(lines.len() > 3, "Should have multiple lines");
}

#[test]
fn test_syntax_highlighting_javascript() {
    let highlighter = SyntaxHighlighter::new();
    let code = r#"
function hello() {
    const x = 42;
    console.log("Hello, world!");
}
"#;

    let lines = highlighter.highlight(code, Some("javascript"));
    assert!(!lines.is_empty(), "Should produce highlighted lines");
    assert!(lines.len() > 3, "Should have multiple lines");
}

#[test]
fn test_syntax_highlighting_go() {
    let highlighter = SyntaxHighlighter::new();
    let code = r#"
func main() {
    x := 42
    fmt.Println("Hello, world!")
}
"#;

    let lines = highlighter.highlight(code, Some("go"));
    assert!(!lines.is_empty(), "Should produce highlighted lines");
    assert!(lines.len() > 3, "Should have multiple lines");
}

#[test]
fn test_syntax_highlighting_auto_detection() {
    let highlighter = SyntaxHighlighter::new_with_theme("base16-ocean.dark");

    // Test Rust detection
    let rust_code = "fn main() { let x = 42; }";
    let lines = highlighter.highlight_auto(rust_code, None);
    assert!(!lines.is_empty());

    // Test Python detection
    let python_code = "def main():\n    x = 42";
    let lines = highlighter.highlight_auto(python_code, None);
    assert!(!lines.is_empty());

    // Test file extension hint
    let code = "some code";
    let lines = highlighter.highlight_auto(code, Some("script.py"));
    assert!(!lines.is_empty());
}

#[test]
fn test_diff_renderer_unified() {
    let renderer = DiffRenderer::new();
    let old = "line 1\nline 2\nline 3";
    let new = "line 1\nline 2 modified\nline 3";

    let lines = renderer.render_unified_diff(old, new, "test.txt");
    assert!(!lines.is_empty(), "Should produce diff lines");
    assert!(lines.len() > 3, "Should have multiple diff lines");
}

#[test]
fn test_diff_renderer_side_by_side() {
    let renderer = DiffRenderer::new();
    let old = "line 1\nline 2\nline 3";
    let new = "line 1\nline 2 modified\nline 3";

    let lines = renderer.render_side_by_side(old, new);
    assert!(!lines.is_empty(), "Should produce diff lines");
    assert!(lines.len() > 2, "Should have header and content");
}

#[test]
fn test_markdown_renderer() {
    let renderer = MarkdownRenderer::new();

    // Test heading
    let markdown = "# Heading 1\nSome text";
    let lines = renderer.parse(markdown);
    assert!(!lines.is_empty());

    // Test code block
    let markdown_with_code = "```rust\nfn main() {}\n```";
    let lines = renderer.parse(markdown_with_code);
    assert!(!lines.is_empty());

    // Test list
    let markdown_list = "- Item 1\n- Item 2";
    let lines = renderer.parse(markdown_list);
    assert!(!lines.is_empty());
}

#[test]
fn test_diff_renderer_empty_strings() {
    let renderer = DiffRenderer::new();

    // Test with empty old string
    let lines = renderer.render_unified_diff("", "new content", "test.txt");
    assert!(!lines.is_empty());

    // Test with empty new string
    let lines = renderer.render_unified_diff("old content", "", "test.txt");
    assert!(!lines.is_empty());

    // Test with both empty
    let lines = renderer.render_unified_diff("", "", "test.txt");
    assert!(!lines.is_empty());
}

#[test]
fn test_syntax_highlighting_multiline_strings() {
    let highlighter = SyntaxHighlighter::new_with_theme("base16-ocean.dark");
    let code = r#"
let multi = "line 1
line 2
line 3";
"#;

    let lines = highlighter.highlight(code, Some("rust"));
    assert!(!lines.is_empty(), "Should handle multiline strings");
}

#[test]
fn test_syntax_highlighting_comments() {
    let highlighter = SyntaxHighlighter::new_with_theme("base16-ocean.dark");

    // Rust single-line comment
    let rust_code = "// This is a comment\nlet x = 42;";
    let lines = highlighter.highlight(rust_code, Some("rust"));
    assert!(!lines.is_empty());

    // Python single-line comment
    let python_code = "# This is a comment\nx = 42";
    let lines = highlighter.highlight(python_code, Some("python"));
    assert!(!lines.is_empty());
}

#[test]
fn test_markdown_renderer_table() {
    let renderer = MarkdownRenderer::new();
    let markdown = "| Header 1 | Header 2 |\n|----------|----------|\n| Cell 1   | Cell 2   |";

    let lines = renderer.parse(markdown);
    assert!(!lines.is_empty(), "Should render tables");
}

#[test]
fn test_syntax_highlighting_numbers() {
    let highlighter = SyntaxHighlighter::new_with_theme("base16-ocean.dark");
    let code = "let x = 42;\nlet y = 3.14;\nlet z = 1000000;";

    let lines = highlighter.highlight(code, Some("rust"));
    assert!(!lines.is_empty());
    assert!(lines.len() >= 3, "Should have at least 3 lines");
}

// ─── Workflow Tests ───────────────────────────────────────────────────────

#[test]
fn test_diff_hunk_format() {
    let renderer = DiffRenderer::new();
    let old = "line 1\nline 2\nline 3\nline 4";
    let new = "line 1\nline 2 modified\nline 3\nline 4 added";

    let lines = renderer.render_hunk_diff(old, new, "example.txt");
    assert!(!lines.is_empty(), "Should produce hunk diff");

    // Verify git-style diff headers by checking span content
    let has_diff_header = lines.iter().any(|line| {
        line.spans
            .iter()
            .any(|span| span.content.contains("diff --git"))
    });
    assert!(has_diff_header, "Should have git diff header");

    let has_old_marker = lines.iter().any(|line| {
        line.spans
            .iter()
            .any(|span| span.content.contains("--- a/example.txt"))
    });
    assert!(has_old_marker, "Should have old file marker");

    let has_new_marker = lines.iter().any(|line| {
        line.spans
            .iter()
            .any(|span| span.content.contains("+++ b/example.txt"))
    });
    assert!(has_new_marker, "Should have new file marker");
}

#[test]
fn test_diff_with_multiple_changes() {
    let renderer = DiffRenderer::new();
    let old = "line 1\nline 2\nline 3\nline 4\nline 5";
    let new = "line 1 modified\nline 2\nline 3 changed\nline 4\nline 5 modified";

    let lines = renderer.render_unified_diff(old, new, "multi_changes.txt");
    assert!(!lines.is_empty());

    // Count how many lines have addition or deletion markers
    let change_count = lines
        .iter()
        .filter(|line| {
            line.spans
                .iter()
                .any(|span| span.content.starts_with('-') || span.content.starts_with('+'))
        })
        .count();

    assert!(change_count >= 3, "Should show at least 3 changed lines");
}

#[test]
fn test_markdown_with_code_blocks() {
    let renderer = MarkdownRenderer::new();
    let markdown = r#"# Code Example

Here's some Rust code:

```rust
fn main() {
    println!("Hello!");
}
```

And here's some Python:

```python
def hello():
    print("Hello!")
```
"#;

    let lines = renderer.parse(markdown);
    assert!(!lines.is_empty());

    // Verify code blocks are rendered by checking for language indicators
    let has_rust_indicator = lines
        .iter()
        .any(|line| line.spans.iter().any(|span| span.content.contains("rust:")));
    assert!(has_rust_indicator, "Should show rust language indicator");

    let has_python_indicator = lines.iter().any(|line| {
        line.spans
            .iter()
            .any(|span| span.content.contains("python:"))
    });
    assert!(
        has_python_indicator,
        "Should show python language indicator"
    );
}

#[test]
fn test_syntax_highlighting_all_supported_languages() {
    let highlighter = SyntaxHighlighter::new_with_theme("base16-ocean.dark");

    let languages: Vec<(&str, &str)> = vec![
        ("rust", "fn main() { let x = 42; }"),
        ("python", "def main():\n    x = 42"),
        ("javascript", "function main() { const x = 42; }"),
        ("go", "func main() { x := 42 }"),
        ("java", "public class Main { int x = 42; }"),
        ("c", "int main() { int x = 42; }"),
        ("cpp", "int main() { int x = 42; }"),
        ("ruby", "x = 42"),
        ("php", "$x = 42;"),
        ("bash", "x=42"),
        ("sql", "SELECT * FROM users"),
        ("html", "<div>Hello</div>"),
        ("css", ".class { color: red; }"),
        ("json", "{\"x\": 42}"),
        ("yaml", "key: value"),
        ("toml", "key = \"value\""),
        ("markdown", "# Heading"),
    ];

    for (lang, code) in languages {
        let lines = highlighter.highlight(code, Some(lang));
        assert!(!lines.is_empty(), "Should highlight {} code", lang);
    }
}

#[test]
fn test_diff_renderer_preserves_content() {
    let renderer = DiffRenderer::new();
    let old = "line 1\nline 2\nline 3";
    let new = "line 1\nline 2 modified\nline 3";

    let lines = renderer.render_unified_diff(old, new, "preserve.txt");
    assert!(!lines.is_empty());

    // Verify original content is visible
    let has_line1 = lines.iter().any(|line| {
        line.spans
            .iter()
            .any(|span| span.content.contains("line 1"))
    });
    assert!(has_line1, "Should show unchanged line 1");

    let has_line2 = lines.iter().any(|line| {
        line.spans
            .iter()
            .any(|span| span.content.contains("line 2"))
    });
    assert!(has_line2, "Should show line 2 (old or new)");

    let has_line3 = lines.iter().any(|line| {
        line.spans
            .iter()
            .any(|span| span.content.contains("line 3"))
    });
    assert!(has_line3, "Should show unchanged line 3");
}

#[test]
fn test_markdown_with_nested_formatting() {
    let renderer = MarkdownRenderer::new();
    let markdown = "**Bold** and *italic* and `code` and **bold with `code`**";

    let lines = renderer.parse(markdown);
    assert!(!lines.is_empty());

    // Just verify it renders without panicking
    assert!(!lines.is_empty(), "Should render complex markdown");
}

#[test]
fn test_syntax_highlighting_swift() {
    let highlighter = SyntaxHighlighter::new_with_theme("base16-ocean.dark");
    let code = r#"
import UIKit

class ViewController: UIViewController {
    var greeting: String = "Hello"
    
    override func viewDidLoad() {
        super.viewDidLoad()
        print(greeting)
    }
}
"#;

    let lines = highlighter.highlight(code, Some("swift"));
    assert!(!lines.is_empty(), "Swift highlighting should work");
    assert!(lines.len() > 5, "Should have multiple lines");
}

#[test]
fn test_syntax_highlighting_kotlin() {
    let highlighter = SyntaxHighlighter::new_with_theme("base16-ocean.dark");
    let code = r#"
package com.example

import kotlinx.coroutines.*

class MainActivity : AppCompatActivity() {
    private val greeting: String = "Hello"
    
    fun onCreate() {
        println(greeting)
    }
}
"#;

    let lines = highlighter.highlight(code, Some("kotlin"));
    assert!(!lines.is_empty(), "Kotlin highlighting should work");
    assert!(lines.len() > 5, "Should have multiple lines");
}

#[test]
fn test_syntax_highlighting_dart() {
    let highlighter = SyntaxHighlighter::new_with_theme("base16-ocean.dark");
    let code = r#"
import 'package:flutter/material.dart';

class MyApp extends StatelessWidget {
  @override
  Widget build(BuildContext context) {
    return Text('Hello Flutter');
  }
}
"#;

    let lines = highlighter.highlight(code, Some("dart"));
    assert!(!lines.is_empty(), "Dart highlighting should work");
    assert!(lines.len() > 5, "Should have multiple lines");
}

#[test]
fn test_syntax_highlighting_lua() {
    let highlighter = SyntaxHighlighter::new_with_theme("base16-ocean.dark");
    let code = r#"
local function greet(name)
    return "Hello, " .. name
end

print(greet("World"))
"#;

    let lines = highlighter.highlight(code, Some("lua"));
    assert!(!lines.is_empty(), "Lua highlighting should work");
    assert!(lines.len() > 3, "Should have multiple lines");
}

#[test]
fn test_syntax_highlighting_scala() {
    let highlighter = SyntaxHighlighter::new_with_theme("base16-ocean.dark");
    let code = r#"
object HelloWorld {
    def main(args: Array[String]): Unit = {
        println("Hello, World!")
    }
}
"#;

    let lines = highlighter.highlight(code, Some("scala"));
    assert!(!lines.is_empty(), "Scala highlighting should work");
    assert!(lines.len() > 3, "Should have multiple lines");
}
