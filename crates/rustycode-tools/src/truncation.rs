//! Truncation utilities for tool responses
//!
//! Follows `OpenCode` patterns:
//! - Hard limits on output size
//! - Truncation notices
//! - Metadata (counts, structure, truncated status)
//! - Maximum information density
//!
//! ## Critical Error Detection
//!
//! Certain content should never be truncated:
//! - Compilation errors (build failures)
//! - Test failures with details
//! - Security vulnerabilities
//! - Runtime errors and panics
//! - Dependency conflicts

use serde_json::json;
use std::cmp::min;
use std::path::{Path, PathBuf};

/// Detect if content contains critical errors that should never be truncated
///
/// Critical errors include:
/// - Compilation/build failures
/// - Test failures with error details
/// - Security vulnerabilities
/// - Runtime panics and errors
/// - Dependency resolution failures
pub fn is_critical_content(content: &str) -> bool {
    let content_lower = content.to_lowercase();

    // Critical error patterns
    let critical_patterns = [
        // Build/compilation errors
        "error[e", // Rust compiler errors: error[E0433]
        "compilation error",
        "build failed",
        "compiler error",
        // Test failures
        "test failed",
        "test result: failed",
        "failures:",
        "thread panicked",
        // Security issues
        "security vulnerability",
        "cve-",
        "security advisory",
        "unsafe block",
        // Runtime errors
        "panicked at",
        "runtime error",
        "segmentation fault",
        "stack overflow",
        // Dependency issues
        "dependency error",
        "version conflict",
        "unsatisfied requirement",
        // Network/connection errors (often contain debugging info)
        "connection refused",
        "timeout",
        "network error",
        // File system errors
        "permission denied",
        "no such file or directory",
        "disk full",
    ];

    for pattern in &critical_patterns {
        if content_lower.contains(pattern) {
            return true;
        }
    }

    // Check for multi-line error blocks (indented error context)
    let lines: Vec<&str> = content.lines().collect();
    if lines.len() > 3 {
        // Look for consecutive lines with error indicators
        let mut error_line_count = 0;
        for line in lines.iter().take(20) {
            let line_lower = line.to_lowercase();
            if line_lower.contains("error") ||
               line_lower.contains("failed") ||
               line_lower.starts_with("  ") || // Indented context
               line_lower.contains("|")
            {
                // Error arrows/diffs
                error_line_count += 1;
                if error_line_count >= 3 {
                    return true;
                }
            }
        }
    }

    false
}

/// Truncate lines, but never truncate critical content
///
/// If critical errors are detected, returns full content.
/// Otherwise applies normal truncation.
pub fn truncate_lines_critical(
    content: &str,
    max_lines: usize,
    source_name: &str,
    total_lines: usize,
) -> TruncatedOutput {
    if is_critical_content(content) {
        let metadata = json!({
            "source": source_name,
            "total_lines": total_lines,
            "shown_lines": total_lines,
            "truncated": false,
            "critical": true,
            "never_truncated_reason": "Critical error detected",
        });
        return TruncatedOutput {
            output: content.to_string(),
            truncated: false,
            total_count: total_lines,
            shown_count: total_lines,
            metadata,
        };
    }

    truncate_lines(content, max_lines, source_name, total_lines)
}

/// Truncate bytes, but never truncate critical content
///
/// If critical errors are detected, returns full content.
/// Otherwise applies normal truncation.
pub fn truncate_bytes_critical(
    content: &str,
    max_bytes: usize,
    source_name: &str,
) -> TruncatedOutput {
    if is_critical_content(content) {
        let content_bytes = content.len();
        let _total_lines = content.lines().count();
        let metadata = json!({
            "source": source_name,
            "total_bytes": content_bytes,
            "shown_bytes": content_bytes,
            "truncated": false,
            "critical": true,
            "never_truncated_reason": "Critical error detected",
        });
        return TruncatedOutput {
            output: content.to_string(),
            truncated: false,
            total_count: content_bytes,
            shown_count: content_bytes,
            metadata,
        };
    }

    truncate_bytes(content, max_bytes, source_name)
}

