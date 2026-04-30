//! Memory-related commands

#[derive(Default)]
pub struct MemoryCommands;

impl MemoryCommands {
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
        let _cmds = MemoryCommands::new();
    }

    #[test]
    fn default_returns_instance() {
        let _cmds = MemoryCommands::default();
    }

    #[test]
    fn new_and_default_produce_same_size() {
        let a = MemoryCommands::new();
        let b = MemoryCommands::default();
        assert_eq!(std::mem::size_of_val(&a), std::mem::size_of_val(&b));
    }

    #[test]
    fn is_zero_sized() {
        assert_eq!(std::mem::size_of::<MemoryCommands>(), 0);
    }

    #[test]
    fn const_new_usable_in_static_context() {
        static _CMDS: MemoryCommands = MemoryCommands::new();
        assert_eq!(std::mem::size_of_val(&_CMDS), 0);
    }

    #[test]
    fn instances_are_moveable() {
        let a = MemoryCommands::new();
        let _b = a;
        // a has been moved; this verifies the type is move-safe at zero size
    }

    #[test]
    fn align_is_one() {
        assert_eq!(std::mem::align_of::<MemoryCommands>(), 1);
    }

    #[test]
    fn multiple_independent_instances() {
        let _a = MemoryCommands::new();
        let _b = MemoryCommands::new();
        let _c = MemoryCommands::default();
    }
}
