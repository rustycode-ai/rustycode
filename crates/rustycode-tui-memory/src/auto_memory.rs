//! Auto memory management

#[derive(Default)]
pub struct AutoMemoryManager;

impl AutoMemoryManager {
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
        let _manager = AutoMemoryManager::new();
    }

    #[test]
    fn default_returns_instance() {
        let _manager = AutoMemoryManager::default();
    }

    #[test]
    fn new_and_default_produce_same_size() {
        let a = AutoMemoryManager::new();
        let b = AutoMemoryManager::default();
        assert_eq!(std::mem::size_of_val(&a), std::mem::size_of_val(&b));
    }

    #[test]
    fn is_zero_sized() {
        assert_eq!(std::mem::size_of::<AutoMemoryManager>(), 0);
    }

    #[test]
    fn const_new_usable_in_static_context() {
        static _MANAGER: AutoMemoryManager = AutoMemoryManager::new();
        assert_eq!(std::mem::size_of_val(&_MANAGER), 0);
    }

    #[test]
    fn instances_are_moveable() {
        let a = AutoMemoryManager::new();
        let _b = a;
        // a has been moved; this verifies the type is move-safe at zero size
    }

    #[test]
    fn align_is_one() {
        // Unit structs have alignment 1
        assert_eq!(std::mem::align_of::<AutoMemoryManager>(), 1);
    }

    #[test]
    fn multiple_independent_instances() {
        let _a = AutoMemoryManager::new();
        let _b = AutoMemoryManager::new();
        let _c = AutoMemoryManager::default();
    }
}
