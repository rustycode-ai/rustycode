use super::*;
use crate::{Tool, ToolContext};
use tempfile::tempdir;

#[test]
fn test_edit_file_valid_operation() {
    let workspace = tempdir().unwrap();
    let test_file = workspace.path().join("test.txt");
    std::fs::write(&test_file, "hello world").unwrap();

    let tool = EditFile;
    let ctx = ToolContext::new(workspace.path());

    let params = serde_json::json!({
        "path": "test.txt",
        "old_text": "world",
        "new_text": "rust"
    });

    let result = tool.execute(params, &ctx);
    assert!(result.is_ok());

    let content = std::fs::read_to_string(&test_file).unwrap();
    assert_eq!(content, "hello rust");
}

#[test]
fn test_edit_file_blocks_path_traversal() {
    let workspace = tempdir().unwrap();
    let ctx = ToolContext::new(workspace.path());

    let params = serde_json::json!({
        "path": "../../../etc/passwd",
        "old_text": "root",
        "new_text": "hacked"
    });

    let tool = EditFile;
    let result = tool.execute(params, &ctx);
    assert!(result.is_err());
}

#[test]
fn test_edit_file_respects_size_limits() {
    let workspace = tempdir().unwrap();
    let test_file = workspace.path().join("test.txt");
    std::fs::write(&test_file, "small").unwrap();

    let tool = EditFile;
    let ctx = ToolContext::new(workspace.path());

    let huge_content = "x".repeat(20 * 1024 * 1024);

    let params = serde_json::json!({
        "path": "test.txt",
        "old_text": "small",
        "new_text": huge_content
    });

    let result = tool.execute(params, &ctx);
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("limit"));
}

#[test]
fn edit_file_tool_name() {
    assert_eq!(EditFile.name(), "Edit");
}

#[test]
fn edit_file_tool_permission() {
    assert_eq!(EditFile.permission(), ToolPermission::Write);
}

#[test]
fn edit_file_schema_has_required_fields() {
    let schema = EditFile.parameters_schema();
    let required = schema["required"].as_array().unwrap();
    assert!(required.iter().any(|r| r == "file_path"));
    assert!(required.iter().any(|r| r == "old_string"));
    assert!(required.iter().any(|r| r == "new_string"));
}

#[test]
fn edit_file_input_serde_roundtrip() {
    let input = EditFileParams {
        file_path: PathBuf::from("src/main.rs"),
        old_string: "fn main".into(),
        new_string: "fn main()".into(),
        replace_all: false,
    };
    let json = serde_json::to_string(&input).unwrap();
    let decoded: EditFileParams = serde_json::from_str(&json).unwrap();
    assert_eq!(decoded.file_path, PathBuf::from("src/main.rs"));
    assert_eq!(decoded.old_string, "fn main");
}

#[test]
fn edit_file_old_text_not_found() {
    let workspace = tempdir().unwrap();
    let test_file = workspace.path().join("test.txt");
    std::fs::write(&test_file, "hello world").unwrap();

    let tool = EditFile;
    let ctx = ToolContext::new(workspace.path());

    let params = serde_json::json!({
        "path": "test.txt",
        "old_text": "does_not_exist",
        "new_text": "replacement"
    });

    let result = tool.execute(params, &ctx).unwrap();
    assert!(result.text.contains("not found"));
}

#[test]
fn edit_file_invalid_params() {
    let workspace = tempdir().unwrap();
    let tool = EditFile;
    let ctx = ToolContext::new(workspace.path());

    let params = serde_json::json!({
        "path": 123
    });

    let result = tool.execute(params, &ctx);
    assert!(result.is_err());
}

#[test]
fn edit_file_rejects_binary_content() {
    let workspace = tempdir().unwrap();
    let test_file = workspace.path().join("test.bin");
    std::fs::write(&test_file, [0xff, 0xfe, 0xfd]).unwrap();

    let tool = EditFile;
    let ctx = ToolContext::new(workspace.path());

    let params = serde_json::json!({
        "path": "test.bin",
        "old_text": "a",
        "new_text": "b"
    });

    let result = tool.execute(params, &ctx);
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("non-UTF-8 file"));
}

