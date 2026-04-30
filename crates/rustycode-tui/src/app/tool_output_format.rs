//! Tool output formatting utilities
//!
//! Provides smart truncation, language auto-detection, and structured
//! formatting for tool execution output displayed in the TUI.
//!
//! Inspired by goose's streaming_buffer and task_execution_display patterns.

/// Strip ANSI escape sequences from a string.
///
/// Tool outputs (especially bash commands) may contain ANSI color codes
/// that render as garbage in the ratatui TUI. This strips them so only
/// the plain text content remains.
pub fn strip_ansi_escapes(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            // Skip ANSI escape sequence: ESC [ ... (letter)
            if chars.peek() == Some(&'[') {
                chars.next(); // consume '['
                              // Skip digits, semicolons, and other parameter bytes
                while let Some(&next) = chars.peek() {
                    if next.is_ascii_digit() || next == ';' || next == '?' {
                        chars.next();
                    } else {
                        break;
                    }
                }
                // Consume the final command byte (letter or @)
                if let Some(&cmd) = chars.peek() {
                    if cmd.is_ascii_alphabetic() || cmd == '@' {
                        chars.next();
                    }
                }
            }
        } else {
            result.push(c);
        }
    }
    result
}

/// Maximum lines to show in collapsed tool output
const COLLAPSED_PREVIEW_LINES: usize = 3;

/// Maximum lines to show for a single code block before truncation.
/// Must match `streaming_render_buffer::MAX_CODE_BLOCK_LINES`.
const MAX_CODE_BLOCK_LINES: usize = 50;

/// Lines to show from start/end when truncating
const TRUNCATION_HEAD_LINES: usize = 10;
const TRUNCATION_TAIL_LINES: usize = 5;

/// A formatted section of tool output
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum OutputSection {
    /// Plain text content
    Text(String),
    /// Code block with detected language
    Code {
        language: String,
        content: String,
        truncated: bool,
        total_lines: usize,
    },
    /// Truncation indicator showing how many lines were hidden
    Truncated { hidden_lines: usize },
}

/// Detect if a string looks like structured data and return the language tag
pub fn detect_language(content: &str) -> Option<&'static str> {
    let trimmed = content.trim();

    // JSON: starts with { or [
    if ((trimmed.starts_with('{') && trimmed.ends_with('}'))
        || (trimmed.starts_with('[') && trimmed.ends_with(']')))
        && serde_json::from_str::<serde_json::Value>(trimmed).is_ok()
    {
        return Some("json");
    }

    // XML/HTML: starts with < and contains </
    if trimmed.starts_with('<') && trimmed.contains("</") && trimmed.ends_with('>') {
        if trimmed.starts_with("<?xml") {
            return Some("xml");
        }
        return Some("html");
    }

    // Rust: common patterns
    if (trimmed.contains("fn ")
        || trimmed.contains("impl ")
        || trimmed.contains("pub struct ")
        || trimmed.contains("use ")
        || trimmed.contains("let mut ")
        || trimmed.contains("pub fn ")
        || trimmed.contains("async fn "))
        && (trimmed.contains("::")
            || trimmed.contains("let ")
            || trimmed.contains("pub ")
            || trimmed.contains('&')
            || trimmed.contains("-> ")
            || trimmed.contains("println!")
            || trimmed.contains("macro_rules!")
            || trimmed.contains('#'))
    {
        return Some("rust");
    }

    // Python: common patterns
    if (trimmed.contains("def ") || trimmed.contains("class ") || trimmed.contains("import "))
        && (trimmed.contains(":") && !trimmed.contains("::"))
    {
        return Some("python");
    }

    // Shell: starts with common commands
    if trimmed.starts_with("$ ")
        || trimmed.starts_with("cargo ")
        || trimmed.starts_with("npm ")
        || trimmed.starts_with("git ")
        || trimmed.starts_with("rustup ")
    {
        return Some("sh");
    }

    // TOML: has [[section]] or [section] = value patterns
    if (trimmed.starts_with('[') || trimmed.contains("\n["))
        && trimmed.contains('=')
        && !trimmed.contains("==")
        && !trimmed.contains("fn ")
        && !trimmed.contains("let ")
    {
        return Some("toml");
    }

    // YAML: common patterns with `:` key-value
    if (trimmed.contains(": ") || trimmed.contains(":\n"))
        && !trimmed.contains("::")
        && !trimmed.contains("fn ")
        && !trimmed.starts_with('{')
    {
        return Some("yaml");
    }

    None
}

