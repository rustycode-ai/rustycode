//! Input state management for multi-line input handling.
//!
//! This module provides the core state types for input management:
//! - Input mode states (single-line vs multi-line)
//! - Complete input state with cursor tracking
//! - Image attachment metadata

use crate::ui::message_types::ImageAttachment;
use crate::unicode::{display_width, next_grapheme_boundary, prev_grapheme_boundary};
use unicode_segmentation::UnicodeSegmentation;

// ── Input Mode States ───────────────────────────────────────────────────────

/// Input mode state
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
#[non_exhaustive]
pub enum InputMode {
    /// Single-line mode (default)
    /// - Enter: Send message
    /// - Option+Enter: Insert newline, switch to MultiLine
    #[default]
    SingleLine,

    /// Multi-line mode
    /// - Enter: Insert newline
    /// - Option+Enter: Send message
    MultiLine,
}

// ── Input State ─────────────────────────────────────────────────────────────

/// Complete input state including text, cursor, and images
#[derive(Clone, Debug, Default)]
pub struct InputState {
    /// Current input mode
    pub mode: InputMode,
    /// Multiple lines for multi-line input
    pub lines: Vec<String>,
    /// Which line we're on (cursor row)
    pub cursor_row: usize,
    /// Position within line (cursor column)
    pub cursor_col: usize,
    /// Pasted images
    pub images: Vec<ImageAttachment>,
    /// Horizontal scroll offset for the current line (in display columns)
    pub display_offset: usize,
    /// Selection anchor column (byte offset). `None` means no selection.
    pub selection_anchor_col: Option<usize>,
    /// Selection anchor row. `None` means no selection.
    pub selection_anchor_row: Option<usize>,
}

impl InputState {
    /// Create new input state
    pub fn new() -> Self {
        Self {
            mode: InputMode::SingleLine,
            lines: vec![String::new()],
            cursor_row: 0,
            cursor_col: 0,
            images: Vec::new(),
            display_offset: 0,
            selection_anchor_col: None,
            selection_anchor_row: None,
        }
    }

    /// Get current line content
    pub fn current_line(&self) -> String {
        self.lines.get(self.cursor_row).cloned().unwrap_or_default()
    }

    /// Get all text as single string
    pub fn all_text(&self) -> String {
        self.lines.join("\n")
    }

    /// Check if input is empty (no text content)
    pub fn is_empty(&self) -> bool {
        self.lines.iter().all(|l| l.is_empty())
    }

    /// Get cursor position in display columns (for rendering)
    pub fn cursor_display_col(&self) -> usize {
        let text = self
            .lines
            .get(self.cursor_row)
            .map(|s| s.as_str())
            .unwrap_or("");
        let pos = self.cursor_col.min(text.len());
        let pos = text.floor_char_boundary(pos);
        display_width(&text[..pos])
    }

    /// Total display width of current line
    pub fn line_display_width(&self) -> usize {
        self.lines
            .get(self.cursor_row)
            .map(|line| display_width(line))
            .unwrap_or(0)
    }

    /// Recalculate display offset so cursor stays visible within `visible_width` columns.
    /// Call after any cursor movement or text change.
    pub fn recalc_display_offset(&mut self, visible_width: usize) {
        let cursor_col = self.cursor_display_col();
        let prefix = cursor_col.saturating_sub(1);
        let min_offset = prefix.saturating_sub(visible_width.saturating_sub(2));
        let max_offset = cursor_col;
        self.display_offset = self.display_offset.clamp(min_offset, max_offset);
    }

    /// Get the byte offset corresponding to the current display_offset on the cursor row.
    /// Used by the renderer to slice the line for horizontal scrolling.
    pub fn scroll_byte_offset(&self) -> usize {
        let line = self
            .lines
            .get(self.cursor_row)
            .map(|s| s.as_str())
            .unwrap_or("");
        let mut display_col = 0;
        for (byte_idx, grapheme) in line.grapheme_indices(true) {
            if display_col >= self.display_offset {
                return byte_idx;
            }
            display_col += display_width(grapheme);
        }
        line.len()
    }

    /// Get the display column of the cursor relative to the display_offset.
    pub fn relative_cursor_display_col(&self) -> usize {
        self.cursor_display_col()
            .saturating_sub(self.display_offset)
    }

    /// Insert a character at cursor position
    pub fn insert_char(&mut self, c: char) {
        if let Some(line) = self.lines.get_mut(self.cursor_row) {
            if self.cursor_col <= line.len() {
                line.insert(self.cursor_col, c);
                self.cursor_col += c.len_utf8();
            }
        }
    }

    /// Insert a string at the current cursor position
    pub fn insert_string(&mut self, s: &str) {
        if let Some(line) = self.lines.get_mut(self.cursor_row) {
            if self.cursor_col <= line.len() {
                line.insert_str(self.cursor_col, s);
                self.cursor_col += s.len();
            }
        }
    }

