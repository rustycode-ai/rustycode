//! Session registry: caches `BashSession` instances by working directory.

use super::session::BashSession;
use anyhow::Result;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Instant;

/// Idle sessions are cleaned up after this many seconds.
pub(super) const IDLE_TIMEOUT_SECS: u64 = 300; // 5 minutes

/// Registry that caches `BashSession` instances by working directory.
///
/// Sessions are reused across tool calls so that environment variables,
/// shell aliases, and working directory changes persist. Idle sessions
/// are evicted after `IDLE_TIMEOUT_SECS` to reclaim resources.
pub(super) struct BashSessionRegistry {
    pub(super) sessions: Mutex<Option<HashMap<PathBuf, Arc<Mutex<BashSession>>>>>,
    pub(super) last_access: Mutex<Option<HashMap<PathBuf, Instant>>>,
}

impl BashSessionRegistry {
    pub(super) const fn new() -> Self {
        Self {
            sessions: Mutex::new(None),
            last_access: Mutex::new(None),
        }
    }

    fn ensure_init(&self) {
        if self
            .sessions
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .is_none()
        {
            *self
                .sessions
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(HashMap::new());
            *self
                .last_access
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(HashMap::new());
        }
    }

    /// Get or create a session for the given working directory.
    pub(super) fn get_or_create(&self, cwd: PathBuf) -> Result<Arc<Mutex<BashSession>>> {
        self.ensure_init();

        let sessions_guard = self
            .sessions
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        // Touch access time (scope the lock to avoid holding two locks simultaneously)
        {
            if let Some(ref mut times) = *self
                .last_access
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
            {
                times.insert(cwd.clone(), Instant::now());
            }
        }

        // Check if session exists (while holding the lock)
        if let Some(ref sessions) = *sessions_guard {
            if let Some(session) = sessions.get(&cwd) {
                return Ok(Arc::clone(session));
            }
        }

        // Create new session — release lock first to avoid holding it during process spawn
        drop(sessions_guard);
        let session = Arc::new(Mutex::new(BashSession::new(cwd.clone())?));

        // Re-acquire lock and insert (double-check in case another thread raced us)
        let mut sessions_guard = self
            .sessions
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(ref mut sessions) = *sessions_guard {
            // Another thread may have created a session while we were spawning
            if let Some(existing) = sessions.get(&cwd) {
                return Ok(Arc::clone(existing));
            }
            sessions.insert(cwd, Arc::clone(&session));
        }

        Ok(session)
    }

    /// Remove and return the session for `cwd`, if any.
    pub(super) fn remove(&self, cwd: &Path) -> Option<Arc<Mutex<BashSession>>> {
        self.ensure_init();
        let removed = {
            let mut sessions = self
                .sessions
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            match sessions.as_mut() {
                Some(s) => s.remove(cwd),
                None => None,
            }
        };
        if let Some(ref mut times) = *self
            .last_access
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
        {
            times.remove(cwd);
        }
        removed
    }

    /// Evict sessions that have been idle longer than `IDLE_TIMEOUT_SECS`.
    pub(super) fn evict_idle(&self) {
        self.ensure_init();
        let now = Instant::now();
        let to_evict: Vec<PathBuf> = {
            let times_guard = self
                .last_access
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            match *times_guard {
                Some(ref times) => times
                    .iter()
                    .filter(|(_, last)| now.duration_since(**last).as_secs() > IDLE_TIMEOUT_SECS)
                    .map(|(p, _)| p.clone())
                    .collect(),
                None => return,
            }
        };
        if !to_evict.is_empty() {
            let mut sessions_guard = self
                .sessions
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if let Some(ref mut sessions) = *sessions_guard {
                for cwd in &to_evict {
                    sessions.remove(cwd);
                }
            }
            drop(sessions_guard);
            let mut times_guard = self
                .last_access
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if let Some(ref mut times) = *times_guard {
                for cwd in &to_evict {
                    times.remove(cwd);
                }
            }
        }
    }
}

/// Global session registry — keyed by canonical working directory.
pub(super) static BASH_SESSION_REGISTRY: BashSessionRegistry = BashSessionRegistry::new();
