#![allow(clippy::uninlined_format_args)]

//! Test cases to reproduce markdown rendering bugs

use rustycode_tui::ui::{MarkdownRenderer, MessageTheme};

fn print_lines(content: &str) {
    let theme = MessageTheme::default();
    let lines = MarkdownRenderer::render_content(content, &theme, None);

    println!("Input: {:?}", content);
    println!("Output ({} lines):", lines.len());
    for (i, line) in lines.iter().enumerate() {
        if line.spans.is_empty() {
            println!("  Line {}: <empty>", i);
        } else {
            let text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
            println!("  Line {}: {:?}", i, text);
        }
    }
    println!();
}

#[test]
fn reproduce_bugs() {
    println!("=== Markdown Bug Reproduction Tests ===\n");

    // Bug 1: Missing spaces between inline elements
    println!("Bug 1: Spaces between elements");
    print_lines("text **bold** more");

    // Bug 2: Missing line breaks
    println!("Bug 2: Line breaks");
    print_lines("Line 1\n\nLine 2");

    // Bug 3: Extra line breaks
    println!("Bug 3: Extra blank lines");
    print_lines("# Heading\n\nText after");

    // Bug 4: Sticky state (formatting leaks)
    println!("Bug 4: Formatting leaks");
    print_lines("**Bold** normal *italic* after");

    // Bug 5: Lists with inline code
    println!("Bug 5: List with code");
    print_lines("- Item with `code`");

    // Bug 6: Complex inline formatting
    println!("Bug 6: Complex inline");
    print_lines("Text with **bold**, *italic*, and `code` all together");

    // Bug 7: Bold in heading
    println!("Bug 7: Bold heading");
    print_lines("# **Bold Heading** text");

    // Bug 8: Inline elements at start
    println!("Bug 8: Start with bold");
    print_lines("**Bold** at start");

    // Bug 9: Multiple spaces
    println!("Bug 9: Multiple spaces");
    print_lines("Word1    Word2");

    // Bug 10: Code followed by text
    println!("Bug 10: Code then text");
    print_lines("`code` then text");
}
