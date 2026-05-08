//! Global state management for BatchTool — stores tool registry per session.
//!
//! BatchTool is a zero-sized struct that uses session-keyed global state to
//! access the ToolRegistry needed for parallel batch execution.

use crate::ToolRegistry;
use dashmap::DashMap;
use once_cell::sync::Lazy;
use std::sync::Arc;

/// Global registry storage keyed by session_id.
/// Each session has its own independent registry for batch execution.
#[allow(clippy::non_std_lazy_statics)]
static BATCH_REGISTRIES: Lazy<DashMap<String, Arc<ToolRegistry>>> = Lazy::new(DashMap::new);

/// Store a tool registry for a session.
pub fn set_batch_registry(session_id: &str, registry: Arc<ToolRegistry>) {
    BATCH_REGISTRIES.insert(session_id.to_string(), registry);
}

/// Retrieve the tool registry for a session.
/// Returns the registry if it exists for the given session_id.
pub fn get_batch_registry(session_id: &str) -> Option<Arc<ToolRegistry>> {
    BATCH_REGISTRIES
        .get(session_id)
        .map(|entry| Arc::clone(&entry))
}

/// Retrieve the tool registry for a session, falling back to a default session
/// if the specific session isn't found. Useful for cases where a default registry
/// should be used when no session-specific registry is set up.
pub fn get_batch_registry_or_default(
    session_id: &str,
    default_session: &str,
) -> Option<Arc<ToolRegistry>> {
    get_batch_registry(session_id).or_else(|| get_batch_registry(default_session))
}

/// Remove a registry from the global store (e.g., on session cleanup).
pub fn remove_batch_registry(session_id: &str) {
    BATCH_REGISTRIES.remove(session_id);
}

/// Get the count of registered batch registries (useful for testing/debugging).
#[cfg(test)]
pub fn registry_count() -> usize {
    BATCH_REGISTRIES.len()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::default_registry;

    #[test]
    fn test_set_and_get_registry() {
        let session_id = "test-session-1";
        let registry = Arc::new(default_registry());

        set_batch_registry(session_id, Arc::clone(&registry));
        let retrieved = get_batch_registry(session_id).unwrap();

        assert!(Arc::ptr_eq(&registry, &retrieved));
    }

    #[test]
    fn test_get_nonexistent_registry() {
        let result = get_batch_registry("nonexistent-session");
        assert!(result.is_none());
    }

    #[test]
    fn test_remove_registry() {
        let session_id = "test-session-2";
        let registry = Arc::new(default_registry());

        set_batch_registry(session_id, registry);
        assert!(get_batch_registry(session_id).is_some());

        remove_batch_registry(session_id);
        assert!(get_batch_registry(session_id).is_none());
    }

    #[test]
    fn test_session_isolation() {
        let session_1 = "session-1";
        let session_2 = "session-2";

        let registry_1 = Arc::new(default_registry());
        let registry_2 = Arc::new(default_registry());

        set_batch_registry(session_1, Arc::clone(&registry_1));
        set_batch_registry(session_2, Arc::clone(&registry_2));

        let retrieved_1 = get_batch_registry(session_1).unwrap();
        let retrieved_2 = get_batch_registry(session_2).unwrap();

        assert!(Arc::ptr_eq(&registry_1, &retrieved_1));
        assert!(Arc::ptr_eq(&registry_2, &retrieved_2));
        assert!(!Arc::ptr_eq(&retrieved_1, &retrieved_2));
    }
}