    /// Insert text at the current cursor position, handling multiline content.
    ///
    /// Normalizes line endings (`\r\n` → `\n`) and splits into lines.
    /// For single-line text, inserts inline. For multiline text, splits
    /// the current line at the cursor and inserts all pasted lines.
    pub fn insert_text_at_cursor(&mut self, text: &str) {
        let normalized = text.replace("\r\n", "\n").replace('\r', "");
        let lines: Vec<&str> = normalized.split('\n').collect();

        if lines.is_empty() {
            return;
        }

        if self.cursor_row >= self.lines.len() {
            self.cursor_row = self.lines.len().saturating_sub(1);
        }

        if lines.len() == 1 {
            let current_line = &mut self.lines[self.cursor_row];
            let col = current_line.floor_char_boundary(self.cursor_col.min(current_line.len()));
            current_line.insert_str(col, lines[0]);
            self.cursor_col = col + lines[0].len();
        } else {
            let current_line = &self.lines[self.cursor_row];
            let col = current_line.floor_char_boundary(self.cursor_col.min(current_line.len()));
            let before = current_line[..col].to_string();
            let after = current_line[col..].to_string();

            self.lines[self.cursor_row] = format!("{}{}", before, lines[0]);

            for (i, line) in lines[1..lines.len() - 1].iter().enumerate() {
                self.lines.insert(self.cursor_row + 1 + i, line.to_string());
            }

            let last_idx = lines.len() - 1;
            let last_pasted_part = lines[last_idx];
            self.lines.insert(
                self.cursor_row + last_idx,
                format!("{}{}", last_pasted_part, after),
            );

            self.cursor_row += last_idx;
            self.cursor_col = last_pasted_part.len();
        }
    }

    /// Delete character before cursor (backspace)
    ///
    /// This now properly deletes entire grapheme clusters, not just bytes.
    /// For Thai text, this means deleting consonant + vowel combinations as one unit.
    pub fn backspace(&mut self) {
        if let Some(line) = self.lines.get_mut(self.cursor_row) {
            if self.cursor_col > 0 {
                // Find the previous grapheme boundary
                let prev_boundary = prev_grapheme_boundary(line, self.cursor_col);

                // Remove the entire grapheme cluster (byte range from prev_boundary to cursor)
                line.replace_range(prev_boundary..self.cursor_col, "");

                // Move cursor to previous grapheme position
                self.cursor_col = prev_boundary;
            } else if self.cursor_row > 0 && self.mode == InputMode::MultiLine {
                // Join with previous line
                let prev_line = self.lines.remove(self.cursor_row);
                self.cursor_row -= 1;
                self.cursor_col = self.lines[self.cursor_row].len();
                self.lines[self.cursor_row].push_str(&prev_line);
            }
        }
    }

    /// Delete character at cursor (delete key)
    ///
    /// This now properly deletes entire grapheme clusters, not just bytes.
    pub fn delete(&mut self) {
        let line_len = self.lines.get(self.cursor_row).map_or(0, |l| l.len());

        if self.cursor_col < line_len {
            // Delete within current line
            if let Some(line) = self.lines.get_mut(self.cursor_row) {
                // Find the next grapheme boundary
                let next_boundary = next_grapheme_boundary(line, self.cursor_col);

                // Remove characters from cursor to next boundary
                let end = next_boundary.min(line.len());
                line.drain(self.cursor_col..end);
            }
        } else if self.cursor_row + 1 < self.lines.len() && self.mode == InputMode::MultiLine {
            // Join with next line - need to be careful with borrow checker
            let next_line = self.lines.remove(self.cursor_row + 1);
            if let Some(line) = self.lines.get_mut(self.cursor_row) {
                line.push_str(&next_line);
            }
        }
    }

    /// Delete word backward (Ctrl+Backspace)
    ///
    /// Deletes from the cursor back to the start of the current word.
    /// A word is defined as a sequence of alphanumeric characters or underscores.
    pub fn delete_word_backward(&mut self) -> String {
        if let Some(line) = self.lines.get_mut(self.cursor_row) {
            if self.cursor_col > 0 {
                // Clamp cursor_col to a valid UTF-8 boundary before slicing
                let col = line.floor_char_boundary(self.cursor_col);
                let text_before = &line[..col];

                let mut start_pos = 0;
                let mut found_non_ws = false;

                for (i, c) in text_before.char_indices() {
                    if c.is_whitespace() {
                        if found_non_ws {
                            start_pos = i;
                        }
                    } else {
                        if !found_non_ws {
                            start_pos = i;
                        }
                        found_non_ws = true;
                    }
                }

                let deleted: String = line[start_pos..col].to_string();
                line.replace_range(start_pos..col, "");
                self.cursor_col = start_pos;
                return deleted;
            }
        }
        String::new()
    }

    /// Delete word forward (Ctrl+Delete)
    ///
    /// Deletes from the cursor to the end of the current word.
    /// A word is defined as a sequence of alphanumeric characters or underscores.
    pub fn delete_word_forward(&mut self) {
        if let Some(line) = self.lines.get_mut(self.cursor_row) {
            let col = line.floor_char_boundary(self.cursor_col);
            let text_after = &line[col..];

            // Find the end of the current word
            let mut end_pos = col;

            for (offset, c) in text_after.char_indices() {
                if c.is_whitespace() {
                    break;
                }
                end_pos = col + offset + c.len_utf8();
            }

            // Remove from cursor to word end
            line.drain(col..end_pos);
            self.cursor_col = col;
        }
    }

    /// Move cursor left by one grapheme cluster
    ///
    /// This properly handles Thai characters, emoji, and other multi-codepoint graphemes.
    pub fn move_cursor_left(&mut self) {
        if let Some(line) = self.lines.get(self.cursor_row) {
            if self.cursor_col > 0 {
                // Move to previous grapheme boundary
                self.cursor_col = prev_grapheme_boundary(line, self.cursor_col);
            }
        }
    }

    /// Move cursor right by one grapheme cluster
    ///
    /// This properly handles Thai characters, emoji, and other multi-codepoint graphemes.
    pub fn move_cursor_right(&mut self) {
        if let Some(line) = self.lines.get(self.cursor_row) {
            if self.cursor_col < line.len() {
                // Move to next grapheme boundary
                self.cursor_col = next_grapheme_boundary(line, self.cursor_col);
            }
        }
    }

