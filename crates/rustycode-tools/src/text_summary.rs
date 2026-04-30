//! Text Summarization Utilities
//!
//! Provides functions for extracting short titles from text responses,
//! stripping XML tags, and generating human-readable summaries.
//!
//! Inspired by goose's `extract_short_title` and `strip_xml_tags` in
//! `providers/base.rs`.
//!
//! # Example
//!
//! ```
//! use rustycode_tools::text_summary::extract_short_title;
//!
//! let title = extract_short_title("I'll help you implement the authentication system");
//! assert!(!title.is_empty());
//!
//! let title = extract_short_title("short");
//! assert_eq!(title, "short");
//! ```

use regex::Regex;

/// Minimum word count for truncation to apply
const TITLE_WORD_THRESHOLD: usize = 8;

/// Valid word count range for quoted title extraction
const QUOTED_TITLE_WORD_RANGE: std::ops::RangeInclusive<usize> = 2..=8;

/// Extract a short, descriptive title from a longer text response.
///
/// Uses several heuristics in priority order:
/// 1. If the text is already short (≤8 words), return as-is
/// 2. Look for quoted phrases (2-8 words) in the text
/// 3. Fall back to the last non-empty line
/// 4. Fall back to the full text
///
/// Inspired by goose's `extract_short_title` for session name generation.
///
/// # Example
///
/// ```
/// use rustycode_tools::text_summary::extract_short_title;
///
/// // Short text returned as-is
/// assert_eq!(extract_short_title("Fix the bug"), "Fix the bug");
///
/// // Short text (≤8 words) returned as-is
/// let title = extract_short_title(r#"I'll implement the "user authentication" module"#);
/// assert_eq!(title, r#"I'll implement the "user authentication" module"#);
///
/// // Long text falls back to last non-empty line
/// let title = extract_short_title("Line one of many\nLine two of the text\nLine three here now\nFinal result to show");
/// assert_eq!(title, "Final result to show");
/// ```
pub fn extract_short_title(text: &str) -> String {
    let trimmed = text.trim();
    let word_count = trimmed.split_whitespace().count();

    // Short text — return as-is
    if word_count <= TITLE_WORD_THRESHOLD {
        return trimmed.to_string();
    }

    // Try to find a quoted phrase with 2-8 words
    if let Some(quoted) = extract_quoted_title(trimmed) {
        return quoted;
    }

    // Fall back to last non-empty line
    if let Some(last) = trimmed.lines().rev().find(|l| !l.trim().is_empty()) {
        let last_trimmed = last.trim();
        if !last_trimmed.is_empty() {
            return last_trimmed.to_string();
        }
    }

    trimmed.to_string()
}

/// Extract a meaningful quoted phrase from text.
///
/// Scans for single-quoted, double-quoted, or backtick-quoted phrases
/// that contain 2-8 words. Returns the last matching phrase.
fn extract_quoted_title(text: &str) -> Option<String> {
    let mut results = Vec::new();
    let mut quote_char: Option<char> = None;
    let mut current = String::new();
    let mut prev_char: Option<char> = None;

    for ch in text.chars() {
        match quote_char {
            None => {
                if matches!(ch, '"' | '\'' | '`') {
                    // Only start a quoted section if the quote isn't part of a word
                    // (e.g., don't match "it's" or "don't")
                    let after_alnum = prev_char.map(char::is_alphanumeric).unwrap_or(false);
                    if !after_alnum {
                        quote_char = Some(ch);
                        current.clear();
                    }
                }
            }
            Some(q) => {
                if ch == q {
                    let trimmed = current.trim().to_string();
                    let wc = trimmed.split_whitespace().count();
                    if QUOTED_TITLE_WORD_RANGE.contains(&wc) {
                        results.push(trimmed);
                    }
                    quote_char = None;
                    current.clear();
                } else {
                    current.push(ch);
                }
            }
        }
        prev_char = Some(ch);
    }

    results.last().cloned()
}

