//! Global state management for TaskTool — stores cwd and runner per session.
//!
//! TaskTool is a zero-sized struct that uses session-keyed global state to
//! access the current working directory and sub-agent runner function.

use dashmap::DashMap;
use once_cell::sync::Lazy;
use std::path::PathBuf;
use std::sync::Arc;

/// Type-erased runner: function that takes (cwd, description, prompt) and returns result
pub type RunnerFn = dyn Fn(&std::path::Path, &str, &str) -> anyhow::Result<String> + Send + Sync;

/// Task state per session: (cwd, optional runner)
pub struct TaskState {
    pub cwd: PathBuf,
    pub runner: Option<Arc<RunnerFn>>,
}

/// Global task state storage keyed by session_id.
/// Each session has its own cwd and runner configuration.
#[allow(clippy::non_std_lazy_statics)]
static TASK_STATES: Lazy<DashMap<String, TaskState>> = Lazy::new(DashMap::new);

/// Store or update task state for a session.
pub fn set_task_state(session_id: &str, cwd: PathBuf, runner: Option<Arc<RunnerFn>>) {
    TASK_STATES.insert(session_id.to_string(), TaskState { cwd, runner });
}

/// Retrieve task state for a session (cwd and runner).
/// Returns None if no state has been set for this session.
pub fn get_task_state(session_id: &str) -> Option<TaskState> {
    TASK_STATES.get(session_id).map(|entry| TaskState {
        cwd: entry.cwd.clone(),
        runner: entry.runner.as_ref().map(Arc::clone),
    })
}

/// Set the runner for a session (update runner only, keep existing cwd).
pub fn set_task_runner(session_id: &str, runner: Arc<RunnerFn>) {
    TASK_STATES
        .entry(session_id.to_string())
        .and_modify(|state| {
            state.runner = Some(Arc::clone(&runner));
        })
        .or_insert_with(|| TaskState {
            cwd: PathBuf::from("."),
            runner: Some(runner),
        });
}

/// Remove task state from the global store (e.g., on session cleanup).
pub fn remove_task_state(session_id: &str) {
    TASK_STATES.remove(session_id);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_set_and_get_state() {
        let session_id = "test-session-1";
        let cwd = PathBuf::from("/tmp/test");
        let runner: Arc<RunnerFn> = Arc::new(|_, _, _| Ok("test output".to_string()));

        set_task_state(session_id, cwd.clone(), Some(Arc::clone(&runner)));
        let state = get_task_state(session_id).unwrap();

        assert_eq!(state.cwd, cwd);
        assert!(state.runner.is_some());
    }

    #[test]
    fn test_get_nonexistent_state() {
        let result = get_task_state("nonexistent-session");
        assert!(result.is_none());
    }

    #[test]
    fn test_remove_state() {
        let session_id = "test-session-2";
        let cwd = PathBuf::from("/tmp/test");

        set_task_state(session_id, cwd, None);
        assert!(get_task_state(session_id).is_some());

        remove_task_state(session_id);
        assert!(get_task_state(session_id).is_none());
    }

    #[test]
    fn test_session_isolation() {
        let session_1 = "session-1";
        let session_2 = "session-2";

        let cwd_1 = PathBuf::from("/tmp/session1");
        let cwd_2 = PathBuf::from("/tmp/session2");

        set_task_state(session_1, cwd_1.clone(), None);
        set_task_state(session_2, cwd_2.clone(), None);

        let state_1 = get_task_state(session_1).unwrap();
        let state_2 = get_task_state(session_2).unwrap();

        assert_eq!(state_1.cwd, cwd_1);
        assert_eq!(state_2.cwd, cwd_2);
        assert_ne!(state_1.cwd, state_2.cwd);
    }

    #[test]
    fn test_set_runner_updates_existing() {
        let session_id = "test-session-3";
        let cwd = PathBuf::from("/tmp/test");

        set_task_state(session_id, cwd.clone(), None);
        let state = get_task_state(session_id).unwrap();
        assert!(state.runner.is_none());

        let runner: Arc<RunnerFn> = Arc::new(|_, _, _| Ok("new runner output".to_string()));
        set_task_runner(session_id, Arc::clone(&runner));

        let updated_state = get_task_state(session_id).unwrap();
        assert!(updated_state.runner.is_some());
        assert_eq!(updated_state.cwd, cwd);
    }
}
