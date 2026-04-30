//! Line ending detection, normalization, and diff utilities.
//!
//! Provides cross-platform line ending handling for file operations.
//! Inspired by Kilocode/OpenCode/Gemini CLI patterns: detect the original
//! line ending style, normalize to LF for internal processing, and restore
//! the original style when writing back.

use similar::{ChangeTag, TextDiff};

/// The line ending style detected in a file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineEnding {
    /// Unix-style: `\n` (LF)
    LF,
    /// Windows-style: `\r\n` (CRLF)
    CRLF,
}

impl LineEnding {
    /// Get the actual string representation of this line ending.
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::LF => "\n",
            Self::CRLF => "\r\n",
        }
    }
}

/// Detect the dominant line ending in text content.
///
/// Checks for CRLF first (since CRLF contains LF, we check CRLF first
/// to avoid false positives). Returns CRLF if any `\r\n` is found,
/// otherwise LF.
pub fn detect_line_ending(content: &str) -> LineEnding {
    if content.contains("\r\n") {
        LineEnding::CRLF
    } else {
        LineEnding::LF
    }
}

/// Normalize line endings to LF for internal processing.
///
/// Converts all CRLF (`\r\n`) sequences to plain LF (`\n`).
/// Also converts bare `\r` (classic Mac OS) to `\n`.
/// This ensures consistent comparison and diff operations regardless
/// of the platform that created the file.
pub fn normalize_to_lf(content: &str) -> String {
    let content = content.replace("\r\n", "\n");
    content.replace('\r', "\n")
}

/// Normalize curly/smart quotes to straight quotes.
///
/// LLMs (especially via copy-paste) sometimes generate curly quotes
/// instead of straight ones, which causes edit mismatches. This function
/// converts all four curly quote variants to their straight equivalents.
pub fn normalize_quotes(content: &str) -> String {
    content
        .replace(['\u{2018}', '\u{2019}'], "'")
        .replace(['\u{201C}', '\u{201D}'], "\"")
}

/// Apply a specific line ending style to content.
///
/// Converts all LF (`\n`) to the specified line ending.
/// Content MUST already be normalized (only LF, no CRLF) before calling this.
/// A debug assertion catches accidental double-conversion.
/// This is used to restore the original line ending style after edits.
pub fn apply_line_ending(content: &str, ending: LineEnding) -> String {
    debug_assert!(
        !content.contains("\r\n"),
        "apply_line_ending called with CRLF content — normalize first"
    );
    match ending {
        LineEnding::LF => content.to_string(),
        LineEnding::CRLF => content.replace('\n', "\r\n"),
    }
}

/// Detect line ending, normalize content, and return both.
///
/// Convenience function that combines detection and normalization.
/// Returns the normalized content and the detected original line ending.
pub fn normalize_and_detect(content: &str) -> (String, LineEnding) {
    let ending = detect_line_ending(content);
    let normalized = normalize_to_lf(content);
    (normalized, ending)
}

