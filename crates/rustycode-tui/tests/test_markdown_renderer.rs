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

//! Comprehensive tests for MarkdownRenderer
//!
//! Tests markdown rendering including:
//! - Headers
//! - Lists (ordered and unordered)
//! - Code blocks
//! - Inline code
//! - Bold and italic
//! - Links and images
//! - Blockquotes
//! - Tables
//! - Task lists
//! - Horizontal rules
//! - Nested structures

use rustycode_tui::MarkdownRenderer;

/// Test basic paragraph rendering
#[test]
fn test_basic_paragraph() {
    let renderer = MarkdownRenderer::new();
    let markdown = "Hello, world!";

    let result = renderer.parse(markdown);

    assert!(!result.is_empty(), "Should render paragraph");
}

/// Test header rendering (all levels)
#[test]
fn test_headers() {
    let renderer = MarkdownRenderer::new();
    let markdown = r#"# Header 1
## Header 2
### Header 3
#### Header 4
##### Header 5
###### Header 6"#;

    let result = renderer.parse(markdown);

    assert!(!result.is_empty(), "Should render headers");
    assert!(result.len() >= 6, "Should have all header levels");
}

/// Test bold and italic text
#[test]
fn test_bold_and_italic() {
    let renderer = MarkdownRenderer::new();
    let markdown = r#"**bold text**
*italic text*
***bold and italic***
__bold__
_italic_"#;

    let result = renderer.parse(markdown);

    assert!(!result.is_empty(), "Should render bold and italic");
}

/// Test inline code
#[test]
fn test_inline_code() {
    let renderer = MarkdownRenderer::new();
    let markdown = r#"This is `inline code` and this is `more code`."#;

    let result = renderer.parse(markdown);

    assert!(!result.is_empty(), "Should render inline code");
}

