//! Context and git isolation for tiered execution.
//!
//! Two isolation mechanisms:
//! - **Tier isolation** (`tier`): Context budgets and tool restrictions per execution tier
//! - **Git worktree isolation** (`worktree`): Isolated git worktrees for parallel milestone execution

pub mod tier;
pub mod worktree;
pub mod worktree_name_gen;

pub use tier::{
    classify_tool, ContextBudget, IsolationConfig, IsolationError, TierIsolation, ToolCapability,
    ToolPolicy,
};
pub use worktree::{
    auto_worktree_branch, get_original_base, in_worktree, Worktree, WorktreeLock, WorktreeManager,
};
pub use worktree_name_gen::generate_worktree_name;