    /// Move cursor to the start of the previous word
    pub fn move_word_backward(&mut self) {
        if let Some(line) = self.lines.get(self.cursor_row) {
            if self.cursor_col > 0 {
                let col = line.floor_char_boundary(self.cursor_col);
                let chars: Vec<char> = line[..col].chars().collect();
                let mut i = chars.len();

                // Skip whitespace
                while i > 0 && chars[i - 1].is_whitespace() {
                    i -= 1;
                }
                // Skip word characters
                while i > 0 && !chars[i - 1].is_whitespace() {
                    i -= 1;
                }

                // Convert char index back to byte index
                self.cursor_col = line[..col]
                    .char_indices()
                    .nth(i)
                    .map(|(byte_idx, _)| byte_idx)
                    .unwrap_or(0);
            }
        }
    }

    /// Move cursor to the start of the next word
    pub fn move_word_forward(&mut self) {
        if let Some(line) = self.lines.get(self.cursor_row) {
            let col = line.floor_char_boundary(self.cursor_col);
            let rest = &line[col..];
            let mut found_ws = false;

            for (offset, c) in rest.char_indices() {
                if c.is_whitespace() {
                    found_ws = true;
                } else if found_ws {
                    self.cursor_col = col + offset;
                    return;
                }
            }

            // No next word found — move to end
            self.cursor_col = line.len();
        }
    }

    /// Move cursor up (multi-line mode)
    ///
    /// Preserves visual column position when possible, using display width.
    pub fn move_cursor_up(&mut self) {
        if self.cursor_row > 0 {
            // Get current display column
            let current_display_col = self.cursor_display_col();

            self.cursor_row -= 1;

            // Try to preserve display column position
            if let Some(line) = self.lines.get(self.cursor_row) {
                // Find the byte position that gives us the closest display column
                let mut best_col = 0;
                let mut best_diff = usize::MAX;

                for (i, _) in line.grapheme_indices(true) {
                    let display_col = display_width(&line[..i]);
                    let diff = display_col.abs_diff(current_display_col);

                    if diff < best_diff {
                        best_diff = diff;
                        best_col = i;
                    }

                    // Stop if we've gone past the target
                    if display_col > current_display_col {
                        break;
                    }
                }

                self.cursor_col = best_col;
            }
        }
    }

    /// Move cursor down (multi-line mode)
    ///
    /// Preserves visual column position when possible, using display width.
    pub fn move_cursor_down(&mut self) {
        if self.cursor_row + 1 < self.lines.len() {
            // Get current display column
            let current_display_col = self.cursor_display_col();

            self.cursor_row += 1;

            // Try to preserve display column position
            if let Some(line) = self.lines.get(self.cursor_row) {
                // Find the byte position that gives us the closest display column
                let mut best_col = 0;
                let mut best_diff = usize::MAX;

                for (i, _) in line.grapheme_indices(true) {
                    let display_col = display_width(&line[..i]);
                    let diff = display_col.abs_diff(current_display_col);

                    if diff < best_diff {
                        best_diff = diff;
                        best_col = i;
                    }

                    // Stop if we've gone past the target
                    if display_col > current_display_col {
                        break;
                    }
                }

                self.cursor_col = best_col;
            }
        }
    }

    /// Clear all input and cleanup temp files
    pub fn clear(&mut self) {
        self.mode = InputMode::SingleLine;
        self.lines = vec![String::new()];
        self.cursor_row = 0;
        self.cursor_col = 0;
        self.display_offset = 0;
        self.selection_anchor_col = None;
        self.selection_anchor_row = None;

        // Cleanup temp image files
        for img in &self.images {
            if let Err(e) = std::fs::remove_file(&img.path) {
                tracing::warn!("Failed to remove temp image file {:?}: {}", img.path, e);
            }
        }

        self.images.clear();
    }

    pub fn has_selection(&self) -> bool {
        self.selection_anchor_col.is_some() && self.selection_anchor_row.is_some()
    }

    pub fn start_selection(&mut self) {
        if self.selection_anchor_col.is_none() {
            self.selection_anchor_col = Some(self.cursor_col);
            self.selection_anchor_row = Some(self.cursor_row);
        }
    }

    pub fn clear_selection(&mut self) {
        self.selection_anchor_col = None;
        self.selection_anchor_row = None;
    }

    pub fn selection_range(&self) -> Option<(usize, usize, usize, usize)> {
        let anchor_row = self.selection_anchor_row?;
        let anchor_col = self.selection_anchor_col?;

        let (start_row, start_col, end_row, end_col) = if anchor_row < self.cursor_row {
            (anchor_row, anchor_col, self.cursor_row, self.cursor_col)
        } else if anchor_row > self.cursor_row {
            (self.cursor_row, self.cursor_col, anchor_row, anchor_col)
        } else if anchor_col <= self.cursor_col {
            (anchor_row, anchor_col, self.cursor_row, self.cursor_col)
        } else {
            (self.cursor_row, self.cursor_col, anchor_row, anchor_col)
        };

        Some((start_row, start_col, end_row, end_col))
    }

