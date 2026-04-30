//! status widget

pub struct StatusWidget;

impl Default for StatusWidget {
    fn default() -> Self {
        Self::new()
    }
}

impl StatusWidget {
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
    fn new_returns_status_widget() {
        let _widget = StatusWidget::new();
    }

    #[test]
    fn default_matches_new() {
        let from_new = StatusWidget::new();
        let from_default = StatusWidget::default();
        assert_eq!(
            std::mem::size_of_val(&from_new),
            std::mem::size_of_val(&from_default)
        );
    }

    #[test]
    fn is_zero_sized() {
        assert_eq!(std::mem::size_of::<StatusWidget>(), 0);
    }

    #[test]
    fn const_new_compiles() {
        const _WIDGET: StatusWidget = StatusWidget::new();
    }

    #[test]
    fn is_send() {
        fn assert_send<T: Send>() {}
        assert_send::<StatusWidget>();
    }

    #[test]
    fn is_sync() {
        fn assert_sync<T: Sync>() {}
        assert_sync::<StatusWidget>();
    }

    #[test]
    fn align_is_one() {
        assert_eq!(std::mem::align_of::<StatusWidget>(), 1);
    }

    #[test]
    fn can_be_used_in_static_context() {
        static _WIDGET: StatusWidget = StatusWidget::new();
    }
}
