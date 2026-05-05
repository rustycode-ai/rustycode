//! Synchronous state machine for query/operation lifecycle management.
//!
//! Synchronous state machine for query/operation lifecycle management.
//! execution of operations that must run exclusively (agent queries, tool
//! execution, event processing).
//!
//! # States
//!
//! - **Idle** — no operation in progress
//! - **Dispatching** — operation dequeued but not yet started
//! - **Running** — operation is executing
//!
//! # Generation Tracking
//!
//! Each successful `try_start()` increments a generation counter. When an
//! operation completes (`end()`), the generation is checked — if it doesn't
//! match, the caller is stale and should skip cleanup. This handles the case
//! where a cancellation started a new operation while the old one's `finally`
//! block is still running.
//!
//! # Example
//!
//! ```ignore
//! use rustycode_protocol::query_guard::QueryGuard;
//!
//! let guard = QueryGuard::new();
//!
//! // Start an operation
//! let gen = guard.try_start().expect("should start");
//! assert!(guard.is_active());
//! assert!(!guard.is_idle());
//!
//! // Concurrent attempt fails
//! assert!(guard.try_start().is_none());
//!
//! // Complete the operation
//! assert!(guard.end(gen));
//! assert!(guard.is_idle());
//! ```

/// Synchronous state machine for exclusive operation execution.
#[derive(Debug)]
pub struct QueryGuard {
    state: QueryState,
    generation: u64,
}

/// The state of a query guard.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum QueryState {
    /// No operation in progress.
    Idle,
    /// Operation dequeued but not yet started.
    Dispatching,
    /// Operation is executing.
    Running,
}

impl QueryGuard {
    pub fn new() -> Self {
        Self {
            state: QueryState::Idle,
            generation: 0,
        }
    }

    /// Reserve the guard for queue processing.
    /// Transitions Idle → Dispatching.
    /// Returns `false` if not idle (another query or dispatch in progress).
    pub fn reserve(&mut self) -> bool {
        if self.state != QueryState::Idle {
            return false;
        }
        self.state = QueryState::Dispatching;
        true
    }

    /// Cancel a reservation when nothing was found to process.
    /// Transitions Dispatching → Idle.
    pub fn cancel_reservation(&mut self) {
        if self.state == QueryState::Dispatching {
            self.state = QueryState::Idle;
        }
    }

    /// Start an operation. Returns the generation number on success,
    /// or `None` if already running.
    ///
    /// Accepts transitions from both Idle (direct submit)
    /// and Dispatching (queue processor path).
    pub fn try_start(&mut self) -> Option<u64> {
        if self.state == QueryState::Running {
            return None;
        }
        self.state = QueryState::Running;
        self.generation = self.generation.saturating_add(1);
        Some(self.generation)
    }

    /// End an operation. Returns `true` if the generation is still current
    /// (caller should perform cleanup). Returns `false` if a newer operation
    /// has started (stale finally block from a cancelled operation).
    pub fn end(&mut self, generation: u64) -> bool {
        if self.generation != generation {
            return false;
        }
        if self.state != QueryState::Running {
            return false;
        }
        self.state = QueryState::Idle;
        true
    }

    /// Force-end the current operation regardless of generation.
    /// Increments generation so stale finally blocks from the cancelled
    /// operation will see a mismatch and skip cleanup.
    pub fn force_end(&mut self) {
        if self.state == QueryState::Idle {
            return;
        }
        self.state = QueryState::Idle;
        self.generation = self.generation.saturating_add(1);
    }

    /// Is the guard active (dispatching or running)?
    pub fn is_active(&self) -> bool {
        self.state != QueryState::Idle
    }

    /// Is the guard idle?
    pub fn is_idle(&self) -> bool {
        self.state == QueryState::Idle
    }

    /// Get the current state.
    pub fn state(&self) -> QueryState {
        self.state
    }

    /// Get the current generation number.
    pub fn generation(&self) -> u64 {
        self.generation
    }
}

impl Default for QueryGuard {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_guard_is_idle() {
        let guard = QueryGuard::new();
        assert!(guard.is_idle());
        assert!(!guard.is_active());
        assert_eq!(guard.state(), QueryState::Idle);
        assert_eq!(guard.generation(), 0);
    }

