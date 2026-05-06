//! Helper functions for brutalist rendering
//!
//! Utility functions for formatting and display in the brutalist TUI.

use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

/// Max display width for tool parameter shortening.
const TOOL_PARAM_DISPLAY_MAX: usize = 50;
/// Max display width before truncating commands/patterns inline.
const INLINE_TRUNCATE_WIDTH: usize = 40;
/// Max display width for a result summary to show inline.
const RESULT_SUMMARY_INLINE_MAX: usize = 80;
/// Characters reserved for ellipsis when shortening paths.
const ELLIPSIS_WIDTH: usize = 3;
/// Minimum available space before falling back to minimal path shortening.
const MIN_PATH_SHORTENING_SPACE: usize = 4;
/// Path component count threshold for path shortening.
const PATH_COMPONENT_SHORTEN_THRESHOLD: usize = 2;

/// Format elapsed seconds into a compact display string.
/// Examples: "3s", "1m4s", "2m"
pub fn format_elapsed_short(secs: u64) -> String {
    if secs < 60 {
        format!("{}s", secs)
    } else {
        let mins = secs / 60;
        let remain_secs = secs % 60;
        if remain_secs == 0 {
            format!("{}m", mins)
        } else {
            format!("{}m{}s", mins, remain_secs)
        }
    }
}

/// Format elapsed seconds as MM:SS (zero-padded).
/// Examples: "00:03", "01:04", "02:00"
pub fn format_elapsed_mmss(secs: u64) -> String {
    format!("{:02}:{:02}", secs / 60, secs % 60)
}

/// Format a Duration for the status bar.
/// Examples: "350ms", "3.2s", "1m05s"
pub fn format_duration_compact(dur: std::time::Duration) -> String {
    let secs = dur.as_secs();
    let ms = dur.as_millis();
    if ms < 1000 {
        format!("{}ms", ms)
    } else if secs < 60 {
        format!("{:.1}s", ms as f64 / 1000.0)
    } else {
        format!("{}m{:02}s", secs / 60, secs % 60)
    }
}

/// Format token count compactly for inline display.
///
/// Examples: "8.2k", "1.5M", "500"
pub fn format_tokens_compact(n: usize) -> String {
    if n >= 1_000_000 {
        format!("{:.1}M", n as f64 / 1_000_000.0)
    } else if n >= 1_000 {
        format!("{:.1}k", n as f64 / 1_000.0)
    } else {
        n.to_string()
    }
}

/// Extract the most relevant parameter from a tool call for inline display.
/// Shows file path for file tools, command for shell tools, query for search tools.
pub fn extract_tool_key_param(
    tool_name: &str,
    input_json: Option<&serde_json::Value>,
    result_summary: &str,
) -> Option<String> {
    let name = tool_name.to_lowercase();

    if let Some(json) = input_json {
        tracing::debug!(
            "extract_tool_key_param: name={} json_len={}",
            name,
            json.to_string().len()
        );
        if name.contains("read")
            || name.contains("write")
            || name.contains("edit")
            || name.contains("create")
            || name.contains("view")
            || name.contains("cat")
        {
            if let Some(path) = json
                .get("file_path")
                .or(json.get("path"))
                .and_then(|v| v.as_str())
            {
                return Some(shorten_tool_param(path, TOOL_PARAM_DISPLAY_MAX));
            }
        }

        if name.contains("bash")
            || name.contains("exec")
            || name.contains("shell")
            || name.contains("run")
        {
            if let Some(cmd) = json
                .get("command")
                .or(json.get("cmd"))
                .and_then(|v| v.as_str())
            {
                let first_line = cmd.lines().next().unwrap_or(cmd);
                let truncated =
                    if <str as UnicodeWidthStr>::width(first_line) > INLINE_TRUNCATE_WIDTH {
                        format!(
                            "{}…",
                            truncate_to_display_width(
                                first_line,
                                INLINE_TRUNCATE_WIDTH.saturating_sub(1)
                            )
                        )
                    } else {
                        first_line.to_string()
                    };
                return Some(truncated);
            }
        }

        if name.contains("grep") || name.contains("search") {
            if let Some(pattern) = json
                .get("pattern")
                .or(json.get("query"))
                .and_then(|v| v.as_str())
            {
                return Some(
                    if <str as UnicodeWidthStr>::width(pattern) > INLINE_TRUNCATE_WIDTH {
                        format!(
                            "{}…",
                            truncate_to_display_width(
                                pattern,
                                INLINE_TRUNCATE_WIDTH.saturating_sub(1)
                            )
                        )
                    } else {
                        pattern.to_string()
                    },
                );
            }
        }

        if name.contains("glob") || name.contains("find") || name.contains("list") {
            if let Some(pattern) = json
                .get("pattern")
                .or(json.get("glob"))
                .and_then(|v| v.as_str())
            {
                return Some(pattern.to_string());
            }
        }
    }

    if !result_summary.is_empty()
        && <str as UnicodeWidthStr>::width(result_summary) < RESULT_SUMMARY_INLINE_MAX
        && (name.contains("read") || name.contains("write") || name.contains("edit"))
        && (result_summary.contains('/') || result_summary.contains('\\'))
    {
        return Some(shorten_tool_param(result_summary, TOOL_PARAM_DISPLAY_MAX));
    }

    None
}

