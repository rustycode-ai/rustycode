use crate::ui::message::ToolExecution;

pub(crate) struct ToolPanelState {
    pub(crate) showing_tool_panel: bool,
    pub(crate) tool_panel_history: Vec<ToolExecution>,
    pub(crate) tool_panel_selected_index: Option<usize>,
    pub(crate) showing_tool_result: bool,
    pub(crate) tool_result_show_full: bool,
    pub(crate) tool_result_scroll_offset: usize,
}

impl ToolPanelState {
    pub(crate) fn new() -> Self {
        Self {
            showing_tool_panel: false,
            tool_panel_history: Vec::new(),
            tool_panel_selected_index: None,
            showing_tool_result: false,
            tool_result_show_full: false,
            tool_result_scroll_offset: 0,
        }
    }

    pub(crate) fn reset(&mut self) {
        self.showing_tool_panel = false;
        self.tool_panel_history.clear();
        self.tool_panel_selected_index = None;
        self.showing_tool_result = false;
        self.tool_result_show_full = false;
        self.tool_result_scroll_offset = 0;
    }

    pub(crate) fn show(&mut self) {
        self.showing_tool_panel = true;
    }

    pub(crate) fn hide(&mut self) {
        self.showing_tool_panel = false;
        self.showing_tool_result = false;
    }

    pub(crate) fn is_visible(&self) -> bool {
        self.showing_tool_panel
    }

    pub(crate) fn push_execution(&mut self, exec: ToolExecution) {
        self.tool_panel_history.push(exec);
    }

    pub(crate) fn select_last(&mut self) {
        if !self.tool_panel_history.is_empty() {
            self.tool_panel_selected_index = Some(self.tool_panel_history.len() - 1);
        }
    }

    pub(crate) fn show_result(&mut self) {
        self.showing_tool_result = true;
        self.tool_result_scroll_offset = 0;
    }

    pub(crate) fn scroll_result(&mut self, delta: i32) {
        let new_offset = if delta >= 0 {
            self.tool_result_scroll_offset
                .saturating_add(delta as usize)
        } else {
            self.tool_result_scroll_offset
                .saturating_sub((-delta) as usize)
        };
        self.tool_result_scroll_offset = new_offset;
    }

    /// Clamp the scroll offset to the valid range for `total_lines` of content.
    ///
    /// Call after rendering or when the total line count is known to prevent
    /// unbounded growth from repeated scroll-down events.
    pub(crate) fn clamp_scroll(&mut self, total_lines: usize) {
        if total_lines == 0 {
            self.tool_result_scroll_offset = 0;
        } else {
            self.tool_result_scroll_offset = self.tool_result_scroll_offset.min(total_lines - 1);
        }
    }
}

impl Default for ToolPanelState {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_starts_hidden_with_empty_history() {
        let state = ToolPanelState::new();
        assert!(!state.showing_tool_panel);
        assert!(state.tool_panel_history.is_empty());
        assert!(state.tool_panel_selected_index.is_none());
        assert!(!state.showing_tool_result);
        assert!(!state.tool_result_show_full);
        assert_eq!(state.tool_result_scroll_offset, 0);
    }

    #[test]
    fn show_makes_visible() {
        let mut state = ToolPanelState::new();
        assert!(!state.is_visible());
        state.show();
        assert!(state.is_visible());
    }

    #[test]
    fn hide_clears_panel_and_result() {
        let mut state = ToolPanelState::new();
        state.show();
        state.showing_tool_result = true;
        state.hide();
        assert!(!state.is_visible());
        assert!(!state.showing_tool_result);
    }

    #[test]
    fn push_execution_adds_to_history() {
        let mut state = ToolPanelState::new();
        state.push_execution(ToolExecution::new(
            "tool".into(),
            "read".into(),
            "summary".into(),
        ));
        assert_eq!(state.tool_panel_history.len(), 1);
    }

