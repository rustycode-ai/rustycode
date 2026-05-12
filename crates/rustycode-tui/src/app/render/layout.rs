//! Per-frame layout snapshot helpers.

use crate::app::event_loop::TUI;
use ratatui::layout::Rect;
use std::collections::HashMap;

/// Derived layout for a single frame.
#[derive(Debug, Clone)]
pub(crate) struct FrameLayoutSnapshot {
    pub message_area: Rect,
    pub sidebar_area: Option<Rect>,
    pub total_lines: usize,
    pub heights: Vec<usize>,
    pub chain_map: HashMap<usize, (bool, bool)>,
    pub message_line_offsets: Vec<usize>,
    pub visible_message_areas: Vec<(usize, Rect)>,
}

impl FrameLayoutSnapshot {
    /// Build a layout snapshot from the message area and precomputed heights.
    pub(crate) fn from_message_layout(
        mut message_area: Rect,
        sidebar_visible: bool,
        scroll_offset_line: usize,
        user_scrolled: bool,
        total_lines: usize,
        heights: Vec<usize>,
        chain_map: HashMap<usize, (bool, bool)>,
    ) -> Self {
        let mut sidebar_area = None;
        if sidebar_visible && message_area.width > 100 {
            let sidebar_width = (message_area.width / 3).clamp(24, 34);
            if message_area.width > sidebar_width {
                let content_width = message_area.width - sidebar_width;
                message_area.width = content_width;
                sidebar_area = Some(Rect {
                    x: message_area.x + content_width,
                    y: message_area.y,
                    width: sidebar_width,
                    height: message_area.height,
                });
            }
        }

        let viewport_height = message_area.height.max(1) as usize;
        let max_scroll = total_lines.saturating_sub(viewport_height);
        let effective_offset = if user_scrolled {
            scroll_offset_line.min(max_scroll)
        } else {
            max_scroll
        };

        let mut message_line_offsets = Vec::with_capacity(heights.len());
        let mut visible_message_areas = Vec::new();
        let mut cum_line = 0usize;

        for (msg_idx, &height) in heights.iter().enumerate() {
            message_line_offsets.push(cum_line);

            let end_line = cum_line + height;
            if end_line <= effective_offset {
                cum_line += height;
                continue;
            }
            if cum_line >= effective_offset + viewport_height {
                break;
            }

            let vis_start = cum_line.saturating_sub(effective_offset);
            let vis_end = end_line
                .saturating_sub(effective_offset)
                .min(viewport_height);
            let vis_height = vis_end.saturating_sub(vis_start) as u16;
            if vis_height > 0 {
                visible_message_areas.push((
                    msg_idx,
                    Rect {
                        x: message_area.x,
                        y: message_area.y + vis_start as u16,
                        width: message_area.width,
                        height: vis_height,
                    },
                ));
            }

            cum_line += height;
        }

        Self {
            message_area,
            sidebar_area,
            total_lines,
            heights,
            chain_map,
            message_line_offsets,
            visible_message_areas,
        }
    }

    /// Apply the snapshot back onto live TUI state in one pass.
    pub(crate) fn apply(&self, tui: &mut TUI) {
        tui.ui.view.viewport_height = self.message_area.height.max(1) as usize;
        tui.ui.view.last_total_lines.set(self.total_lines);
        tui.ui.view.messages_area.set(self.message_area);
        tui.ui.sidebar_area.set(self.sidebar_area.unwrap_or_default());

        {
            let mut offsets = tui.message_line_offsets.borrow_mut();
            offsets.clear();
            offsets.extend_from_slice(&self.message_line_offsets);
        }

        tui.clear_message_areas();
        for (msg_idx, rect) in &self.visible_message_areas {
            tui.register_message_area(*msg_idx, *rect);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::layout::Rect;

    #[test]
    fn splits_sidebar_and_tracks_visible_rows() {
        let layout = FrameLayoutSnapshot::from_message_layout(
            Rect::new(0, 1, 120, 20),
            true,
            0,
            false,
            30,
            vec![4, 5, 6],
            HashMap::from([
                (0, (false, false)),
                (1, (false, false)),
                (2, (false, false)),
            ]),
        );

        assert_eq!(layout.message_area.width, 86);
        assert_eq!(layout.message_area.height, 20);
        assert_eq!(
            layout.sidebar_area,
            Some(Rect {
                x: 86,
                y: 1,
                width: 34,
                height: 20,
            })
        );
        assert_eq!(layout.message_line_offsets, vec![0, 4, 9]);
        assert_eq!(layout.visible_message_areas.len(), 1);
    }
}