/// Test code blocks with syntax highlighting
#[test]
fn test_code_blocks() {
    let renderer = MarkdownRenderer::new();
    let markdown = r#"```rust
fn main() {
    println!("Hello, World!");
}
```

```python
def main():
    print("Hello, World!")
```"#;

    let result = renderer.parse(markdown);

    assert!(!result.is_empty(), "Should render code blocks");
}

/// Test unordered lists
#[test]
fn test_unordered_lists() {
    let renderer = MarkdownRenderer::new();
    let markdown = r#"* Item 1
* Item 2
* Item 3
  - Nested item 1
  - Nested item 2"#;

    let result = renderer.parse(markdown);

    assert!(!result.is_empty(), "Should render unordered lists");
}

/// Test ordered lists
#[test]
fn test_ordered_lists() {
    let renderer = MarkdownRenderer::new();
    let markdown = r#"1. First item
2. Second item
3. Third item
   1. Nested item
   2. Another nested item"#;

    let result = renderer.parse(markdown);

    assert!(!result.is_empty(), "Should render ordered lists");
}

/// Test links
#[test]
fn test_links() {
    let renderer = MarkdownRenderer::new();
    let markdown = r#"[
![Link text](https://example.com)
[Another link](https://example.com/page)"#;

    let result = renderer.parse(markdown);

    assert!(!result.is_empty(), "Should render links");
}

/// Test images
#[test]
fn test_images() {
    let renderer = MarkdownRenderer::new();
    let markdown = r#"![Alt text](image.png)
![Another](https://example.com/image.jpg)"#;

    let result = renderer.parse(markdown);

    assert!(!result.is_empty(), "Should render images");
}

/// Test blockquotes
#[test]
fn test_blockquotes() {
    let renderer = MarkdownRenderer::new();
    let markdown = r#"> This is a blockquote
> with multiple lines
>
> > Nested blockquote"#;

    let result = renderer.parse(markdown);

    assert!(!result.is_empty(), "Should render blockquotes");
}

/// Test horizontal rules
#[test]
fn test_horizontal_rules() {
    let renderer = MarkdownRenderer::new();
    let markdown = r#"---

***

___"#;

    let _result = renderer.parse(markdown);

    // Horizontal rules may not be supported by simple renderer
    // Test that renderer doesn't crash on this input
}

/// Test tables
#[test]
fn test_tables() {
    let renderer = MarkdownRenderer::new();
    let markdown = r#"| Header 1 | Header 2 | Header 3 |
|----------|----------|----------|
| Cell 1   | Cell 2   | Cell 3   |
| Cell 4   | Cell 5   | Cell 6   |"#;

    let result = renderer.parse(markdown);

    assert!(!result.is_empty(), "Should render tables");
}

/// Test task lists
#[test]
fn test_task_lists() {
    let renderer = MarkdownRenderer::new();
    let markdown = r#"- [x] Completed task
- [ ] Incomplete task
- [ ] Another incomplete task"#;

    let result = renderer.parse(markdown);

    assert!(!result.is_empty(), "Should render task lists");
}

/// Test combined elements
#[test]
fn test_combined_elements() {
    let renderer = MarkdownRenderer::new();
    let markdown = r#"# Document Title

This is a paragraph with **bold** and *italic* text.

## Code Example

```rust
fn main() {
    println!("Hello");
}
```

## List

* Item 1
* Item 2

> A blockquote
"#;

    let result = renderer.parse(markdown);

    assert!(!result.is_empty(), "Should render combined elements");
}

/// Test empty markdown
#[test]
fn test_empty_markdown() {
    let renderer = MarkdownRenderer::new();
    let result = renderer.parse("");

    // Should handle gracefully
    assert!(
        result.len() <= 1,
        "Empty markdown should produce minimal output"
    );
}

/// Test markdown with only whitespace
#[test]
fn test_whitespace_only() {
    let renderer = MarkdownRenderer::new();
    let result = renderer.parse("   \n\n   \t\t\n");

    assert!(
        result.len() <= 2,
        "Whitespace only should produce minimal output"
    );
}

/// Test markdown with special characters
#[test]
fn test_special_characters() {
    let renderer = MarkdownRenderer::new();
    let markdown = r#"Special characters: < > & " '
Escaped: \* \_ \[ \]"#;

    let result = renderer.parse(markdown);

    assert!(!result.is_empty(), "Should handle special characters");
}

/// Test markdown with Unicode
#[test]
fn test_unicode() {
    let renderer = MarkdownRenderer::new();
    let markdown = r#"Unicode: 你好 🚀 🎉
Emoji: 😀 🐱 🌍
Arabic: مرحبا
Russian: Привет"#;

    let result = renderer.parse(markdown);

    assert!(!result.is_empty(), "Should handle Unicode");
}

/// Test very long lines
#[test]
fn test_very_long_lines() {
    let renderer = MarkdownRenderer::new();
    let long_line = "a".repeat(10000);
    let markdown = format!("# Title\n\n{}", long_line);

    let result = renderer.parse(&markdown);

    assert!(!result.is_empty(), "Should handle very long lines");
}

/// Test deeply nested lists
#[test]
fn test_deeply_nested_lists() {
    let renderer = MarkdownRenderer::new();
    let markdown = r#"* Level 1
  * Level 2
    * Level 3
      * Level 4
        * Level 5"#;

    let result = renderer.parse(markdown);

    assert!(!result.is_empty(), "Should handle deeply nested lists");
}

/// Test code blocks with various languages
#[test]
fn test_code_blocks_various_languages() {
    let renderer = MarkdownRenderer::new();
    let markdown = r#"```rust
fn main() {}
```

```python
def main(): pass
```

```javascript
function main() {}
```

```go
func main() {}
```

```java
class Main {}
```"#;

    let result = renderer.parse(markdown);

    assert!(!result.is_empty(), "Should handle various languages");
}

/// Test inline code with special characters
#[test]
fn test_inline_code_special_chars() {
    let renderer = MarkdownRenderer::new();
    let markdown = r#"Code with special chars: `<div>`, `&amp;`, `"quotes"`"#;

    let result = renderer.parse(markdown);

    assert!(
        !result.is_empty(),
        "Should handle special chars in inline code"
    );
}

/// Test multiple consecutive blank lines
#[test]
fn test_multiple_blank_lines() {
    let renderer = MarkdownRenderer::new();
    let markdown = r#"Line 1


Line 2


Line 3"#;

    let result = renderer.parse(markdown);

    assert!(!result.is_empty(), "Should handle multiple blank lines");
}

/// Test strikethrough (if supported)
#[test]
fn test_strikethrough() {
    let renderer = MarkdownRenderer::new();
    let markdown = r#"~~strikethrough text~~"#;

    let result = renderer.parse(markdown);

    // May or may not be supported depending on parser
    assert!(
        !result.is_empty(),
        "Should handle strikethrough or ignore it"
    );
}

/// Test HTML in markdown
#[test]
fn test_html_in_markdown() {
    let renderer = MarkdownRenderer::new();
    let markdown = r#"<div class="custom">Custom HTML</div>
<p>Paragraph tag</p>"#;

    let _result = renderer.parse(markdown);

    // HTML may not be supported by simple renderer
    // Test that renderer doesn't crash on HTML input
}

/// Test reference-style links
#[test]
fn test_reference_links() {
    let renderer = MarkdownRenderer::new();
    let markdown = r#"[Link][ref]

[ref]: https://example.com"#;

    let result = renderer.parse(markdown);

    assert!(!result.is_empty(), "Should handle reference links");
}

/// Test footnotes (if supported)
#[test]
fn test_footnotes() {
    let renderer = MarkdownRenderer::new();
    let markdown = r#"This is a footnote[^1]

[^1]: This is the footnote content"#;

    let result = renderer.parse(markdown);

    // May or may not be supported
    assert!(!result.is_empty(), "Should handle footnotes or ignore them");
}

/// Test definition lists (if supported)
#[test]
fn test_definition_lists() {
    let renderer = MarkdownRenderer::new();
    let markdown = r#"Term 1
: Definition 1

Term 2
: Definition 2"#;

    let result = renderer.parse(markdown);

    // May or may not be supported
    assert!(
        !result.is_empty(),
        "Should handle definition lists or ignore them"
    );
}

/// Test large document
#[test]
fn test_large_document() {
    let renderer = MarkdownRenderer::new();

    let markdown: String = (0..100)
        .map(|i| format!("## Section {}\n\nContent for section {}.\n", i, i))
        .collect::<Vec<_>>()
        .join("\n");

    let result = renderer.parse(&markdown);

    assert!(!result.is_empty(), "Should handle large documents");
    assert!(
        result.len() > 100,
        "Large document should produce multiple lines"
    );
}

/// Test consistency across multiple renders
#[test]
fn test_consistency() {
    let renderer = MarkdownRenderer::new();
    let markdown = "# Test\n\nContent";

    let result1 = renderer.parse(markdown);
    let result2 = renderer.parse(markdown);

    assert_eq!(result1.len(), result2.len(), "Results should be consistent");
}

/// Test malformed markdown doesn't crash
#[test]
fn test_malformed_markdown() {
    let renderer = MarkdownRenderer::new();
    let markdown = r#"**unclosed bold
*unclosed italic
[unclosed link](unclosed url
```unclosed code block"#;

    let result = renderer.parse(markdown);

    // Should not crash and should produce some output
    assert!(
        !result.is_empty(),
        "Should handle malformed markdown gracefully"
    );
}