#[test]
fn edit_file_shows_diff_output() {
    let workspace = tempdir().unwrap();
    let test_file = workspace.path().join("test.txt");
    std::fs::write(&test_file, "hello world\nfoo bar\n").unwrap();

    let tool = EditFile;
    let ctx = ToolContext::new(workspace.path());

    let params = serde_json::json!({
        "path": "test.txt",
        "old_text": "world\nfoo",
        "new_text": "rust\nbaz"
    });

    let result = tool.execute(params, &ctx).unwrap();
    assert!(result.text.contains("Changes in test.txt"));
    assert!(result.text.contains("+2 -2"));
    assert!(result.text.contains("+hello rust"));
    assert!(result.text.contains("-hello world"));
}

#[test]
fn edit_file_line_ending_normalized_match() {
    // File has CRLF, search text has LF — should still match
    let workspace = tempdir().unwrap();
    let test_file = workspace.path().join("test.txt");
    std::fs::write(&test_file, "hello\r\nworld\r\n").unwrap();

    let tool = EditFile;
    let ctx = ToolContext::new(workspace.path());

    let params = serde_json::json!({
        "path": "test.txt",
        "old_text": "hello\nworld",
        "new_text": "hello\nrust"
    });

    let result = tool.execute(params, &ctx);
    assert!(result.is_ok());
}

#[test]
fn edit_file_trimmed_match_uses_new_text() {
    let workspace = tempdir().unwrap();
    let test_file = workspace.path().join("test.txt");
    std::fs::write(&test_file, "fn main() {\n    println!(\"hi\");\n}\n").unwrap();

    let tool = EditFile;
    let ctx = ToolContext::new(workspace.path());

    let params = serde_json::json!({
        "path": "test.txt",
        "old_text": "fn main() {\nprintln!(\"hi\");\n}",
        "new_text": "fn main() {\nprintln!(\"bye\");\n}"
    });

    let result = tool.execute(params, &ctx).unwrap();
    let content = std::fs::read_to_string(&test_file).unwrap();
    assert!(content.contains("bye"));
    assert!(!content.contains("hi"));
    assert!(result.text.contains("Edited test.txt"));
}

#[test]
fn edit_file_trimmed_match_preserves_replacement_indentation() {
    let workspace = tempdir().unwrap();
    let test_file = workspace.path().join("test.txt");
    std::fs::write(&test_file, "if true {\n    old_call();\n}\n").unwrap();

    let tool = EditFile;
    let ctx = ToolContext::new(workspace.path());

    let params = serde_json::json!({
        "path": "test.txt",
        "old_text": "if true {\nold_call();\n}",
        "new_text": "if true {\n        new_call();\n    nested();\n}"
    });

    let result = tool.execute(params, &ctx).unwrap();
    let content = std::fs::read_to_string(&test_file).unwrap();
    assert!(content.contains("        new_call();"));
    assert!(content.contains("    nested();"));
    assert!(result.text.contains("Edited test.txt"));
}

#[test]
fn edit_file_not_found_shows_context() {
    let workspace = tempdir().unwrap();
    let test_file = workspace.path().join("test.txt");
    std::fs::write(&test_file, "line one\nline two\nline three").unwrap();

    let tool = EditFile;
    let ctx = ToolContext::new(workspace.path());

    let params = serde_json::json!({
        "path": "test.txt",
        "old_text": "not here",
        "new_text": "replacement"
    });

    let result = tool.execute(params, &ctx).unwrap();
    assert!(result.text.contains("File content"));
    assert!(result.text.contains("line one"));
    assert!(result.text.contains("Searched for"));
}

#[test]
fn edit_file_rejects_empty_old_text() {
    let workspace = tempdir().unwrap();
    let test_file = workspace.path().join("test.txt");
    std::fs::write(&test_file, "hello world").unwrap();

    let tool = EditFile;
    let ctx = ToolContext::new(workspace.path());

    let params = serde_json::json!({
        "path": "test.txt",
        "old_text": "",
        "new_text": "injected"
    });

    let result = tool.execute(params, &ctx);
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("empty"));
}

