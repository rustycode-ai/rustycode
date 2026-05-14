//! Shared bench tool registry — same real tools as TUI, minus interactive ones.

use rustycode_tools::providers::symbol_tools::{
    CodeContextTool, FindSymbolTool, OutlineFileTool, TsNodesTool, TsQueryTool,
};
use rustycode_tools::providers::{
    ApplyPatchTool, BashTool, EditFile, GitDiffTool, GitLogTool, GitStatusTool, GlobTool, GrepTool,
    ListDirTool, ReadFileTool, WriteFileTool,
};
use rustycode_tools::ToolRegistry;

use super::thinking_guide::ThinkingGuideTool;

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
    registry.register(ApplyPatchTool);

    // Bash — runs commands on host (native) or needs docker wrapper
    registry.register(BashTool);

    // Git tools (read-only — no GitCommit for bench)
    registry.register(GitStatusTool);
    registry.register(GitDiffTool);
    registry.register(GitLogTool);

    // Thinking guide — lightweight workflow nudge
    registry.register(ThinkingGuideTool::new());

    registry
}

/// Build a tool registry with tree-sitter symbol tools added.
///
/// Same as [`build_bench_registry`] plus symbol tools for precise code navigation.
pub fn build_bench_registry_with_symbol_tools() -> ToolRegistry {
    let mut registry = build_bench_registry();

    // Symbol tools — tree-sitter based code navigation
    registry.register(FindSymbolTool);
    registry.register(TsQueryTool);
    registry.register(TsNodesTool);
    registry.register(OutlineFileTool);
    registry.register(CodeContextTool);

    registry
}
