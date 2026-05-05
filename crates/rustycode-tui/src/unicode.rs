//! Unicode helper functions for proper text handling.

use unicode_width::UnicodeWidthStr;

/// Calculate display width accounting for wide characters (CJK, emoji).
pub fn display_width(text: &str) -> usize {
    text.width()
}

pub fn prev_grapheme_boundary(text: &str, byte_pos: usize) -> usize {
    use unicode_segmentation::UnicodeSegmentation;
    let mut prev = 0;
    for (i, _) in text.grapheme_indices(true) {
        if i >= byte_pos {
            break;
        }
        prev = i;
    }
    prev
}

pub fn next_grapheme_boundary(text: &str, byte_pos: usize) -> usize {
    use unicode_segmentation::UnicodeSegmentation;
    text.grapheme_indices(true)
        .find(|(i, _)| *i > byte_pos)
        .map(|(i, _)| i)
        .unwrap_or(text.len())
}

/// Truncate a string at a byte boundary that is safe for UTF-8.
///
/// Returns a string slice guaranteed to end on a valid char boundary.
/// If `max_bytes` lands inside a multi-byte character, it backs up to
/// the previous char boundary.
pub fn truncate_bytes(text: &str, max_bytes: usize) -> &str {
    if text.len() <= max_bytes {
        return text;
    }
    // Find the last valid char boundary at or before max_bytes
    let mut boundary = max_bytes;
    while boundary > 0 && !text.is_char_boundary(boundary) {
        boundary -= 1;
    }
    &text[..boundary]
}

/// Truncate a string to fit within a given **display width**.
///
/// Unlike `truncate_bytes` which operates on byte count, this function
/// accounts for wide characters (CJK, emoji) that occupy 2+ terminal columns.
/// Returns a `String` (owned) because the cut point may not align with the
/// original slice boundaries when appending "...".
///
/// If the text fits within `max_width`, returns it unchanged.
/// Otherwise, truncates at a display-width boundary and appends "...".
pub fn truncate_display(text: &str, max_width: usize) -> String {
    use unicode_width::UnicodeWidthChar;
    if display_width(text) <= max_width {
        return text.to_string();
    }
    // If max_width is too small for "..." (3 cols), do char-by-char truncation
    // without ellipsis to avoid exceeding the requested width.
    if max_width < 3 {
        let mut acc = 0usize;
        let mut cut = 0;
        for (i, ch) in text.char_indices() {
            if let Some(w) = ch.width() {
                if acc + w > max_width {
                    break;
                }
                acc += w;
            }
            cut = i + ch.len_utf8();
        }
        return text[..cut].to_string();
    }
    let ellipsis_width = 3; // "..."
    let target_width = max_width - ellipsis_width;
    let mut acc_width = 0usize;
    let mut cut = 0;
    for (i, ch) in text.char_indices() {
        if let Some(w) = ch.width() {
            if acc_width + w > target_width {
                break;
            }
            acc_width += w;
        }
        cut = i + ch.len_utf8();
    }
    format!("{}...", &text[..cut])
}

#[cfg(test)]
mod tests {
    use super::*;
    use unicode_segmentation::UnicodeSegmentation;

    #[test]
    fn test_display_width_ascii() {
        assert_eq!(display_width("Hello"), 5);
        assert_eq!(display_width(""), 0);
    }

    #[test]
    fn test_display_width_thai() {
        // Thai characters are typically 1 column wide
        // Note: unicode-segmentation treats these as 4 graphemes, not 5
        assert_eq!(display_width("สวัสดี"), 4); // 4 graphemes
        assert_eq!(display_width("เขียน"), 4); // 4 graphemes
    }

    #[test]
    fn test_display_width_emoji() {
        // Most emoji are 2 columns wide
        assert_eq!(display_width("🌍"), 2);
        assert_eq!(display_width("😀"), 2);

        // Family emoji is 2 columns despite being multiple codepoints
        assert_eq!(display_width("👨‍👩‍👧‍👦"), 2);
    }

