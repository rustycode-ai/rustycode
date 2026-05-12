use crate::line_endings::{detect_line_ending, normalize_quotes, normalize_to_lf};

/// Try exact string match
pub fn try_exact_match(content: &str, old_text: &str) -> Option<(usize, usize)> {
    content
        .find(old_text)
        .map(|start| (start, start + old_text.len()))
}

/// Try matching after normalizing line endings (CRLF → LF).
/// Normalizes both content and `old_text` to LF, performs replacement, then
/// restores original line endings.
pub fn try_normalized_match(content: &str, old_text: &str, new_text: &str) -> Option<String> {
    let norm_content = normalize_to_lf(content);
    let norm_old = normalize_to_lf(old_text);
    if norm_content.contains(&norm_old) {
        let norm_new = normalize_to_lf(new_text);
        let result = norm_content.replacen(&norm_old, &norm_new, 1);
        // Restore original line ending style
        let ending = detect_line_ending(content);
        Some(crate::line_endings::apply_line_ending(&result, ending))
    } else {
        None
    }
}

/// Try matching after normalizing curly/smart quotes to straight quotes.
/// Handles the case where LLMs generate `\u{201C}`/`\u{201D}` (curly double)
/// or `\u{2018}`/`\u{2019}` (curly single) instead of straight quotes.
///
/// Quote normalization preserves character count (1 char → 1 char) but changes
/// byte length (3-byte curly quote → 1-byte straight quote), so we must map
/// through character indices rather than using byte offsets directly.
pub fn try_quote_normalized_match(content: &str, old_text: &str, new_text: &str) -> Option<String> {
    let norm_content = normalize_quotes(content);
    let norm_old = normalize_quotes(old_text);
    let match_start_byte = norm_content.find(&norm_old)?;

    // Convert byte offset in normalized content to character offset
    let start_char_idx = norm_content[..match_start_byte].chars().count();
    let match_char_count = norm_old.chars().count();
    let end_char_idx = start_char_idx + match_char_count;

    // Map character offsets to byte offsets in the original content
    let byte_start = content
        .char_indices()
        .nth(start_char_idx)
        .map(|(i, _)| i)
        .unwrap_or(content.len());
    let byte_end = content
        .char_indices()
        .nth(end_char_idx)
        .map(|(i, _)| i)
        .unwrap_or(content.len());

    let mut result =
        String::with_capacity(content.len() - (byte_end - byte_start) + new_text.len());
    result.push_str(&content[..byte_start]);
    result.push_str(new_text);
    result.push_str(&content[byte_end..]);
    Some(result)
}

/// Try matching where each line is trimmed of whitespace.
/// Returns the full file content with the matched window replaced by `new_text`.
pub fn try_trimmed_match(content: &str, old_text: &str, new_text: &str) -> Option<String> {
    let content_lines: Vec<&str> = content.lines().collect();
    let old_lines: Vec<&str> = old_text.lines().collect();
    if old_lines.is_empty() || old_lines.len() > content_lines.len() {
        return None;
    }
    for (i, window) in content_lines.windows(old_lines.len()).enumerate() {
        if window
            .iter()
            .zip(old_lines.iter())
            .all(|(file_line, old_line)| file_line.trim() == old_line.trim())
        {
            // Found matching window — reconstruct the full file with replacement
            let line_ending = detect_line_ending(content);
            let normalized_new = normalize_to_lf(new_text);
            let new_lines: Vec<&str> = normalized_new.lines().collect();

            let mut result_lines =
                Vec::with_capacity(content_lines.len() - old_lines.len() + new_lines.len());
            // Lines before the match
            result_lines.extend_from_slice(&content_lines[..i]);
            // Replacement lines
            result_lines.extend_from_slice(&new_lines);
            // Lines after the match
            let after = i + old_lines.len();
            result_lines.extend_from_slice(&content_lines[after..]);

            let mut joined = result_lines.join(line_ending.as_str());
            // Preserve trailing newline if original had one
            if content.ends_with('\n') || content.ends_with("\r\n") {
                joined.push_str(line_ending.as_str());
            }
            return Some(joined);
        }
    }
    None
}
