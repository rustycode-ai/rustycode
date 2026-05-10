//! Canonical tool name constants — single source of truth.
//!
//! All tool-name comparisons and registrations across the workspace should
//! reference these constants instead of hardcoding string literals.
//!
//! **Naming convention**: All tool names use PascalCase (e.g., `ListDir`,
//! `GitStatus`, `LspDiagnostics`). This is consistent with the LLM-facing
//! API that agents and models consume.
//!
//! `rustycode-tools-api` re-exports this module via
//! `pub use rustycode_protocol::tool_names;` so crates that depend on
//! tools-api can use either path.

// ── Core file tools ────────────────────────────────────────────────────────

pub const READ: &str = "Read";
pub const WRITE: &str = "Write";
pub const EDIT: &str = "Edit";
pub const MULTI_EDIT: &str = "MultiEdit";
pub const APPLY_PATCH: &str = "ApplyPatch";
pub const SEARCH_REPLACE: &str = "SearchReplace";
pub const NOTEBOOK_EDIT: &str = "NotebookEdit";
pub const ATOMIC_WRITE: &str = "AtomicWrite";

// ── Shell tools ────────────────────────────────────────────────────────────

pub const BASH: &str = "Bash";
pub const POWERSHELL: &str = "PowerShell";
pub const CMD: &str = "Cmd";
pub const REPL: &str = "Repl";

// ── Search / explore tools ─────────────────────────────────────────────────

pub const GREP: &str = "Grep";
pub const GLOB: &str = "Glob";
pub const FIND: &str = "Find";
pub const INSPECT: &str = "Inspect";
pub const LIST_DIR: &str = "ListDir";
pub const CODESEARCH: &str = "Codesearch";

// ── Git tools ──────────────────────────────────────────────────────────────

pub const GIT_STATUS: &str = "GitStatus";
pub const GIT_DIFF: &str = "GitDiff";
pub const GIT_LOG: &str = "GitLog";
pub const GIT_COMMIT: &str = "GitCommit";
pub const GIT_PUSH: &str = "GitPush";
pub const GIT_RESET: &str = "GitReset";

// ── Web tools ──────────────────────────────────────────────────────────────

pub const WEB_SEARCH: &str = "WebSearch";
pub const WEB_FETCH: &str = "WebFetch";
pub const BROWSER_FETCH: &str = "BrowserFetch";

// ── Todo / memory / skill tools ────────────────────────────────────────────

pub const TODO_WRITE: &str = "TodoWrite";
pub const TODO_UPDATE: &str = "TodoUpdate";
pub const TODO_READ: &str = "TodoRead";
pub const MEMORY_SEARCH: &str = "MemorySearch";
pub const MEMORY_LIST: &str = "MemoryList";
pub const SKILL_LIST: &str = "SkillList";
pub const DOCTOR: &str = "Doctor";

// ── Reasoning tools ────────────────────────────────────────────────────────

pub const REASONING_RESEARCH: &str = "ReasoningResearch";
pub const REASONING_DECOMPOSE: &str = "ReasoningDecompose";
pub const REASONING_VALIDATE: &str = "ReasoningValidate";
pub const REASONING_INTEGRATE: &str = "ReasoningIntegrate";

// ── Interactive question tool ───────────────────────────────────────────────

pub const ASK_USER_QUESTION: &str = "AskUserQuestion";

// ── MCP resource tools ─────────────────────────────────────────────────────

pub const READ_MCP_RESOURCE: &str = "ReadMcpResource";
pub const LIST_MCP_RESOURCES: &str = "ListMcpResources";

// ── Docker tools ───────────────────────────────────────────────────────────

pub const DOCKER_RUN: &str = "DockerRun";
pub const DOCKER_BUILD: &str = "DockerBuild";
pub const DOCKER_PS: &str = "DockerPs";
pub const DOCKER_STOP: &str = "DockerStop";
pub const DOCKER_LOGS: &str = "DockerLogs";
pub const DOCKER_INSPECT: &str = "DockerInspect";
pub const DOCKER_IMAGES: &str = "DockerImages";

// ── Cron tools ─────────────────────────────────────────────────────────────

pub const CRON_CREATE: &str = "CronCreate";
pub const CRON_LIST: &str = "CronList";
pub const CRON_DELETE: &str = "CronDelete";

// ── LSP tools ──────────────────────────────────────────────────────────────

