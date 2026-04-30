//! Test markdown rendering with actual output verification

use ratatui::style::Color;
use rustycode_tui::ui::markdown::{MarkdownRenderer, MessageTheme};

fn main() {
    let theme = MessageTheme::default();

    // Test 1: Heading followed by list (check for green bullets)
    println!("=== Test 1: Heading followed by list ===");
    let content1 = "# Plan\n\n* Phase 1\n* Phase 2";
    let lines1 = MarkdownRenderer::render_content(content1, &theme, None);
    println!("Content: {:?}", content1);
    println!("Lines: {} generated", lines1.len());
    for (i, line) in lines1.iter().enumerate() {
        let line_str = line.to_string();
        println!("  Line {}: {:?}", i, line_str);
        for (j, span) in line.spans.iter().enumerate() {
            println!(
                "    Span {}: {:?} | fg: {:?}",
                j, span.content, span.style.fg
            );
        }
    }

    // Test 2: Code block rendering
    println!("\n=== Test 2: Code block ===");
    let content2 = "```rust\nlet x = 42;\nprintln!(\"Hello\");\n```";
    let lines2 = MarkdownRenderer::render_content(content2, &theme, None);
    println!("Content: {:?}", content2);
    println!("Lines: {} generated", lines2.len());
    for (i, line) in lines2.iter().enumerate() {
        let line_str = line.to_string();
        println!("  Line {}: {:?}", i, line_str);
        for (j, span) in line.spans.iter().enumerate() {
            println!(
                "    Span {}: {:?} | fg: {:?}",
                j, span.content, span.style.fg
            );
        }
    }

    // Test 3: Bold heading followed by list
    println!("\n=== Test 3: Bold heading followed by list ===");
    let content3 = "# **Bold Heading**\n\n* Item 1";
    let lines3 = MarkdownRenderer::render_content(content3, &theme, None);
    println!("Content: {:?}", content3);
    println!("Lines: {} generated", lines3.len());
    for (i, line) in lines3.iter().enumerate() {
        let line_str = line.to_string();
        println!("  Line {}: {:?}", i, line_str);
        for (j, span) in line.spans.iter().enumerate() {
            println!(
                "    Span {}: {:?} | fg: {:?}",
                j, span.content, span.style.fg
            );
        }
    }

    // Test 4: Italic followed by list
    println!("\n=== Test 4: Italic followed by list ===");
    let content4 = "Text with *italic*\n\n* Bullet";
    let lines4 = MarkdownRenderer::render_content(content4, &theme, None);
    println!("Content: {:?}", content4);
    println!("Lines: {} generated", lines4.len());
    for (i, line) in lines4.iter().enumerate() {
        let line_str = line.to_string();
        println!("  Line {}: {:?}", i, line_str);
        for (j, span) in line.spans.iter().enumerate() {
            println!(
                "    Span {}: {:?} | fg: {:?}",
                j, span.content, span.style.fg
            );
        }
    }

    // Test 5: Numbered list with multi-digit numbers
    println!("\n=== Test 5: Numbered list (10+) ===");
    let content5 = "1. Item 1\n\n10. Item 10\n\n11. Item 11";
    let lines5 = MarkdownRenderer::render_content(content5, &theme, None);
    println!("Content: {:?}", content5);
    println!("Lines: {} generated", lines5.len());
    for (i, line) in lines5.iter().enumerate() {
        let line_str = line.to_string();
        println!("  Line {}: {:?}", i, line_str);
        for (j, span) in line.spans.iter().enumerate() {
            println!(
                "    Span {}: {:?} | fg: {:?}",
                j, span.content, span.style.fg
            );
        }
    }

    // Test 6: Long code line with special characters
    println!("\n=== Test 6: Long code line ===");
    let content6 =
        "```rust\nlet result = some_very_long_function_name().unwrap().expect(\"failed\");\n```";
    let lines6 = MarkdownRenderer::render_content(content6, &theme, None);
    println!("Content: {:?}", content6);
    println!("Lines: {} generated", lines6.len());
    for (i, line) in lines6.iter().enumerate() {
        let line_str = line.to_string();
        println!("  Line {}: {:?} (len: {})", i, line_str, line_str.len());
        for (j, span) in line.spans.iter().enumerate() {
            println!(
                "    Span {}: {:?} | fg: {:?}",
                j, span.content, span.style.fg
            );
        }
    }

    // Verify bullet colors are NOT green
    println!("\n=== Style verification ===");
    for (i, line) in lines1.iter().enumerate() {
        for (j, span) in line.spans.iter().enumerate() {
            if span.content.contains('•') {
                if span.style.fg == Some(Color::Green) {
                    println!("ERROR: Bullet is GREEN! Line {} Span {}", i, j);
                } else if span.style.fg == Some(Color::Yellow) {
                    println!("OK: Bullet is YELLOW at Line {} Span {}", i, j);
                } else {
                    println!(
                        "WARN: Bullet color is {:?} at Line {} Span {}",
                        span.style.fg, i, j
                    );
                }
            }
        }
    }

    // Verify code blocks have syntax highlighting (RGB colors)
    println!("\n=== Code highlighting verification ===");
    for line in &lines2 {
        for span in &line.spans {
            // Code blocks should use RGB colors for syntax highlighting
            if span.content.contains("let") || span.content.contains("println") {
                if let Some(Color::Rgb(r, g, b)) = span.style.fg {
                    println!(
                        "OK: Syntax highlight found - RGB({}, {}, {}) for: {:?}",
                        r, g, b, span.content
                    );
                } else {
                    println!("WARN: No RGB color for syntax: {:?}", span.content);
                }
            }
        }
    }

    println!("\n=== All tests completed ===");
}