#[test]
fn edit_file_trimmed_match_preserves_surrounding_content() {
    // Bug regression test: trimmed match must NOT discard lines around the match
    let workspace = tempdir().unwrap();
    let test_file = workspace.path().join("test.txt");
    std::fs::write(
        &test_file,
        "line one\nline two\n    line three\nline four\nline five\n",
    )
    .unwrap();

    let tool = EditFile;
    let ctx = ToolContext::new(workspace.path());

    let params = serde_json::json!({
        "path": "test.txt",
        "old_text": "line two\nline three\nline four",
        "new_text": "replaced two\nreplaced three\nreplaced four"
    });

    let _ = tool.execute(params, &ctx).unwrap();
    let content = std::fs::read_to_string(&test_file).unwrap();
    // Must preserve lines before and after the match
    assert!(
        content.contains("line one"),
        "should preserve line before match"
    );
    assert!(
        content.contains("line five"),
        "should preserve line after match"
    );
    assert!(content.contains("replaced two"));
}

#[test]
fn edit_file_exact_match_multiline() {
    let workspace = tempdir().unwrap();
    let test_file = workspace.path().join("test.txt");
    std::fs::write(&test_file, "aaa\nbbb\nccc\nddd\neee\n").unwrap();

    let tool = EditFile;
    let ctx = ToolContext::new(workspace.path());

    let params = serde_json::json!({
        "path": "test.txt",
        "old_text": "bbb\nccc\nddd",
        "new_text": "BBB\nCCC\nDDD"
    });

    let _ = tool.execute(params, &ctx).unwrap();
    let content = std::fs::read_to_string(&test_file).unwrap();
    assert_eq!(content, "aaa\nBBB\nCCC\nDDD\neee\n");
}

#[test]
fn edit_file_normalized_match_preserves_crlf() {
    let workspace = tempdir().unwrap();
    let test_file = workspace.path().join("test.txt");
    std::fs::write(&test_file, "alpha\r\nbeta\r\ngamma\r\n").unwrap();

    let tool = EditFile;
    let ctx = ToolContext::new(workspace.path());

    let params = serde_json::json!({
        "path": "test.txt",
        "old_text": "alpha\nbeta",
        "new_text": "ALPHA\nBETA"
    });

    let result = tool.execute(params, &ctx).unwrap();
    assert!(result.text.contains("Edited test.txt"));
    let content = std::fs::read_to_string(&test_file).unwrap();
    // CRLF should be preserved
    assert!(
        content.contains("ALPHA\r\nBETA"),
        "CRLF should be preserved in output"
    );
    assert!(
        content.contains("gamma\r\n"),
        "unmatched line should keep CRLF"
    );
}

#[test]
fn edit_file_exact_match_single_line_no_newline() {
    // File with no trailing newline should remain without one after exact match
    let workspace = tempdir().unwrap();
    let test_file = workspace.path().join("test.txt");
    std::fs::write(&test_file, "hello world").unwrap();

    let tool = EditFile;
    let ctx = ToolContext::new(workspace.path());

    let params = serde_json::json!({
        "path": "test.txt",
        "old_text": "world",
        "new_text": "rust"
    });

    let _ = tool.execute(params, &ctx).unwrap();
    let content = std::fs::read_to_string(&test_file).unwrap();
    assert_eq!(content, "hello rust");
    assert!(!content.ends_with('\n'), "should not add trailing newline");
}

/// Integration test: read normalizes CRLF→LF, edit handles CRLF files correctly
#[test]
fn edit_file_after_read_normalization() {
    // Simulate what happens in production: read_file gives the LLM LF-normalized
    // content, the LLM sends old_text with LF, but the actual file has CRLF.
    // edit_file's normalized match strategy handles this.
    let workspace = tempdir().unwrap();
    let test_file = workspace.path().join("test.txt");
    std::fs::write(&test_file, "line one\r\nline two\r\nline three\r\n").unwrap();

    // LLM sees (from read_file): "line one\nline two\nline three"
    // LLM sends old_text with LF, edit_file should handle CRLF
    let tool = EditFile;
    let ctx = ToolContext::new(workspace.path());

    let params = serde_json::json!({
        "path": "test.txt",
        "old_text": "line two",
        "new_text": "LINE TWO"
    });

    let result = tool.execute(params, &ctx);
    assert!(result.is_ok());

    let content = std::fs::read_to_string(&test_file).unwrap();
    // CRLF should be preserved in unchanged lines
    assert!(
        content.contains("line one\r\n"),
        "CRLF preserved before edit"
    );
    assert!(content.contains("LINE TWO"), "replacement applied");
    assert!(
        content.contains("line three\r\n"),
        "CRLF preserved after edit"
    );
}

