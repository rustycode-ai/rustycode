//! Debug tests for markdown rendering - run with: cargo test debug_rendering -- --nocapture

use ratatui::style::Color;
use rustycode_ui_core::{MarkdownRenderer, MessageTheme};

#[test]
fn debug_heading_with_list() {
    let theme = MessageTheme::default();
    let content = "# Plan\n\n* Phase 1\n* Phase 2";
    let lines = MarkdownRenderer::render_content(content, &theme, None);

    println!("\n=== Heading with list ===");
    println!("Input: {:?}", content);
    for (i, line) in lines.iter().enumerate() {
        let line_str = line.to_string();
        println!("Line {}: {:?}", i, line_str);
        for (j, span) in line.spans.iter().enumerate() {
            println!(
                "  Span {}: content={:?} fg={:?}",
                j, span.content, span.style.fg
            );
        }
    }
}

#[test]
fn debug_code_block_rendering() {
    let theme = MessageTheme::default();
    let content = "```rust\nlet x = 42;\nprintln!(\"Hello\");\n```";
    let lines = MarkdownRenderer::render_content(content, &theme, None);

    println!("\n=== Code block ===");
    println!("Input: {:?}", content);
    for (i, line) in lines.iter().enumerate() {
        let line_str = line.to_string();
        println!(
            "Line {}: {:?} (len: {})",
            i,
            line_str,
            line_str.chars().count()
        );
        for (j, span) in line.spans.iter().enumerate() {
            println!(
                "  Span {}: content={:?} fg={:?}",
                j, span.content, span.style.fg
            );
        }
    }
}

#[test]
fn debug_bold_heading_with_list() {
    let theme = MessageTheme::default();
    let content = "# **Bold Heading**\n\n* Item 1";
    let lines = MarkdownRenderer::render_content(content, &theme, None);

    println!("\n=== Bold heading with list ===");
    println!("Input: {:?}", content);
    for (i, line) in lines.iter().enumerate() {
        let line_str = line.to_string();
        println!("Line {}: {:?}", i, line_str);
        for (j, span) in line.spans.iter().enumerate() {
            if span.content.contains('•') {
                println!("  Bullet at line {} span {}: fg={:?}", i, j, span.style.fg);
            }
        }
    }
}

#[test]
fn debug_list_spacing() {
    let theme = MessageTheme::default();
    // Content with text followed by a list
    let content = "Some text above\n\n* Item 1\n* Item 2";
    let lines = MarkdownRenderer::render_content(content, &theme, None);

    println!("\n=== List spacing ===");
    println!("Input: {:?}", content);
    for (i, line) in lines.iter().enumerate() {
        let line_str = line.to_string();
        let is_empty = line_str.is_empty() || line.spans.is_empty();
        println!("Line {}: {:?} (empty: {})", i, line_str, is_empty);
    }

    // Verify there's a blank line between "Some text above" and the first bullet
    let text_line_idx = lines
        .iter()
        .position(|l| l.to_string().contains("Some text above"));
    let bullet_line_idx = lines.iter().position(|l| l.to_string().contains('•'));

    if let (Some(text_idx), Some(bullet_idx)) = (text_line_idx, bullet_line_idx) {
        println!(
            "Text at line {}, bullet at line {}, gap = {}",
            text_idx,
            bullet_idx,
            bullet_idx - text_idx
        );
        assert!(
            bullet_idx > text_idx + 1,
            "Should have blank line between text and list"
        );
    }
}

#[test]
fn debug_spacing_after_list() {
    let theme = MessageTheme::default();
    // Content with list followed by text
    let content = "* Item 1\n* Item 2\n\nText after list";
    let lines = MarkdownRenderer::render_content(content, &theme, None);

    println!("\n=== Spacing after list ===");
    println!("Input: {:?}", content);
    for (i, line) in lines.iter().enumerate() {
        let line_str = line.to_string();
        let is_empty = line_str.is_empty() || line.spans.is_empty();
        println!("Line {}: {:?} (empty: {})", i, line_str, is_empty);
    }

    // Verify there's spacing between list and following text
    let last_bullet_idx = lines.iter().rposition(|l| l.to_string().contains('•'));
    let text_after_idx = lines
        .iter()
        .position(|l| l.to_string().contains("Text after"));

    if let (Some(bullet_idx), Some(text_idx)) = (last_bullet_idx, text_after_idx) {
        println!(
            "Last bullet at line {}, text at line {}, gap = {}",
            bullet_idx,
            text_idx,
            text_idx - bullet_idx
        );
        assert!(text_idx > bullet_idx, "Text should come after list");
    }
}

#[test]
fn debug_spacing_between_paragraphs() {
    let theme = MessageTheme::default();
    // Content with two paragraphs
    let content = "First paragraph\n\nSecond paragraph";
    let lines = MarkdownRenderer::render_content(content, &theme, None);

    println!("\n=== Spacing between paragraphs ===");
    println!("Input: {:?}", content);
    for (i, line) in lines.iter().enumerate() {
        let line_str = line.to_string();
        let is_empty = line_str.is_empty() || line.spans.is_empty();
        println!("Line {}: {:?} (empty: {})", i, line_str, is_empty);
    }

    // Count non-empty lines
    let non_empty_count = lines
        .iter()
        .filter(|l| {
            let s = l.to_string();
            !s.is_empty() && !l.spans.is_empty()
        })
        .count();

    println!("Non-empty lines: {}", non_empty_count);
    assert_eq!(non_empty_count, 2, "Should have 2 paragraph lines");
}