    #[test]
    fn select_last_sets_index() {
        let mut state = ToolPanelState::new();
        state.push_execution(ToolExecution::new("a".into(), "a".into(), "a".into()));
        state.push_execution(ToolExecution::new("b".into(), "b".into(), "b".into()));
        state.select_last();
        assert_eq!(state.tool_panel_selected_index, Some(1));
    }

    #[test]
    fn select_last_noop_on_empty() {
        let mut state = ToolPanelState::new();
        state.select_last();
        assert!(state.tool_panel_selected_index.is_none());
    }

    #[test]
    fn scroll_result_positive() {
        let mut state = ToolPanelState::new();
        state.scroll_result(5);
        assert_eq!(state.tool_result_scroll_offset, 5);
        state.scroll_result(3);
        assert_eq!(state.tool_result_scroll_offset, 8);
    }

    #[test]
    fn scroll_result_negative_clamps_to_zero() {
        let mut state = ToolPanelState::new();
        state.tool_result_scroll_offset = 3;
        state.scroll_result(-5);
        assert_eq!(state.tool_result_scroll_offset, 0);
    }

    #[test]
    fn reset_clears_history_and_selection() {
        let mut state = ToolPanelState::new();
        state.showing_tool_panel = true;
        state.tool_panel_history.push(ToolExecution::new(
            "tool".into(),
            "read".into(),
            "summary".into(),
        ));
        state.tool_panel_selected_index = Some(0);
        state.showing_tool_result = true;
        state.tool_result_show_full = true;
        state.tool_result_scroll_offset = 50;

        state.reset();

        assert!(!state.showing_tool_panel);
        assert!(state.tool_panel_history.is_empty());
        assert!(state.tool_panel_selected_index.is_none());
        assert!(!state.showing_tool_result);
        assert!(!state.tool_result_show_full);
        assert_eq!(state.tool_result_scroll_offset, 0);
    }

    /// Regression: scroll_result() could set tool_result_scroll_offset to
    /// usize::MAX when no tool result exists. clamp_scroll(total_lines=0)
    /// now resets to 0.
    #[test]
    fn test_clamp_scroll_resets_on_empty_content() {
        let mut state = ToolPanelState::new();
        state.tool_result_scroll_offset = 9999;
        state.clamp_scroll(0);
        assert_eq!(state.tool_result_scroll_offset, 0);
    }

    /// Regression: clamp_scroll prevents offset from exceeding content.
    #[test]
    fn test_clamp_scroll_caps_to_total_lines() {
        let mut state = ToolPanelState::new();
        state.tool_result_scroll_offset = 500;
        state.clamp_scroll(10);
        assert_eq!(state.tool_result_scroll_offset, 9);
    }

    /// Verify scroll_result + clamp_scroll on empty content keeps offset at 0.
    #[test]
    fn test_scroll_and_clamp_on_empty_result() {
        let mut state = ToolPanelState::new();
        state.scroll_result(100);
        state.clamp_scroll(0);
        assert_eq!(state.tool_result_scroll_offset, 0);

        state.scroll_result(-50);
        state.clamp_scroll(0);
        assert_eq!(state.tool_result_scroll_offset, 0);
    }

    /// Regression: scroll offset must not underflow when scrolling up from zero.
    #[test]
    fn tool_result_scroll_does_not_underflow() {
        let mut state = ToolPanelState::new();
        assert_eq!(state.tool_result_scroll_offset, 0);
        state.scroll_result(-100);
        assert_eq!(state.tool_result_scroll_offset, 0);
    }

    /// Regression: scroll offset saturates instead of overflowing on huge positive delta.
    #[test]
    fn tool_result_scroll_saturating_add() {
        let mut state = ToolPanelState::new();
        state.tool_result_scroll_offset = usize::MAX - 5;
        state.scroll_result(100);
        assert_eq!(state.tool_result_scroll_offset, usize::MAX);
    }
}
