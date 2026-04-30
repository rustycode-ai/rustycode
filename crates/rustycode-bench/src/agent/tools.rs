//! Shared bench tool registry — same real tools as TUI, minus interactive ones.

use rustycode_tools::providers::{
    BashTool, EditFile, GitDiffTool, GitLogTool, GitStatusTool, GlobTool, GrepTool, ListDirTool,
    ReadFileTool, SearchReplace, WriteFileTool,
};
use rustycode_tools::ToolRegistry;

/// Build a tool registry with production tools suitable for benchmarking.
///
/// Uses the same real tools as the TUI. Interactive tools (AskUser, Question)
/// and LSP tools (no language server in bench containers) are excluded.
pub fn build_bench_registry() -> ToolRegistry {
    let mut registry = ToolRegistry::new();

    // File tools — operate on ctx.cwd (workspace path from environment)
    registry.register(ReadFileTool);
    registry.register(WriteFileTool);
    registry.register(ListDirTool);
    registry.register(EditFile);
    registry.register(GrepTool);
    registry.register(GlobTool);
    registry.register(SearchReplace);

    // Bash — runs commands on host (native) or needs docker wrapper
    registry.register(BashTool);

    // Git tools (read-only — no GitCommit for bench)
    registry.register(GitStatusTool);
    registry.register(GitDiffTool);
    registry.register(GitLogTool);

    // Structured thinking — task decomposition, strategy selection, phase tracking
    #[cfg(feature = "real-agent")]
    registry.register(rustycode_orchestration::structured_thinking_tool_impl::StructuredThinkingTool::new(None));

    registry
}