/// Integration test: trimmed match works for indented code
#[test]
fn edit_file_trimmed_match_for_indented_code() {
    // LLM often normalizes indentation. Trimmed match handles this.
    let workspace = tempdir().unwrap();
    let test_file = workspace.path().join("test.rs");
    std::fs::write(
        &test_file,
        "fn main() {\n    let x = 1;\n    println!(\"{}\", x);\n}\n",
    )
    .unwrap();

    let tool = EditFile;
    let ctx = ToolContext::new(workspace.path());

    // LLM sends without indentation
    let params = serde_json::json!({
        "path": "test.rs",
        "old_text": "let x = 1;\nprintln!(\"{}\", x);",
        "new_text": "let x = 2;\nprintln!(\"{}\", x);"
    });

    let result = tool.execute(params, &ctx);
    assert!(result.is_ok());

    let content = std::fs::read_to_string(&test_file).unwrap();
    assert!(content.contains("let x = 2;"), "replacement applied");
    assert!(content.contains("fn main()"), "surrounding code preserved");
    assert!(content.contains("}"), "closing brace preserved");
}

/// Integration test: non-unique match without replace_all returns warning
#[test]
fn edit_file_non_unique_match_returns_warning() {
    let workspace = tempdir().unwrap();
    let test_file = workspace.path().join("test.txt");
    std::fs::write(&test_file, "aaa\nbbb\naaa\nccc\naaa\n").unwrap();

    let tool = EditFile;
    let ctx = ToolContext::new(workspace.path());

    let params = serde_json::json!({
        "path": "test.txt",
        "old_text": "aaa",
        "new_text": "XXX"
    });

    let result = tool.execute(params, &ctx).unwrap();
    // Should warn about multiple matches instead of silently replacing
    assert!(
        result.text.contains("Found 3 matches"),
        "should warn about multiple matches: {}",
        result.text
    );
    // File should be unchanged
    let content = std::fs::read_to_string(&test_file).unwrap();
    assert_eq!(
        content, "aaa\nbbb\naaa\nccc\naaa\n",
        "file unchanged after non-unique match"
    );
}

/// Regression test: trimmed match must preserve trailing newlines
#[test]
fn edit_file_trimmed_match_preserves_trailing_newline() {
    let workspace = tempdir().unwrap();
    let test_file = workspace.path().join("test.txt");
    std::fs::write(&test_file, "if true {\n    old();\n}\n").unwrap();

    let tool = EditFile;
    let ctx = ToolContext::new(workspace.path());

    let params = serde_json::json!({
        "path": "test.txt",
        "old_text": "if true {\nold();\n}",
        "new_text": "if true {\nnew();\n}"
    });

    let _ = tool.execute(params, &ctx).unwrap();
    let content = std::fs::read_to_string(&test_file).unwrap();
    assert!(
        content.ends_with('\n'),
        "trailing newline must be preserved, got: {:?}",
        content
    );
    assert!(content.contains("new();"));
}

/// Regression test: trimmed match must preserve CRLF trailing newlines
#[test]
fn edit_file_trimmed_match_preserves_crlf_trailing_newline() {
    let workspace = tempdir().unwrap();
    let test_file = workspace.path().join("test.txt");
    std::fs::write(&test_file, "if true {\r\n    old();\r\n}\r\n").unwrap();

    let tool = EditFile;
    let ctx = ToolContext::new(workspace.path());

    let params = serde_json::json!({
        "path": "test.txt",
        "old_text": "if true {\nold();\n}",
        "new_text": "if true {\nnew();\n}"
    });

    let _ = tool.execute(params, &ctx).unwrap();
    let content = std::fs::read_to_string(&test_file).unwrap();
    assert!(
        content.ends_with("\r\n"),
        "CRLF trailing newline must be preserved, got: {:?}",
        content
    );
    assert!(content.contains("new();"));
}

// ── Unit tests for matching functions ──

#[test]
fn try_exact_match_finds_substring() {
    let content = "hello world";
    assert_eq!(try_exact_match(content, "world"), Some((6, 11)));
    assert_eq!(try_exact_match(content, "missing"), None);
}

#[test]
fn try_exact_match_empty_old_text() {
    // Empty old_text should match at position 0
    assert_eq!(try_exact_match("content", ""), Some((0, 0)));
}