    pub fn selected_text(&self) -> Option<String> {
        let (start_row, start_col, end_row, end_col) = self.selection_range()?;

        if start_row == end_row {
            let line = self.lines.get(start_row)?;
            let start = line.floor_char_boundary(start_col.min(line.len()));
            let end = line.floor_char_boundary(end_col.min(line.len()));
            Some(line[start..end].to_string())
        } else {
            let mut result = String::new();

            if let Some(line) = self.lines.get(start_row) {
                let start = line.floor_char_boundary(start_col.min(line.len()));
                result.push_str(&line[start..]);
                result.push('\n');
            }

            for row in (start_row + 1)..end_row {
                if let Some(line) = self.lines.get(row) {
                    result.push_str(line);
                    result.push('\n');
                }
            }

            if let Some(line) = self.lines.get(end_row) {
                let end = line.floor_char_boundary(end_col.min(line.len()));
                result.push_str(&line[..end]);
            }

            Some(result)
        }
    }

    /// Check if a given byte position on a given row falls within the active selection.
    pub fn is_byte_selected(&self, row: usize, byte_idx: usize) -> bool {
        let (start_row, start_col, end_row, end_col) = match self.selection_range() {
            Some(range) => range,
            None => return false,
        };

        if row < start_row || row > end_row {
            return false;
        }
        if row == start_row && byte_idx < start_col {
            return false;
        }
        if row == end_row && byte_idx >= end_col {
            return false;
        }
        true
    }

    /// Set text content, replacing all current content
    ///
    /// Handles multi-line content by splitting on newlines.
    /// Cursor moves to the end of the last line.
    pub fn set_text(&mut self, text: &str) {
        if text.contains('\n') {
            self.mode = InputMode::MultiLine;
            self.lines = text.lines().map(|s| s.to_string()).collect();
            if self.lines.is_empty() {
                self.lines = vec![String::new()];
            }
            self.cursor_row = self.lines.len() - 1;
            self.cursor_col = self.lines.last().map(|l| l.len()).unwrap_or(0);
        } else {
            self.mode = InputMode::SingleLine;
            self.lines = vec![text.to_string()];
            self.cursor_row = 0;
            self.cursor_col = text.len();
        }
    }

    /// Insert newline at cursor position
    ///
    /// Splits the current line at the cursor position, preserving grapheme boundaries.
    pub fn insert_newline(&mut self) {
        if self.cursor_row < self.lines.len() {
            let current_line = &mut self.lines[self.cursor_row];

            // Ensure cursor is at a valid UTF-8 boundary
            let col = current_line.floor_char_boundary(self.cursor_col.min(current_line.len()));

            // Split at the validated boundary
            let before = current_line[..col].to_string();
            let after = current_line[col..].to_string();

            *current_line = before;
            self.lines.insert(self.cursor_row + 1, after);
            self.cursor_row += 1;
            self.cursor_col = 0;
        }
    }

    /// Collapse multi-line to single line
    ///
    /// Joins all lines with spaces, placing cursor at the end.
    pub fn flatten_to_single_line(&mut self) {
        let single = self.lines.join(" ");
        self.lines = vec![single];
        self.cursor_row = 0;
        self.cursor_col = self.lines[0].len();
    }

    /// Remove image by ID and cleanup temp file
    pub fn remove_image(&mut self, id: &str) -> bool {
        if let Some(pos) = self.images.iter().position(|img| img.id == id) {
            let img = self.images.remove(pos);

            // Cleanup temp file
            if let Err(e) = std::fs::remove_file(&img.path) {
                tracing::warn!("Failed to remove temp image file {:?}: {}", img.path, e);
            }

            true
        } else {
            false
        }
    }

    /// Cleanup all temp files (call on exit)
    pub fn cleanup(&mut self) {
        for img in &self.images {
            if let Err(e) = std::fs::remove_file(&img.path) {
                tracing::warn!("Failed to remove temp image file {:?}: {}", img.path, e);
            }
        }
        self.images.clear();
    }
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_input_state_new() {
        let state = InputState::new();
        assert_eq!(state.mode, InputMode::SingleLine);
        assert_eq!(state.lines.len(), 1);
        assert_eq!(state.lines[0], "");
        assert_eq!(state.cursor_row, 0);
        assert_eq!(state.cursor_col, 0);
    }

    #[test]
    fn test_insert_char() {
        let mut state = InputState::new();
        state.insert_char('H');
        state.insert_char('i');
        assert_eq!(state.lines[0], "Hi");
        assert_eq!(state.cursor_col, 2);
    }

    #[test]
    fn test_backspace() {
        let mut state = InputState::new();
        state.lines[0] = "Hello".to_string();
        state.cursor_col = 5;
        state.backspace();
        assert_eq!(state.lines[0], "Hell");
        assert_eq!(state.cursor_col, 4);
    }

    #[test]
    fn test_insert_newline() {
        let mut state = InputState::new();
        state.lines[0] = "Hello World".to_string();
        state.cursor_col = 5;
        state.insert_newline();
        assert_eq!(state.lines.len(), 2);
        assert_eq!(state.lines[0], "Hello");
        assert_eq!(state.lines[1], " World");
        assert_eq!(state.cursor_row, 1);
        assert_eq!(state.cursor_col, 0);
    }

    #[test]
    fn test_flatten_to_single_line() {
        let mut state = InputState::new();
        state.lines = vec!["Line 1".to_string(), "Line 2".to_string()];
        state.flatten_to_single_line();
        assert_eq!(state.lines.len(), 1);
        assert_eq!(state.lines[0], "Line 1 Line 2");
        assert_eq!(state.cursor_row, 0);
    }