/// Format tool output for display with truncation support
///
/// Returns a list of formatted lines suitable for TUI rendering.
/// Long outputs are truncated with a summary of hidden lines.
pub fn format_tool_output(output: &str, is_expanded: bool, width: usize) -> Vec<String> {
    if output.is_empty() {
        return vec!["  (no output)".to_string()];
    }

    let lines: Vec<&str> = output.lines().collect();

    if is_expanded {
        // Show all lines, but truncate individual code blocks if present
        format_expanded_output(&lines, width)
    } else {
        // Collapsed: show preview lines only
        format_collapsed_output(&lines)
    }
}

/// Format output in collapsed mode (preview only)
fn format_collapsed_output(lines: &[&str]) -> Vec<String> {
    let mut result = Vec::new();

    let preview_count = COLLAPSED_PREVIEW_LINES.min(lines.len());

    for line in lines.iter().take(preview_count) {
        result.push(format!("  {}", line));
    }

    if lines.len() > COLLAPSED_PREVIEW_LINES {
        let remaining = lines.len() - COLLAPSED_PREVIEW_LINES;
        result.push(format!("  … +{} lines (Enter to expand)", remaining));
    }

    result
}

/// Format output in expanded mode (full content with smart truncation)
fn format_expanded_output(lines: &[&str], _width: usize) -> Vec<String> {
    let mut result = Vec::new();

    // For very long outputs, truncate the middle
    if lines.len() > MAX_CODE_BLOCK_LINES * 2 {
        // Show head
        for line in lines.iter().take(TRUNCATION_HEAD_LINES) {
            result.push(format!("  {}", line));
        }

        let hidden = lines.len() - TRUNCATION_HEAD_LINES - TRUNCATION_TAIL_LINES;
        result.push(format!("  ┊ {} lines hidden ┊", hidden));

        // Show tail
        for line in lines.iter().skip(lines.len() - TRUNCATION_TAIL_LINES) {
            result.push(format!("  {}", line));
        }
    } else {
        for line in lines {
            result.push(format!("  {}", line));
        }
    }

    result
}

/// Get a short summary of tool output (for status line)
///
/// Extracts meaningful information from tool output:
/// - File paths from read/write/edit operations
/// - Diff summaries (+N/-N lines) for edit operations
/// - Match counts from grep/search
/// - Line/char counts for large outputs
pub fn output_summary(output: &str) -> String {
    if output.is_empty() {
        return "no output".to_string();
    }

    // Single-pass line counting + diff detection (avoids multiple full scans)
    let mut line_count = 0usize;
    let mut added = 0usize;
    let mut removed = 0usize;
    for line in output.lines() {
        line_count += 1;
        let trimmed = line.trim_start();
        if trimmed.starts_with('+') && !trimmed.starts_with("+++") {
            added += 1;
        } else if trimmed.starts_with('-') && !trimmed.starts_with("---") {
            removed += 1;
        }
    }

    // Try diff summary from the counts we already have
    if added + removed >= 3 {
        let mut parts = Vec::new();
        if added > 0 {
            parts.push(format!("+{}", added));
        }
        if removed > 0 {
            parts.push(format!("-{}", removed));
        }
        if !parts.is_empty() {
            if let Some(path) = extract_file_path(output) {
                return format!("{} {}", parts.join("/"), path);
            }
            return parts.join("/");
        }
    }

    // Try to extract a file path from the output
    if let Some(path) = extract_file_path(output) {
        if line_count > 5 {
            return format!("{} ({} lines)", path, line_count);
        }
        return path;
    }

    // Try to extract grep/search match count
    if let Some(match_summary) = extract_grep_summary(output, line_count) {
        return match_summary;
    }

    let char_count = output.len(); // byte count is close enough for display

    if line_count <= 1 {
        if char_count > 80 {
            format!("{} chars", char_count)
        } else {
            output.to_string()
        }
    } else if line_count <= 3 && char_count < 200 {
        // Short output: show first line (no allocation needed)
        let first = output.lines().next().unwrap_or("");
        if first.len() > 60 {
            format!("{}…", crate::unicode::truncate_bytes(first, 60))
        } else {
            first.to_string()
        }
    } else {
        format!("{} lines, {} chars", line_count, char_count)
    }
}

/// Extract a file path from tool output (e.g., "Read file: /path/to/file" or "/path/to/file")
fn extract_file_path(output: &str) -> Option<String> {
    let first_line = output.lines().next()?;

    // Pattern: "File: /path" or "file: /path"
    if let Some(rest) = first_line
        .strip_prefix("File: ")
        .or_else(|| first_line.strip_prefix("file: "))
    {
        let path = rest.split_whitespace().next().unwrap_or(rest);
        return Some(shorten_path(path));
    }

    // Pattern: "/path/to/file" at start of line (absolute path)
    if first_line.starts_with('/') && first_line.contains('.') {
        let path = first_line.split_whitespace().next().unwrap_or(first_line);
        // Must look like a real file path (has extension or common dir)
        if path.contains('.') || path.contains("src/") || path.contains("crates/") {
            return Some(shorten_path(path));
        }
    }

    None
}