    #[test]
    fn test_prev_grapheme_boundary() {
        let text = "Hello";
        assert_eq!(prev_grapheme_boundary(text, 5), 4);
        assert_eq!(prev_grapheme_boundary(text, 1), 0);
        assert_eq!(prev_grapheme_boundary(text, 0), 0);

        let thai = "สวัสดี";
        let byte_pos = thai
            .grapheme_indices(true)
            .nth(2)
            .map(|(i, _)| i)
            .unwrap_or(0);
        let prev = prev_grapheme_boundary(thai, byte_pos);
        let expected = thai
            .grapheme_indices(true)
            .nth(1)
            .map(|(i, _)| i)
            .unwrap_or(0);
        assert_eq!(prev, expected);
    }

    #[test]
    fn test_next_grapheme_boundary() {
        let text = "Hello";
        assert_eq!(next_grapheme_boundary(text, 0), 1);
        assert_eq!(next_grapheme_boundary(text, 4), 5);
        assert_eq!(next_grapheme_boundary(text, 5), 5);

        let thai = "สวัสดี";
        let byte_pos = thai
            .grapheme_indices(true)
            .nth(1)
            .map(|(i, _)| i)
            .unwrap_or(0);
        let next = next_grapheme_boundary(thai, byte_pos);
        let expected = thai
            .grapheme_indices(true)
            .nth(2)
            .map(|(i, _)| i)
            .unwrap_or(thai.len());
        assert_eq!(next, expected);
    }

    #[test]
    fn test_zero_width_joiners() {
        // Family emoji: man + ZWJ + woman + ZWJ + girl + ZWJ + boy
        // This is 1 grapheme despite being 7 codepoints
        let family = "👨‍👩‍👧‍👦";
        // Family emoji should have display width >= 2
        assert!(display_width(family) >= 2);
    }

    #[test]
    fn test_combining_diacritics() {
        // 'é' can be represented as 'e' + combining acute accent
        let combined = "e\u{0301}"; // e + combining acute
        assert_eq!(display_width(combined), 1);
    }

    #[test]
    fn test_truncate_bytes_ascii() {
        assert_eq!(truncate_bytes("Hello World", 5), "Hello");
        assert_eq!(truncate_bytes("Hi", 10), "Hi");
        assert_eq!(truncate_bytes("", 5), "");
    }

    #[test]
    fn test_truncate_display_fits() {
        assert_eq!(truncate_display("Hi", 10), "Hi");
        assert_eq!(truncate_display("", 5), "");
    }

    #[test]
    fn test_truncate_display_with_ellipsis() {
        assert_eq!(truncate_display("Hello World", 8), "Hello...");
    }

    #[test]
    fn test_truncate_display_small_width() {
        assert_eq!(truncate_display("Hello", 0), "");
        assert_eq!(truncate_display("Hello", 1), "H");
        assert_eq!(truncate_display("Hello", 2), "He");
        assert_eq!(truncate_display("Hello", 3), "...");
    }

    #[test]
    fn test_truncate_display_wide_chars() {
        let emoji = "🌍🌍🌍";
        assert_eq!(display_width(emoji), 6);
        assert_eq!(truncate_display(emoji, 5), "🌍...");
    }

    #[test]
    fn test_truncate_bytes_multibyte() {
        // "สวัสดี" is 6 code points, each 3 bytes = 18 bytes total
        let thai = "สวัสดี";
        // Truncate at 7 bytes — lands inside char, backs up to 6 = "สว"
        let truncated = truncate_bytes(thai, 7);
        assert_eq!(truncated, "สว");
        // At exact boundary
        assert_eq!(truncate_bytes(thai, 6), "สว");
        assert_eq!(truncate_bytes(thai, 9), "สวั");
        assert_eq!(truncate_bytes(thai, 18), "สวัสดี");
    }
}