/// Generate a compact unified diff between old and new content.
///
/// Returns a human-readable diff string suitable for LLM consumption.
/// Shows added/removed lines with +/- prefixes and context lines.
/// Limits output to `max_lines` to avoid flooding the LLM context.
pub fn generate_diff(old: &str, new: &str, file_path: &str, max_lines: usize) -> String {
    let diff = TextDiff::from_lines(old, new);

    let mut result = Vec::new();
    let mut additions = 0usize;
    let mut deletions = 0usize;

    for change in diff.iter_all_changes() {
        let sign = match change.tag() {
            ChangeTag::Delete => {
                deletions += 1;
                "-"
            }
            ChangeTag::Insert => {
                additions += 1;
                "+"
            }
            ChangeTag::Equal => " ",
        };
        let line = change.to_string_lossy();
        let trimmed = line.trim_end_matches('\n').trim_end_matches('\r');
        result.push(format!("{sign}{trimmed}"));
    }

    if additions == 0 && deletions == 0 {
        return format!("No changes in {file_path}");
    }

    let total = result.len();
    if total > max_lines {
        let kept: Vec<_> = result
            .iter()
            .filter(|l| l.starts_with('+') || l.starts_with('-'))
            .take(max_lines)
            .cloned()
            .collect();
        format!(
            "Changes in {} (+{} -{}):\n{}\n(showing {}/{} diff lines)",
            file_path,
            additions,
            deletions,
            kept.join("\n"),
            kept.len().min(max_lines),
            total
        )
    } else {
        format!(
            "Changes in {} (+{} -{}):\n{}",
            file_path,
            additions,
            deletions,
            result.join("\n")
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_lf() {
        assert_eq!(detect_line_ending("hello\nworld"), LineEnding::LF);
    }

    #[test]
    fn test_detect_crlf() {
        assert_eq!(detect_line_ending("hello\r\nworld"), LineEnding::CRLF);
    }

    #[test]
    fn test_detect_mixed_defaults_to_crlf() {
        assert_eq!(detect_line_ending("hello\r\nworld\n"), LineEnding::CRLF);
    }

    #[test]
    fn test_detect_empty() {
        assert_eq!(detect_line_ending(""), LineEnding::LF);
    }

    #[test]
    fn test_normalize_to_lf() {
        assert_eq!(normalize_to_lf("hello\r\nworld"), "hello\nworld");
    }

    #[test]
    fn test_normalize_already_lf() {
        assert_eq!(normalize_to_lf("hello\nworld"), "hello\nworld");
    }

    #[test]
    fn test_normalize_mixed() {
        assert_eq!(
            normalize_to_lf("line1\r\nline2\nline3\r\n"),
            "line1\nline2\nline3\n"
        );
    }

    #[test]
    fn test_apply_lf() {
        assert_eq!(
            apply_line_ending("hello\nworld", LineEnding::LF),
            "hello\nworld"
        );
    }

    #[test]
    fn test_apply_crlf() {
        assert_eq!(
            apply_line_ending("hello\nworld", LineEnding::CRLF),
            "hello\r\nworld"
        );
    }

    #[test]
    fn test_roundtrip_crlf() {
        let original = "line1\r\nline2\r\nline3\r\n";
        let (normalized, ending) = normalize_and_detect(original);
        assert_eq!(ending, LineEnding::CRLF);
        assert_eq!(normalized, "line1\nline2\nline3\n");
        let restored = apply_line_ending(&normalized, ending);
        assert_eq!(restored, original);
    }

    #[test]
    fn test_roundtrip_lf() {
        let original = "line1\nline2\nline3\n";
        let (normalized, ending) = normalize_and_detect(original);
        assert_eq!(ending, LineEnding::LF);
        assert_eq!(normalized, original);
    }

    #[test]
    fn test_line_ending_as_str() {
        assert_eq!(LineEnding::LF.as_str(), "\n");
        assert_eq!(LineEnding::CRLF.as_str(), "\r\n");
    }

    #[test]
    fn test_generate_diff_simple() {
        let old = "hello\nworld\n";
        let new = "hello\nrust\n";
        let diff = generate_diff(old, new, "test.txt", 100);
        assert!(diff.contains("Changes in test.txt"));
        assert!(diff.contains("+1 -1"));
        assert!(diff.contains("-world"));
        assert!(diff.contains("+rust"));
    }

    #[test]
    fn test_generate_diff_no_changes() {
        let content = "same\ncontent\n";
        let diff = generate_diff(content, content, "same.txt", 100);
        assert!(diff.contains("No changes"));
    }

    #[test]
    fn test_generate_diff_new_file() {
        let diff = generate_diff("", "new content\n", "new.txt", 100);
        assert!(diff.contains("+new content"));
    }

    #[test]
    fn test_normalize_bare_cr() {
        // Classic Mac OS line endings: bare \r without \n
        assert_eq!(normalize_to_lf("hello\rworld"), "hello\nworld");
        assert_eq!(normalize_to_lf("a\rb\r\nc"), "a\nb\nc");
    }

    #[test]
    fn test_detect_bare_cr_treated_as_lf() {
        // Bare \r (no \r\n) should detect as LF since we don't have a CR-only variant
        assert_eq!(detect_line_ending("hello\rworld"), LineEnding::LF);
    }

    #[test]
    fn test_generate_diff_truncation_keeps_changes() {
        let old = (1..=100)
            .map(|i| format!("line {i}"))
            .collect::<Vec<_>>()
            .join("\n");
        // Change only 2 lines so all changes fit in limit of 5
        let new_lines: Vec<String> = (1..=100)
            .map(|i| {
                if i == 50 || i == 51 {
                    format!("CHANGED {i}")
                } else {
                    format!("line {i}")
                }
            })
            .collect();
        let new_content = new_lines.join("\n");
        let diff = generate_diff(&old, &new_content, "big.txt", 5);
        assert!(
            diff.contains("CHANGED"),
            "truncated diff should still contain changes: {diff}"
        );
    }

    #[test]
    fn test_apply_line_endings_on_normalized_content() {
        let normalized = "line1\nline2\nline3\n";
        let crlf = apply_line_ending(normalized, LineEnding::CRLF);
        assert_eq!(crlf, "line1\r\nline2\r\nline3\r\n");
        let back = normalize_to_lf(&crlf);
        assert_eq!(back, normalized);
    }

    #[test]
    fn test_roundtrip_bare_cr() {
        // Bare \r gets normalized to \n, then roundtrips as LF
        let original = "line1\rline2\rline3\r";
        let (normalized, ending) = normalize_and_detect(original);
        assert_eq!(ending, LineEnding::LF);
        assert_eq!(normalized, "line1\nline2\nline3\n");
    }

    #[test]
    fn test_normalize_curly_double_quotes() {
        assert_eq!(
            normalize_quotes("\u{201C}hello world\u{201D}"),
            "\"hello world\""
        );
    }

    #[test]
    fn test_normalize_curly_single_quotes() {
        assert_eq!(normalize_quotes("\u{2018}it\u{2019}s"), "'it's");
    }

    #[test]
    fn test_normalize_mixed_quotes() {
        assert_eq!(
            normalize_quotes("\u{201C}she said \u{2018}hi\u{2019}\u{201D}"),
            "\"she said 'hi'\""
        );
    }

    #[test]
    fn test_normalize_no_curly_quotes() {
        assert_eq!(normalize_quotes("'hello' \"world\""), "'hello' \"world\"");
    }
}
