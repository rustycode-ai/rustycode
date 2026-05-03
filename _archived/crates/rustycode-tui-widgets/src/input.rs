//! input widget

pub struct InputWidget;

impl Default for InputWidget {
    fn default() -> Self {
        Self::new()
    }
}

impl InputWidget {
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
    fn new_returns_input_widget() {
        let _widget = InputWidget::new();
    }

    #[test]
    fn default_matches_new() {
        let from_new = InputWidget::new();
        let from_default = InputWidget::default();
        // Zero-sized types are always equal via structural comparison
        assert_eq!(
            std::mem::size_of_val(&from_new),
            std::mem::size_of_val(&from_default)
        );
    }

    #[test]
    fn is_zero_sized() {
        assert_eq!(std::mem::size_of::<InputWidget>(), 0);
    }

    #[test]
    fn const_new_compiles() {
        const _WIDGET: InputWidget = InputWidget::new();
    }

    #[test]
    fn is_send() {
        fn assert_send<T: Send>() {}
        assert_send::<InputWidget>();
    }

    #[test]
    fn is_sync() {
        fn assert_sync<T: Sync>() {}
        assert_sync::<InputWidget>();
    }

    #[test]
    fn multiple_instances_independent() {
        let _a = InputWidget::new();
        let _b = InputWidget::default();
        let _c = InputWidget::new();
        // Should compile and run without issue — zero-sized, no state
    }

    #[test]
    fn clone_is_not_derived_so_identity_is_value_based() {
        // InputWidget does not derive Clone; verify it can still be constructed freely
        let _w1 = InputWidget::new();
        let _w2 = InputWidget::new();
    }
}