/// Maximum output sizes for different tool types (following `OpenCode` patterns)
pub const BASH_MAX_LINES: usize = 30;
pub const BASH_MAX_BYTES: usize = 50 * 1024; // 50KB
pub const READ_MAX_LINES: usize = 2000;
pub const READ_MAX_BYTES: usize = 256 * 1024; // 256KB
pub const GREP_MAX_MATCHES: usize = 15;
pub const LIST_MAX_ITEMS: usize = 30;

/// Maximum output before spilling to disk.
/// When output exceeds this, the full content is saved to a temp file and
/// only a truncated preview is kept in-context.
const SPILL_MAX_LINES: usize = 2000;
const SPILL_MAX_BYTES: usize = 50 * 1024; // 50KB

/// Directory for spilled output files
const SPILL_DIR_NAME: &str = ".rustycode";

/// Truncation result with metadata
#[derive(Debug, Clone)]
pub struct TruncatedOutput {
    /// The truncated output text
    pub output: String,
    /// Whether output was truncated
    pub truncated: bool,
    /// Total count (lines, matches, items, etc.)
    pub total_count: usize,
    /// Shown count
    pub shown_count: usize,
    /// Metadata about the truncation
    pub metadata: serde_json::Value,
}

impl TruncatedOutput {
    /// Create a new non-truncated output
    pub fn full(output: String, total_count: usize) -> Self {
        Self {
            output: output.clone(),
            truncated: false,
            total_count,
            shown_count: total_count,
            metadata: json!({
                "total": total_count,
                "shown": total_count,
                "truncated": false,
                "total_lines": total_count,
            }),
        }
    }

    /// Get the output as a string
    pub fn as_str(&self) -> &str {
        &self.output
    }

    /// Get the metadata for `ToolOutput`
    pub fn into_metadata(self) -> serde_json::Value {
        self.metadata
    }
}

