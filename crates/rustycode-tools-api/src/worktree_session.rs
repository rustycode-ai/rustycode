//! Session-scoped worktree state shared across crates.
//!
//! Lives in `rustycode-tools-api` (leaf crate) so both `rustycode-tools`
//! and `rustycode-runtime` can use it without circular dependencies.
//!
//! Each session gets its own CWD tracking keyed by session ID, preventing
//! cross-session contamination when multiple sessions run in the same process.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

static SESSION_CWDS: OnceLock<Mutex<HashMap<String, PathBuf>>> = OnceLock::new();

fn cwds() -> &'static Mutex<HashMap<String, PathBuf>> {
    SESSION_CWDS.get_or_init(|| Mutex::new(HashMap::new()))
}

const DEFAULT_KEY: &str = "__default__";

fn key(session_id: Option<&str>) -> String {
    session_id.map_or_else(|| DEFAULT_KEY.to_owned(), str::to_owned)
}

/// Get the session's original CWD (before entering worktree).
/// Returns `None` if not in a worktree session.
pub fn session_original_cwd() -> Option<PathBuf> {
    session_original_cwd_for(None)
}

/// Get the original CWD for a specific session.
pub fn session_original_cwd_for(session_id: Option<&str>) -> Option<PathBuf> {
    let map = cwds()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    map.get(&key(session_id)).cloned()
}

/// Store the original CWD when entering a worktree session.
pub fn set_session_original_cwd(path: PathBuf) {
    set_session_original_cwd_for(None, path);
}

/// Store the original CWD for a specific session.
pub fn set_session_original_cwd_for(session_id: Option<&str>, path: PathBuf) {
    let mut map = cwds()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    map.insert(key(session_id), path);
}

/// Clear the session worktree state (when exiting worktree).
pub fn clear_session_original_cwd() {
    clear_session_original_cwd_for(None);
}

/// Clear the worktree state for a specific session.
pub fn clear_session_original_cwd_for(session_id: Option<&str>) {
    let mut map = cwds()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    map.remove(&key(session_id));
}

/// Check if currently in a worktree session (entered via `worktree_enter`).
pub fn in_worktree_session() -> bool {
    session_original_cwd().is_some()
}

/// Check if a specific session is in a worktree.
pub fn in_worktree_session_for(session_id: Option<&str>) -> bool {
    session_original_cwd_for(session_id).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_session_state() {
        clear_session_original_cwd();
        assert!(!in_worktree_session());
        assert!(session_original_cwd().is_none());

        set_session_original_cwd(PathBuf::from("/tmp/test-project"));
        assert!(in_worktree_session());
        assert_eq!(
            session_original_cwd(),
            Some(PathBuf::from("/tmp/test-project"))
        );

        clear_session_original_cwd();
        assert!(!in_worktree_session());
        assert!(session_original_cwd().is_none());
    }

    #[test]
    fn sessions_are_isolated() {
        clear_session_original_cwd_for(Some("a"));
        clear_session_original_cwd_for(Some("b"));

        set_session_original_cwd_for(Some("a"), PathBuf::from("/tmp/a"));
        set_session_original_cwd_for(Some("b"), PathBuf::from("/tmp/b"));

        assert_eq!(
            session_original_cwd_for(Some("a")),
            Some(PathBuf::from("/tmp/a"))
        );
        assert_eq!(
            session_original_cwd_for(Some("b")),
            Some(PathBuf::from("/tmp/b"))
        );

        // Clearing one does not affect the other
        clear_session_original_cwd_for(Some("a"));
        assert!(session_original_cwd_for(Some("a")).is_none());
        assert_eq!(
            session_original_cwd_for(Some("b")),
            Some(PathBuf::from("/tmp/b"))
        );

        clear_session_original_cwd_for(Some("b"));
    }
}