/// Test performance with complex document
#[test]
fn test_performance_complex_document() {
    let renderer = MarkdownRenderer::new();

    let markdown: String = (0..50)
        .map(|i| {
            format!(
                r#"## Section {}

* List item {}
* Another item {}

```rust
fn test_{}() {{
    println!("Section {}", i);
}}
```

> Blockquote for section {}

---

"#,
                i, i, i, i, i, i
            )
        })
        .collect::<Vec<_>>()
        .join("\n");

    let start = std::time::Instant::now();
    let result = renderer.parse(&markdown);
    let duration = start.elapsed();

    assert!(!result.is_empty(), "Should render complex document");
    assert!(
        duration.as_secs() < 5,
        "Complex document should render quickly"
    );
}

/// Test markdown with code fence without language
#[test]
fn test_code_fence_no_language() {
    let renderer = MarkdownRenderer::new();
    let markdown = r#"```
fn main() {
    println!("No language specified");
}
```"#;

    let result = renderer.parse(markdown);

    assert!(
        !result.is_empty(),
        "Should handle code fence without language"
    );
}

/// Test inline HTML in markdown
#[test]
fn test_inline_html() {
    let renderer = MarkdownRenderer::new();
    let markdown = r#"Paragraph with <strong>HTML</strong> and <em>more</em> markup."#;

    let result = renderer.parse(markdown);

    assert!(!result.is_empty(), "Should handle inline HTML");
}

/// Test autolinks
#[test]
fn test_autolinks() {
    let renderer = MarkdownRenderer::new();
    let markdown = r#"Visit https://example.com for more info.
Email test@example.com"#;

    let result = renderer.parse(markdown);

    assert!(!result.is_empty(), "Should handle autolinks");
}
