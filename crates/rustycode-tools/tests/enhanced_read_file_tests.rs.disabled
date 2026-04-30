/// Tests for enhanced read_file tool features
///
/// This test file covers:
/// - Pattern matching with regex
/// - File statistics calculation
/// - Pagination with offset/limit
/// - Comment line counting
/// - Complexity estimation
use rustycode_tools::{count_comment_lines, estimate_complexity};

#[test]
fn test_comment_counting_by_language() {
    // Test comment counting for different languages

    // Rust comments
    let rust_lines = vec![
        "fn main() {",
        "    // This is a comment",
        "    println!(\"test\");",
        "}",
    ];
    let count = count_comment_lines(&rust_lines, Some("rust"));
    assert_eq!(count, 1);

    // Python comments
    let py_lines = vec![
        "def main():",
        "    # This is a comment",
        "    print('test')",
    ];
    let count = count_comment_lines(&py_lines, Some("python"));
    assert_eq!(count, 1);

    // Shell comments
    let shell_lines = vec!["#!/bin/bash", "# Comment", "echo test"];
    let count = count_comment_lines(&shell_lines, Some("shell"));
    assert_eq!(count, 2);

    // JSON has no comments
    let json_lines = vec!["{", "  \"key\": \"value\"", "}"];
    let count = count_comment_lines(&json_lines, Some("json"));
    assert_eq!(count, 0);
}

#[test]
fn test_comment_counting_multiline_comments() {
    // Test handling of different comment styles

    // C-style multiline comment
    let c_lines = vec![
        "int main() {",
        "    /* This is a",
        "       multiline comment */",
        "    return 0;",
        "}",
    ];
    let count = count_comment_lines(&c_lines, Some("c"));
    assert!(count >= 1); // Should detect the multiline comment
}

#[test]
fn test_complexity_estimation() {
    // Test complexity estimation logic

    // Simple file
    let complexity = estimate_complexity(30, 5);
    assert_eq!(complexity, "simple");

    // Medium file
    let complexity = estimate_complexity(150, 30);
    assert_eq!(complexity, "medium");

    // Large file
    let complexity = estimate_complexity(300, 60);
    assert_eq!(complexity, "high");

    // Very large file
    let complexity = estimate_complexity(1000, 200);
    assert_eq!(complexity, "very_high");
}

#[test]
fn test_complexity_with_high_comment_ratio() {
    // File with many comments (medium complexity due to file size)
    // 200 lines with 150 comments = 50 code lines (25% code ratio)
    // Files 200-499 lines are "medium" unless code ratio > 60%
    let complexity = estimate_complexity(200, 150);
    assert_eq!(complexity, "medium");
}

#[test]
fn test_complexity_with_low_comment_ratio() {
    // File with few comments (high complexity)
    let complexity = estimate_complexity(200, 10);
    assert_eq!(complexity, "high");
}
