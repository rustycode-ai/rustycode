//! Budget allocation types for multi-agent tasks.

use serde::{Deserialize, Serialize};

/// Budget allocation for an agent run or team task.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BudgetAllocation {
    /// Maximum tokens allowed for the run.
    pub max_tokens: u64,
    /// Maximum cost in USD allowed for the run.
    pub max_cost_usd: f64,
    /// Remaining tokens.
    pub remaining_tokens: u64,
    /// Remaining cost.
    pub remaining_cost_usd: f64,
}
