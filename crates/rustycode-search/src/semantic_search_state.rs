//! Global state management for SemanticSearchTool — stores index per session.
//!
//! SemanticSearchTool is a zero-sized struct that uses session-keyed global state to
//! access the semantic index, its metadata, and the project root for the session.

use dashmap::DashMap;
use once_cell::sync::Lazy;
use parking_lot::Mutex;
use std::path::PathBuf;
use std::sync::Arc;

use super::semantic_search::{IndexMetadata, SemanticIndex};

/// Search state for a session: project_root + index + metadata
pub struct SearchState {
    pub project_root: PathBuf,
    pub index: Arc<Mutex<Option<SemanticIndex>>>,
    pub metadata: Arc<Mutex<Option<IndexMetadata>>>,
}

/// Global search state storage keyed by session_id.
/// Each session has its own project root and semantic index.
#[allow(clippy::non_std_lazy_statics)]
static SEARCH_STATES: Lazy<DashMap<String, SearchState>> = Lazy::new(DashMap::new);

/// Initialize search state for a session (project_root only; index is lazy).
pub fn set_search_root(session_id: &str, project_root: PathBuf) {
    SEARCH_STATES.insert(
        session_id.to_string(),
        SearchState {
            project_root,
            index: Arc::new(Mutex::new(None)),
            metadata: Arc::new(Mutex::new(None)),
        },
    );
}

/// Retrieve search state for a session (clones Arc references).
/// Returns None if no state has been set for this session.
pub fn get_search_state(session_id: &str) -> Option<SearchState> {
    SEARCH_STATES.get(session_id).map(|entry| SearchState {
        project_root: entry.project_root.clone(),
        index: Arc::clone(&entry.index),
        metadata: Arc::clone(&entry.metadata),
    })
}

/// Remove search state from the global store (e.g., on session cleanup).
pub fn remove_search_state(session_id: &str) {
    SEARCH_STATES.remove(session_id);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_set_and_get_state() {
        let session_id = "test-session-1";
        let root = PathBuf::from("/tmp/test");
        set_search_root(session_id, root.clone());

        let state = get_search_state(session_id).unwrap();
        assert_eq!(state.project_root, root);
        assert!(state.index.lock().is_none());
        assert!(state.metadata.lock().is_none());
    }

    #[test]
    fn test_get_nonexistent_state() {
        let result = get_search_state("nonexistent-session");
        assert!(result.is_none());
    }

    #[test]
    fn test_remove_state() {
        let session_id = "test-session-2";
        let root = PathBuf::from("/tmp/test");
        set_search_root(session_id, root);
        assert!(get_search_state(session_id).is_some());

        remove_search_state(session_id);
        assert!(get_search_state(session_id).is_none());
    }

    #[test]
    fn test_session_isolation() {
        let session_1 = "session-isolation-1";
        let session_2 = "session-isolation-2";

        let root_1 = PathBuf::from("/tmp/session1");
        let root_2 = PathBuf::from("/tmp/session2");

        set_search_root(session_1, root_1.clone());
        set_search_root(session_2, root_2.clone());

        let state_1 = get_search_state(session_1).unwrap();
        let state_2 = get_search_state(session_2).unwrap();

        assert_eq!(state_1.project_root, root_1);
        assert_eq!(state_2.project_root, root_2);
        assert_ne!(state_1.project_root, state_2.project_root);
    }
}
