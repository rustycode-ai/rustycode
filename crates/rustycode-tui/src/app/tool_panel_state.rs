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
}

impl Default for ToolPanelState {
    fn default() -> Self {
        Self::new()
    }
}