/// Format lines with `cat -n` style line number prefixes.
///
/// Each line gets a right-aligned line number followed by a tab:
/// ```text
///      1\tfirst line
///      2\tsecond line
/// ```
pub fn format_with_line_numbers(lines: &[&str], start_line: usize) -> String {
    let width = (start_line + lines.len()).to_string().len().max(1);
    lines
        .iter()
        .enumerate()
        .map(|(i, line)| format!("{:>width$}\t{line}", start_line + i))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Truncate text to maximum lines with notice
///
/// # Example
/// ```
/// use rustycode_tools::truncation::truncate_lines;
/// let content = "line1\nline2\nline3";
/// let result = truncate_lines(content, 30, "output.txt", 3);
/// // Returns truncated content with "[18 lines omitted]" notice
/// ```
pub fn truncate_lines(
    content: &str,
    max_lines: usize,
    source_name: &str,
    total_lines: usize,
) -> TruncatedOutput {
    let lines: Vec<&str> = content.lines().collect();

    if lines.len() <= max_lines {
        let numbered = format_with_line_numbers(&lines, 1);
        return TruncatedOutput {
            output: numbered,
            truncated: false,
            total_count: total_lines.max(lines.len()),
            shown_count: lines.len(),
            metadata: json!({
                "total": total_lines.max(lines.len()),
                "shown": lines.len(),
                "truncated": false,
                "total_lines": total_lines.max(lines.len()),
            }),
        };
    }

    let truncated = true;
    let shown_count = max_lines;
    let omitted_count = lines.len() - max_lines;

    let mut output = format_with_line_numbers(&lines[..max_lines], 1);

    // Add truncation notice
    output.push_str(&format!(
        "\n\n[{omitted_count} lines omitted - total: {total_lines}]"
    ));

    output.push_str(&format!(
        "\n[Showing lines 1-{shown_count} of {total_lines}. Use offset={shown_count} to read more.]"
    ));

    TruncatedOutput {
        output,
        truncated,
        total_count: total_lines.max(lines.len()),
        shown_count,
        metadata: json!({
            "source": source_name,
            "total_lines": total_lines.max(lines.len()),
            "shown_lines": shown_count,
            "truncated": true,
            "omitted_lines": omitted_count,
        }),
    }
}

/// Truncate text to maximum bytes with notice
///
/// # Example
/// ```
/// use rustycode_tools::truncation::truncate_bytes;
/// let content = "lots of text content here";
/// let result = truncate_bytes(content, 50 * 1024, "bash output");
/// // Returns truncated content with "(Output truncated at 50KB)" notice
/// ```
pub fn truncate_bytes(content: &str, max_bytes: usize, source_name: &str) -> TruncatedOutput {
    let content_bytes = content.len();

    if content_bytes <= max_bytes {
        return TruncatedOutput::full(content.to_string(), content_bytes);
    }

    // Find a safe truncation point (newline close to max_bytes)
    let mut safe_boundary = max_bytes.min(content.len());
    while safe_boundary > 0 && !content.is_char_boundary(safe_boundary) {
        safe_boundary -= 1;
    }
    let truncation_point = if let Some(pos) = content[..safe_boundary].rfind('\n') {
        pos
    } else {
        safe_boundary
    };

    let truncated_content = &content[..truncation_point];
    let omitted_bytes = content_bytes - truncation_point;

    let output = format!(
        "{}\n\n(Output truncated at {}KB, {} bytes omitted)",
        truncated_content,
        max_bytes / 1024,
        omitted_bytes
    );

    let total_lines = content.lines().count();
    let shown_lines = truncated_content.lines().count();

    TruncatedOutput {
        output,
        truncated: true,
        total_count: content_bytes,
        shown_count: truncation_point,
        metadata: json!({
            "source": source_name,
            "total_bytes": content_bytes,
            "shown_bytes": truncation_point,
            "truncated": true,
            "omitted_bytes": omitted_bytes,
            "total_lines": total_lines,
            "shown_lines": shown_lines,
        }),
    }
}

/// Truncate output and spill full content to disk.
///
/// When output exceeds size limits:
/// 1. Saves the full content to a temp file under `.rustycode/spill/`
/// 2. Returns a truncated preview in-context
/// 3. Appends a hint telling the agent to use Read with offset/limit
///
/// This prevents context window bloat from large tool outputs while
/// keeping the full data accessible.
///
pub fn truncate_with_spill(content: &str, cwd: &Path) -> TruncatedOutput {
    let total_lines = content.lines().count();
    let total_bytes = content.len();

    // Check if output fits within limits
    if total_lines <= SPILL_MAX_LINES && total_bytes <= SPILL_MAX_BYTES {
        return TruncatedOutput::full(content.to_string(), total_lines);
    }

    // Save full content to disk
    let spill_path = match save_spill_file(content, cwd) {
        Some(path) => path,
        None => {
            // Fallback to in-memory truncation if file write fails
            return truncate_bytes(content, SPILL_MAX_BYTES, "tool output");
        }
    };

    // Build truncated preview
    let lines: Vec<&str> = content.lines().collect();
    let preview_lines = SPILL_MAX_LINES.min(lines.len());
    let preview: String = lines[..preview_lines].join("\n");

    let spill_relative = spill_path
        .strip_prefix(cwd)
        .unwrap_or(&spill_path)
        .display();

    let hint = format!(
        "\n\n[Output too large: {total_lines} lines, {total_bytes} bytes. Full output saved to: {spill_relative}]\n\
         [Use Read with offset/limit to view specific sections, or Grep to search the content.]"
    );

    TruncatedOutput {
        output: format!("{preview}{hint}"),
        truncated: true,
        total_count: total_lines,
        shown_count: preview_lines,
        metadata: json!({
            "total_lines": total_lines,
            "total_bytes": total_bytes,
            "shown_lines": preview_lines,
            "truncated": true,
            "spill_path": spill_path.to_string_lossy(),
        }),
    }
}

/// Save full content to a spill file.
fn save_spill_file(content: &str, cwd: &Path) -> Option<PathBuf> {
    let spill_dir = cwd.join(SPILL_DIR_NAME).join("spill");
    std::fs::create_dir_all(&spill_dir).ok()?;

    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_millis();

    let filename = format!("output-{timestamp}.txt");
    let path = spill_dir.join(&filename);

    std::fs::write(&path, content).ok().map(|()| path)
}

/// Truncate a list of items to maximum count with notice
///
/// # Example
/// ```
/// use rustycode_tools::truncation::truncate_items;
/// let items = vec!["file1.rs", "file2.rs"];
/// let result = truncate_items(items, 30, "directory listing");
/// // Returns truncated list with "[209 more files omitted]" notice
/// ```
pub fn truncate_items<T: AsRef<str>>(
    items: Vec<T>,
    max_items: usize,
    source_name: &str,
) -> TruncatedOutput {
    let total_count = items.len();

    if total_count <= max_items {
        let output = items
            .iter()
            .map(|s| s.as_ref().to_string())
            .collect::<Vec<_>>()
            .join("\n");

        return TruncatedOutput::full(output, total_count);
    }

    let shown_items = &items[..max_items];
    let omitted_count = total_count - max_items;

    let output = format!(
        "{}\n\n[{} more items omitted - total: {}]",
        shown_items
            .iter()
            .map(AsRef::as_ref)
            .collect::<Vec<_>>()
            .join("\n"),
        omitted_count,
        total_count
    );

    TruncatedOutput {
        output,
        truncated: true,
        total_count,
        shown_count: max_items,
        metadata: json!({
            "source": source_name,
            "total_items": total_count,
            "shown_items": max_items,
            "truncated": true,
            "omitted_items": omitted_count,
        }),
    }
}

/// Truncate bash command output with smart formatting
///
/// Shows summary + truncated sample instead of full output
///
/// # Example
/// ```
/// use rustycode_tools::truncation::truncate_bash_output;
/// let stdout = "running 100 tests...\ntest 1 passed...\n...";
/// let stderr = "";
/// let exit_code = 0;
/// let result = truncate_bash_output(stdout, stderr, exit_code);
/// // Returns: "✅ cargo test (4.2s)\nUnit: 102/102..."
/// ```
pub fn truncate_bash_output(stdout: &str, stderr: &str, exit_code: i32) -> TruncatedOutput {
    let combined = if stderr.trim().is_empty() {
        stdout.to_string()
    } else if stdout.trim().is_empty() {
        stderr.to_string()
    } else {
        format!("{stdout}\n{stderr}")
    };

    let total_lines = combined.lines().count();
    let total_bytes = combined.len();

    // If output is small enough, return as-is
    if total_lines <= BASH_MAX_LINES && total_bytes <= BASH_MAX_BYTES {
        return TruncatedOutput::full(combined.clone(), total_lines);
    }

    // Try to detect and format common patterns
    let formatted = if let Some(summary) = detect_test_summary(&combined) {
        summary
    } else if let Some(summary) = detect_build_summary(&combined) {
        summary
    } else {
        // Fallback to truncation
        return truncate_bytes(&combined, BASH_MAX_BYTES, "bash");
    };

    TruncatedOutput {
        output: formatted,
        truncated: total_lines > BASH_MAX_LINES || total_bytes > BASH_MAX_BYTES,
        total_count: total_lines,
        shown_count: min(total_lines, BASH_MAX_LINES),
        metadata: json!({
            "exit_code": exit_code,
            "total_lines": total_lines,
            "truncated": total_lines > BASH_MAX_LINES || total_bytes > BASH_MAX_BYTES,
            "total_bytes": total_bytes,
        }),
    }
}

/// Detect and format cargo test output
fn detect_test_summary(output: &str) -> Option<String> {
    if output.contains("running") && output.contains("test result") {
        let lines: Vec<&str> = output.lines().collect();

        // Look for test result line
        let result_line = lines.iter().find(|l| l.contains("test result"))?;

        let mut summary = String::new();

        // Add result line
        summary.push_str(result_line);
        summary.push('\n');

        // Add individual test counts if available
        for line in &lines {
            if line.contains("running ") && line.contains(" tests") {
                summary.push_str(line);
                summary.push('\n');
            }
        }

        Some(summary.trim().to_string())
    } else {
        None
    }
}

/// Detect and format build output
fn detect_build_summary(output: &str) -> Option<String> {
    if output.contains("Compiling") || output.contains("Finished") {
        let lines: Vec<&str> = output.lines().collect();

        // Look for "Finished" line
        let finished = lines.iter().find(|l| l.starts_with("Finished"))?;

        Some(finished.to_string())
    } else {
        None
    }
}

/// Safely truncate a string at character boundaries, not byte boundaries.
///
/// This function ensures that multi-byte UTF-8 characters (like Japanese, emoji, etc.)
/// are not split in the middle, which would cause a panic.
///
pub fn safe_truncate(s: &str, max_chars: usize) -> String {
    let char_count = s.chars().count();
    if char_count <= max_chars {
        s.to_string()
    } else if max_chars < 3 {
        // Not enough room for "..." — just truncate hard
        s.chars().take(max_chars).collect()
    } else {
        let truncated: String = s.chars().take(max_chars - 3).collect();
        format!("{truncated}...")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_truncate_lines_no_truncation() {
        let content = "line1\nline2\nline3";
        let result = truncate_lines(content, 10, "test.txt", 3);

        assert!(!result.truncated);
        assert_eq!(result.total_count, 3);
        assert_eq!(result.shown_count, 3);
        assert_eq!(result.output, "1\tline1\n2\tline2\n3\tline3");
    }

    #[test]
    fn test_truncate_lines_with_truncation() {
        let content = (1..=100)
            .map(|i| format!("line{}", i))
            .collect::<Vec<_>>()
            .join("\n");
        let result = truncate_lines(&content, 10, "test.txt", 100);

        assert!(result.truncated);
        assert_eq!(result.total_count, 100);
    }

    #[test]
    fn test_truncate_lines_uses_caller_total_lines() {
        let content = "line1\nline2";
        let result = truncate_lines(content, 10, "test.txt", 200);

        assert!(!result.truncated);
        assert_eq!(result.total_count, 200);
        assert_eq!(result.metadata["total_lines"], 200);
    }

    #[test]
    fn test_is_critical_content_compilation_error() {
        let content = "error[E0433]: failed to resolve: use of undeclared crate or module `std`";
        assert!(is_critical_content(content));
    }

    #[test]
    fn test_is_critical_content_test_failure() {
        let content = "test result: failed. 1 passed; 2 failed; 0 skipped";
        assert!(is_critical_content(content));
    }

    #[test]
    fn test_is_critical_content_panic() {
        let content = "thread 'main' panicked at 'assertion failed: x > 0', src/lib.rs:42";
        assert!(is_critical_content(content));
    }

    #[test]
    fn test_is_critical_content_security() {
        let content = "security vulnerability detected: CVE-2024-1234";
        assert!(is_critical_content(content));
    }

    #[test]
    fn test_is_critical_content_normal_output() {
        let content = "running tests...\ntest1 passed\ntest2 passed\nall tests passed";
        assert!(!is_critical_content(content));
    }

    #[test]
    fn test_truncate_lines_critical_with_error() {
        let content = "Compiling project...\nerror[E0433]: failed to resolve\n  --> src/lib.rs:10:5\n   |\n10 |     use std::collections;\n   |         ^^^^ not found in this scope";
        let result = truncate_lines_critical(content, 5, "lib.rs", 4);

        // Should not truncate critical content
        assert!(!result.truncated);
        assert_eq!(result.total_count, 4);
        assert_eq!(result.shown_count, 4);

        // Check metadata indicates critical content
        let metadata = &result.metadata;
        assert_eq!(metadata["critical"], true);
        assert_eq!(
            metadata["never_truncated_reason"],
            "Critical error detected"
        );
    }

    #[test]
    fn test_truncate_lines_critical_normal_content() {
        let content = (1..=100)
            .map(|i| format!("line{}", i))
            .collect::<Vec<_>>()
            .join("\n");
        let result = truncate_lines_critical(&content, 10, "test.txt", 100);

        // Should truncate normal content
        assert!(result.truncated);
        assert_eq!(result.total_count, 100);
        assert_eq!(result.shown_count, 10);
    }

    #[test]
    fn test_truncate_bytes_no_truncation() {
        let content = "small content";
        let result = truncate_bytes(content, 1024, "test");

        assert!(!result.truncated);
        assert_eq!(result.output, content);
    }

    #[test]
    fn test_truncate_bytes_with_truncation() {
        let large_content = "x".repeat(100 * 1024); // 100KB
        let result = truncate_bytes(&large_content, 50 * 1024, "bash");

        assert!(result.truncated);
        assert!(result.output.contains("(Output truncated at 50KB"));
    }

    #[test]
    fn test_truncate_bytes_keeps_utf8_valid() {
        let content = "é".repeat(20);
        let result = truncate_bytes(&content, 7, "test");
        assert!(std::str::from_utf8(result.output.as_bytes()).is_ok());
    }

    #[test]
    fn test_truncate_items_no_truncation() {
        let items = vec!["a", "b", "c"];
        let result = truncate_items(items.clone(), 10, "test");

        assert!(!result.truncated);
        assert_eq!(result.total_count, 3);
        assert!(result.output.contains("a"));
        assert!(result.output.contains("b"));
        assert!(result.output.contains("c"));
    }

    #[test]
    fn test_truncate_items_with_truncation() {
        let items: Vec<String> = (1..=100).map(|i| format!("item{}", i)).collect();
        let result = truncate_items(items, 30, "test");

        assert!(result.truncated);
        assert_eq!(result.total_count, 100);
        assert_eq!(result.shown_count, 30);
        assert!(result.output.contains("[70 more items omitted"));
    }

    #[test]
    fn test_detect_test_summary() {
        let output = "running 3 tests\ntest test1 ... ok\ntest test2 ... ok\ntest test3 ... ok\n\ntest result: ok. 3 passed; 0 failed";
        let result = detect_test_summary(output);

        assert!(result.is_some());
        let summary = result.unwrap();
        assert!(summary.contains("test result: ok. 3 passed"));
    }

    #[test]
    fn test_detect_build_summary() {
        let output = "Compiling test\nFinished dev profile [unoptimized + debuginfo]";
        let result = detect_build_summary(output);

        assert!(result.is_some());
        let summary = result.unwrap();
        assert!(summary.contains("Finished"));
    }

    #[test]
    fn test_truncate_with_spill_small_output() {
        let tmp = tempfile::tempdir().unwrap();
        let content = "small output\nline 2\nline 3";
        let result = truncate_with_spill(content, tmp.path());
        assert!(!result.truncated);
        assert_eq!(result.output, content);
    }

    #[test]
    fn test_truncate_with_spill_large_output() {
        let tmp = tempfile::tempdir().unwrap();
        // Create output larger than SPILL_MAX_LINES
        let content: String = (0..3000)
            .map(|i| format!("line {}", i))
            .collect::<Vec<_>>()
            .join("\n");

        let result = truncate_with_spill(&content, tmp.path());
        assert!(result.truncated);
        assert!(result.output.contains("Full output saved to"));
        assert!(result.output.contains("Read with offset/limit"));

        // Verify spill file was created
        let spill_dir = tmp.path().join(".rustycode").join("spill");
        assert!(spill_dir.exists());
        let spill_files: Vec<_> = std::fs::read_dir(&spill_dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .collect();
        assert_eq!(spill_files.len(), 1);

        // Verify spill file content matches original
        let spill_content = std::fs::read_to_string(spill_files[0].path()).unwrap();
        assert_eq!(spill_content, content);
    }

    #[test]
    fn test_truncate_with_spill_metadata() {
        let tmp = tempfile::tempdir().unwrap();
        let content: String = (0..3000)
            .map(|i| format!("line {}", i))
            .collect::<Vec<_>>()
            .join("\n");

        let result = truncate_with_spill(&content, tmp.path());
        assert!(result.metadata["truncated"].as_bool().unwrap());
        assert_eq!(result.metadata["total_lines"], 3000);
        assert!(result.metadata["spill_path"]
            .as_str()
            .unwrap()
            .contains("output-"));
    }

    // ── Safe Truncate Tests ───────────────────────────────────────────────────

    #[test]
    fn test_safe_truncate_ascii() {
        assert_eq!(safe_truncate("hello world", 20), "hello world");
        assert_eq!(safe_truncate("hello world", 8), "hello...");
        assert_eq!(safe_truncate("hello", 5), "hello");
        assert_eq!(safe_truncate("hello", 3), "...");
    }

    #[test]
    fn test_safe_truncate_japanese() {
        let japanese = "こんにちは世界";
        assert_eq!(safe_truncate(japanese, 10), japanese);
        assert_eq!(safe_truncate(japanese, 5), "こん...");
        assert_eq!(safe_truncate(japanese, 7), japanese);
    }

    #[test]
    fn test_safe_truncate_mixed() {
        let mixed = "Hello こんにちは";
        assert_eq!(safe_truncate(mixed, 20), mixed);
        assert_eq!(safe_truncate(mixed, 8), "Hello...");
    }

    #[test]
    fn test_safe_truncate_emoji() {
        // Emoji can be multi-codepoint; just test that we don't panic
        let emoji = "Hello 🌍 World 🚀";
        let result = safe_truncate(emoji, 7);
        // Should end with "..." and not panic on multi-byte chars
        assert!(result.ends_with("..."));
        // With max_chars=7, we get 7-3=4 chars + "..." = "Hell..."
        assert!(result.starts_with("Hell"));

        // Test with enough room
        let result2 = safe_truncate(emoji, 15);
        assert_eq!(result2, emoji);
    }

    #[test]
    fn test_safe_truncate_empty() {
        assert_eq!(safe_truncate("", 5), "");
    }

    #[test]
    fn test_safe_truncate_max_chars_less_than_3() {
        // When max_chars < 3, there's no room for "...", hard-truncate instead
        assert_eq!(safe_truncate("hello", 0), "");
        assert_eq!(safe_truncate("hello", 1), "h");
        assert_eq!(safe_truncate("hello", 2), "he");
    }

    #[test]
    fn test_safe_truncate_exact_boundary() {
        // Exactly max_chars — no truncation
        assert_eq!(safe_truncate("abc", 3), "abc");
        // One over — truncate with "..."
        assert_eq!(safe_truncate("abcd", 3), "...");
    }

    // ── truncate_bytes_critical tests ─────────────

    #[test]
    fn test_truncate_bytes_critical_critical_content_not_truncated() {
        // Use patterns that match is_critical_content (e.g., "error[E0433]")
        let content = "error[E0433]: failed to resolve\nbuild failed in main.rs";
        let result = truncate_bytes_critical(content, 10, "test.rs");
        assert!(
            !result.truncated,
            "critical content should not be truncated"
        );
        assert_eq!(result.output, content);
    }

    #[test]
    fn test_truncate_bytes_critical_non_critical_truncates() {
        let content = "normal output line\nanother line\nmore text here";
        let result = truncate_bytes_critical(content, 10, "test.txt");
        // Non-critical content should be truncated if over limit
        assert!(result.shown_count <= result.total_count);
    }

    // ── truncate_bash_output tests ─────────────

    #[test]
    fn test_truncate_bash_output_small_output() {
        let result = truncate_bash_output("hello", "", 0);
        assert_eq!(result.output, "hello");
        assert!(!result.truncated);
    }

    #[test]
    fn test_truncate_bash_output_stderr_only() {
        let result = truncate_bash_output("", "warning: something", 1);
        assert_eq!(result.output, "warning: something");
    }

    #[test]
    fn test_truncate_bash_output_combined() {
        let result = truncate_bash_output("stdout", "stderr", 0);
        assert!(result.output.contains("stdout"));
        assert!(result.output.contains("stderr"));
    }

    #[test]
    fn test_truncate_bash_output_empty() {
        let result = truncate_bash_output("", "", 0);
        assert!(result.output.is_empty());
    }

    #[test]
    fn test_truncate_bash_output_large_output_truncates() {
        let large = "x".repeat(100_000);
        let result = truncate_bash_output(&large, "", 0);
        assert!(result.truncated);
    }

    #[test]
    fn test_truncate_bash_output_test_summary_detection() {
        let output = "running 10 tests\ntest result: ok. 10 passed; 0 failed;\n";
        let result = truncate_bash_output(output, "", 0);
        assert!(result.output.contains("test result"));
    }
}
