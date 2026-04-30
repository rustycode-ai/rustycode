// ── Context Budget Tracking ────────────────────────────────────────────────────

/// Budget tracker for context token usage.
///
/// # Example
///
/// ```
/// use rustycode_core::context::ContextBudget;
///
/// let mut budget = ContextBudget::new(10000);
/// assert_eq!(budget.remaining(), 10000);
///
/// budget.reserve(5000)?;
/// assert_eq!(budget.remaining(), 5000);
///
/// budget.use_reserved(3000)?;
/// assert_eq!(budget.remaining(), 5000);
/// # Ok::<(), anyhow::Error>(())
/// ```
#[derive(Debug, Clone)]
pub struct ContextBudget {
    /// Total token budget for the context window
    total_budget: usize,
    /// Tokens currently reserved but not yet used
    reserved: usize,
    /// Tokens actually used (tracked separately from reservations)
    used: usize,
}

impl ContextBudget {
    /// Create a new context budget with the specified total.
    ///
    /// # Arguments
    ///
    /// * `total_budget` - Maximum tokens allowed in context
    pub fn new(total_budget: usize) -> Self {
        Self {
            total_budget,
            reserved: 0,
            used: 0,
        }
    }

    /// Get the total budget capacity.
    pub fn total(&self) -> usize {
        self.total_budget
    }

    /// Get the remaining budget (total - reserved - used).
    pub fn remaining(&self) -> usize {
        self.total_budget
            .saturating_sub(self.reserved)
            .saturating_sub(self.used)
    }

    /// Get currently reserved tokens.
    pub fn reserved(&self) -> usize {
        self.reserved
    }

    /// Get actually used tokens.
    pub fn used(&self) -> usize {
        self.used
    }

    /// Reserve tokens for a potential context item.
    ///
    /// # Arguments
    ///
    /// * `tokens` - Number of tokens to reserve
    ///
    /// # Returns
    ///
    /// * `Ok(())` if reservation succeeded
    /// * `Err` if would exceed budget
    pub fn reserve(&mut self, tokens: usize) -> anyhow::Result<()> {
        let would_be_reserved = self.reserved.saturating_add(tokens);
        let total_allocated = would_be_reserved.saturating_add(self.used);

        if total_allocated > self.total_budget {
            Err(anyhow::anyhow!(
                "Cannot reserve {} tokens: would exceed budget (total: {}, reserved: {}, used: {})",
                tokens,
                self.total_budget,
                self.reserved,
                self.used
            ))
        } else {
            self.reserved = would_be_reserved;
            Ok(())
        }
    }

    /// Mark some reserved tokens as actually used.
    ///
    /// This is called when a reserved item is actually added to context.
    ///
    /// # Arguments
    ///
    /// * `tokens` - Number of tokens to mark as used
    ///
    /// # Returns
    ///
    /// * `Ok(())` if usage succeeded
    /// * `Err` if trying to use more than reserved
    pub fn use_reserved(&mut self, tokens: usize) -> anyhow::Result<()> {
        if tokens > self.reserved {
            Err(anyhow::anyhow!(
                "Cannot use {} tokens: only {} reserved",
                tokens,
                self.reserved
            ))
        } else {
            self.reserved = self.reserved.saturating_sub(tokens);
            self.used = self.used.saturating_add(tokens);
            Ok(())
        }
    }

    /// Release unused reservation back to the budget.
    ///
    /// # Arguments
    ///
    /// * `tokens` - Number of tokens to release
    pub fn release(&mut self, tokens: usize) {
        self.reserved = self.reserved.saturating_sub(tokens);
    }

    /// Reset the budget (clear all reservations and usage).
    pub fn reset(&mut self) {
        self.reserved = 0;
        self.used = 0;
    }

    /// Check if the budget is exhausted (no remaining tokens).
    pub fn is_exhausted(&self) -> bool {
        self.remaining() == 0
    }

    /// Calculate budget utilization as a percentage (0.0 to 1.0).
    pub fn utilization(&self) -> f64 {
        if self.total_budget == 0 {
            0.0
        } else {
            let allocated = self.reserved + self.used;
            (allocated as f64) / (self.total_budget as f64)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_budget_creation() {
        let budget = ContextBudget::new(1000);
        assert_eq!(budget.total(), 1000);
        assert_eq!(budget.remaining(), 1000);
        assert_eq!(budget.reserved(), 0);
        assert_eq!(budget.used(), 0);
        assert!(!budget.is_exhausted());
        assert_eq!(budget.utilization(), 0.0);
    }

    #[test]
    fn test_budget_reserve() {
        let mut budget = ContextBudget::new(1000);

        budget.reserve(500).unwrap();
        assert_eq!(budget.reserved(), 500);
        assert_eq!(budget.remaining(), 500);

        budget.reserve(300).unwrap();
        assert_eq!(budget.reserved(), 800);
        assert_eq!(budget.remaining(), 200);
    }

    #[test]
    fn test_budget_reserve_exceeds() {
        let mut budget = ContextBudget::new(1000);
        budget.reserve(800).unwrap();

        let result = budget.reserve(300);
        assert!(result.is_err());
        assert_eq!(budget.reserved(), 800); // Unchanged
    }

    #[test]
    fn test_budget_use_reserved() {
        let mut budget = ContextBudget::new(1000);
        budget.reserve(500).unwrap();

        budget.use_reserved(300).unwrap();
        assert_eq!(budget.reserved(), 200);
        assert_eq!(budget.used(), 300);
        assert_eq!(budget.remaining(), 500);
    }

    #[test]
    fn test_budget_use_reserved_exceeds() {
        let mut budget = ContextBudget::new(1000);
        budget.reserve(500).unwrap();

        let result = budget.use_reserved(600);
        assert!(result.is_err());
        assert_eq!(budget.reserved(), 500); // Unchanged
        assert_eq!(budget.used(), 0); // Unchanged
    }

    #[test]
    fn test_budget_release() {
        let mut budget = ContextBudget::new(1000);
        budget.reserve(500).unwrap();

        budget.release(200);
        assert_eq!(budget.reserved(), 300);
        assert_eq!(budget.remaining(), 700);
    }

    #[test]
    fn test_budget_reset() {
        let mut budget = ContextBudget::new(1000);
        budget.reserve(500).unwrap();
        budget.use_reserved(300).unwrap();

        budget.reset();
        assert_eq!(budget.reserved(), 0);
        assert_eq!(budget.used(), 0);
        assert_eq!(budget.remaining(), 1000);
    }

    #[test]
    fn test_budget_utilization() {
        let mut budget = ContextBudget::new(1000);
        assert_eq!(budget.utilization(), 0.0);

        budget.reserve(500).unwrap();
        assert!((budget.utilization() - 0.5).abs() < 0.01);

        budget.use_reserved(300).unwrap();
        assert!((budget.utilization() - 0.5).abs() < 0.01); // Still 0.5
    }

    #[test]
    fn test_budget_is_exhausted() {
        let mut budget = ContextBudget::new(1000);
        assert!(!budget.is_exhausted());

        budget.reserve(1000).unwrap();
        assert!(budget.is_exhausted());

        budget.release(500);
        assert!(!budget.is_exhausted());
    }
}