    #[test]
    fn test_multiline_navigation() {
        let mut state = InputState::new();
        state.mode = InputMode::MultiLine;
        state.lines = vec![
            "Line 1".to_string(),
            "Line 2".to_string(),
            "Line 3".to_string(),
        ];
        state.cursor_row = 1;
        state.cursor_col = 3;

        state.move_cursor_up();
        assert_eq!(state.cursor_row, 0);
        assert_eq!(state.cursor_col, 3); // Clamped to line length

        state.move_cursor_down();
        assert_eq!(state.cursor_row, 1);

        state.move_cursor_down();
        assert_eq!(state.cursor_row, 2);

        // Can't go past end
        state.move_cursor_down();
        assert_eq!(state.cursor_row, 2);
    }

    #[test]
    fn test_clear() {
        let mut state = InputState::new();
        state.mode = InputMode::MultiLine;
        state.lines = vec!["Line 1".to_string(), "Line 2".to_string()];
        state.cursor_row = 1;
        state.cursor_col = 3;
        state.images.push(ImageAttachment {
            id: "test".to_string(),
            path: PathBuf::from("/tmp/test.png"),
            mime_type: "image/png".to_string(),
            preview: Some("preview".to_string()),
            data_base64: None,
            width: None,
            height: None,
        });

        state.clear();

        assert_eq!(state.mode, InputMode::SingleLine);
        assert_eq!(state.lines.len(), 1);
        assert_eq!(state.lines[0], "");
        assert_eq!(state.cursor_row, 0);
        assert_eq!(state.cursor_col, 0);
        assert_eq!(state.images.len(), 0);
    }

    #[test]
    fn test_remove_image() {
        let mut state = InputState::new();
        state.images.push(ImageAttachment {
            id: "img1".to_string(),
            path: PathBuf::from("/tmp/img1.png"),
            mime_type: "image/png".to_string(),
            preview: Some("preview1".to_string()),
            data_base64: None,
            width: None,
            height: None,
        });
        state.images.push(ImageAttachment {
            id: "img2".to_string(),
            path: PathBuf::from("/tmp/img2.png"),
            mime_type: "image/png".to_string(),
            preview: Some("preview2".to_string()),
            data_base64: None,
            width: None,
            height: None,
        });

        assert!(state.remove_image("img1"));
        assert_eq!(state.images.len(), 1);
        assert_eq!(state.images[0].id, "img2");

        assert!(!state.remove_image("nonexistent"));
        assert_eq!(state.images.len(), 1);
    }

    #[test]
    fn test_all_text() {
        let mut state = InputState::new();
        state.lines = vec![
            "Line 1".to_string(),
            "Line 2".to_string(),
            "Line 3".to_string(),
        ];
        assert_eq!(state.all_text(), "Line 1\nLine 2\nLine 3");
    }

    #[test]
    fn test_current_line() {
        let mut state = InputState::new();
        state.lines = vec!["Line 1".to_string(), "Line 2".to_string()];
        state.cursor_row = 1;
        assert_eq!(state.current_line(), "Line 2");
    }

    #[test]
    fn test_move_cursor_left_right() {
        let mut state = InputState::new();
        state.lines[0] = "Hello".to_string();
        state.cursor_col = 5;

        state.move_cursor_left();
        assert_eq!(state.cursor_col, 4);

        state.move_cursor_right();
        assert_eq!(state.cursor_col, 5);

        // Can't go past end
        state.move_cursor_right();
        assert_eq!(state.cursor_col, 5);

        // Can't go past start
        state.cursor_col = 0;
        state.move_cursor_left();
        assert_eq!(state.cursor_col, 0);
    }

    #[test]
    fn test_delete() {
        let mut state = InputState::new();
        state.lines[0] = "Hello".to_string();
        state.cursor_col = 1;

        state.delete();
        assert_eq!(state.lines[0], "Hllo");
        assert_eq!(state.cursor_col, 1);
    }

    #[test]
    fn test_backspace_thai() {
        let mut state = InputState::new();
        // Thai greeting: สวัสดี (sawatdee)
        // Unicode treats this as 4 graphemes: ส, วั (ว + combining vowel), ส, ดี (ด + combining vowel)
        state.lines[0] = "สวัสดี".to_string();
        state.cursor_col = state.lines[0].len();

        // Delete last Thai grapheme cluster (ดี = consonant ด + vowel ี)
        state.backspace();
        // Result should be สวัส (3 graphemes)
        assert_eq!(state.lines[0], "สวัส");
        assert_eq!(state.cursor_col, "สวัส".len());
    }

    #[test]
    fn test_delete_thai() {
        let mut state = InputState::new();
        // Thai greeting: สวัสดี (sawatdee)
        state.lines[0] = "สวัสดี".to_string();
        state.cursor_col = 0;

        // Delete first Thai character (should delete entire grapheme)
        state.delete();
        assert_eq!(state.lines[0], "วัสดี");
        assert_eq!(state.cursor_col, 0);
    }

    #[test]
    fn test_insert_newline_at_beginning() {
        let mut state = InputState::new();
        state.lines[0] = "hello".to_string();
        state.cursor_col = 0;

        state.insert_newline();
        assert_eq!(state.lines.len(), 2);
        assert_eq!(state.lines[0], "");
        assert_eq!(state.lines[1], "hello");
        assert_eq!(state.cursor_row, 1);
        assert_eq!(state.cursor_col, 0);
    }

    #[test]
    fn test_insert_newline_at_end() {
        let mut state = InputState::new();
        state.lines[0] = "hello".to_string();
        state.cursor_col = 5;

        state.insert_newline();
        assert_eq!(state.lines.len(), 2);
        assert_eq!(state.lines[0], "hello");
        assert_eq!(state.lines[1], "");
        assert_eq!(state.cursor_row, 1);
        assert_eq!(state.cursor_col, 0);
    }

