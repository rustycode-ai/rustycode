//! Memory relevance utilities

#[derive(Default)]
pub struct MemoryRelevance;

impl MemoryRelevance {
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
        let _relevance = MemoryRelevance::new();
    }

    #[test]
    fn default_returns_instance() {
        let _relevance = MemoryRelevance::default();
    }

    #[test]
    fn new_and_default_produce_same_size() {
        let a = MemoryRelevance::new();
        let b = MemoryRelevance::default();
        assert_eq!(std::mem::size_of_val(&a), std::mem::size_of_val(&b));
    }

    #[test]
    fn is_zero_sized() {
        assert_eq!(std::mem::size_of::<MemoryRelevance>(), 0);
    }

    #[test]
    fn const_new_usable_in_static_context() {
        static _RELEVANCE: MemoryRelevance = MemoryRelevance::new();
        assert_eq!(std::mem::size_of_val(&_RELEVANCE), 0);
    }

    #[test]
    fn instances_are_moveable() {
        let a = MemoryRelevance::new();
        let _b = a;
        // a has been moved; this verifies the type is move-safe at zero size
    }

    #[test]
    fn align_is_one() {
        assert_eq!(std::mem::align_of::<MemoryRelevance>(), 1);
    }

    #[test]
    fn multiple_independent_instances() {
        let _a = MemoryRelevance::new();
        let _b = MemoryRelevance::new();
        let _c = MemoryRelevance::default();
    }
}
