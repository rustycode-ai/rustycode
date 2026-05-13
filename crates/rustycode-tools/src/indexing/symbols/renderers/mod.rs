pub mod file_outline;
pub mod repo_map;

pub use file_outline::render_file_outline;
pub use repo_map::render_repo_map;

/// Approximate characters per token (used for budget estimation).
pub const CHARS_PER_TOKEN: usize = 4;