    #[test]
    fn try_start_from_idle() {
        let mut guard = QueryGuard::new();
        let gen = guard.try_start();
        assert_eq!(gen, Some(1));
        assert!(guard.is_active());
        assert_eq!(guard.state(), QueryState::Running);
    }

    #[test]
    fn try_start_fails_when_running() {
        let mut guard = QueryGuard::new();
        guard.try_start();
        assert!(guard.try_start().is_none());
    }

    #[test]
    fn end_succeeds_with_matching_generation() {
        let mut guard = QueryGuard::new();
        let gen = guard.try_start().unwrap();
        assert!(guard.end(gen));
        assert!(guard.is_idle());
    }

    #[test]
    fn end_fails_with_stale_generation() {
        let mut guard = QueryGuard::new();
        let gen = guard.try_start().unwrap();
        guard.force_end(); // increments generation
        assert!(!guard.end(gen)); // stale
    }

    #[test]
    fn end_fails_when_not_running() {
        let mut guard = QueryGuard::new();
        assert!(!guard.end(1));
    }

    #[test]
    fn force_end_from_running() {
        let mut guard = QueryGuard::new();
        guard.try_start();
        guard.force_end();
        assert!(guard.is_idle());
        assert_eq!(guard.generation(), 2); // started at 1, force_end increments
    }

    #[test]
    fn force_end_from_idle_is_noop() {
        let mut guard = QueryGuard::new();
        guard.force_end();
        assert!(guard.is_idle());
        assert_eq!(guard.generation(), 0);
    }

    #[test]
    fn reserve_from_idle() {
        let mut guard = QueryGuard::new();
        assert!(guard.reserve());
        assert_eq!(guard.state(), QueryState::Dispatching);
        assert!(guard.is_active());
    }

    #[test]
    fn reserve_fails_when_not_idle() {
        let mut guard = QueryGuard::new();
        guard.try_start();
        assert!(!guard.reserve());
    }

    #[test]
    fn cancel_reservation() {
        let mut guard = QueryGuard::new();
        guard.reserve();
        guard.cancel_reservation();
        assert!(guard.is_idle());
    }

    #[test]
    fn cancel_reservation_noop_when_not_dispatching() {
        let mut guard = QueryGuard::new();
        guard.cancel_reservation(); // should be fine
        assert!(guard.is_idle());
    }

    #[test]
    fn try_start_from_dispatching() {
        let mut guard = QueryGuard::new();
        guard.reserve();
        let gen = guard.try_start();
        assert_eq!(gen, Some(1));
        assert_eq!(guard.state(), QueryState::Running);
    }

    #[test]
    fn full_lifecycle_reserve_dispatch_run_end() {
        let mut guard = QueryGuard::new();

        // Reserve
        assert!(guard.reserve());
        assert_eq!(guard.state(), QueryState::Dispatching);

        // Start
        let gen = guard.try_start().unwrap();
        assert_eq!(guard.state(), QueryState::Running);

        // End
        assert!(guard.end(gen));
        assert_eq!(guard.state(), QueryState::Idle);
    }

    #[test]
    fn stale_end_skips_cleanup_after_force() {
        let mut guard = QueryGuard::new();
        let gen1 = guard.try_start().unwrap();

        // Force end (e.g., cancellation)
        guard.force_end();
        let gen2 = guard.try_start().unwrap();
        assert_eq!(gen2, 3); // gen1=1, force_end→2, try_start→3

        // Stale end from gen1 should be rejected
        assert!(!guard.end(gen1));

        // Current gen end should succeed
        assert!(guard.end(gen2));
    }

    #[test]
    fn multiple_cycles() {
        let mut guard = QueryGuard::new();
        for i in 1..=5 {
            let gen = guard.try_start().unwrap();
            assert_eq!(gen, i as u64);
            assert!(guard.end(gen));
        }
        assert!(guard.is_idle());
    }

    #[test]
    fn default_is_same_as_new() {
        let guard = QueryGuard::default();
        assert!(guard.is_idle());
    }
}
