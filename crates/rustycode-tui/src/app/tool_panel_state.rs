use crate::ui::message::ToolExecution;

#[non_exhaustive]
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
}
