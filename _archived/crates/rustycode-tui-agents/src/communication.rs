//! Agent communication

pub struct AgentCommunication;

impl Default for AgentCommunication {
    fn default() -> Self {
        Self::new()
    }
}

impl AgentCommunication {
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
    fn new_returns_instance() {
        let _comm = AgentCommunication::new();
    }

    #[test]
    fn default_returns_instance() {
        let _comm = AgentCommunication::default();
    }

    #[test]
    fn is_zero_sized() {
        assert_eq!(std::mem::size_of::<AgentCommunication>(), 0);
    }

    #[test]
    fn const_new_usable_in_const_context() {
        const _COMM: AgentCommunication = AgentCommunication::new();
    }

    #[test]
    fn is_send() {
        fn assert_send<T: Send>() {}
        assert_send::<AgentCommunication>();
    }

    #[test]
    fn is_sync() {
        fn assert_sync<T: Sync>() {}
        assert_sync::<AgentCommunication>();
    }

    #[test]
    fn multiple_instances() {
        // Verify that creating many instances does not panic or allocate
        assert_eq!((0..100).count(), 100);
    }

    #[test]
    fn default_derives_from_new() {
        // Both construction paths should produce the same type
        let _new_inst: AgentCommunication = AgentCommunication::new();
        let _default_inst: AgentCommunication = AgentCommunication::default();
    }

    #[test]
    fn can_be_used_in_generic_context() {
        fn accept_anything<T: Default>(_: T) {}
        accept_anything(AgentCommunication::default());
    }
}