#[test]
fn try_normalized_match_crlf_to_lf() {
    let content = "line1\r\nline2\r\nline3\r\n";
    let result = try_normalized_match(content, "line1\nline2", "foo\nbar");
    assert!(result.is_some());
    let output = result.unwrap();
    // Should preserve CRLF line endings
    assert!(output.contains("\r\n"));
    assert!(output.contains("foo"));
    assert!(output.contains("bar"));
}

#[test]
fn try_normalized_match_no_match_returns_none() {
    let content = "line1\r\nline2\r\n";
    assert_eq!(
        try_normalized_match(content, "missing", "replacement"),
        None
    );
}

#[test]
fn try_trimmed_match_ignores_whitespace() {
    let content = "  fn main()  \n    println!(\"hi\");  \n  }  \n";
    let result = try_trimmed_match(
        content,
        "fn main()\nprintln!(\"hi\");\n}",
        "fn main()\nprintln!(\"bye\");\n}",
    );
    assert!(result.is_some());
    assert!(result.unwrap().contains("bye"));
}

#[test]
fn try_trimmed_match_no_match_returns_none() {
    let content = "hello\nworld\n";
    assert_eq!(try_trimmed_match(content, "foo\nbar", "baz"), None);
}

#[test]
fn try_trimmed_match_empty_old_returns_none() {
    let content = "hello\n";
    assert_eq!(try_trimmed_match(content, "", "replacement"), None);
}

// ── Quote normalization tests ──

#[test]
fn try_quote_normalized_match_curly_double_quotes() {
    let content = "let msg = \u{201C}hello world\u{201D};\n";
    let result =
        try_quote_normalized_match(content, "let msg = \"hello world\";", "let msg = \"bye\";");
    assert!(result.is_some());
    let output = result.unwrap();
    assert!(
        output.contains("\"bye\""),
        "should replace content portion, got: {output:?}"
    );
    assert!(
        output.ends_with(";\n"),
        "should preserve suffix after match"
    );
}

#[test]
fn try_quote_normalized_match_curly_single_quotes() {
    let content = "it\u{2019}s working\n";
    let result = try_quote_normalized_match(content, "it's working", "it's broken");
    assert!(result.is_some());
    assert!(result.unwrap().contains("broken"));
}

#[test]
fn try_quote_normalized_match_no_match_returns_none() {
    let content = "hello world\n";
    assert_eq!(
        try_quote_normalized_match(content, "missing", "replacement"),
        None
    );
}

#[test]
fn edit_file_curly_quotes_in_old_text() {
    let workspace = tempdir().unwrap();
    let test_file = workspace.path().join("test.txt");
    // File has curly quotes
    std::fs::write(&test_file, "msg = \u{201C}hello\u{201D}\n").unwrap();

    let tool = EditFile;
    let ctx = ToolContext::new(workspace.path());

    // LLM sends straight quotes in old_text
    let params = serde_json::json!({
        "path": "test.txt",
        "old_text": "msg = \"hello\"",
        "new_text": "msg = \"world\""
    });

    let result = tool.execute(params, &ctx);
    assert!(result.is_ok());
    let content = std::fs::read_to_string(&test_file).unwrap();
    assert!(content.contains("world"), "replacement should be applied");
}

#[test]
fn edit_file_curly_quotes_in_file_and_old_text() {
    let workspace = tempdir().unwrap();
    let test_file = workspace.path().join("test.txt");
    // Both file and old_text have curly quotes — exact match should handle it
    std::fs::write(&test_file, "msg = \u{201C}hello\u{201D}\n").unwrap();

    let tool = EditFile;
    let ctx = ToolContext::new(workspace.path());

    let params = serde_json::json!({
        "path": "test.txt",
        "old_text": "msg = \u{201C}hello\u{201D}",
        "new_text": "msg = \u{201C}world\u{201D}"
    });

    let result = tool.execute(params, &ctx);
    assert!(result.is_ok());
    let content = std::fs::read_to_string(&test_file).unwrap();
    assert!(
        content.contains("\u{201C}world\u{201D}"),
        "exact match with curly quotes"
    );
}

#[test]
fn edit_file_accepts_catalog_field_names() {
    // The LLM tool schema uses file_path/old_string/new_string
    let workspace = tempdir().unwrap();
    let test_file = workspace.path().join("test.txt");
    std::fs::write(&test_file, "hello world\n").unwrap();

    let tool = EditFile;
    let ctx = ToolContext::new(workspace.path());

    let params = serde_json::json!({
        "file_path": "test.txt",
        "old_string": "hello",
        "new_string": "goodbye"
    });

    let result = tool.execute(params, &ctx);
    assert!(result.is_ok());
    let content = std::fs::read_to_string(&test_file).unwrap();
    assert!(content.contains("goodbye"));
}

