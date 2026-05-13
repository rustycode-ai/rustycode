use crate::ToolFilter;
use rustycode_tools_api::ToolRegistry;

/// Create a default tool registry with all zero-config built-in tools.
///
/// Includes all built-in tools except those requiring runtime state (todo, semantic search, agent).
/// Stateful tools must be registered separately by the caller.
pub fn default_registry() -> ToolRegistry {
    default_registry_filtered(&ToolFilter::full(
        std::env::current_dir().unwrap_or_default(),
    ))
}

/// Create a tool registry filtered by platform, provider, and runtime environment.
///
/// Tools are registered conditionally based on what the environment supports.
/// Provider caps are applied at registration time; for mid-session provider changes,
/// use `ToolRegistry::list_for_filter()` to re-filter the tool list per request.
pub fn default_registry_filtered(filter: &ToolFilter) -> ToolRegistry {
    use crate::providers::brief::BriefTool;
    use crate::providers::codesearch::CodeSearchTool;
    use crate::providers::fs::apply_patch::ApplyPatchTool;
    use crate::providers::fs::edit::EditFile;
    use crate::providers::fs::multiedit::MultiEditTool;
    use crate::providers::lsp::*;
    use crate::providers::notebook::NotebookEditTool;
    use crate::providers::send_message::SendMessageTool;
    use crate::providers::task_output::{TaskOutputTool, TaskStopTool};
    use crate::providers::tool_search::ToolSearchTool;
    use crate::providers::web::browser::BrowserFetchTool;
    use crate::providers::web::fetch::WebFetchTool;
    use crate::providers::web::search::WebSearchTool;
    use crate::providers::symbol_tools::{CheckSymbolDriftTool, CodeContextTool, FindSymbolTool, OutlineFileTool};
    use crate::providers::{
        BashTool, CmdTool, FindTool, GlobTool, GrepTool, InspectTool, ListDirTool, PowerShellTool,
        QuestionTool, ReadFileTool, WriteFileTool,
    };

    let mut reg = ToolRegistry::new();

    // If the provider doesn't support tools at all, return empty registry.
    if !filter.should_register_tools() {
        return reg;
    }

    // Core file system tools — always registered.
    reg.register(ReadFileTool);
    reg.register(WriteFileTool);
    reg.register(ListDirTool);
    reg.register(EditFile);

    // Search & exploration — always registered for non-local providers.
    reg.register(GrepTool);
    reg.register(GlobTool);
    reg.register(FindTool);
    if !filter.provider_caps.is_local {
        reg.register(InspectTool);
        reg.register(CodeSearchTool);
    }
    reg.register(ApplyPatchTool);

    // Command execution — register platform-appropriate shells.
    reg.register(BashTool);
    if filter.should_register_pwsh() {
        reg.register(PowerShellTool);
    }
    if filter.should_register_cmd() {
        reg.register(CmdTool);
    }

    // LSP tools — only if LSP is available and provider handles complex schemas.
    if filter.should_register_lsp() {
        reg.register(LspDiagnosticsTool);
        reg.register(LspHoverTool);
        reg.register(LspDefinitionTool);
        reg.register(LspCompletionTool);
        reg.register(LspDocumentSymbolsTool);
        reg.register(LspReferencesTool);
        reg.register(LspFullDiagnosticsTool);
        reg.register(LspCodeActionsTool);
        reg.register(LspRenameTool);
        reg.register(LspFormattingTool);
        reg.register(LspGetSymbolsOverviewTool);
        reg.register(LspFindSymbolTool);
        reg.register(LspReplaceSymbolBodyTool);
        reg.register(LspInsertBeforeSymbolTool);
        reg.register(LspInsertAfterSymbolTool);
        reg.register(LspSafeDeleteSymbolTool);
        reg.register(LspRenameSymbolTool);
        reg.register(LspAnalyzeSymbolTool);
        reg.register(LspExtractSymbolTool);
        reg.register(LspInlineSymbolTool);
        reg.register(LspWorkspaceSymbolsTool);
    }

    // Web & search — skip browser_fetch for local providers (complex schema).
    reg.register(WebFetchTool);
    if filter.should_register_complex_tools() {
        reg.register(BrowserFetchTool);
    }
    reg.register(WebSearchTool);

    // Complex edit tools — skip for local providers (large schemas).
    if filter.should_register_complex_tools() {
        reg.register(MultiEditTool);
    }
    if filter.should_register_python() {
        reg.register(NotebookEditTool);
    }

    // Task management & communication.
    reg.register(BriefTool);
    reg.register(SendMessageTool);
    reg.register(TaskOutputTool);
    reg.register(TaskStopTool);
    reg.register(ToolSearchTool);

    // Symbol tools — fast structural navigation without reading entire files.
    reg.register(FindSymbolTool);
    reg.register(CodeContextTool);
    reg.register(OutlineFileTool);
    reg.register(CheckSymbolDriftTool);
    reg.register(StructuralPatchTool);

    // Interactive tools — always registered.
    reg.register(QuestionTool);

    reg
}

