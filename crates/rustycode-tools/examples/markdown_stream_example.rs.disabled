//! Markdown Stream Example
//!
//! This example demonstrates how to use the MarkdownStream processor
//! for real-time markdown rendering during LLM streaming.

use rustycode_tools::markdown_stream::{MarkdownElement, MarkdownStream};

fn main() {
    println!("=== Markdown Stream Processor Demo ===\n");

    // Example 1: Simple text streaming
    println!("1. Simple text streaming:");
    let mut stream = MarkdownStream::new();
    let chunk1 = stream.push("Hello, ");
    println!("   Chunk 1: {:?}", chunk1);

    let chunk2 = stream.push("world!");
    println!("   Chunk 2: {:?}", chunk2);
    println!("   Full content: {}\n", stream.content());

    // Example 2: Code block detection
    println!("2. Code block streaming:");
    let mut stream = MarkdownStream::new();

    stream.push("```rust\n");
    println!(
        "   Opened code block, incomplete: {}",
        stream.is_incomplete()
    );

    stream.push("fn main() {\n");
    println!("   Still incomplete: {}", stream.is_incomplete());

    stream.push("    println!(\"Hello\");\n");
    stream.push("}\n");
    stream.push("```");
    println!("   Closed, incomplete: {}", stream.is_incomplete());
    println!("   Full content:\n{}\n", stream.content());

    // Example 3: Incomplete code block handling
    println!("3. Handling incomplete code blocks:");
    let mut stream = MarkdownStream::new();
    stream.push("```python\n");
    stream.push("print('Hello')\n");
    // Stream ends here without closing the code block

    println!("   Incomplete: {}", stream.is_incomplete());
    let sanitized = stream.sanitized_output();
    println!("   Sanitized output:\n{}\n", sanitized);

    // Example 4: Header detection
    println!("4. Header detection:");
    let headers = vec![
        "# H1",
        "## H2",
        "### H3",
        "#### H4",
        "##### H5",
        "###### H6",
    ];
    for header in headers {
        let mut stream = MarkdownStream::new();
        let chunk = stream.push(&format!("{}\n", header));
        if let MarkdownElement::Header { level } = chunk.element_type {
            println!("   Detected: {} (level {})", header, level);
        }
    }
    println!();

    // Example 5: List item detection
    println!("5. List item detection:");
    let list_items = ["- Item 1", "* Item 2", "  - Nested item"];
    for item in list_items {
        let mut stream = MarkdownStream::new();
        let chunk = stream.push(&format!("{}\n", item));
        if chunk.element_type == MarkdownElement::ListItem {
            println!("   Detected list item: {}", item);
        }
    }
    println!();

    // Example 6: Reset functionality
    println!("6. Reset functionality:");
    let mut stream = MarkdownStream::new();
    stream.push("```rust\n");
    stream.push("code");
    println!(
        "   Before reset - incomplete: {}, content: '{}'",
        stream.is_incomplete(),
        stream.content()
    );

    stream.reset();
    println!(
        "   After reset - incomplete: {}, content: '{}'",
        stream.is_incomplete(),
        stream.content()
    );
    println!();

    println!("=== Demo Complete ===");
}
