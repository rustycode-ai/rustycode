// ── Context Module ────────────────────────────────────────────────────────────
//
// This module provides token budget tracking, enforcement for context assembly,
// and ignore pattern support (.rustycodeignore / .gitignore).

pub mod budget;
pub mod budget_enforcement;
pub mod ignore;
pub mod lru_cache;
pub mod token_counter;

// Re-exports for backward compatibility
pub use budget::ContextBudget;
pub use budget_enforcement::{enforce_budget, enforce_budget_prioritized};
pub use ignore::RustyCodeIgnore;
pub use lru_cache::LruCache;
pub use token_counter::{CachedTokenCounter, ChatMessageInfo, TokenCounter, TokenProvider};