#[cfg(test)]
mod filtered_registry_tests {
    use super::*;
    use rustycode_tools_api::{PlatformEnv, ProviderCaps, RuntimeEnv};

    fn tool_names(registry: &ToolRegistry) -> Vec<String> {
        registry.list().iter().map(|t| t.name.clone()).collect()
    }

    #[test]
    fn no_tools_provider_returns_empty_registry() {
        let filter = ToolFilter {
            platform: PlatformEnv::default(),
            provider_caps: ProviderCaps::no_tools(),
            runtime: RuntimeEnv::full(std::env::current_dir().unwrap()),
        };
        let reg = default_registry_filtered(&filter);
        assert!(reg.list().is_empty());
    }

    #[test]
    fn local_provider_skips_complex_tools() {
        let filter = ToolFilter {
            platform: PlatformEnv::default(),
            provider_caps: ProviderCaps::local(),
            runtime: RuntimeEnv::full(std::env::current_dir().unwrap()),
        };
        let reg = default_registry_filtered(&filter);
        let names = tool_names(&reg);

        // Core tools still present
        assert!(names.contains(&"Read".to_string()));
        assert!(names.contains(&"Bash".to_string()));
        assert!(names.contains(&"Grep".to_string()));

        // Complex-schema tools skipped
        assert!(!names.iter().any(|n| n.starts_with("lsp_")));
        assert!(!names.contains(&"MultiEdit".to_string()));
        assert!(!names.contains(&"BrowserFetch".to_string()));
    }

    #[test]
    fn no_git_repo_skips_git_tools() {
        let mut runtime = RuntimeEnv::full(std::env::current_dir().unwrap());
        runtime.has_git_repo = false;
        let filter = ToolFilter {
            platform: PlatformEnv::default(),
            provider_caps: ProviderCaps::full(),
            runtime,
        };
        let reg = default_registry_filtered(&filter);
        let names = tool_names(&reg);

        assert!(!names.contains(&"GitStatus".to_string()));
        assert!(!names.contains(&"GitDiff".to_string()));
        assert!(!names.contains(&"GitLog".to_string()));
        assert!(!names.contains(&"GitCommit".to_string()));
    }

    #[test]
    fn full_filter_registers_all_core_tools() {
        let filter = ToolFilter::full(std::env::current_dir().unwrap());
        let reg = default_registry_filtered(&filter);
        let names = tool_names(&reg);

        assert!(names.contains(&"Read".to_string()));
        assert!(names.contains(&"Write".to_string()));
        assert!(names.contains(&"Edit".to_string()));
        assert!(names.contains(&"Bash".to_string()));
        assert!(names.contains(&"Grep".to_string()));
        assert!(names.contains(&"Glob".to_string()));
    }

    #[test]
    fn lsp_tools_require_lsp_running() {
        let mut runtime = RuntimeEnv::full(std::env::current_dir().unwrap());
        runtime.has_lsp_running = false;
        let filter = ToolFilter {
            platform: PlatformEnv::default(),
            provider_caps: ProviderCaps::full(),
            runtime,
        };
        let reg = default_registry_filtered(&filter);
        let names = tool_names(&reg);

        assert!(!names.iter().any(|n| n.starts_with("lsp_")));
    }
}