/// Strip all XML-style tags from text, keeping only the content.
///
/// Removes both matched tag pairs (with content between) and standalone tags.
/// This is useful for cleaning up structured responses before displaying them.
///
/// Inspired by goose's `strip_xml_tags` in `providers/base.rs`.
///
/// # Example
///
/// ```
/// use rustycode_tools::text_summary::strip_xml_tags;
///
/// let clean = strip_xml_tags("<thinking>My reasoning</thinking>Final answer");
/// assert_eq!(clean, "Final answer");
///
/// let clean = strip_xml_tags("<tag>content</tag>");
/// assert_eq!(clean, "");
/// ```
pub fn strip_xml_tags(text: &str) -> String {
    static BLOCK_RE: std::sync::LazyLock<Regex> = std::sync::LazyLock::new(|| {
        Regex::new(r"(?s)<([a-zA-Z][a-zA-Z0-9_]*)[^>]*>.*?</[a-zA-Z][a-zA-Z0-9_]*>").unwrap()
    });
    static TAG_RE: std::sync::LazyLock<Regex> =
        std::sync::LazyLock::new(|| Regex::new(r"</?[a-zA-Z][a-zA-Z0-9_]*[^>]*>").unwrap());

    // First pass: remove complete tag pairs with their content
    let pass1 = BLOCK_RE.replace_all(text, "");
    // Second pass: remove any remaining standalone tags
    TAG_RE.replace_all(&pass1, "").into_owned()
}

/// Strip a specific XML tag from text, keeping the inner content.
///
/// Unlike `strip_xml_tags` which removes everything, this preserves
/// the content between the opening and closing tags.
///
/// # Example
///
/// ```
/// use rustycode_tools::text_summary::strip_specific_tag;
///
/// let clean = strip_specific_tag("Hello <b>world</b> foo", "b");
/// assert_eq!(clean, "Hello world foo");
/// ```
pub fn strip_specific_tag(text: &str, tag: &str) -> String {
    let open_tag = format!("<{tag}>");
    let close_tag = format!("</{tag}>");
    text.replace(&open_tag, "").replace(&close_tag, "")
}

/// Generate a summary from text by taking the first N non-empty lines.
///
/// Useful for creating brief previews of longer responses.
///
/// # Example
///
/// ```
/// use rustycode_tools::text_summary::take_first_lines;
///
/// let text = "Line 1\nLine 2\nLine 3\nLine 4";
/// let summary = take_first_lines(text, 2);
/// assert_eq!(summary, "Line 1\nLine 2");
/// ```
pub fn take_first_lines(text: &str, max_lines: usize) -> String {
    text.lines()
        .filter(|l| !l.trim().is_empty())
        .take(max_lines)
        .collect::<Vec<_>>()
        .join("\n")
}

