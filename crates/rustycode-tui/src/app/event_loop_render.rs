//! Rendering helpers for the event loop
//!
//! Extracted from event_loop.rs to separate rendering logic from the main loop.

use crate::ui::message::Message;

/// Calculate the range of visible messages based on scroll offset
pub fn calculate_visible_range(
    message_count: usize,
    scroll_offset: usize,
    viewport_height: usize,
) -> std::ops::Range<usize> {
    if message_count == 0 {
        return 0..0;
    }

    // Simple approximation: assume each message is at least 3 lines
    let avg_msg_height = 3;
    let visible_count = (viewport_height / avg_msg_height).max(1);

    let start = scroll_offset.min(message_count.saturating_sub(1));
    let end = (start + visible_count).min(message_count);

    start..end
}

/// Estimate message height for scrolling calculations
pub fn estimate_message_height(message: &Message, _width: usize) -> usize {
    // Base height: header + content
    let content_lines = message.content.lines().count().max(1);
    let mut height = 1 + content_lines;

    // Add tool summary height
    if message.has_tools() {
        height += match message.tools_expansion {
            crate::ui::message::ExpansionLevel::Collapsed => 1,
            crate::ui::message::ExpansionLevel::Expanded => 2 + message.tool_count(),
            crate::ui::message::ExpansionLevel::Deep => 2 + message.tool_count() + 4,
        };
    }

    // Add thinking height
    if message.has_thinking() {
        height += match message.thinking_expansion {
            crate::ui::message::ExpansionLevel::Collapsed => 1,
            _ => {
                2 + message
                    .thinking
                    .as_ref()
                    .map(|t| t.lines().count())
                    .unwrap_or(0)
                    .min(8)
            }
        };
    }

    height.max(3) // Minimum height
}

/// Viewport state for rendering calculations
#[derive(Debug, Clone)]
pub struct ViewportState {
    pub height: usize,
}

impl ViewportState {
    pub fn new() -> Self {
        Self { height: 20 }
    }

    pub fn update(&mut self, height: usize, message_count: usize, scroll_offset: &mut usize) {
        self.height = height;
        // Adjust scroll offset if needed
        let max_offset = message_count.saturating_sub(self.height);
        if *scroll_offset > max_offset {
            *scroll_offset = max_offset;
        }
    }
}

impl Default for ViewportState {
    fn default() -> Self {
        Self::new()
    }
}

/// Auto-scroll helper to scroll to the latest message
pub fn auto_scroll_to_latest(message_count: usize, viewport_height: usize) -> (usize, usize) {
    if message_count > 0 {
        let selected_message = message_count - 1;
        let scroll_offset = if viewport_height > 0 {
            selected_message.saturating_sub(viewport_height - 1)
        } else {
            0
        };
        (selected_message, scroll_offset)
    } else {
        (0, 0)
    }
}