pub const LSP_DIAGNOSTICS: &str = "LspDiagnostics";
pub const LSP_HOVER: &str = "LspHover";
pub const LSP_DEFINITION: &str = "LspDefinition";
pub const LSP_COMPLETION: &str = "LspCompletion";
pub const LSP_DOCUMENT_SYMBOLS: &str = "LspDocumentSymbols";
pub const LSP_REFERENCES: &str = "LspReferences";
pub const LSP_FULL_DIAGNOSTICS: &str = "LspFullDiagnostics";
pub const LSP_CODE_ACTIONS: &str = "LspCodeActions";
pub const LSP_RENAME: &str = "LspRename";
pub const LSP_FORMATTING: &str = "LspFormatting";
pub const LSP_GET_SYMBOLS_OVERVIEW: &str = "LspGetSymbolsOverview";
pub const LSP_FIND_SYMBOL: &str = "LspFindSymbol";
pub const LSP_REPLACE_SYMBOL_BODY: &str = "LspReplaceSymbolBody";
pub const LSP_INSERT_BEFORE_SYMBOL: &str = "LspInsertBeforeSymbol";
pub const LSP_INSERT_AFTER_SYMBOL: &str = "LspInsertAfterSymbol";
pub const LSP_SAFE_DELETE_SYMBOL: &str = "LspSafeDeleteSymbol";
pub const LSP_RENAME_SYMBOL: &str = "LspRenameSymbol";
pub const LSP_ANALYZE_SYMBOL: &str = "LspAnalyzeSymbol";
pub const LSP_EXTRACT_SYMBOL: &str = "LspExtractSymbol";
pub const LSP_INLINE_SYMBOL: &str = "LspInlineSymbol";
pub const LSP_WORKSPACE_SYMBOLS: &str = "LspWorkspaceSymbols";
pub const LSP_IMPLEMENTATION: &str = "LspImplementation";
pub const LSP_INCOMING_CALLS: &str = "LspIncomingCalls";
pub const LSP_OUTGOING_CALLS: &str = "LspOutgoingCalls";

// ── Text editor formats (version identifiers, not tool names) ───────────────

pub const TEXT_EDITOR_NEWEST: &str = "text_editor_20250728";
pub const TEXT_EDITOR_LEGACY: &str = "text_editor_20250124";

// ── Database / HTTP tools ──────────────────────────────────────────────────

pub const HTTP_GET: &str = "HttpGet";
pub const HTTP_POST: &str = "HttpPost";
pub const HTTP_PUT: &str = "HttpPut";
pub const HTTP_DELETE: &str = "HttpDelete";

// ── Task / delegation tools ────────────────────────────────────────────────

pub const DELEGATE_TASK: &str = "DelegateTask";
pub const SEND_MESSAGE: &str = "SendMessage";
pub const TASK_OUTPUT: &str = "TaskOutput";
pub const TASK_STOP: &str = "TaskStop";
pub const DECOMPOSE_PROBLEM: &str = "DecomposeProblem";

// ── Plan management tools ─────────────────────────────────────────────────

pub const CREATE_PLAN_FROM_TEMPLATE: &str = "CreatePlanFromTemplate";
pub const SAVE_PLAN: &str = "SavePlan";
pub const LOAD_PLAN: &str = "LoadPlan";
pub const LIST_PLANS: &str = "ListPlans";
pub const APPROVE_PLAN: &str = "ApprovePlan";

// ── Worktree tools ────────────────────────────────────────────────────────

pub const WORKTREE_CREATE: &str = "WorktreeCreate";
pub const WORKTREE_LIST: &str = "WorktreeList";
pub const WORKTREE_DELETE: &str = "WorktreeDelete";
pub const WORKTREE_ENTER: &str = "WorktreeEnter";
pub const WORKTREE_EXIT: &str = "WorktreeExit";

// ── Team tools ────────────────────────────────────────────────────────────

pub const TEAM_CREATE: &str = "TeamCreate";
pub const TEAM_DELETE: &str = "TeamDelete";

// ── Coverage / database tools ───────────────────────────────────────────

pub const COVERAGE: &str = "coverage";
pub const DATABASE_SCHEMA: &str = "database_schema";

// ── Multi-edit tool (snake_case alias) ──────────────────────────────────

pub const MULTI_EDIT_ALIAS: &str = "multi_edit";

// ── Task / batch tools (lowercase LLM-facing names) ────────────────────

pub const TASK: &str = "task";
pub const BATCH: &str = "batch";
pub const CREATE_PLAN: &str = "create_plan";

// ── Search / indexing tools ───────────────────────────────────────────────

pub const SEMANTIC_SEARCH: &str = "SemanticSearch";
pub const TOOL_SEARCH: &str = "ToolSearch";
pub const STRUCTURED_THINKING: &str = "structured_thinking";

// ── Tool classification ────────────────────────────────────────────────────

pub fn is_write_tool(name: &str) -> bool {
    match name {
        WRITE | EDIT | MULTI_EDIT | APPLY_PATCH | NOTEBOOK_EDIT | ATOMIC_WRITE => true,
        _ => has_segment(&name.to_lowercase(), WRITE_SEGMENTS),
    }
}

pub fn is_bash_tool(name: &str) -> bool {
    match name {
        BASH => true,
        _ => has_segment(&name.to_lowercase(), BASH_SEGMENTS),
    }
}

pub fn is_read_tool(name: &str) -> bool {
    match name {
        READ | GLOB | GREP | LIST_DIR | WEB_FETCH | WEB_SEARCH | FIND => true,
        _ => has_segment(&name.to_lowercase(), READ_SEGMENTS),
    }
}

