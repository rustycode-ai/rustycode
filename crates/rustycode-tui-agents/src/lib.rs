//! `RustyCode` TUI Agents - Agent management for TUI.
//!
//! This crate provides agent lifecycle management for the TUI:
//!
//! - **Agent Manager**: Spawn, monitor, and manage agents
//! - **Agent Display**: UI for agent status and progress
//! - **Agent Communication**: Bidirectional communication with agents
//! - **Agent Lifecycle**: Start, stop, cancel, and cleanup operations

pub mod communication;
pub mod display;
pub mod lifecycle;
pub mod manager;

pub use display::AgentDisplay;
pub use lifecycle::AgentLifecycle;
pub use manager::AgentManager;

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::default_constructed_unit_structs
)]
mod tests {
    use super::*;

    // --- Re-export correctness: re-exported types can be constructed ---

    #[test]
    fn re_exported_agent_display_constructible() {
        let _via_reexport = AgentDisplay::new();
        let _via_reexport_default = AgentDisplay::default();
    }

    #[test]
    fn re_exported_agent_lifecycle_constructible() {
        let _via_reexport = AgentLifecycle::new();
        let _via_reexport_default = AgentLifecycle::default();
    }

    #[test]
    fn re_exported_agent_manager_constructible() {
        let _via_reexport = AgentManager::new();
        let _via_reexport_default = AgentManager::default();
    }

    // --- Cross-type properties ---

    #[test]
    fn all_types_are_zero_sized() {
        assert_eq!(std::mem::size_of::<AgentDisplay>(), 0);
        assert_eq!(std::mem::size_of::<AgentLifecycle>(), 0);
        assert_eq!(std::mem::size_of::<AgentManager>(), 0);
        assert_eq!(std::mem::size_of::<communication::AgentCommunication>(), 0);
    }

    #[test]
    fn all_types_have_default() {
        let _display: AgentDisplay = AgentDisplay::default();
        let _lifecycle: AgentLifecycle = AgentLifecycle::default();
        let _manager: AgentManager = AgentManager::default();
        let _comm: communication::AgentCommunication = communication::AgentCommunication::default();
    }

    #[test]
    fn all_types_are_send() {
        fn assert_send<T: Send>() {}
        assert_send::<AgentDisplay>();
        assert_send::<AgentLifecycle>();
        assert_send::<AgentManager>();
        assert_send::<communication::AgentCommunication>();
    }

    #[test]
    fn all_types_are_sync() {
        fn assert_sync<T: Sync>() {}
        assert_sync::<AgentDisplay>();
        assert_sync::<AgentLifecycle>();
        assert_sync::<AgentManager>();
        assert_sync::<communication::AgentCommunication>();
    }

    #[test]
    fn all_types_work_in_generic_default_context() {
        fn use_default<T: Default>(_: T) {}
        use_default(AgentDisplay::default());
        use_default(AgentLifecycle::default());
        use_default(AgentManager::default());
        use_default(communication::AgentCommunication::default());
    }

    #[test]
    fn module_types_match_re_exports_by_size() {
        // Verify that re-exported types and module-level types are the same size (zero)
        assert_eq!(
            std::mem::size_of::<AgentDisplay>(),
            std::mem::size_of::<display::AgentDisplay>()
        );
        assert_eq!(
            std::mem::size_of::<AgentLifecycle>(),
            std::mem::size_of::<lifecycle::AgentLifecycle>()
        );
        assert_eq!(
            std::mem::size_of::<AgentManager>(),
            std::mem::size_of::<manager::AgentManager>()
        );
    }
}