    #[test]
    fn test_insert_newline_at_middle() {
        let mut state = InputState::new();
        state.lines[0] = "hello world".to_string();
        state.cursor_col = 5;

        state.insert_newline();
        assert_eq!(state.lines.len(), 2);
        assert_eq!(state.lines[0], "hello");
        assert_eq!(state.lines[1], " world");
        assert_eq!(state.cursor_row, 1);
        assert_eq!(state.cursor_col, 0);
    }

    #[test]
    fn test_insert_newline_cursor_beyond_line_clamped() {
        let mut state = InputState::new();
        state.lines[0] = "hi".to_string();
        state.cursor_col = 100; // far beyond line length

        state.insert_newline();
        // Should clamp to end of line
        assert_eq!(state.lines[0], "hi");
        assert_eq!(state.lines[1], "");
        assert_eq!(state.cursor_row, 1);
        assert_eq!(state.cursor_col, 0);
    }

    #[test]
    fn test_multiple_newlines() {
        let mut state = InputState::new();
        state.set_text("abc");
        state.cursor_col = 1;

        state.insert_newline(); // a|bc -> a\n bc
        state.insert_newline(); // (empty)|\n bc -> \n\n bc
        assert_eq!(state.lines.len(), 3);
        assert_eq!(state.lines[0], "a");
        assert_eq!(state.lines[1], "");
        assert_eq!(state.lines[2], "bc");
    }

    // === UTF-8 boundary safety tests ===
    //
    // These tests verify that methods using cursor_col as a byte index
    // do not panic when cursor_col lands at a non-character boundary
    // in multi-byte UTF-8 text (e.g., mid-character in Chinese or emoji).

    #[test]
    fn test_cursor_display_col_mid_byte_utf8_does_not_panic() {
        let mut state = InputState::new();
        // "abc" followed by Chinese character 你 (3 bytes: 0xE4 0xBD 0xA0)
        state.lines[0] = "abc你".to_string();
        // Set cursor_col to byte 4 -- mid-character inside 你
        state.cursor_col = 4;

        // Should not panic; should floor to byte 3 (after "abc")
        let col = state.cursor_display_col();
        assert_eq!(col, 3);
    }

    #[test]
    fn test_insert_newline_mid_byte_utf8_does_not_panic() {
        let mut state = InputState::new();
        // Emoji: 😀 is 4 bytes (0xF0 0x9F 0x98 0x80)
        state.lines[0] = "ab😀cd".to_string();
        // Set cursor_col to byte 3 -- mid-character inside the emoji
        state.cursor_col = 3;

        state.insert_newline();
        // Should not panic; should floor to byte 2 (after "ab")
        assert_eq!(state.lines[0], "ab");
        assert_eq!(state.lines[1], "😀cd");
        assert_eq!(state.cursor_row, 1);
        assert_eq!(state.cursor_col, 0);
    }

    #[test]
    fn test_insert_newline_mid_byte_chinese_does_not_panic() {
        let mut state = InputState::new();
        // 你好: 你=3 bytes, 好=3 bytes
        state.lines[0] = "你好".to_string();
        // Set cursor_col to byte 4 -- inside 好 (bytes 3..6)
        state.cursor_col = 4;

        state.insert_newline();
        // Should floor to byte 3 (after 你)
        assert_eq!(state.lines[0], "你");
        assert_eq!(state.lines[1], "好");
    }

    #[test]
    fn test_delete_word_backward_mid_byte_utf8_does_not_panic() {
        let mut state = InputState::new();
        state.lines[0] = "hello 你好 world".to_string();
        // Set cursor_col to byte 8 -- mid-character inside 你 (bytes 6..9)
        state.cursor_col = 8;

        let deleted = state.delete_word_backward();
        // Should not panic; cursor_col is floored to byte 6 (start of 你).
        // text_before = "hello " (bytes 0..6), word boundary finds space at byte 5,
        // so it deletes the space. Result: "hello你好 world"
        assert!(!deleted.is_empty());
    }

    #[test]
    fn test_cursor_display_col_emoji_mid_byte() {
        let mut state = InputState::new();
        state.lines[0] = "😀👍".to_string();
        // Set cursor to byte 6 -- inside 👍 (bytes 4..8)
        state.cursor_col = 6;

        // Should not panic; floors to byte 4 (after 😀)
        let col = state.cursor_display_col();
        assert_eq!(col, 2); // 😀 has display width 2
    }

    #[test]
    fn test_cursor_col_beyond_line_len_clamps_gracefully() {
        let mut state = InputState::new();
        state.lines[0] = "你好".to_string(); // 6 bytes total
        state.cursor_col = 100; // Way beyond

        state.insert_newline();
        assert_eq!(state.lines[0], "你好");
        assert_eq!(state.lines[1], "");
    }

    // === Horizontal scroll (display_offset) tests ===

    #[test]
    fn test_recalc_display_offset_no_scroll_when_text_fits() {
        let mut state = InputState::new();
        state.lines[0] = "hello".to_string();
        state.cursor_col = 5;
        state.display_offset = 0;
        state.recalc_display_offset(80);
        assert_eq!(state.display_offset, 0);
    }

    #[test]
    fn test_recalc_display_offset_scrolls_when_cursor_past_edge() {
        let mut state = InputState::new();
        // 100 chars, cursor at end
        state.lines[0] = "a".repeat(100);
        state.cursor_col = 100;
        state.display_offset = 0;
        // Only 60 cols visible
        state.recalc_display_offset(60);
        // Should scroll so cursor is visible: offset = 100 - 59 = 41
        assert_eq!(state.display_offset, 41);
    }