/// Progress bar character sets for consistent rendering across the TUI.
/// Context/token bars use line-drawing characters; tool/phase bars use block characters.
pub const PROGRESS_CHARS_CONTEXT: (&str, &str) = ("━", "╌");
pub const PROGRESS_CHARS_TOOLS: (&str, &str) = ("█", "░");

/// Render a text progress bar with configurable characters.
///
/// Returns a string like `"━━━━╌╌╌╌╌╌"` or `"████░░░░"`.
/// Use `PROGRESS_CHARS_CONTEXT` or `PROGRESS_CHARS_TOOLS` for consistent styling.
pub fn progress_bar(width: usize, filled: usize, filled_char: &str, empty_char: &str) -> String {
    let clamped = filled.min(width);
    let empty = width.saturating_sub(clamped);
    format!(
        "{}{}",
        filled_char.repeat(clamped),
        empty_char.repeat(empty)
    )
}

/// Shorten a tool parameter (typically a file path) for compact display.
/// Abbreviates middle components to their first letter while preserving
/// the filename and prefix.
pub fn shorten_tool_param(s: &str, max_len: usize) -> String {
    if <str as UnicodeWidthStr>::width(s) <= max_len {
        return s.to_string();
    }

    let display = if let Ok(home) = std::env::var("HOME") {
        if s.starts_with(&home) {
            format!("~{}", &s[home.len()..])
        } else {
            s.to_string()
        }
    } else {
        s.to_string()
    };

    if <str as UnicodeWidthStr>::width(display.as_str()) <= max_len {
        return display;
    }

    let components: Vec<&str> = display.split('/').collect();
    if components.len() <= PATH_COMPONENT_SHORTEN_THRESHOLD {
        return format!(
            "{}…",
            truncate_to_display_width(&display, max_len.saturating_sub(1))
        );
    }

    let first = components.first().unwrap_or(&"");
    let last = components.last().unwrap_or(&"");

    let prefix = if first.is_empty() { "/" } else { "" };
    let suffix = format!("/{}", last);

    let available = max_len.saturating_sub(
        <str as UnicodeWidthStr>::width(prefix)
            + <str as UnicodeWidthStr>::width(suffix.as_str())
            + ELLIPSIS_WIDTH,
    );

    if available < MIN_PATH_SHORTENING_SPACE {
        let first_char = first.chars().next().unwrap_or('/');
        let last_budget = max_len.saturating_sub(
            <str as UnicodeWidthStr>::width(prefix)
                + UnicodeWidthChar::width(first_char).unwrap_or(0)
                + 1,
        );
        return format!(
            "{}{}…{}",
            prefix,
            first_char,
            truncate_to_display_width(last, last_budget)
        );
    }

    let mut result = prefix.to_string();
    let first_char = first.chars().next().unwrap_or('/');
    result.push(first_char);

    for comp in components
        .iter()
        .skip(1)
        .take(components.len() - PATH_COMPONENT_SHORTEN_THRESHOLD)
    {
        let next_width = <str as UnicodeWidthStr>::width(result.as_str())
            + <str as UnicodeWidthStr>::width(*comp)
            + <str as UnicodeWidthStr>::width(suffix.as_str())
            + ELLIPSIS_WIDTH;
        if next_width > max_len {
            break;
        }
        result.push('/');
        let comp_char = comp.chars().next().unwrap_or('?');
        result.push(comp_char);
    }

    result.push('…');
    result.push_str(&suffix);

    result
}

pub(crate) fn truncate_to_display_width(s: &str, max_width: usize) -> String {
    let mut out = String::new();
    let mut width = 0;

    for ch in s.chars() {
        let ch_width = UnicodeWidthChar::width(ch).unwrap_or(0);
        if width + ch_width > max_width {
            break;
        }
        out.push(ch);
        width += ch_width;
    }

    out
}

