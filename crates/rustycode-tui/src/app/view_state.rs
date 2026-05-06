use ratatui::layout::Rect;
use std::cell::Cell;
use std::time::{Duration, Instant};

/// Duration to suppress auto-scroll after the user manually scrolls
const AUTO_SCROLL_SUPPRESS: Duration = Duration::from_secs(2);

/// Viewport and scroll state for the message list.
///
/// Groups the scroll, selection, and mouse-selection fields extracted from
/// the TUI god object so the view concerns have a single owner.
#[derive(Debug)]
#[non_exhaustive]
pub struct ViewState {
    /// Current scroll position in lines
    pub scroll_offset_line: usize,
    /// Index of the currently selected/highlighted message
    pub selected_message: usize,
    /// Visible height of the message area (in terminal rows)
    pub viewport_height: usize,
    /// Total rendered lines from the last frame (used for scroll clamping)
    pub last_total_lines: Cell<usize>,
    /// Bounding rect of the messages area (for click detection)
    pub messages_area: Cell<Rect>,
    /// Start position of an active mouse text selection
    pub mouse_selection_start: Cell<Option<(u16, u16)>>,
    /// Whether the mouse dragged during this selection
    pub mouse_selection_dragged: Cell<bool>,
    /// True when the user has scrolled up away from the bottom
    pub user_scrolled: bool,
    /// When the user last scrolled — auto-scroll suppressed while recent
    pub last_user_scroll_time: Instant,
}

impl ViewState {
    pub fn new() -> Self {
        Self {
            scroll_offset_line: 0,
            selected_message: 0,
            viewport_height: 0,
            last_total_lines: Cell::new(0),
            messages_area: Cell::new(Rect::default()),
            mouse_selection_start: Cell::new(None),
            mouse_selection_dragged: Cell::new(false),
            user_scrolled: false,
            last_user_scroll_time: Instant::now(),
        }
    }

    /// Mark that the user manually scrolled — suppresses auto-scroll for a
    /// short window so the UI doesn't yank back to the bottom immediately.
    pub fn mark_user_scrolled(&mut self) {
        self.user_scrolled = true;
        self.last_user_scroll_time = Instant::now();
    }

    /// Check whether auto-scroll should be suppressed because the user
    /// recently scrolled up.
    pub fn should_suppress_auto_scroll(&self) -> bool {
        self.user_scrolled && self.last_user_scroll_time.elapsed() < AUTO_SCROLL_SUPPRESS
    }

    /// Reset user-scroll state so auto-scroll resumes immediately.
    pub fn clear_user_scroll(&mut self) {
        self.user_scrolled = false;
    }

    /// Clamp scroll position so it never exceeds the total line count.
    pub fn clamp_scroll(&mut self, total_lines: usize) {
        if total_lines > self.viewport_height {
            let max_scroll = total_lines - self.viewport_height;
            if self.scroll_offset_line > max_scroll {
                self.scroll_offset_line = max_scroll;
            }
        } else {
            self.scroll_offset_line = 0;
        }
    }
}

impl Default for ViewState {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_initializes_defaults() {
        let vs = ViewState::new();
        assert_eq!(vs.scroll_offset_line, 0);
        assert_eq!(vs.selected_message, 0);
        assert_eq!(vs.viewport_height, 0);
        assert!(!vs.user_scrolled);
    }

    #[test]
    fn mark_user_scrolled_sets_flag() {
        let mut vs = ViewState::new();
        assert!(!vs.user_scrolled);
        vs.mark_user_scrolled();
        assert!(vs.user_scrolled);
        assert!(vs.should_suppress_auto_scroll());
    }

    #[test]
    fn clear_user_scroll_resets() {
        let mut vs = ViewState::new();
        vs.mark_user_scrolled();
        assert!(vs.should_suppress_auto_scroll());
        vs.clear_user_scroll();
        assert!(!vs.should_suppress_auto_scroll());
    }

    #[test]
    fn clamp_scroll_within_bounds() {
        let mut vs = ViewState::new();
        vs.viewport_height = 10;
        vs.scroll_offset_line = 95;
        vs.clamp_scroll(100);
        assert_eq!(vs.scroll_offset_line, 90); // 100 - 10
    }

    #[test]
    fn clamp_scroll_already_valid() {
        let mut vs = ViewState::new();
        vs.viewport_height = 10;
        vs.scroll_offset_line = 30;
        vs.clamp_scroll(100);
        assert_eq!(vs.scroll_offset_line, 30); // unchanged
    }

    #[test]
    fn clamp_scroll_when_content_fits_viewport() {
        let mut vs = ViewState::new();
        vs.viewport_height = 100;
        vs.scroll_offset_line = 50;
        vs.clamp_scroll(50); // total lines <= viewport
        assert_eq!(vs.scroll_offset_line, 0);
    }

    #[test]
    fn messages_area_cell_roundtrip() {
        let vs = ViewState::new();
        let rect = Rect::new(5, 10, 80, 24);
        vs.messages_area.set(rect);
        assert_eq!(vs.messages_area.get(), rect);
    }
}
