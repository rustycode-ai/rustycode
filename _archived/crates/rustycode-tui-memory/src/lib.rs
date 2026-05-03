//! `RustyCode` TUI Memory - Memory management for TUI.
//!
//! This crate handles all memory-related functionality for the TUI:
//!
//! - **Auto Memory**: Automatic memory management and persistence
//! - **Memory Injection**: Context injection from memory
//! - **Memory Commands**: Memory-related slash commands
//! - **Memory Relevance**: Memory ranking and filtering
//! - **Memory Threading**: Thread-safe memory operations

pub mod auto_memory;
pub mod commands;
pub mod injection;
pub mod relevance;
pub mod threading;

pub use auto_memory::AutoMemoryManager;
pub use injection::MemoryInjector;
pub use threading::ThreadSafeMemory;

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
    fn auto_memory_manager_re_exported() {
        let _ = AutoMemoryManager::new();
    }

    #[test]
    fn memory_injector_re_exported() {
        let _ = MemoryInjector::new();
    }

    #[test]
    fn thread_safe_memory_re_exported() {
        let _ = ThreadSafeMemory::new();
    }

    #[test]
    fn memory_commands_accessible_via_module() {
        let _ = commands::MemoryCommands::new();
    }

    #[test]
    fn memory_relevance_accessible_via_module() {
        let _ = relevance::MemoryRelevance::new();
    }

    #[test]
    fn all_types_are_zero_sized() {
        assert_eq!(std::mem::size_of::<AutoMemoryManager>(), 0);
        assert_eq!(std::mem::size_of::<MemoryInjector>(), 0);
        assert_eq!(std::mem::size_of::<ThreadSafeMemory>(), 0);
        assert_eq!(std::mem::size_of::<commands::MemoryCommands>(), 0);
        assert_eq!(std::mem::size_of::<relevance::MemoryRelevance>(), 0);
    }

    #[test]
    fn all_types_support_default() {
        let _ = AutoMemoryManager::default();
        let _ = MemoryInjector::default();
        let _ = ThreadSafeMemory::default();
        let _ = commands::MemoryCommands::default();
        let _ = relevance::MemoryRelevance::default();
    }
}