#[test]
fn debug_spacing_heading_to_paragraph() {
    let theme = MessageTheme::default();
    // Content with heading followed by paragraph
    let content = "# Heading\n\nParagraph text";
    let lines = MarkdownRenderer::render_content(content, &theme, None);

    println!("\n=== Spacing heading to paragraph ===");
    println!("Input: {:?}", content);
    for (i, line) in lines.iter().enumerate() {
        let line_str = line.to_string();
        let is_empty = line_str.is_empty() || line.spans.is_empty();
        println!("Line {}: {:?} (empty: {})", i, line_str, is_empty);
    }

    let heading_idx = lines.iter().position(|l| l.to_string().contains("Heading"));
    let paragraph_idx = lines
        .iter()
        .position(|l| l.to_string().contains("Paragraph"));

    if let (Some(h_idx), Some(p_idx)) = (heading_idx, paragraph_idx) {
        println!(
            "Heading at line {}, paragraph at line {}, gap = {}",
            h_idx,
            p_idx,
            p_idx - h_idx
        );
        assert!(p_idx > h_idx, "Paragraph should come after heading");
    }
}

#[test]
fn debug_spacing_code_block_to_text() {
    let theme = MessageTheme::default();
    // Content with code block followed by text
    let content = "```rust\nlet x = 42;\n```\n\nText after code";
    let lines = MarkdownRenderer::render_content(content, &theme, None);

    println!("\n=== Spacing code block to text ===");
    println!("Input: {:?}", content);
    for (i, line) in lines.iter().enumerate() {
        let line_str = line.to_string();
        let is_empty = line_str.is_empty() || line.spans.is_empty();
        println!("Line {}: {:?} (empty: {})", i, line_str, is_empty);
    }

    let code_end_idx = lines.iter().rposition(|l| l.to_string().contains('└'));
    let text_idx = lines
        .iter()
        .position(|l| l.to_string().contains("Text after"));

    if let (Some(c_idx), Some(t_idx)) = (code_end_idx, text_idx) {
        println!(
            "Code ends at line {}, text at line {}, gap = {}",
            c_idx,
            t_idx,
            t_idx - c_idx
        );
        assert!(t_idx > c_idx, "Text should come after code block");
    }
}

#[test]
fn debug_spacing_multiple_lists() {
    let theme = MessageTheme::default();
    // Content with two separate lists (need text between them to separate)
    let content = "* First list item\n\n\nSome text\n\n\n* Second list item";
    let lines = MarkdownRenderer::render_content(content, &theme, None);

    println!("\n=== Spacing multiple lists ===");
    println!("Input: {:?}", content);
    for (i, line) in lines.iter().enumerate() {
        let line_str = line.to_string();
        let is_empty = line_str.is_empty() || line.spans.is_empty();
        println!("Line {}: {:?} (empty: {})", i, line_str, is_empty);
    }

    // Should have exactly 2 bullet lines
    let bullet_count = lines.iter().filter(|l| l.to_string().contains('•')).count();

    println!("Bullet count: {}", bullet_count);
    assert_eq!(
        bullet_count, 2,
        "Should have 2 list items from separate lists"
    );
}

#[test]
fn debug_numbered_list_spacing() {
    let theme = MessageTheme::default();
    // Content with text followed by numbered list
    let content = "Text above\n\n1. First item\n2. Second item";
    let lines = MarkdownRenderer::render_content(content, &theme, None);

    println!("\n=== Numbered list spacing ===");
    println!("Input: {:?}", content);
    for (i, line) in lines.iter().enumerate() {
        let line_str = line.to_string();
        let is_empty = line_str.is_empty() || line.spans.is_empty();
        println!("Line {}: {:?} (empty: {})", i, line_str, is_empty);
    }

    let text_idx = lines
        .iter()
        .position(|l| l.to_string().contains("Text above"));
    let numbered_idx = lines.iter().position(|l| l.to_string().contains("1."));

    if let (Some(t_idx), Some(n_idx)) = (text_idx, numbered_idx) {
        println!(
            "Text at line {}, numbered at line {}, gap = {}",
            t_idx,
            n_idx,
            n_idx - t_idx
        );
        assert!(
            n_idx > t_idx + 1,
            "Should have blank line between text and numbered list"
        );
    }
}

#[test]
fn debug_style_leak_emphasis_to_list() {
    let theme = MessageTheme::default();
    // Content with emphasis followed by list
    let content = "Some *italic* text\n\n* Item 1";
    let lines = MarkdownRenderer::render_content(content, &theme, None);

    println!("\n=== Style leak: emphasis to list ===");
    println!("Input: {:?}", content);

    for (i, line) in lines.iter().enumerate() {
        for (j, span) in line.spans.iter().enumerate() {
            if span.content.contains('•') {
                println!("Bullet at line {} span {}: fg={:?}", i, j, span.style.fg);
                assert_eq!(
                    span.style.fg,
                    Some(Color::Blue),
                    "Bullet should be Blue, not leaked italic color"
                );
            }
        }
    }
}

#[test]
fn debug_style_leak_strong_to_list() {
    let theme = MessageTheme::default();
    // Content with strong followed by list
    let content = "Some **bold** text\n\n* Item 1";
    let lines = MarkdownRenderer::render_content(content, &theme, None);

    println!("\n=== Style leak: strong to list ===");
    println!("Input: {:?}", content);

    for (i, line) in lines.iter().enumerate() {
        for (j, span) in line.spans.iter().enumerate() {
            if span.content.contains('•') {
                println!("Bullet at line {} span {}: fg={:?}", i, j, span.style.fg);
                assert_eq!(
                    span.style.fg,
                    Some(Color::Blue),
                    "Bullet should be Blue, not leaked bold color"
                );
            }
        }
    }
}
