//! Task-Local Session Context
//!
//! Propagates session ID and working directory through async call chains
//! without requiring explicit parameter passing. Uses tokio's `task_local!`
//! macro for async-safe context propagation.
//!
//! Inspired by goose's session_context pattern.
//!
//! ## Usage
//!
//! ```ignore
//! use rustycode_core::session_context::{with_session_context, current_session_id};
//!
//! // Wrap an async task with session context
//! with_session_context(Some("sess_123".into()), PathBuf::from("/project"), async {
//!     // Any code in this async block can access session info
//!     let session_id = current_session_id();
//!     assert_eq!(session_id, Some("sess_123".into()));
//! }).await;
//! ```

use std::path::PathBuf;
use tokio::task_local;

task_local! {
    /// Current session ID, available within async contexts
    pub static SESSION_ID: Option<String>;
    /// Current working directory, available within async contexts
    pub static WORKING_DIR: Option<PathBuf>;
}

/// Run a future with session context set.
///
/// This allows any code within `f` to call `current_session_id()` and
/// `current_working_dir()` to access the session context without explicit
/// parameter passing.
pub async fn with_session_context<F>(
    session_id: Option<String>,
    working_dir: Option<PathBuf>,
    f: F,
) -> F::Output
where
    F: std::future::Future,
{
    match (session_id, working_dir) {
        (Some(sid), Some(wd)) => {
            SESSION_ID
                .scope(Some(sid), WORKING_DIR.scope(Some(wd), f))
                .await
        }
        (Some(sid), None) => SESSION_ID.scope(Some(sid), f).await,
        (None, Some(wd)) => WORKING_DIR.scope(Some(wd), f).await,
        (None, None) => f.await,
    }
}

/// Run a future with only a session ID
pub async fn with_session_id<F>(session_id: Option<String>, f: F) -> F::Output
where
    F: std::future::Future,
{
    if let Some(id) = session_id {
        SESSION_ID.scope(Some(id), f).await
    } else {
        f.await
    }
}

/// Run a future with only a working directory
pub async fn with_working_dir<F>(working_dir: Option<PathBuf>, f: F) -> F::Output
where
    F: std::future::Future,
{
    if let Some(dir) = working_dir {
        WORKING_DIR.scope(Some(dir), f).await
    } else {
        f.await
    }
}

/// Get the current session ID from task-local context
pub fn current_session_id() -> Option<String> {
    SESSION_ID.try_with(|id| id.clone()).ok().flatten()
}

/// Get the current working directory from task-local context
pub fn current_working_dir() -> Option<PathBuf> {
    WORKING_DIR.try_with(|dir| dir.clone()).ok().flatten()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_session_id_available_when_set() {
        with_session_id(Some("test-session-123".to_string()), async {
            assert_eq!(current_session_id(), Some("test-session-123".to_string()));
        })
        .await;
    }

    #[tokio::test]
    async fn test_session_id_none_when_not_set() {
        assert_eq!(current_session_id(), None);
    }

    #[tokio::test]
    async fn test_session_id_scoped_correctly() {
        assert_eq!(current_session_id(), None);

        with_session_id(Some("outer-session".to_string()), async {
            assert_eq!(current_session_id(), Some("outer-session".to_string()));

            with_session_id(Some("inner-session".to_string()), async {
                assert_eq!(current_session_id(), Some("inner-session".to_string()));
            })
            .await;

            assert_eq!(current_session_id(), Some("outer-session".to_string()));
        })
        .await;

        assert_eq!(current_session_id(), None);
    }

    #[tokio::test]
    async fn test_working_dir_available_when_set() {
        let dir = PathBuf::from("/tmp/test");
        with_working_dir(Some(dir.clone()), async {
            assert_eq!(current_working_dir(), Some(dir));
        })
        .await;
    }

    #[tokio::test]
    async fn test_full_context() {
        with_session_context(
            Some("sess-1".to_string()),
            Some(PathBuf::from("/project")),
            async {
                assert_eq!(current_session_id(), Some("sess-1".to_string()));
                assert_eq!(current_working_dir(), Some(PathBuf::from("/project")));
            },
        )
        .await;
    }

    #[tokio::test]
    async fn test_context_survives_await() {
        with_session_id(Some("persistent-session".to_string()), async {
            assert_eq!(current_session_id(), Some("persistent-session".to_string()));

            tokio::time::sleep(tokio::time::Duration::from_millis(1)).await;

            assert_eq!(current_session_id(), Some("persistent-session".to_string()));
        })
        .await;
    }

    #[tokio::test]
    async fn test_explicitly_none() {
        with_session_id(None, async {
            assert_eq!(current_session_id(), None);
        })
        .await;
    }
}
