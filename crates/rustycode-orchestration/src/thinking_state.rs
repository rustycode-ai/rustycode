//! Global state management for StructuredThinkingTool — stores session-keyed thinking state.
//!
//! StructuredThinkingTool is a zero-sized struct that uses session-keyed global state to
//! access thinking session data (task_id, current_phase, reasoning_store, stuck_detector).
//! State is lazily initialized on first tool execution for each session.

use dashmap::DashMap;
use once_cell::sync::Lazy;
use parking_lot::Mutex;
use std::path::PathBuf;
use std::sync::Arc;

use crate::ask_user_tool::StuckDetector;
use crate::reasoning_store::ReasoningStore;

/// Session-specific thinking state: task tracking, phase, persistence, stuck detection
pub struct ThinkingState {
    pub task_id: Mutex<Option<String>>,
    pub current_phase: Mutex<u32>,
    pub reasoning_store: Mutex<Option<ReasoningStore>>,
    pub stuck_detector: Mutex<StuckDetector>,
}

impl ThinkingState {
    /// Create a new thinking state for a session
    pub fn new(store_path: Option<PathBuf>) -> Self {
        Self {
            task_id: Mutex::new(None),
            current_phase: Mutex::new(1),
            reasoning_store: Mutex::new(store_path.map(ReasoningStore::new)),
            stuck_detector: Mutex::new(StuckDetector::with_default_config()),
        }
    }
}

/// Global thinking state storage keyed by session_id
#[allow(clippy::non_std_lazy_statics)]
static THINKING_STATES: Lazy<DashMap<String, Arc<ThinkingState>>> = Lazy::new(DashMap::new);

/// Get or initialize thinking state for a session
///
/// Lazily creates state on first call for a given session_id.
/// Returns Arc reference that can be cloned and shared.
pub fn get_or_init_thinking_state(
    session_id: &str,
    store_path: Option<PathBuf>,
) -> Arc<ThinkingState> {
    THINKING_STATES
        .entry(session_id.to_string())
        .or_insert_with(|| Arc::new(ThinkingState::new(store_path)))
        .clone()
}

/// Retrieve thinking state for a session (returns None if not initialized)
pub fn get_thinking_state(session_id: &str) -> Option<Arc<ThinkingState>> {
    THINKING_STATES
        .get(session_id)
        .map(|entry| Arc::clone(&entry))
}

/// Remove thinking state from global store (e.g., on session cleanup)
pub fn remove_thinking_state(session_id: &str) {
    THINKING_STATES.remove(session_id);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lazy_init_on_get_or_init() {
        let session_id = "test-session-1";
        let state = get_or_init_thinking_state(session_id, None);
        assert_eq!(*state.task_id.lock(), None);
        assert_eq!(*state.current_phase.lock(), 1);
    }

    #[test]
    fn test_get_returns_none_before_init() {
        let result = get_thinking_state("nonexistent-session");
        assert!(result.is_none());
    }

    #[test]
    fn test_multiple_calls_same_session_return_same_state() {
        let session_id = "test-session-2";
        let state1 = get_or_init_thinking_state(session_id, None);

        // Mutate through state1
        {
            let mut phase = state1.current_phase.lock();
            *phase = 3;
        }

        // Get again and verify mutation is visible
        let state2 = get_or_init_thinking_state(session_id, None);
        assert_eq!(*state2.current_phase.lock(), 3);
    }

    #[test]
    fn test_remove_state() {
        let session_id = "test-session-3";
        get_or_init_thinking_state(session_id, None);
        assert!(get_thinking_state(session_id).is_some());

        remove_thinking_state(session_id);
        assert!(get_thinking_state(session_id).is_none());
    }

    #[test]
    fn test_session_isolation() {
        let session_1 = "session-isolation-1";
        let session_2 = "session-isolation-2";

        let state_1 = get_or_init_thinking_state(session_1, None);
        let state_2 = get_or_init_thinking_state(session_2, None);

        // Modify state_1
        {
            let mut phase = state_1.current_phase.lock();
            *phase = 5;
        }

        // state_2 should not be affected
        assert_eq!(*state_2.current_phase.lock(), 1);
    }
}