pub fn count_consecutive(bytes: &[u8], start: usize, byte: u8) -> usize {
    bytes[start..].iter().take_while(|&&b| b == byte).count()
}

pub fn find_consecutive(bytes: &[u8], byte: u8, count: usize) -> Option<usize> {
    bytes
        .windows(count)
        .position(|window| window.iter().all(|&b| b == byte))
}

pub fn find_byte_pair(bytes: &[u8], byte: u8) -> Option<usize> {
    find_consecutive(bytes, byte, 2)
}

pub fn find_byte(bytes: &[u8], byte: u8) -> Option<usize> {
    bytes.iter().position(|&b| b == byte)
}

pub fn tool_type_icon(name: &str) -> &'static str {
    let n = name.to_lowercase();
    if n.contains("read") || n.contains("view") || n.contains("cat") {
        "◎"
    } else if n.contains("write") || n.contains("edit") || n.contains("create") {
        "✎"
    } else if n.contains("bash") || n.contains("shell") || n.contains("exec") {
        "▸"
    } else if n.contains("search") || n.contains("grep") || n.contains("find") {
        "⌕"
    } else if n.contains("glob") || n.contains("list") {
        "⋮"
    } else if n.contains("diff") || n.contains("patch") {
        "≠"
    } else if n.contains("git") {
        "⎇"
    } else if n.contains("mcp") || n.contains("server") {
        "◉"
    } else if n.contains("apply") || n.contains("tool") {
        "▶"
    } else {
        "○"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_shorten_tool_param_ascii() {
        let result = shorten_tool_param("/a/b/c/d/e/f", 10);
        assert!(result.contains('…'), "should contain ellipsis: {}", result);
    }

    #[test]
    fn test_shorten_tool_param_short_enough() {
        let path = "/short/path";
        assert_eq!(shorten_tool_param(path, 50), path);
    }

    #[test]
    fn test_shorten_tool_param_multibyte_component() {
        // Path with Chinese characters — should not panic on UTF-8 boundary
        let path = "/项目/代码/文件/测试/结尾";
        // Width is 25, must use max_len < 25 to trigger shortening
        let result = shorten_tool_param(path, 24);
        assert!(result.contains('…'), "should contain ellipsis: {}", result);
    }

    #[test]
    fn test_shorten_tool_param_two_components() {
        // Only 2 components — should truncate with ellipsis
        let path = "/verylongdirectoryname/file.txt";
        let result = shorten_tool_param(path, 20);
        assert!(result.contains('…'), "should contain ellipsis: {}", result);
    }

    #[test]
    fn test_shorten_tool_param_wide_text_not_prematurely_cut() {
        let path = "/项目/代码";
        assert_eq!(shorten_tool_param(path, 12), path);
    }

    #[test]
    fn test_extract_tool_key_param_uses_display_width() {
        let input = serde_json::json!({
            "path": "/项目/代码"
        });
        let extracted = extract_tool_key_param("read_file", Some(&input), "");
        assert_eq!(extracted.as_deref(), Some("/项目/代码"));
    }

    #[test]
    fn test_format_elapsed_short_seconds() {
        assert_eq!(format_elapsed_short(0), "0s");
        assert_eq!(format_elapsed_short(5), "5s");
        assert_eq!(format_elapsed_short(59), "59s");
    }

    #[test]
    fn test_format_elapsed_short_minutes() {
        assert_eq!(format_elapsed_short(60), "1m");
        assert_eq!(format_elapsed_short(120), "2m");
        assert_eq!(format_elapsed_short(61), "1m1s");
        assert_eq!(format_elapsed_short(125), "2m5s");
    }

    #[test]
    fn test_format_elapsed_mmss_basic() {
        assert_eq!(format_elapsed_mmss(0), "00:00");
        assert_eq!(format_elapsed_mmss(5), "00:05");
        assert_eq!(format_elapsed_mmss(59), "00:59");
        assert_eq!(format_elapsed_mmss(60), "01:00");
        assert_eq!(format_elapsed_mmss(65), "01:05");
        assert_eq!(format_elapsed_mmss(125), "02:05");
        assert_eq!(format_elapsed_mmss(3661), "61:01");
    }

    #[test]
    fn test_format_duration_compact_millis() {
        assert_eq!(
            format_duration_compact(std::time::Duration::from_millis(350)),
            "350ms"
        );
    }

    #[test]
    fn test_format_duration_compact_seconds() {
        assert_eq!(
            format_duration_compact(std::time::Duration::from_millis(3200)),
            "3.2s"
        );
    }

    #[test]
    fn test_format_duration_compact_minutes() {
        assert_eq!(
            format_duration_compact(std::time::Duration::from_secs(65)),
            "1m05s"
        );
    }

    #[test]
    fn test_format_duration_compact_exact_minute() {
        assert_eq!(
            format_duration_compact(std::time::Duration::from_secs(60)),
            "1m00s"
        );
    }

    #[test]
    fn test_format_tokens_compact_values() {
        assert_eq!(format_tokens_compact(0), "0");
        assert_eq!(format_tokens_compact(500), "500");
        assert_eq!(format_tokens_compact(999), "999");
        assert_eq!(format_tokens_compact(1_000), "1.0k");
        assert_eq!(format_tokens_compact(8_200), "8.2k");
        assert_eq!(format_tokens_compact(999_999), "1000.0k");
        assert_eq!(format_tokens_compact(1_000_000), "1.0M");
        assert_eq!(format_tokens_compact(1_500_000), "1.5M");
    }

    #[test]
    fn test_count_consecutive_bytes() {
        assert_eq!(count_consecutive(b"aaaabc", 0, b'a'), 4);
        assert_eq!(count_consecutive(b"aaaabc", 4, b'b'), 1);
        assert_eq!(count_consecutive(b"aaaabc", 5, b'c'), 1);
        assert_eq!(count_consecutive(b"aaaa", 0, b'a'), 4);
        assert_eq!(count_consecutive(b"bbbb", 0, b'a'), 0);
    }

    #[test]
    fn test_find_consecutive_bytes() {
        assert_eq!(find_consecutive(b"aabbcc", b'a', 2), Some(0));
        assert_eq!(find_consecutive(b"aabbcc", b'b', 2), Some(2));
        assert_eq!(find_consecutive(b"aabbcc", b'c', 2), Some(4));
        assert_eq!(find_consecutive(b"abcdef", b'a', 2), None);
    }

    #[test]
    fn test_find_byte_pair() {
        assert_eq!(find_byte_pair(b"hello**world", b'*'), Some(5));
        assert_eq!(find_byte_pair(b"no pairs here", b'*'), None);
        assert_eq!(find_byte_pair(b"~~strike~~", b'~'), Some(0));
    }

    #[test]
    fn test_find_single_byte() {
        assert_eq!(find_byte(b"hello", b'e'), Some(1));
        assert_eq!(find_byte(b"hello", b'z'), None);
        assert_eq!(find_byte(b"", b'a'), None);
    }

    #[test]
    fn test_tool_type_icon_categories() {
        assert_eq!(tool_type_icon("read_file"), "◎");
        assert_eq!(tool_type_icon("View"), "◎");
        assert_eq!(tool_type_icon("write_file"), "✎");
        assert_eq!(tool_type_icon("Edit"), "✎");
        assert_eq!(tool_type_icon("bash"), "▸");
        assert_eq!(tool_type_icon("Search"), "⌕");
        assert_eq!(tool_type_icon("grep"), "⌕");
        assert_eq!(tool_type_icon("glob"), "⋮");
        assert_eq!(tool_type_icon("diff"), "≠");
        assert_eq!(tool_type_icon("git_status"), "⎇");
        assert_eq!(tool_type_icon("mcp_server"), "◉");
        assert_eq!(tool_type_icon("apply_patch"), "≠");
        assert_eq!(tool_type_icon("unknown"), "○");
    }

    #[test]
    fn test_progress_bar_full() {
        assert_eq!(progress_bar(8, 8, "━", "╌"), "━━━━━━━━");
        assert_eq!(progress_bar(8, 0, "━", "╌"), "╌╌╌╌╌╌╌╌");
    }

    #[test]
    fn test_progress_bar_partial() {
        assert_eq!(progress_bar(10, 6, "█", "░"), "██████░░░░");
        assert_eq!(progress_bar(8, 3, "━", "╌"), "━━━╌╌╌╌╌");
    }

    #[test]
    fn test_progress_bar_clamps_overflow() {
        assert_eq!(progress_bar(5, 99, "█", "░"), "█████");
    }

    #[test]
    fn test_truncate_to_display_width_ascii() {
        assert_eq!(truncate_to_display_width("hello world", 5), "hello");
        assert_eq!(truncate_to_display_width("hello", 10), "hello");
        assert_eq!(truncate_to_display_width("", 5), "");
    }

    #[test]
    fn test_truncate_to_display_width_cjk() {
        // Each CJK char is width 2
        assert_eq!(truncate_to_display_width("项目代码", 4), "项目");
        assert_eq!(truncate_to_display_width("项目代码", 3), "项");
        assert_eq!(truncate_to_display_width("项目代码", 1), "");
    }
}