/// Truncate text to a maximum number of words, appending "..." if truncated.
///
/// # Example
///
/// ```
/// use rustycode_tools::text_summary::truncate_words;
///
/// let text = "one two three four five six seven eight";
/// assert_eq!(truncate_words(text, 4), "one two three four...");
///
/// let short = "one two";
/// assert_eq!(truncate_words(short, 5), "one two");
/// ```
pub fn truncate_words(text: &str, max_words: usize) -> String {
    let words: Vec<&str> = text.split_whitespace().collect();
    if words.len() <= max_words {
        return text.to_string();
    }
    let truncated: Vec<&str> = words.into_iter().take(max_words).collect();
    format!("{}...", truncated.join(" "))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_short_title_short_text() {
        assert_eq!(extract_short_title("Fix the bug"), "Fix the bug");
        assert_eq!(extract_short_title("short"), "short");
        assert_eq!(extract_short_title(""), "");
    }

    #[test]
    fn test_extract_short_title_quoted_phrase() {
        let title = extract_short_title(
            r#"I'll implement the "user authentication" module for the project today"#,
        );
        assert_eq!(title, "user authentication");
    }

    #[test]
    fn test_extract_short_title_single_quotes() {
        let title = extract_short_title(
            "The 'database migration' is now complete and ready for deployment",
        );
        assert_eq!(title, "database migration");
    }

    #[test]
    fn test_extract_short_title_backtick_quotes() {
        let title =
            extract_short_title("I created the `config parser` utility function as you requested");
        assert_eq!(title, "config parser");
    }

    #[test]
    fn test_extract_short_title_last_line_fallback() {
        let title = extract_short_title("Line one is here\nLine two is there\nFinal result here");
        assert_eq!(title, "Final result here");
    }

    #[test]
    fn test_extract_short_title_no_contraction_match() {
        // "it's" should NOT start a quoted section - the apostrophe in a
        // contraction should not be treated as a quote delimiter
        let title =
            extract_short_title("The project's implementation is done for the system module today");
        // Should fall back to last line or full text — no quoted phrase extracted
        // because the apostrophe after "project" is mid-word
        assert!(!title.contains('"'));
    }

    #[test]
    fn test_extract_short_title_multiple_quoted() {
        let title =
            extract_short_title(r#"First implement "basic auth" then add "OAuth support" finally"#);
        // Should pick the last quoted phrase
        assert_eq!(title, "OAuth support");
    }

    #[test]
    fn test_strip_xml_tags_thinking() {
        let clean = strip_xml_tags("<thinking>My reasoning here</thinking>Final answer");
        assert_eq!(clean, "Final answer");
    }

    #[test]
    fn test_strip_xml_tags_multiple() {
        let clean = strip_xml_tags("<a>one</a>middle<b>two</b>end");
        assert_eq!(clean, "middleend");
    }

    #[test]
    fn test_strip_xml_tags_no_tags() {
        assert_eq!(strip_xml_tags("plain text"), "plain text");
    }

    #[test]
    fn test_strip_xml_tags_empty() {
        assert_eq!(strip_xml_tags(""), "");
    }

    #[test]
    fn test_strip_xml_tags_standalone() {
        let clean = strip_xml_tags("before<br/>after");
        assert_eq!(clean, "beforeafter");
    }

    #[test]
    fn test_strip_specific_tag() {
        let clean = strip_specific_tag("Hello <b>world</b> foo", "b");
        assert_eq!(clean, "Hello world foo");
    }

    #[test]
    fn test_strip_specific_tag_not_present() {
        let clean = strip_specific_tag("Hello world", "b");
        assert_eq!(clean, "Hello world");
    }

    #[test]
    fn test_take_first_lines() {
        let text = "Line 1\n\nLine 2\nLine 3\nLine 4";
        assert_eq!(take_first_lines(text, 2), "Line 1\nLine 2");
    }

    #[test]
    fn test_take_first_lines_all() {
        let text = "Line 1\nLine 2";
        assert_eq!(take_first_lines(text, 10), "Line 1\nLine 2");
    }

    #[test]
    fn test_take_first_lines_empty() {
        assert_eq!(take_first_lines("", 5), "");
    }

    #[test]
    fn test_take_first_lines_skips_empty() {
        let text = "\n\nLine 1\n\n\nLine 2\n";
        assert_eq!(take_first_lines(text, 1), "Line 1");
    }

    #[test]
    fn test_truncate_words_short() {
        assert_eq!(truncate_words("one two", 5), "one two");
    }

    #[test]
    fn test_truncate_words_exact() {
        assert_eq!(truncate_words("one two three", 3), "one two three");
    }

    #[test]
    fn test_truncate_words_truncated() {
        assert_eq!(
            truncate_words("one two three four five", 3),
            "one two three..."
        );
    }

    #[test]
    fn test_truncate_words_empty() {
        assert_eq!(truncate_words("", 3), "");
    }

    #[test]
    fn test_extract_quoted_title_too_short() {
        // Single word in quotes is too short (< 2 words)
        let result = extract_quoted_title(r#"The "x" variable"#);
        assert_eq!(result, None);
    }

    #[test]
    fn test_extract_quoted_title_too_long() {
        // 9+ words is too long
        let result = extract_quoted_title(
            r#"This is a "one two three four five six seven eight nine" phrase"#,
        );
        assert_eq!(result, None);
    }

    #[test]
    fn test_strip_xml_tags_multiline_content() {
        let input = "<thinking>\nLine 1\nLine 2\n</thinking>Answer";
        assert_eq!(strip_xml_tags(input), "Answer");
    }
}