#[test]
fn edit_file_replace_all() {
    let workspace = tempdir().unwrap();
    let test_file = workspace.path().join("test.txt");
    std::fs::write(&test_file, "aaa\nbbb\naaa\nccc\naaa\n").unwrap();

    let tool = EditFile;
    let ctx = ToolContext::new(workspace.path());

    let params = serde_json::json!({
        "path": "test.txt",
        "old_text": "aaa",
        "new_text": "XXX",
        "replace_all": true
    });

    let result = tool.execute(params, &ctx);
    assert!(result.is_ok());
    let content = std::fs::read_to_string(&test_file).unwrap();
    assert_eq!(
        content, "XXX\nbbb\nXXX\nccc\nXXX\n",
        "all occurrences replaced"
    );
}

#[test]
fn edit_file_replace_all_false_warns_on_multiple_matches() {
    let workspace = tempdir().unwrap();
    let test_file = workspace.path().join("test.txt");
    std::fs::write(&test_file, "aaa\nbbb\naaa\n").unwrap();

    let tool = EditFile;
    let ctx = ToolContext::new(workspace.path());

    let params = serde_json::json!({
        "path": "test.txt",
        "old_text": "aaa",
        "new_text": "XXX",
        "replace_all": false
    });

    let result = tool.execute(params, &ctx).unwrap();
    assert!(
        result.text.contains("Found 2 matches"),
        "should warn about multiple matches: {}",
        result.text
    );
    // File should be unchanged
    let content = std::fs::read_to_string(&test_file).unwrap();
    assert_eq!(
        content, "aaa\nbbb\naaa\n",
        "file unchanged without replace_all"
    );
}

#[test]
fn edit_file_unique_match_replaces_without_replace_all() {
    let workspace = tempdir().unwrap();
    let test_file = workspace.path().join("test.txt");
    std::fs::write(&test_file, "aaa\nbbb\nccc\n").unwrap();

    let tool = EditFile;
    let ctx = ToolContext::new(workspace.path());

    let params = serde_json::json!({
        "path": "test.txt",
        "old_text": "bbb",
        "new_text": "XXX",
        "replace_all": false
    });

    let result = tool.execute(params, &ctx);
    assert!(result.is_ok());
    let content = std::fs::read_to_string(&test_file).unwrap();
    assert_eq!(content, "aaa\nXXX\nccc\n", "unique match replaced");
}

#[test]
fn edit_file_not_found_suggests_similar() {
    let workspace = tempdir().unwrap();
    // Create a file with a similar name
    std::fs::write(workspace.path().join("main.rs"), "fn main() {}").unwrap();

    let tool = EditFile;
    let ctx = ToolContext::new(workspace.path());

    let params = serde_json::json!({
        "path": "man.rs",
        "old_text": "fn main",
        "new_text": "fn foo"
    });

    let result = tool.execute(params, &ctx);
    let err = result.unwrap_err().to_string();
    assert!(err.contains("path not found"), "got: {err}");
    // Should suggest main.rs since it has a similar stem
    assert!(err.contains("Did you mean"), "got: {err}");
    assert!(
        err.contains("main.rs"),
        "should suggest main.rs, got: {err}"
    );
}

#[test]
fn suggest_similar_finds_matching_stem() {
    let workspace = tempdir().unwrap();
    std::fs::write(workspace.path().join("config.toml"), "").unwrap();
    std::fs::write(workspace.path().join("config.yaml"), "").unwrap();
    std::fs::write(workspace.path().join("other.txt"), "").unwrap();

    let target = std::path::Path::new("config.json");
    let suggestions = suggest_similar_files(target, workspace.path());
    assert!(!suggestions.is_empty(), "should find similar files");
    assert!(
        suggestions.iter().any(|s| s.contains("config")),
        "should suggest config files, got: {suggestions:?}"
    );
}

#[test]
fn suggest_similar_empty_for_no_matches() {
    let workspace = tempdir().unwrap();
    let target = std::path::Path::new("completely_unique_name.xyz");
    let suggestions = suggest_similar_files(target, workspace.path());
    assert!(suggestions.is_empty(), "should find no matches");
}
