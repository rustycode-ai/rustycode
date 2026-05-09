use rustycode_tools::providers::lsp::{
    LspAnalyzeSymbolTool, LspCodeActionsTool, LspCompletionTool, LspDefinitionTool,
    LspDiagnosticsTool, LspDocumentSymbolsTool, LspExtractSymbolTool, LspFindSymbolTool,
    LspFormattingTool, LspFullDiagnosticsTool, LspGetSymbolsOverviewTool, LspHoverTool,
    LspInlineSymbolTool, LspInsertAfterSymbolTool, LspInsertBeforeSymbolTool, LspReferencesTool,
    LspRenameSymbolTool, LspRenameTool, LspReplaceSymbolBodyTool, LspSafeDeleteSymbolTool,
};
use rustycode_tools::{ToolContext, ToolExecutor, ToolRegistry};
use std::path::PathBuf;
use std::sync::Arc;

/// Build a tool executor that exposes the core LSP tool set over MCP.
///
/// The standalone MCP server uses this executor so clients can discover and
/// call the same LSP tools that the TUI exposes for navigation and refactors.
pub fn build_lsp_tool_executor(workspace_root: PathBuf) -> ToolExecutor {
    let mut registry = ToolRegistry::new();
    register_lsp_tools(&mut registry);
    ToolExecutor::new(Arc::new(registry), ToolContext::new(workspace_root))
}

fn register_lsp_tools(registry: &mut ToolRegistry) {
    registry.register(LspDiagnosticsTool);
    registry.register(LspHoverTool);
    registry.register(LspDefinitionTool);
    registry.register(LspCompletionTool);
    registry.register(LspDocumentSymbolsTool);
    registry.register(LspReferencesTool);
    registry.register(LspFullDiagnosticsTool);
    registry.register(LspCodeActionsTool);
    registry.register(LspRenameTool);
    registry.register(LspFormattingTool);
    registry.register(LspGetSymbolsOverviewTool);
    registry.register(LspFindSymbolTool);
    registry.register(LspReplaceSymbolBodyTool);
    registry.register(LspInsertBeforeSymbolTool);
    registry.register(LspInsertAfterSymbolTool);
    registry.register(LspSafeDeleteSymbolTool);
    registry.register(LspRenameSymbolTool);
    registry.register(LspAnalyzeSymbolTool);
    registry.register(LspExtractSymbolTool);
    registry.register(LspInlineSymbolTool);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lsp_executor_lists_navigation_tools() {
        let executor = build_lsp_tool_executor(PathBuf::from("."));
        let names: Vec<String> = executor.list().into_iter().map(|tool| tool.name).collect();

        assert!(names.contains(&"LspHover".to_string()));
        assert!(names.contains(&"LspDefinition".to_string()));
        assert!(names.contains(&"LspCompletion".to_string()));
        assert!(names.contains(&"LspDiagnostics".to_string()));
    }
}
