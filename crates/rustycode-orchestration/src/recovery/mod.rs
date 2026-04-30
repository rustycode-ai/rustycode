//! Crash recovery and lock file management.
//!
//! Enables recovery from crashes by tracking the current unit and session state.
//! On startup, checks for a stale crash lock and recovers the session if the
//! lock process is dead.

pub mod crash_recovery;

pub use crash_recovery::*;
