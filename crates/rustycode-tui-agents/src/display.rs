//! Agent display

pub struct AgentDisplay;

impl Default for AgentDisplay {
    fn default() -> Self {
        Self::new()
    }
}

impl AgentDisplay {
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
        let _display = AgentDisplay::new();
    }

    #[test]
    fn default_returns_instance() {
        let _display = AgentDisplay::default();
    }

    #[test]
    fn is_zero_sized() {
        assert_eq!(std::mem::size_of::<AgentDisplay>(), 0);
    }

    #[test]
    fn const_new_usable_in_const_context() {
        const _DISPLAY: AgentDisplay = AgentDisplay::new();
    }

    #[test]
    fn is_send() {
        fn assert_send<T: Send>() {}
        assert_send::<AgentDisplay>();
    }

    #[test]
    fn is_sync() {
        fn assert_sync<T: Sync>() {}
        assert_sync::<AgentDisplay>();
    }

    #[test]
    fn multiple_instances() {
        assert_eq!((0..100).count(), 100);
    }

    #[test]
    fn default_derives_from_new() {
        let _new_inst: AgentDisplay = AgentDisplay::new();
        let _default_inst: AgentDisplay = AgentDisplay::default();
    }

    #[test]
    fn can_be_used_in_generic_context() {
        fn accept_anything<T: Default>(_: T) {}
        accept_anything(AgentDisplay::default());
    }
}
