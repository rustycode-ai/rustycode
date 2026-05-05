//! Slash command handler modules.

pub mod compact;
pub mod copilot;
pub mod formatting;
pub mod hook;
pub mod load;
pub mod lsp;
pub mod marketplace;
pub mod mcp;
pub mod memory;
pub mod memory_advanced;
pub mod review;
pub mod save;
pub mod skill;
pub mod skill_suggestions;
pub mod skillify;
pub mod stats;
pub mod theme;

// Re-export command handlers for convenience
pub use compact::{execute_compaction, handle_compact_command, CompactAction};
pub use copilot::handle_copilot_login_command;
pub use hook::handle_hook_command;
pub use load::handle_load_command;
pub use lsp::handle_lsp_command;
pub use marketplace::handle_marketplace_command;
pub use mcp::handle_mcp_command;
pub use memory_advanced::handle_memory_command;
pub use review::handle_review_command;
pub use save::handle_save_command;
pub use skill::{handle_skill_command, handle_skills_browser};
pub use skill_suggestions::handle_skill_suggestions_command;
pub use skillify::handle_skillify_command;
pub use stats::{handle_stats_command, SessionStats, StatsResult};
pub use theme::{handle_theme_command, ThemeCommandResult};
