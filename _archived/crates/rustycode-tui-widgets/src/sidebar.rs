//! sidebar widget

pub struct SidebarWidget;

impl Default for SidebarWidget {
    fn default() -> Self {
        Self::new()
    }
}

impl SidebarWidget {
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
    fn new_returns_sidebar_widget() {
        let _widget = SidebarWidget::new();
    }

    #[test]
    fn default_matches_new() {
        let from_new = SidebarWidget::new();
        let from_default = SidebarWidget::default();
        assert_eq!(
            std::mem::size_of_val(&from_new),
            std::mem::size_of_val(&from_default)
        );
    }

    #[test]
    fn is_zero_sized() {
        assert_eq!(std::mem::size_of::<SidebarWidget>(), 0);
    }

    #[test]
    fn const_new_compiles() {
        const _WIDGET: SidebarWidget = SidebarWidget::new();
    }

    #[test]
    fn is_send() {
        fn assert_send<T: Send>() {}
        assert_send::<SidebarWidget>();
    }

    #[test]
    fn is_sync() {
        fn assert_sync<T: Sync>() {}
        assert_sync::<SidebarWidget>();
    }

    #[test]
    fn align_is_one() {
        assert_eq!(std::mem::align_of::<SidebarWidget>(), 1);
    }

    #[test]
    fn default_impl_calls_new() {
        // Default::default delegates to new(); both produce the same ZST
        let from_new = SidebarWidget::new();
        let from_default = SidebarWidget::default();
        let _ = (&from_new, &from_default);
    }
}