pub fn is_search_tool(name: &str) -> bool {
    match name {
        GREP | GLOB | WEB_SEARCH => true,
        _ => has_segment(&name.to_lowercase(), SEARCH_SEGMENTS),
    }
}

// ── Segment matching ──────────────────────────────────────────────────────

const WRITE_SEGMENTS: &[&str] = &["write", "edit", "create", "insert", "patch", "replace"];
const BASH_SEGMENTS: &[&str] = &["bash", "shell", "exec", "run", "cmd"];
const READ_SEGMENTS: &[&str] = &["read", "view", "cat", "fetch", "get"];
const SEARCH_SEGMENTS: &[&str] = &["search", "grep", "find", "list", "glob"];

pub fn has_segment(name_lower: &str, words: &[&str]) -> bool {
    name_lower
        .split(['_', '-', ':'])
        .any(|seg| !seg.is_empty() && words.contains(&seg))
}

// ── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_match_write_tools() {
        assert!(is_write_tool(WRITE));
        assert!(is_write_tool(EDIT));
        assert!(is_write_tool(MULTI_EDIT));
        assert!(is_write_tool(APPLY_PATCH));
        assert!(is_write_tool(NOTEBOOK_EDIT));
        assert!(!is_write_tool(READ));
        assert!(!is_write_tool(BASH));
    }

    #[test]
    fn exact_match_bash_tools() {
        assert!(is_bash_tool(BASH));
        assert!(!is_bash_tool(WRITE));
        assert!(!is_bash_tool(READ));
    }

    #[test]
    fn exact_match_read_tools() {
        assert!(is_read_tool(READ));
        assert!(is_read_tool(GLOB));
        assert!(is_read_tool(GREP));
        assert!(is_read_tool(LIST_DIR));
        assert!(!is_read_tool(WRITE));
        assert!(!is_read_tool(BASH));
    }

    #[test]
    fn segment_match_mcp_tools() {
        assert!(is_write_tool("mcp__server__write_file"));
        assert!(is_write_tool("create_record"));
        assert!(is_bash_tool("shell_exec"));
        assert!(is_read_tool("fetch_data"));
        assert!(is_search_tool("grep_content"));
    }

    #[test]
    fn no_false_positives() {
        assert!(!is_write_tool("thread_reader"));
        assert!(!is_write_tool("already_written"));
        assert!(!is_bash_tool("runtime_check"));
        assert!(!is_read_tool("already_ready"));
        assert!(!is_search_tool("listener_port"));
    }

    #[test]
    fn pascal_case_exact() {
        assert!(is_write_tool("Write"));
        assert!(is_write_tool("Edit"));
        assert!(is_read_tool("ListDir"));
        assert!(is_read_tool("Grep"));
        assert!(is_bash_tool("Bash"));
    }

    #[test]
    fn has_segment_basic() {
        assert!(has_segment("write_file", &["write"]));
        assert!(has_segment("mcp__server__edit", &["edit"]));
        assert!(has_segment("shell-exec", &["exec"]));
        assert!(!has_segment("thread_reader", &["read"]));
    }

    #[test]
    fn all_constants_are_pascal_case() {
        // Verify all tool name constants follow PascalCase
        let pascal_names: &[&str] = &[
            READ,
            WRITE,
            EDIT,
            MULTI_EDIT,
            APPLY_PATCH,
            SEARCH_REPLACE,
            NOTEBOOK_EDIT,
            ATOMIC_WRITE,
            BASH,
            POWERSHELL,
            CMD,
            REPL,
            GREP,
            GLOB,
            FIND,
            INSPECT,
            LIST_DIR,
            CODESEARCH,
            GIT_STATUS,
            GIT_DIFF,
            GIT_LOG,
            GIT_COMMIT,
            GIT_PUSH,
            GIT_RESET,
            WEB_SEARCH,
            WEB_FETCH,
            BROWSER_FETCH,
            TODO_WRITE,
            TODO_UPDATE,
            TODO_READ,
            MEMORY_SEARCH,
            MEMORY_LIST,
            SKILL_LIST,
            DOCTOR,
            DOCKER_RUN,
            DOCKER_BUILD,
            DOCKER_PS,
            DOCKER_STOP,
            DOCKER_LOGS,
            DOCKER_INSPECT,
            DOCKER_IMAGES,
            CRON_CREATE,
            CRON_LIST,
            CRON_DELETE,
            LSP_DIAGNOSTICS,
            LSP_HOVER,
            LSP_DEFINITION,
            LSP_COMPLETION,
            LSP_DOCUMENT_SYMBOLS,
            LSP_REFERENCES,
        ];
        for name in pascal_names {
            // First char must be uppercase
            assert!(
                name.chars().next().is_some_and(|c| c.is_uppercase()),
                "Tool name '{}' should start with uppercase (PascalCase)",
                name
            );
        }
    }
}
