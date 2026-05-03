//! message widget

pub struct MessageWidget;

impl Default for MessageWidget {
    fn default() -> Self {
        Self::new()
    }
}

impl MessageWidget {
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
    fn new_returns_message_widget() {
        let _widget = MessageWidget::new();
    }

    #[test]
    fn default_matches_new() {
        let from_new = MessageWidget::new();
        let from_default = MessageWidget::default();
        assert_eq!(
            std::mem::size_of_val(&from_new),
            std::mem::size_of_val(&from_default)
        );
    }

    #[test]
    fn is_zero_sized() {
        assert_eq!(std::mem::size_of::<MessageWidget>(), 0);
    }

    #[test]
    fn const_new_compiles() {
        const _WIDGET: MessageWidget = MessageWidget::new();
    }

    #[test]
    fn is_send() {
        fn assert_send<T: Send>() {}
        assert_send::<MessageWidget>();
    }

    #[test]
    fn is_sync() {
        fn assert_sync<T: Sync>() {}
        assert_sync::<MessageWidget>();
    }

    #[test]
    fn align_is_one() {
        assert_eq!(std::mem::align_of::<MessageWidget>(), 1);
    }

    #[test]
    fn default_trait_is_consistent() {
        // Calling default() twice should produce identical (zero-sized) values
        let _a = MessageWidget::default();
        let _b = MessageWidget::default();
    }
}