/// Extract grep/search match count from output
fn extract_grep_summary(output: &str, line_count: usize) -> Option<String> {
    if line_count == 0 {
        return None;
    }

    // Pattern: "N matches found" or "Found N results"
    let lower = output.to_lowercase();
    if let Some(idx) = lower.find("match") {
        // Look backwards for a number
        let before = &lower[..idx];
        if let Some(num) = before.split_whitespace().last() {
            if let Ok(n) = num.parse::<usize>() {
                return Some(format!("{} matches", n));
            }
        }
    }

    // Pattern: output is just file paths with line numbers (grep results)
    // Count non-empty lines as matches
    let non_empty = output.lines().filter(|l| !l.trim().is_empty()).count();
    if non_empty > 0 && non_empty <= 50 {
        // Check if lines look like grep output (path:number:content or path:content)
        let grep_like = output.lines().take(5).all(|l| {
            let trimmed = l.trim();
            trimmed.is_empty() || trimmed.contains(':')
        });
        if grep_like && non_empty > 1 {
            return Some(format!("{} matches", non_empty));
        }
    }

    None
}

/// Shorten a file path for display: keep last 2 components
fn shorten_path(path: &str) -> String {
    let parts: Vec<&str> = path.rsplitn(3, '/').collect();
    match parts.len() {
        0 => path.to_string(),
        1 => parts[0].to_string(),
        2 => format!("{}/{}", parts[1], parts[0]),
        _ => format!("…/{}/{}", parts[1], parts[0]),
    }
}

