//! Mouse input handling
//!
//! Handles mouse scroll events and click interactions.
//!
//! # Drag Selection
//!
//! Mouse drag selection is handled by the app so we can keep copies panel-aware:
//! - Drag inside the transcript copies transcript text
//! - Drag inside the sidebar copies sidebar text
//! - Scroll wheel still works via captured events

use crate::app::event_loop::TUI;
use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};

impl TUI {
    fn scroll_help_by(&mut self, lines: usize, down: bool) {
        if down {
            self.help_state.scroll_offset = self.help_state.scroll_offset.saturating_add(lines);
        } else {
            self.help_state.scroll_offset = self.help_state.scroll_offset.saturating_sub(lines);
        }
        self.dirty = true;
    }

    fn scroll_tool_result_by(&mut self, lines: usize, down: bool) {
        if down {
            self.tool_result_scroll_offset = self.tool_result_scroll_offset.saturating_add(lines);
        } else {
            self.tool_result_scroll_offset = self.tool_result_scroll_offset.saturating_sub(lines);
        }
        self.dirty = true;
    }

    /// Handle mouse scroll events with position-aware routing.
    ///
    /// Route wheel input to the most specific visible scrollable surface under the pointer.
    /// The sidebar is visible by default, so it must only consume wheel input when the
    /// cursor is actually over the sidebar area.
    pub(crate) fn handle_mouse_scroll(&mut self, mouse: MouseEvent) {
        let scroll_speed = self.tui_config.behavior.get_mouse_scroll_speed();

        if self.showing_tool_result {
            match mouse.kind {
                MouseEventKind::ScrollUp => {
                    self.scroll_tool_result_by(scroll_speed as usize, false);
                }
                MouseEventKind::ScrollDown => {
                    self.scroll_tool_result_by(scroll_speed as usize, true);
                }
                _ => {}
            }
            return;
        }

        if self.help_state.visible {
            match mouse.kind {
                MouseEventKind::ScrollUp => self.scroll_help_by(scroll_speed as usize, false),
                MouseEventKind::ScrollDown => self.scroll_help_by(scroll_speed as usize, true),
                _ => {}
            }
            return;
        }

        let sidebar_area = self.sidebar_area.get();
        let mouse_in_sidebar = self.session_sidebar.is_visible()
            && Self::mouse_point_in_area((mouse.column, mouse.row), sidebar_area);

        if mouse_in_sidebar {
            match mouse.kind {
                MouseEventKind::ScrollUp => {
                    for _ in 0..scroll_speed {
                        self.session_sidebar.scroll_up();
                    }
                    self.dirty = true;
                }
                MouseEventKind::ScrollDown => {
                    for _ in 0..scroll_speed {
                        self.session_sidebar.scroll_down();
                    }
                    self.dirty = true;
                }
                _ => {}
            }
            return;
        }

        match mouse.kind {
            MouseEventKind::ScrollUp => {
                self.scroll_up_by(scroll_speed as usize);
            }
            MouseEventKind::ScrollDown => {
                self.scroll_down_by(scroll_speed as usize);
            }
            _ => {}
        }
    }

    fn mouse_point_in_area(point: (u16, u16), area: ratatui::layout::Rect) -> bool {
        let (col, row) = point;
        col >= area.x && col < area.x + area.width && row >= area.y && row < area.y + area.height
    }

    fn mouse_message_index_at(&self, col: u16, row: u16) -> Option<usize> {
        self.message_areas
            .borrow()
            .iter()
            .find(|(_, rect)| Self::mouse_point_in_area((col, row), *rect))
            .map(|(idx, _)| *idx)
    }

    fn handle_mouse_selection_start(&self, mouse: MouseEvent) {
        self.mouse_selection_start
            .set(Some((mouse.column, mouse.row)));
        self.mouse_selection_dragged.set(false);
    }

    fn handle_mouse_selection_drag(&self, mouse: MouseEvent) {
        if self.mouse_selection_start.get().is_some() {
            self.mouse_selection_dragged.set(true);
        } else {
            self.mouse_selection_start
                .set(Some((mouse.column, mouse.row)));
        }
    }

    fn finish_mouse_selection(&mut self, mouse: MouseEvent) {
        let Some(start) = self.mouse_selection_start.get() else {
            return;
        };

        let end = (mouse.column, mouse.row);
        let dragged = self.mouse_selection_dragged.get() || start != end;
        self.mouse_selection_start.set(None);
        self.mouse_selection_dragged.set(false);

        if !dragged {
            self.handle_mouse_click(mouse);
            return;
        }

        let sidebar_area = self.sidebar_area.get();
        if Self::mouse_point_in_area(start, sidebar_area) {
            if let Err(e) = self.copy_sidebar_text() {
                tracing::error!("Failed to copy sidebar selection: {}", e);
            }
            return;
        }

        let message_area = self.messages_area.get();
        if Self::mouse_point_in_area(start, message_area) {
            let start_idx = self.mouse_message_index_at(start.0, start.1);
            let end_idx = self.mouse_message_index_at(end.0, end.1);

            match (start_idx, end_idx) {
                (Some(a), Some(b)) => {
                    let (lo, hi) = if a <= b { (a, b) } else { (b, a) };
                    if let Err(e) = self.copy_message_range(lo, hi) {
                        tracing::error!("Failed to copy transcript selection: {}", e);
                    }
                }
                (Some(idx), None) | (None, Some(idx)) => {
                    if let Err(e) = self.copy_message_range(idx, idx) {
                        tracing::error!("Failed to copy transcript selection: {}", e);
                    }
                }
                (None, None) => {}
            }
        }
    }

