//! Shared text processing utilities.

/// Approximate token count for a string using word-boundary counting.
///
/// More accurate for code than chars/4: code has many short punctuation tokens
/// that get merged by char-based heuristics, and long identifiers that get
/// under-split. Word-based counting matches real tokenizer behaviour more
/// closely across prose, code, and mixed content.
///
/// Returns at least 1 for any non-empty string (punctuation-only content still
/// consumes tokens). Returns 0 for the empty string.
pub fn estimate_tokens(s: &str) -> usize {
    let words = s.split_whitespace().count();
    if words == 0 && !s.is_empty() {
        1 // punctuation-only content still consumes tokens
    } else {
        words
    }
}

/// Truncate a string to at most `max_len` characters, appending `"..."` if truncated.
///
/// Character-safe: never splits a multi-byte UTF-8 codepoint.
pub fn truncate_with_ellipsis(s: &str, max_len: usize) -> String {
    if s.chars().count() <= max_len {
        return s.to_string();
    }
    let truncated: String = s.chars().take(max_len).collect();
    format!("{truncated}...")
}

/// Truncate tool output to fit within `max_bytes`, keeping head and tail.
///
/// When errors are detected in the output, preserves more of the tail (error
/// details). Otherwise, keeps the last quarter for pagination/hint lines.
pub fn truncate_tool_output(output: &str, max_bytes: usize) -> String {
    if output.len() <= max_bytes {
        return output.to_string();
    }

    let out_lower = output.to_lowercase();
    let has_errors = out_lower.contains("error")
        || out_lower.contains("traceback")
        || out_lower.contains("failed")
        || out_lower.contains("segmentation fault")
        || out_lower.contains("command not found");

    let (head_bytes, tail_bytes) = if has_errors {
        (max_bytes / 6, max_bytes * 5 / 6)
    } else {
        (max_bytes / 4, max_bytes * 3 / 4)
    };

    let head_end = output
        .char_indices()
        .take_while(|(i, _)| *i < head_bytes)
        .last()
        .map_or(0, |(i, c)| i + c.len_utf8());

    let tail_start_offset = output.len().saturating_sub(tail_bytes);
    let tail_start = output
        .char_indices()
        .find(|(i, _)| *i >= tail_start_offset)
        .map_or(output.len(), |(i, _)| i);

    if tail_start > head_end {
        let skipped = tail_start - head_end;
        format!(
            "{}\n\n[...{skipped} bytes truncated...]\n\n{}",
            &output[..head_end],
            &output[tail_start..]
        )
    } else {
        output.to_string()
    }
}

/// Strip ANSI CSI escape sequences from a string, keeping visible text.
pub fn strip_ansi_escapes(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            if chars.peek() == Some(&'[') {
                chars.next();
                while let Some(&next) = chars.peek() {
                    if ('\x30'..='\x3f').contains(&next) || ('\x20'..='\x2f').contains(&next) {
                        chars.next();
                    } else {
                        break;
                    }
                }
                if chars.peek().is_some_and(|c| ('\x40'..='\x7e').contains(c)) {
                    chars.next();
                }
            }
        } else {
            result.push(c);
        }
    }
    result
}

pub const fn default_true() -> bool {
    true
}

pub const fn default_false() -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_simple_color() {
        assert_eq!(strip_ansi_escapes("\x1b[31mError\x1b[0m"), "Error");
    }

    #[test]
    fn strip_multiple() {
        assert_eq!(
            strip_ansi_escapes("\x1b[1;32mOK\x1b[0m \x1b[33mw\x1b[0m"),
            "OK w"
        );
    }

    #[test]
    fn strip_plain() {
        assert_eq!(strip_ansi_escapes("plain text"), "plain text");
    }

    #[test]
    fn truncate_short() {
        assert_eq!(truncate_with_ellipsis("hello", 10), "hello");
    }

    #[test]
    fn truncate_exact() {
        assert_eq!(truncate_with_ellipsis("hello", 5), "hello");
    }

    #[test]
    fn truncate_long() {
        assert_eq!(truncate_with_ellipsis("hello world", 5), "hello...");
    }

    #[test]
    fn truncate_tool_output_short() {
        assert_eq!(truncate_tool_output("ok", 100), "ok");
    }

    #[test]
    fn truncate_tool_output_long_no_error() {
        let input = "a".repeat(200);
        let result = truncate_tool_output(&input, 100);
        assert!(result.contains("bytes truncated"));
        assert!(result.starts_with('a'));
    }

    #[test]
    fn truncate_tool_output_with_error() {
        let input = format!("{}\n{}", "x".repeat(200), "Error: something failed");
        let result = truncate_tool_output(&input, 100);
        assert!(result.contains("bytes truncated"));
        assert!(result.contains("Error: something failed"));
    }
}