/// Format a duration in milliseconds for display
pub fn format_duration(ms: u64) -> String {
    if ms < 1000 {
        format!("{}ms", ms)
    } else if ms < 60_000 {
        format!("{:.1}s", ms as f64 / 1000.0)
    } else {
        let secs = ms / 1000;
        let mins = secs / 60;
        let remain_secs = secs % 60;
        format!("{}m {}s", mins, remain_secs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_json() {
        assert_eq!(detect_language(r#"{"key": "value"}"#), Some("json"));
        assert_eq!(detect_language(r#"[1, 2, 3]"#), Some("json"));
        assert_eq!(detect_language(r#"{"nested": {"key": 42}}"#), Some("json"));
        assert_eq!(detect_language("not json"), None);
    }

    #[test]
    fn test_detect_xml() {
        assert_eq!(
            detect_language("<?xml version=\"1.0\"?>\n<root></root>"),
            Some("xml")
        );
        assert_eq!(detect_language("<div>content</div>"), Some("html"));
    }

    #[test]
    fn test_detect_rust() {
        assert_eq!(
            detect_language("fn main() {\n    println!(\"hello\");\n}"),
            Some("rust")
        );
        assert_eq!(
            detect_language("use std::io;\n\npub struct Foo;"),
            Some("rust")
        );
    }

    #[test]
    fn test_detect_python() {
        assert_eq!(
            detect_language("def hello():\n    print('hi')"),
            Some("python")
        );
    }

    #[test]
    fn test_detect_shell() {
        assert_eq!(detect_language("$ cargo build"), Some("sh"));
        assert_eq!(detect_language("git status"), Some("sh"));
    }

    #[test]
    fn test_detect_toml() {
        assert_eq!(
            detect_language("[dependencies]\nserde = \"1.0\""),
            Some("toml")
        );
    }

    #[test]
    fn test_detect_yaml() {
        assert_eq!(detect_language("name: test\nversion: 1.0"), Some("yaml"));
    }

    #[test]
    fn test_detect_plain_text() {
        assert_eq!(detect_language("just some text"), None);
        assert_eq!(detect_language(""), None);
    }

    #[test]
    fn test_format_collapsed_short() {
        let output = "line1\nline2";
        let result = format_tool_output(output, false, 80);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0], "  line1");
        assert_eq!(result[1], "  line2");
    }

    #[test]
    fn test_format_collapsed_long() {
        let lines: Vec<String> = (0..20).map(|i| format!("line {}", i)).collect();
        let output = lines.join("\n");
        let result = format_tool_output(&output, false, 80);

        // Should show preview + truncation indicator
        assert!(result.len() <= COLLAPSED_PREVIEW_LINES + 1);
        assert!(result.last().unwrap().contains("lines"));
        assert!(result.last().unwrap().contains("Enter"));
    }

    #[test]
    fn test_format_expanded_short() {
        let output = "line1\nline2\nline3";
        let result = format_tool_output(output, true, 80);
        assert_eq!(result.len(), 3);
    }

    #[test]
    fn test_format_expanded_very_long() {
        let lines: Vec<String> = (0..200).map(|i| format!("line {}", i)).collect();
        let output = lines.join("\n");
        let result = format_tool_output(&output, true, 80);

        // Should have head + truncation indicator + tail
        assert!(result.len() < 200);
        assert!(result.iter().any(|l| l.contains("lines hidden")));
    }

    #[test]
    fn test_output_summary_empty() {
        assert_eq!(output_summary(""), "no output");
    }

    #[test]
    fn test_output_summary_short() {
        assert_eq!(output_summary("hello"), "hello");
    }

    #[test]
    fn test_output_summary_long_single_line() {
        let long = "a".repeat(100);
        assert_eq!(output_summary(&long), "100 chars");
    }

    #[test]
    fn test_output_summary_multi_line() {
        let multi = "line1\nline2\nline3\nline4\nline5";
        let char_count = multi.chars().count();
        assert_eq!(
            output_summary(multi),
            format!("5 lines, {} chars", char_count)
        );
    }

    #[test]
    fn test_format_duration() {
        assert_eq!(format_duration(50), "50ms");
        assert_eq!(format_duration(500), "500ms");
        assert_eq!(format_duration(1500), "1.5s");
        assert_eq!(format_duration(90000), "1m 30s");
    }

    #[test]
    fn test_format_empty_output() {
        let result = format_tool_output("", false, 80);
        assert_eq!(result, vec!["  (no output)"]);
    }

    #[test]
    fn test_strip_ansi_simple() {
        assert_eq!(strip_ansi_escapes("\x1b[31mError\x1b[0m"), "Error");
    }

    #[test]
    fn test_strip_ansi_multiple() {
        assert_eq!(
            strip_ansi_escapes("\x1b[1;32mOK\x1b[0m \x1b[33mwarn\x1b[0m"),
            "OK warn"
        );
    }

    #[test]
    fn test_strip_ansi_no_codes() {
        assert_eq!(strip_ansi_escapes("plain text"), "plain text");
    }

    #[test]
    fn test_strip_ansi_empty() {
        assert_eq!(strip_ansi_escapes(""), "");
    }

    #[test]
    fn test_strip_ansi_clear_screen() {
        assert_eq!(strip_ansi_escapes("\x1b[2J\x1b[HHello"), "Hello");
    }

    #[test]
    fn test_output_summary_file_path() {
        let summary = output_summary("File: /src/main.rs\nline 1\nline 2");
        assert!(
            summary.contains("main.rs"),
            "Expected file name, got: {}",
            summary
        );
    }

    #[test]
    fn test_output_summary_diff() {
        let diff = "File: src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n+added line\n+another\n-removed\n-context";
        let summary = output_summary(diff);
        // Diff detection counts +/- lines including diff headers, so counts may differ
        assert!(
            summary.contains("+") || summary.contains("-"),
            "Expected diff summary, got: {}",
            summary
        );
        assert!(
            summary.contains("lib.rs"),
            "Expected file path, got: {}",
            summary
        );
    }

    #[test]
    fn test_output_summary_grep_matches() {
        let grep_output = "src/main.rs:10:fn main() {\nsrc/lib.rs:5:pub fn foo() {";
        let summary = output_summary(grep_output);
        assert!(
            summary.contains("2 matches"),
            "Expected match count, got: {}",
            summary
        );
    }

    #[test]
    fn test_output_summary_large_file_path() {
        let output = "File: /Users/nat/dev/project/src/components/button.rs\nline1\nline2\nline3\nline4\nline5\nline6";
        let summary = output_summary(output);
        assert!(
            summary.contains("button.rs"),
            "Expected file name in summary, got: {}",
            summary
        );
        assert!(
            summary.contains("lines"),
            "Expected line count, got: {}",
            summary
        );
    }

    #[test]
    fn test_shorten_path() {
        assert_eq!(shorten_path("file.rs"), "file.rs");
        assert_eq!(shorten_path("src/file.rs"), "src/file.rs");
        assert_eq!(
            shorten_path("/very/long/path/to/src/file.rs"),
            "…/src/file.rs"
        );
    }

    #[test]
    fn test_output_summary_diff_only() {
        let diff = "+new line 1\n+new line 2\n+new line 3\n-old line\n-context";
        let summary = output_summary(diff);
        // -context line counts as a removal (starts with '-')
        assert!(
            summary.contains("+3/-2"),
            "Expected diff summary, got: {}",
            summary
        );
    }
}