    /// Handle mouse click — toggle collapse on clicked message
    #[allow(dead_code)]
    pub(crate) fn handle_mouse_click(&mut self, mouse: MouseEvent) {
        let (col, row) = (mouse.column, mouse.row);

        // Check if click is on the scroll-to-bottom indicator
        if self.user_scrolled {
            let msg_area = self.messages_area.get();
            let bottom_row = msg_area.y + msg_area.height.saturating_sub(1);
            if row == bottom_row && col >= msg_area.x && col < msg_area.x + msg_area.width {
                // Click on scroll-to-bottom indicator — jump to bottom
                self.user_scrolled = false;
                self.auto_scroll();
                self.dirty = true;
                return;
            }
        }

        // Find which message was clicked
        let areas = self.message_areas.borrow();
        if let Some(&(msg_idx, _)) = areas
            .iter()
            .find(|(_, rect)| self.point_in_rect((col, row), *rect))
        {
            drop(areas); // Release borrow before mutating messages
            if msg_idx < self.messages.len() {
                // Update selection so keyboard navigation continues from clicked position
                self.selected_message = msg_idx;
                let msg = &mut self.messages[msg_idx];
                // Toggle tool expansion for assistant messages with tools
                // Toggle collapse for all other messages (user and assistant without tools)
                if msg.role == crate::ui::message::MessageRole::Assistant
                    && msg.tool_executions.as_ref().is_some_and(|t| !t.is_empty())
                {
                    msg.tools_expansion = match msg.tools_expansion {
                        crate::ui::message::ExpansionLevel::Collapsed => {
                            crate::ui::message::ExpansionLevel::Expanded
                        }
                        _ => crate::ui::message::ExpansionLevel::Collapsed,
                    };
                } else {
                    msg.collapsed = !msg.collapsed;
                }
                self.dirty = true;
            }
        }
    }

    pub(crate) fn handle_mouse_input(&mut self, mouse: MouseEvent) {
        match mouse.kind {
            MouseEventKind::Down(MouseButton::Left) => self.handle_mouse_selection_start(mouse),
            MouseEventKind::Drag(MouseButton::Left) => self.handle_mouse_selection_drag(mouse),
            MouseEventKind::Up(MouseButton::Left) => self.finish_mouse_selection(mouse),
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::TUI;
    use crossterm::event::{KeyModifiers, MouseEvent, MouseEventKind};
    use ratatui::layout::Rect;

    #[test]
    fn tool_result_scroll_uses_mouse_wheel_direction() {
        let mut tui = TUI {
            showing_tool_result: true,
            tool_result_scroll_offset: 10,
            ..TUI::default()
        };

        tui.scroll_tool_result_by(3, false);
        assert_eq!(tui.tool_result_scroll_offset, 7);

        tui.scroll_tool_result_by(5, true);
        assert_eq!(tui.tool_result_scroll_offset, 12);
    }

    #[test]
    fn help_scroll_uses_mouse_wheel_direction() {
        let mut tui = TUI::default();
        tui.help_state.visible = true;
        tui.help_state.scroll_offset = 10;

        tui.scroll_help_by(4, false);
        assert_eq!(tui.help_state.scroll_offset, 6);

        tui.scroll_help_by(8, true);
        assert_eq!(tui.help_state.scroll_offset, 14);
    }

    #[test]
    fn transcript_scrolls_when_mouse_is_outside_sidebar() {
        let mut tui = TUI {
            scroll_offset_line: 30,
            viewport_height: 10,
            user_scrolled: false,
            ..TUI::default()
        };
        tui.last_total_lines.set(100);
        tui.sidebar_area.set(Rect {
            x: 0,
            y: 0,
            width: 20,
            height: 20,
        });

        let mouse = MouseEvent {
            kind: MouseEventKind::ScrollUp,
            column: 40,
            row: 5,
            modifiers: KeyModifiers::NONE,
        };

        tui.handle_mouse_scroll(mouse);

        assert!(tui.user_scrolled);
        assert_eq!(tui.scroll_offset_line, 87);
    }

    #[test]
    fn sidebar_consumes_wheel_when_mouse_is_over_it() {
        let mut tui = TUI {
            scroll_offset_line: 30,
            viewport_height: 10,
            user_scrolled: false,
            ..TUI::default()
        };
        tui.last_total_lines.set(100);
        tui.sidebar_area.set(Rect {
            x: 0,
            y: 0,
            width: 20,
            height: 20,
        });

        let mouse = MouseEvent {
            kind: MouseEventKind::ScrollUp,
            column: 5,
            row: 5,
            modifiers: KeyModifiers::NONE,
        };

        tui.handle_mouse_scroll(mouse);

        assert_eq!(tui.scroll_offset_line, 30);
    }
}
