//! Memory injection utilities

#[derive(Default)]
pub struct MemoryInjector;

impl MemoryInjector {
    pub const fn new() -> Self {
        Self
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::default_constructed_unit_structs,
    clippy::no_effect_underscore_binding
)]
mod tests {
    use super::*;

    #[test]
    fn new_returns_instance() {
        let _injector = MemoryInjector::new();
    }

    #[test]
    fn default_returns_instance() {
        let _injector = MemoryInjector::default();
    }

    #[test]
    fn new_and_default_produce_same_size() {
        let a = MemoryInjector::new();
        let b = MemoryInjector::default();
        assert_eq!(std::mem::size_of_val(&a), std::mem::size_of_val(&b));
    }

    #[test]
    fn is_zero_sized() {
        assert_eq!(std::mem::size_of::<MemoryInjector>(), 0);
    }

    #[test]
    fn const_new_usable_in_static_context() {
        static _INJECTOR: MemoryInjector = MemoryInjector::new();
        assert_eq!(std::mem::size_of_val(&_INJECTOR), 0);
    }

    #[test]
    fn instances_are_moveable() {
        let a = MemoryInjector::new();
        let _b = a;
        // a has been moved; this verifies the type is move-safe at zero size
    }

    #[test]
    fn align_is_one() {
        assert_eq!(std::mem::align_of::<MemoryInjector>(), 1);
    }

    #[test]
    fn multiple_independent_instances() {
        let _a = MemoryInjector::new();
        let _b = MemoryInjector::new();
        let _c = MemoryInjector::default();
    }
}