    #[test]
    fn test_recalc_display_offset_doesnt_overscroll() {
        let mut state = InputState::new();
        state.lines[0] = "a".repeat(100);
        state.cursor_col = 50;
        state.display_offset = 0;
        state.recalc_display_offset(60);
        // Cursor at col 50, visible width 60: fits without scroll
        assert_eq!(state.display_offset, 0);
    }

    #[test]
    fn test_scroll_byte_offset_ascii() {
        let mut state = InputState::new();
        state.lines[0] = "abcdefghij".to_string();
        state.display_offset = 5;
        let byte_off = state.scroll_byte_offset();
        assert_eq!(byte_off, 5);
    }

    #[test]
    fn test_scroll_byte_offset_cjk() {
        let mut state = InputState::new();
        // 你=2 display cols each, 你好你好 = 8 display cols
        state.lines[0] = "你好你好".to_string();
        state.display_offset = 4; // skip first 2 chars (你好)
        let byte_off = state.scroll_byte_offset();
        assert_eq!(byte_off, 6); // 你好 = 6 bytes
    }

    #[test]
    fn test_relative_cursor_display_col() {
        let mut state = InputState::new();
        state.lines[0] = "a".repeat(100);
        state.cursor_col = 80;
        state.display_offset = 40;
        assert_eq!(state.relative_cursor_display_col(), 40);
    }

    // === set_text with newlines (Bug #2 fix) ===

    #[test]
    fn test_set_text_single_line() {
        let mut state = InputState::new();
        state.set_text("hello world");
        assert_eq!(state.lines, vec!["hello world"]);
        assert_eq!(state.cursor_col, 11);
        assert_eq!(state.cursor_row, 0);
    }

    #[test]
    fn test_set_text_preserves_newlines() {
        let mut state = InputState::new();
        state.set_text("line one\nline two\nline three");
        assert_eq!(state.lines, vec!["line one", "line two", "line three"]);
        assert_eq!(state.cursor_row, 2);
        assert_eq!(state.cursor_col, 10);
        assert_eq!(state.mode, InputMode::MultiLine);
    }

    #[test]
    fn test_set_text_trailing_newline() {
        let mut state = InputState::new();
        // str::lines() strips trailing newline, so "hello\nworld\n" -> ["hello", "world"]
        state.set_text("hello\nworld\n");
        assert_eq!(state.lines, vec!["hello", "world"]);
        assert_eq!(state.cursor_row, 1);
        assert_eq!(state.cursor_col, 5);
    }

    // ── Selection API tests ────────────────────────────────────────────────────

    // start_selection / clear_selection / has_selection

    #[test]
    fn test_selection_start_sets_anchor() {
        let mut state = InputState::new();
        state.lines[0] = "Hello World".to_string();
        state.cursor_col = 5;
        state.cursor_row = 0;
        state.start_selection();
        assert_eq!(state.selection_anchor_col, Some(5));
        assert_eq!(state.selection_anchor_row, Some(0));
    }

    #[test]
    fn test_selection_start_idempotent() {
        let mut state = InputState::new();
        state.lines[0] = "Hello World".to_string();
        state.cursor_col = 2;
        state.cursor_row = 0;
        state.start_selection();
        state.cursor_col = 7;
        state.start_selection();
        assert_eq!(state.selection_anchor_col, Some(2));
        assert_eq!(state.selection_anchor_row, Some(0));
    }

    #[test]
    fn test_selection_clear_resets() {
        let mut state = InputState::new();
        state.cursor_col = 4;
        state.start_selection();
        assert!(state.has_selection());
        state.clear_selection();
        assert_eq!(state.selection_anchor_col, None);
        assert_eq!(state.selection_anchor_row, None);
    }

    #[test]
    fn test_selection_has_selection_false_initially() {
        let state = InputState::new();
        assert!(!state.has_selection());
    }

    #[test]
    fn test_selection_has_selection_true_after_start() {
        let mut state = InputState::new();
        state.start_selection();
        assert!(state.has_selection());
    }

    // get_selection_range (normalized: returns (start_row, start_col, end_row, end_col))

    #[test]
    fn test_selection_range_single_line_forward() {
        let mut state = InputState::new();
        state.lines[0] = "Hello World".to_string();
        state.cursor_col = 2;
        state.start_selection();
        state.cursor_col = 5;
        assert_eq!(state.selection_range(), Some((0, 2, 0, 5)));
    }

    #[test]
    fn test_selection_range_single_line_backward() {
        let mut state = InputState::new();
        state.lines[0] = "Hello World".to_string();
        state.cursor_col = 5;
        state.start_selection();
        state.cursor_col = 2;
        assert_eq!(state.selection_range(), Some((0, 2, 0, 5)));
    }

    #[test]
    fn test_selection_range_multi_line_forward() {
        let mut state = InputState::new();
        state.lines = vec![
            "Line0".to_string(),
            "Line1".to_string(),
            "Line2".to_string(),
        ];
        state.cursor_row = 0;
        state.cursor_col = 3;
        state.start_selection();
        state.cursor_row = 2;
        state.cursor_col = 1;
        assert_eq!(state.selection_range(), Some((0, 3, 2, 1)));
    }

    #[test]
    fn test_selection_range_multi_line_backward() {
        let mut state = InputState::new();
        state.lines = vec![
            "Line0".to_string(),
            "Line1".to_string(),
            "Line2".to_string(),
        ];
        state.cursor_row = 2;
        state.cursor_col = 1;
        state.start_selection();
        state.cursor_row = 0;
        state.cursor_col = 3;
        assert_eq!(state.selection_range(), Some((0, 3, 2, 1)));
    }

