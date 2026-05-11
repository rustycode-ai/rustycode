//! Token budget tracking trait for LLM usage enforcement.
//!
//! Wraps existing `CostTracker` and `TokenBudgetState` behind a uniform
//! interface so the LLM provider layer can report usage without depending
//! on orchestration internals.

/// Trait for tracking and enforcing token budgets per session.
pub trait TokenAccountant: Send + Sync {
    /// Record prompt tokens consumed by a session.
    fn track_prompt(&self, tokens: u64, session_id: &str);

    /// Record completion tokens consumed by a session.
    fn track_completion(&self, tokens: u64, session_id: &str);

    /// Return the remaining token budget for a session, if a budget is set.
    fn remaining_budget(&self, session_id: &str) -> Option<u64>;

    /// Enforce the token limit. Returns `Err` if the session has exceeded its budget.
    fn enforce_limit(&self, session_id: &str) -> Result<(), TokenBudgetError>;
}

/// Error returned when a token budget is exceeded.
#[derive(Debug, thiserror::Error)]
pub enum TokenBudgetError {
    #[error("token budget exceeded for session {session_id}: used {used}, budget {budget}")]
    Exceeded {
        session_id: String,
        used: u64,
        budget: u64,
    },
    #[error("no budget configured for session {0}")]
    NoBudget(String),
}

/// A no-op accountant that does no tracking. Useful for tests and when
/// budget enforcement is disabled.
pub struct NoopTokenAccountant;

impl TokenAccountant for NoopTokenAccountant {
    fn track_prompt(&self, _tokens: u64, _session_id: &str) {}
    fn track_completion(&self, _tokens: u64, _session_id: &str) {}
    fn remaining_budget(&self, _session_id: &str) -> Option<u64> {
        None
    }
    fn enforce_limit(&self, _session_id: &str) -> Result<(), TokenBudgetError> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn noop_accountant_never_errors() {
        let acc = NoopTokenAccountant;
        acc.track_prompt(1000, "test");
        acc.track_completion(500, "test");
        assert!(acc.remaining_budget("test").is_none());
        assert!(acc.enforce_limit("test").is_ok());
    }
}
