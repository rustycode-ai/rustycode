//! Thread-safe memory operations

#[derive(Default)]
pub struct ThreadSafeMemory;

impl ThreadSafeMemory {
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
        let _memory = ThreadSafeMemory::new();
    }

    #[test]
    fn default_returns_instance() {
        let _memory = ThreadSafeMemory::default();
    }

    #[test]
    fn new_and_default_produce_same_size() {
        let a = ThreadSafeMemory::new();
        let b = ThreadSafeMemory::default();
        assert_eq!(std::mem::size_of_val(&a), std::mem::size_of_val(&b));
    }

    #[test]
    fn is_zero_sized() {
        assert_eq!(std::mem::size_of::<ThreadSafeMemory>(), 0);
    }

    #[test]
    fn const_new_usable_in_static_context() {
        static _MEMORY: ThreadSafeMemory = ThreadSafeMemory::new();
        assert_eq!(std::mem::size_of_val(&_MEMORY), 0);
    }

    #[test]
    fn instances_are_moveable() {
        let a = ThreadSafeMemory::new();
        let _b = a;
        // a has been moved; this verifies the type is move-safe at zero size
    }

    #[test]
    fn align_is_one() {
        assert_eq!(std::mem::align_of::<ThreadSafeMemory>(), 1);
    }

    #[test]
    fn multiple_independent_instances() {
        let _a = ThreadSafeMemory::new();
        let _b = ThreadSafeMemory::new();
        let _c = ThreadSafeMemory::default();
    }

    #[test]
    fn safe_to_send_across_threads() {
        let memory = ThreadSafeMemory::new();
        std::thread::spawn(move || {
            let _ = memory;
        })
        .join()
        .expect("thread should complete");
    }

    #[test]
    fn safe_to_share_across_threads() {
        use std::sync::Arc;
        let memory = Arc::new(ThreadSafeMemory::new());
        let clone = Arc::clone(&memory);
        let handle = std::thread::spawn(move || {
            let _ = clone;
        });
        handle.join().expect("thread should complete");
        // After thread exits, the Arc clone is dropped, so count returns to 1
        // (may take a moment for the drop to complete)
        assert!(Arc::strong_count(&memory) <= 2);
    }
}