    #[test]
    fn test_selection_range_none_when_no_selection() {
        let state = InputState::new();
        assert_eq!(state.selection_range(), None);
    }

    #[test]
    fn test_selection_range_same_position() {
        let mut state = InputState::new();
        state.lines[0] = "Hello".to_string();
        state.cursor_col = 3;
        state.cursor_row = 0;
        state.start_selection();
        assert_eq!(state.selection_range(), Some((0, 3, 0, 3)));
    }

    // get_selected_text

    #[test]
    fn test_selection_text_single_line() {
        let mut state = InputState::new();
        state.lines[0] = "Hello World".to_string();
        state.cursor_col = 2;
        state.start_selection();
        state.cursor_col = 7;
        assert_eq!(state.selected_text(), Some("llo W".to_string()));
    }

    #[test]
    fn test_selection_text_entire_line() {
        let mut state = InputState::new();
        state.lines[0] = "Hello".to_string();
        state.cursor_col = 0;
        state.start_selection();
        state.cursor_col = 5;
        assert_eq!(state.selected_text(), Some("Hello".to_string()));
    }

    #[test]
    fn test_selection_text_multi_line() {
        let mut state = InputState::new();
        state.lines = vec!["Hello".to_string(), "World".to_string(), "Foo".to_string()];
        state.cursor_row = 0;
        state.cursor_col = 2;
        state.start_selection();
        state.cursor_row = 2;
        state.cursor_col = 1;
        assert_eq!(state.selected_text(), Some("llo\nWorld\nF".to_string()));
    }

    #[test]
    fn test_selection_text_adjacent_lines() {
        let mut state = InputState::new();
        state.lines = vec!["Hello".to_string(), "World".to_string()];
        state.cursor_row = 0;
        state.cursor_col = 3;
        state.start_selection();
        state.cursor_row = 1;
        state.cursor_col = 2;
        assert_eq!(state.selected_text(), Some("lo\nWo".to_string()));
    }

    #[test]
    fn test_selection_text_none_when_no_selection() {
        let state = InputState::new();
        assert_eq!(state.selected_text(), None);
    }

    #[test]
    fn test_selection_text_empty_input() {
        let state = InputState::new();
        assert_eq!(state.selected_text(), None);
    }

    // is_byte_selected

    #[test]
    fn test_is_byte_selected_within_range() {
        let mut state = InputState::new();
        state.lines[0] = "Hello World".to_string();
        state.cursor_col = 2;
        state.start_selection();
        state.cursor_col = 7;
        assert!(state.is_byte_selected(0, 3));
        assert!(state.is_byte_selected(0, 4));
        assert!(state.is_byte_selected(0, 6));
    }

    #[test]
    fn test_is_byte_selected_before_range() {
        let mut state = InputState::new();
        state.lines[0] = "Hello World".to_string();
        state.cursor_col = 2;
        state.start_selection();
        state.cursor_col = 7;
        assert!(!state.is_byte_selected(0, 1));
    }

    #[test]
    fn test_is_byte_selected_after_range() {
        let mut state = InputState::new();
        state.lines[0] = "Hello World".to_string();
        state.cursor_col = 2;
        state.start_selection();
        state.cursor_col = 7;
        assert!(!state.is_byte_selected(0, 7));
    }

    #[test]
    fn test_is_byte_selected_on_boundary() {
        let mut state = InputState::new();
        state.lines[0] = "Hello World".to_string();
        state.cursor_col = 2;
        state.start_selection();
        state.cursor_col = 7;
        assert!(state.is_byte_selected(0, 2));
    }

    #[test]
    fn test_is_byte_selected_no_selection() {
        let state = InputState::new();
        assert!(!state.is_byte_selected(0, 0));
    }

    #[test]
    fn test_is_byte_selected_multi_line() {
        let mut state = InputState::new();
        state.lines = vec!["Hello".to_string(), "World".to_string(), "Foo".to_string()];
        state.cursor_row = 0;
        state.cursor_col = 3;
        state.start_selection();
        state.cursor_row = 2;
        state.cursor_col = 2;
        assert!(!state.is_byte_selected(0, 2));
        assert!(state.is_byte_selected(0, 3));
        assert!(state.is_byte_selected(0, 4));
        assert!(state.is_byte_selected(1, 0));
        assert!(state.is_byte_selected(1, 4));
        assert!(state.is_byte_selected(2, 0));
        assert!(state.is_byte_selected(2, 1));
        assert!(!state.is_byte_selected(2, 2));
        assert!(!state.is_byte_selected(3, 0));
    }

    // Edge cases

    #[test]
    fn test_selection_with_unicode_text() {
        let mut state = InputState::new();
        // 'é' is 2 bytes UTF-8; byte 0='H', bytes 1-2='é', byte 3='l', byte 4='l', byte 5='o'
        state.lines[0] = "Héllo Wörld".to_string();
        state.cursor_col = 3;
        state.start_selection();
        state.cursor_col = 6;
        assert_eq!(state.selected_text(), Some("llo".to_string()));
        assert!(state.is_byte_selected(0, 3));
        assert!(state.is_byte_selected(0, 5));
        assert!(!state.is_byte_selected(0, 6));
    }

    #[test]
    fn test_selection_cursor_beyond_line_length() {
        let mut state = InputState::new();
        state.lines[0] = "Hi".to_string();
        state.cursor_col = 0;
        state.start_selection();
        state.cursor_col = 100;
        assert_eq!(state.selected_text(), Some("Hi".to_string()));
    }
}
