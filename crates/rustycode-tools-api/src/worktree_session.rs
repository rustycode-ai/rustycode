//! Session-scoped worktree state shared across crates.
//!
//! Lives in `rustycode-tools-api` (leaf crate) so both `rustycode-tools`
//! and `rustycode-runtime` can use it without circular dependencies.

use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

/// Original project root before entering a worktree via `worktree_enter`.
static SESSION_ORIGINAL_CWD: OnceLock<Mutex<Option<PathBuf>>> = OnceLock::new();

/// Get the session's original CWD (before entering worktree).
/// Returns `None` if not in a worktree session.
pub fn session_original_cwd() -> Option<PathBuf> {
    SESSION_ORIGINAL_CWD.get().and_then(|m| {
        m.lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    })
}

/// Store the original CWD when entering a worktree session.
pub fn set_session_original_cwd(path: PathBuf) {
    let lock = SESSION_ORIGINAL_CWD.get_or_init(|| Mutex::new(None));
    *lock
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(path);
}

/// Clear the session worktree state (when exiting worktree).
pub fn clear_session_original_cwd() {
    if let Some(lock) = SESSION_ORIGINAL_CWD.get() {
        *lock
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = None;
    }
}

/// Check if currently in a worktree session (entered via `worktree_enter`).
pub fn in_worktree_session() -> bool {
    session_original_cwd().is_some()
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
}
