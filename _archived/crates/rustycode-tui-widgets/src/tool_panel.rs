//! `tool_panel` widget

pub struct ToolPanelWidget;

impl Default for ToolPanelWidget {
    fn default() -> Self {
        Self::new()
    }
}

impl ToolPanelWidget {
    pub const fn new() -> Self {
        Self
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::default_constructed_unit_structs
)]
mod tests {
    use super::*;

    #[test]
    fn new_returns_tool_panel_widget() {
        let _widget = ToolPanelWidget::new();
    }

    #[test]
    fn default_matches_new() {
        let from_new = ToolPanelWidget::new();
        let from_default = ToolPanelWidget::default();
        assert_eq!(
            std::mem::size_of_val(&from_new),
            std::mem::size_of_val(&from_default)
        );
    }

    #[test]
    fn is_zero_sized() {
        assert_eq!(std::mem::size_of::<ToolPanelWidget>(), 0);
    }

    #[test]
    fn const_new_compiles() {
        const _WIDGET: ToolPanelWidget = ToolPanelWidget::new();
    }

    #[test]
    fn is_send() {
        fn assert_send<T: Send>() {}
        assert_send::<ToolPanelWidget>();
    }

    #[test]
    fn is_sync() {
        fn assert_sync<T: Sync>() {}
        assert_sync::<ToolPanelWidget>();
    }

    #[test]
    fn align_is_one() {
        assert_eq!(std::mem::align_of::<ToolPanelWidget>(), 1);
    }

    #[test]
    #[allow(clippy::needless_collect)]
    fn can_be_collected() {
        let widgets: Vec<ToolPanelWidget> = (0..10).map(|_| ToolPanelWidget::new()).collect();
        assert_eq!(widgets.len(), 10);
    }
}
