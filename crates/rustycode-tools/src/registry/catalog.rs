//! Tool Catalog - Enum-based registry for all rustycode tools
//!
//! Inspired by `forge_domain`'s `ToolCatalog` pattern, this module provides an
//! exhaustive enumeration of all tools with tagged serialization, case-insensitive
//! lookup, and policy operation mapping.
//!
//! # Architecture
//!
//! The `ToolCatalog` enum provides:
//! - **Exhaustive enumeration**: All tools in one place, compile-time verified
//! - **Tagged serialization**: Internally tagged serde format for consistent JSON
//! - **Type safety**: Each variant carries its own input type
//! - **Case-insensitive lookup**: Tool names are case-insensitive for UX
//! - **Policy mapping**: Direct mapping to permission levels
//!
//! # Example
//!
//! ```ignore
//!
//! // Serialize a tool call
//! let tool = ToolCatalog::ReadFile(ReadFileInput {
//!     file_path: "/path/to/file.txt".to_string(),
//!     offset: None,
//!     limit: None,
//! });
//!
//! let json = to_value(&tool).unwrap();
//! assert_eq!(json["name"], "read_file");
//!
//! // Check if tool exists
//! assert!(ToolCatalog::contains("ReadFile"));  // Case-insensitive
//! assert!(ToolCatalog::contains("read_file"));
//! ```

use serde::{Deserialize, Serialize};

/// Master enum of all built-in tools.
///
/// Uses internally tagged serde for consistent serialization format:
/// ```json
/// {
///   "name": "read_file",
///   "arguments": {
///     "file_path": "/path/to/file.txt",
///     "offset": null,
///     "limit": null
///   }
/// }
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "name", content = "arguments", rename_all = "snake_case")]
#[non_exhaustive]
pub enum ToolCatalog {
    // File operations
    ReadFile(ReadFileInput),
    WriteFile(WriteFileInput),
    EditFile(EditFileInput),
    ListDir(ListDirInput),

    // Shell operations
    Bash(BashInput),

    // Code intelligence
    Glob(GlobInput),
    Grep(GrepInput),

    // Version control
    GitStatus(GitStatusInput),
    GitDiff(GitDiffInput),
    GitLog(GitLogInput),
    GitCommit(GitCommitInput),

    // Language Server Protocol
    LspDiagnostics(LspDiagnosticsInput),
    LspHover(LspHoverInput),
    LspDefinition(LspDefinitionInput),
    LspCompletion(LspCompletionInput),
    LspDocumentSymbols(LspDocumentSymbolsInput),
    LspReferences(LspReferencesInput),

    // Web operations
    WebFetch(WebFetchInput),
    WebSearch(WebSearchInput),

    // Task management
    TaskCreate(TaskCreateInput),
    TaskUpdate(TaskUpdateInput),
    TaskList,
    TaskGet(TaskGetInput),

    // Plan management
    PlanCreate(PlanCreateInput),
    PlanSave(PlanSaveInput),
    PlanLoad(PlanLoadInput),
    PlanList(PlanListInput),

    // Testing
    RunTests(RunTestsInput),
    RunTest(RunTestInput),
    RunBench(RunBenchInput),
    Coverage(CoverageInput),

    // Docker
    DockerBuild(DockerBuildInput),
    DockerRun(DockerRunInput),
    DockerPs(DockerPsInput),
    DockerStop(DockerStopInput),
    DockerLogs(DockerLogsInput),
    DockerInspect(DockerInspectInput),
    DockerImages(DockerImagesInput),

    // Code editing
    MultiEdit(MultiEditInput),
    ApplyPatch(ApplyPatchInput),
    ClaudeTextEditor(ClaudeTextEditorInput),

    // Code intelligence
    CodeSearch(CodeSearchInput),
    SemanticSearch(SemanticSearchInput),
    Question(QuestionInput),

    // Database
    DatabaseQuery(DatabaseQueryInput),
    DatabaseSchema(DatabaseSchemaInput),
    DatabaseTransaction(DatabaseTransactionInput),

    // HTTP API
    HttpGet(HttpGetInput),
    HttpPost(HttpPostInput),
    HttpPut(HttpPutInput),
    HttpDelete(HttpDeleteInput),

    // Notebook editing
    NotebookEdit(NotebookEditInput),

    // Skill discovery
    Skill(SkillInput),

    // Worktree management
    WorktreeCreate(WorktreeCreateInput),
    WorktreeList,
    WorktreeDelete(WorktreeDeleteInput),

    // User communication
    Brief(BriefInput),

    // Inter-agent communication
    SendMessage(SendMessageInput),

    // Background task management
    TaskOutput(TaskOutputInput),
    TaskStop(TaskStopInput),

    // Team coordination
    TeamCreate(TeamCreateInput),
    TeamDelete,

    // Cron scheduling
    CronCreate(CronCreateInput),
    CronDelete(CronDeleteInput),
    CronList,

    // Tool search (deferred loading)
    ToolSearch(ToolSearchInput),

    // MCP resources
    ListMcpResources(ListMcpResourcesInput),
    ReadMcpResource(ReadMcpResourceInput),

    // REPL
    REPL(REPLInput),
}

// Input Types

/// Input for reading a file
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReadFileInput {
    pub file_path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub offset: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<usize>,
}

/// Input for writing a file
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WriteFileInput {
    pub file_path: String,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub create_parents: Option<bool>,
}

/// Input for editing a file (search and replace)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EditFileInput {
    pub file_path: String,
    pub old_string: String,
    pub new_string: String,
}

/// Input for listing directory contents
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ListDirInput {
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recursive: Option<bool>,
}

/// Input for bash execution
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BashInput {
    pub command: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout_secs: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub restart: Option<bool>,
}

/// Input for glob pattern matching
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GlobInput {
    pub pattern: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
}

/// Input for grep search
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GrepInput {
    pub pattern: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub before_context: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub after_context: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_matches_per_file: Option<usize>,
}

/// Input for git status
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GitStatusInput {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
}

/// Input for git diff
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GitDiffInput {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cached: Option<bool>,
}

/// Input for git log
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GitLogInput {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_count: Option<usize>,
}

/// Input for git commit
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GitCommitInput {
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub paths: Option<Vec<String>>,
}

/// Input for LSP diagnostics
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LspDiagnosticsInput {
    pub file_path: String,
}

/// Input for LSP hover
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LspHoverInput {
    pub file_path: String,
    pub line: usize,
    pub character: usize,
}

/// Input for LSP definition
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LspDefinitionInput {
    pub file_path: String,
    pub line: usize,
    pub character: usize,
}

/// Input for LSP completion
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LspCompletionInput {
    pub file_path: String,
    pub line: usize,
    pub character: usize,
}

/// Input for LSP document symbols
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LspDocumentSymbolsInput {
    pub file_path: String,
}

/// Input for LSP references
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LspReferencesInput {
    pub file_path: String,
    pub line: usize,
    pub character: usize,
}

/// Input for web fetch
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WebFetchInput {
    pub url: String,
}

/// Input for web search
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WebSearchInput {
    pub query: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub num_results: Option<usize>,
}

/// Input for creating a task
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TaskCreateInput {
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,
}

/// Input for updating a task
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TaskUpdateInput {
    pub task_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// Input for getting a task
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TaskGetInput {
    pub task_id: String,
}

/// Input for creating a plan
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PlanCreateInput {
    pub template_name: String,
    pub parameters: serde_json::Value,
}

/// Input for saving a plan
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PlanSaveInput {
    pub plan_id: String,
    pub content: String,
}

/// Input for loading a plan
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PlanLoadInput {
    pub plan_id: String,
}

/// Input for listing plans
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PlanListInput {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filter: Option<String>,
}

/// Input for running tests
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RunTestsInput {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub args: Option<Vec<String>>,
}

/// Input for running a specific test
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RunTestInput {
    pub test_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub args: Option<Vec<String>>,
}

/// Input for running benchmarks
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RunBenchInput {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub args: Option<Vec<String>>,
}

/// Input for coverage report
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CoverageInput {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub format: Option<String>,
}

/// Input for Docker build
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DockerBuildInput {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dockerfile: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tag: Option<String>,
}

/// Input for Docker run
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DockerRunInput {
    pub image: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub args: Option<Vec<String>>,
}

/// Input for Docker ps
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DockerPsInput {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub all: Option<bool>,
}

/// Input for Docker stop
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DockerStopInput {
    pub container_id: String,
}

/// Input for Docker logs
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DockerLogsInput {
    pub container_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tail: Option<String>,
}

/// Input for Docker inspect
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DockerInspectInput {
    pub container_id: String,
}

/// Input for Docker images
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DockerImagesInput {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub all: Option<bool>,
}

/// Input for multi-edit
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MultiEditInput {
    pub file_path: String,
    pub edits: Vec<EditOperation>,
}

/// Single edit operation
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EditOperation {
    pub old_string: String,
    pub new_string: String,
}

/// Input for applying a patch
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ApplyPatchInput {
    pub patch_content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strip_level: Option<usize>,
}

/// Input for Claude text editor
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ClaudeTextEditorInput {
    pub file_path: String,
    pub edits: Vec<TextEdit>,
}

/// Single text edit operation
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TextEdit {
    pub old_text: String,
    pub new_text: String,
}

/// Input for code search
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CodeSearchInput {
    pub query: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
}

/// Input for semantic search
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SemanticSearchInput {
    pub query: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
}

/// Input for question answering
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct QuestionInput {
    pub question: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context: Option<String>,
}

/// Input for database query
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DatabaseQueryInput {
    pub query: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<Vec<serde_json::Value>>,
}

/// Input for database schema
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DatabaseSchemaInput {
    pub table_name: String,
}

/// Input for database transaction
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DatabaseTransactionInput {
    pub queries: Vec<String>,
}

/// Input for HTTP GET
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HttpGetInput {
    pub url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub headers: Option<std::collections::HashMap<String, String>>,
}

/// Input for HTTP POST
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HttpPostInput {
    pub url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub headers: Option<std::collections::HashMap<String, String>>,
}

/// Input for HTTP PUT
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HttpPutInput {
    pub url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub headers: Option<std::collections::HashMap<String, String>>,
}

/// Input for HTTP DELETE
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HttpDeleteInput {
    pub url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub headers: Option<std::collections::HashMap<String, String>>,
}

/// Input for notebook cell editing
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NotebookEditInput {
    pub notebook_path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cell_id: Option<String>,
    pub new_source: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cell_type: Option<String>,
    #[serde(default = "default_edit_mode")]
    pub edit_mode: String,
}

fn default_edit_mode() -> String {
    "replace".to_string()
}

/// Input for skill discovery and invocation
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SkillInput {
    pub skill: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub args: Option<String>,
}

/// Input for creating a worktree
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorktreeCreateInput {
    pub name: String,
    pub branch: String,
    #[serde(default = "default_worktree_type")]
    pub wt_type: String,
}

fn default_worktree_type() -> String {
    "feature".to_string()
}

/// Input for deleting a worktree
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorktreeDeleteInput {
    pub name: String,
}

/// Input for brief (user message)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BriefInput {
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attachments: Option<Vec<String>>,
}

/// Input for `send_message` (inter-agent)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SendMessageInput {
    pub to: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
}

/// Input for `task_output`
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TaskOutputInput {
    pub task_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub block: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout: Option<u64>,
}

/// Input for `task_stop`
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TaskStopInput {
    pub task_id: String,
}

/// Input for `team_create`
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TeamCreateInput {
    pub team_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_type: Option<String>,
}

/// Input for `cron_create`
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CronCreateInput {
    pub cron: String,
    pub prompt: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recurring: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub durable: Option<bool>,
}

/// Input for `cron_delete`
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CronDeleteInput {
    pub id: String,
}

/// Input for `tool_search`
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ToolSearchInput {
    pub query: String,
}

/// Input for `list_mcp_resources`
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ListMcpResourcesInput {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub server: Option<String>,
}

/// Input for `read_mcp_resource`
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReadMcpResourceInput {
    pub server: String,
    pub uri: String,
}

/// Input for REPL execution
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct REPLInput {
    pub action: String,
    pub research_session_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub execution_timeout: Option<u64>,
}

// Permission Levels

/// Permission level required for a tool
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum ToolPermission {
    /// No restrictions (informational operations)
    None,
    /// Read-only filesystem access
    Read,
    /// Write filesystem access
    Write,
    /// Execute commands
    Execute,
    /// Network access
    Network,
}

// Tool Catalog Implementation

/// Normalize a tool name to `snake_case`.
///
/// Converts "`ReadFile`" → "`read_file`", "`READ_FILE`" → "`read_file`",
/// "`read_file`" → "`read_file`" (unchanged).
#[allow(dead_code)]
fn normalize_tool_name(name: &str) -> String {
    // First, check if already snake_case (contains underscores)
    if name.contains('_') {
        return name.to_lowercase();
    }
    // Convert CamelCase to snake_case
    let mut result = String::with_capacity(name.len() + 4);
    for (i, ch) in name.chars().enumerate() {
        if ch.is_uppercase() {
            if i > 0 {
                result.push('_');
            }
            result.extend(ch.to_lowercase());
        } else {
            result.push(ch);
        }
    }
    result
}

impl ToolCatalog {
    /// Check if a tool name is registered (case-insensitive)
    ///
    /// # Example
    ///
    /// ```ignore
    /// use rustycode_tools::tool_registry::ToolCatalog;
    ///
    /// let tool = ToolCatalog::ReadFile(ReadFileInput {
    ///     file_path: "/path/to/file.txt".to_string(),
    ///     offset: None,
    ///     limit: None,
    /// });
    ///
    /// assert!(tool.description().contains("file"));
    /// ```
    pub const fn description(&self) -> &'static str {
        match self {
            // File operations
            Self::ReadFile(_) => "Read file contents from the local filesystem",
            Self::WriteFile(_) => "Write content to a file on the local filesystem",
            Self::EditFile(_) => "Perform exact string replacements in files",
            Self::ListDir(_) => "List directory contents",

            // Shell operations
            Self::Bash(_) => "Execute a bash command and return output",

            // Code intelligence
            Self::Glob(_) => "Find files matching a glob pattern",
            Self::Grep(_) => "Search file contents using ripgrep patterns",

            // Version control
            Self::GitStatus(_) => "Get git working tree status",
            Self::GitDiff(_) => "Show git diff",
            Self::GitLog(_) => "Show git commit history",
            Self::GitCommit(_) => "Create a git commit",

            // Language Server Protocol
            Self::LspDiagnostics(_) => "Get LSP diagnostics for a file",
            Self::LspHover(_) => "Get hover information from LSP",
            Self::LspDefinition(_) => "Go to definition using LSP",
            Self::LspCompletion(_) => "Get code completions from LSP",
            Self::LspDocumentSymbols(_) => "Get document symbols from LSP",
            Self::LspReferences(_) => "Find references using LSP",

            // Web operations
            Self::WebFetch(_) => "Fetch content from a URL",
            Self::WebSearch(_) => "Search the web",

            // Task management
            Self::TaskCreate(_) => "Create a new task",
            Self::TaskUpdate(_) => "Update an existing task",
            Self::TaskList => "List all tasks",
            Self::TaskGet(_) => "Get details of a specific task",

            // Plan management
            Self::PlanCreate(_) => "Create a plan from template",
            Self::PlanSave(_) => "Save a plan",
            Self::PlanLoad(_) => "Load a plan",
            Self::PlanList(_) => "List all plans",

            // Testing
            Self::RunTests(_) => "Run all tests",
            Self::RunTest(_) => "Run a specific test",
            Self::RunBench(_) => "Run benchmarks",
            Self::Coverage(_) => "Generate code coverage report",

            // Docker
            Self::DockerBuild(_) => "Build a Docker image",
            Self::DockerRun(_) => "Run a Docker container",
            Self::DockerPs(_) => "List Docker containers",
            Self::DockerStop(_) => "Stop a Docker container",
            Self::DockerLogs(_) => "Get Docker container logs",
            Self::DockerInspect(_) => "Inspect a Docker container",
            Self::DockerImages(_) => "List Docker images",

            // Code editing
            Self::MultiEdit(_) => "Apply multiple edits to a file",
            Self::ApplyPatch(_) => "Apply a patch file",
            Self::ClaudeTextEditor(_) => "Edit files using Claude text editor",

            // Code intelligence
            Self::CodeSearch(_) => "Search code using code search API",
            Self::SemanticSearch(_) => "Semantic code search",
            Self::Question(_) => "Ask a question about code",

            // Database
            Self::DatabaseQuery(_) => "Execute a database query",
            Self::DatabaseSchema(_) => "Get database schema",
            Self::DatabaseTransaction(_) => "Execute a database transaction",

            // HTTP API
            Self::HttpGet(_) => "HTTP GET request",
            Self::HttpPost(_) => "HTTP POST request",
            Self::HttpPut(_) => "HTTP PUT request",
            Self::HttpDelete(_) => "HTTP DELETE request",
            _ => "General tool",
        }
    }

    /// Get the permission level required for this tool
    ///
    /// # Example
    ///
    /// ```ignore
    /// use rustycode_tools::tool_registry::{ToolCatalog, ToolPermission};
    ///
    /// let read_tool = ToolCatalog::ReadFile(ReadFileInput {
    ///     file_path: "/path/to/file.txt".to_string(),
    ///     offset: None,
    ///     limit: None,
    /// });
    ///
    /// assert_eq!(read_tool.permission(), ToolPermission::Read);
    /// ```
    pub const fn permission(&self) -> ToolPermission {
        match self {
            // Read-only operations
            Self::ReadFile(_)
            | Self::ListDir(_)
            | Self::Glob(_)
            | Self::Grep(_)
            | Self::GitStatus(_)
            | Self::GitDiff(_)
            | Self::GitLog(_)
            | Self::LspDiagnostics(_)
            | Self::LspHover(_)
            | Self::LspDefinition(_)
            | Self::LspCompletion(_)
            | Self::LspDocumentSymbols(_)
            | Self::LspReferences(_)
            | Self::TaskList
            | Self::TaskGet(_)
            | Self::PlanList(_)
            | Self::DockerPs(_)
            | Self::DockerImages(_)
            | Self::CodeSearch(_)
            | Self::SemanticSearch(_)
            | Self::Question(_)
            | Self::DatabaseSchema(_)
            | Self::Coverage(_)
            | Self::WorktreeList
            | Self::TaskOutput(_)
            | Self::CronList
            | Self::ListMcpResources(_)
            | Self::ReadMcpResource(_) => ToolPermission::Read,

            // No permission required (informational/invocation)
            Self::Skill(_)
            | Self::Brief(_)
            | Self::SendMessage(_)
            | Self::CronCreate(_)
            | Self::ToolSearch(_) => ToolPermission::None,

            // Write operations
            Self::WriteFile(_)
            | Self::EditFile(_)
            | Self::GitCommit(_)
            | Self::TaskCreate(_)
            | Self::TaskUpdate(_)
            | Self::PlanCreate(_)
            | Self::PlanSave(_)
            | Self::MultiEdit(_)
            | Self::ApplyPatch(_)
            | Self::ClaudeTextEditor(_)
            | Self::NotebookEdit(_)
            | Self::TeamCreate(_)
            | Self::TeamDelete => ToolPermission::Write,

            // Execute operations
            Self::Bash(_)
            | Self::RunTests(_)
            | Self::RunTest(_)
            | Self::RunBench(_)
            | Self::DockerBuild(_)
            | Self::DockerRun(_)
            | Self::DockerStop(_)
            | Self::DockerLogs(_)
            | Self::DockerInspect(_)
            | Self::DatabaseQuery(_)
            | Self::DatabaseTransaction(_)
            | Self::WorktreeCreate(_)
            | Self::WorktreeDelete(_)
            | Self::TaskStop(_)
            | Self::CronDelete(_)
            | Self::REPL(_) => ToolPermission::Execute,

            // Network operations
            Self::WebFetch(_)
            | Self::WebSearch(_)
            | Self::PlanLoad(_)
            | Self::HttpGet(_)
            | Self::HttpPost(_)
            | Self::HttpPut(_)
            | Self::HttpDelete(_) => ToolPermission::Network,
        }
    }

    /// Get the tool name as a string
    ///
    /// # Example
    ///
    /// ```ignore
    /// use rustycode_tools::tool_registry::ToolCatalog;
    ///
    /// let tool = ToolCatalog::ReadFile(ReadFileInput {
    ///     file_path: "/path/to/file.txt".to_string(),
    ///     offset: None,
    ///     limit: None,
    /// });
    /// assert_eq!(tool.name(), "read_file");
    /// ```
    pub const fn name(&self) -> &'static str {
        match self {
            Self::ReadFile(_) => "read_file",
            Self::WriteFile(_) => "write_file",
            Self::EditFile(_) => "edit_file",
            Self::ListDir(_) => "list_dir",
            Self::Bash(_) => "bash",
            Self::Glob(_) => "glob",
            Self::Grep(_) => "grep",
            Self::GitStatus(_) => "git_status",
            Self::GitDiff(_) => "git_diff",
            Self::GitLog(_) => "git_log",
            Self::GitCommit(_) => "git_commit",
            Self::LspDiagnostics(_) => "lsp_diagnostics",
            Self::LspHover(_) => "lsp_hover",
            Self::LspDefinition(_) => "lsp_definition",
            Self::LspCompletion(_) => "lsp_completion",
            Self::LspDocumentSymbols(_) => "lsp_document_symbols",
            Self::LspReferences(_) => "lsp_references",
            Self::WebFetch(_) => "web_fetch",
            Self::WebSearch(_) => "web_search",
            Self::TaskCreate(_) => "task_create",
            Self::TaskUpdate(_) => "task_update",
            Self::TaskList => "task_list",
            Self::TaskGet(_) => "task_get",
            Self::PlanCreate(_) => "plan_create",
            Self::PlanSave(_) => "plan_save",
            Self::PlanLoad(_) => "plan_load",
            Self::PlanList(_) => "plan_list",
            Self::RunTests(_) => "run_tests",
            Self::RunTest(_) => "run_test",
            Self::RunBench(_) => "run_bench",
            Self::Coverage(_) => "coverage",
            Self::DockerBuild(_) => "docker_build",
            Self::DockerRun(_) => "docker_run",
            Self::DockerPs(_) => "docker_ps",
            Self::DockerStop(_) => "docker_stop",
            Self::DockerLogs(_) => "docker_logs",
            Self::DockerInspect(_) => "docker_inspect",
            Self::DockerImages(_) => "docker_images",
            Self::MultiEdit(_) => "multi_edit",
            Self::ApplyPatch(_) => "apply_patch",
            Self::ClaudeTextEditor(_) => "claude_text_editor",
            Self::CodeSearch(_) => "code_search",
            Self::SemanticSearch(_) => "semantic_search",
            Self::Question(_) => "question",
            Self::DatabaseQuery(_) => "database_query",
            Self::DatabaseSchema(_) => "database_schema",
            Self::DatabaseTransaction(_) => "database_transaction",
            Self::HttpGet(_) => "http_get",
            Self::HttpPost(_) => "http_post",
            Self::HttpPut(_) => "http_put",
            Self::HttpDelete(_) => "http_delete",
            Self::NotebookEdit(_) => "notebook_edit",
            Self::Skill(_) => "skill",
            Self::WorktreeCreate(_) => "worktree_create",
            Self::WorktreeList => "worktree_list",
            Self::WorktreeDelete(_) => "worktree_delete",
            Self::Brief(_) => "brief",
            Self::SendMessage(_) => "send_message",
            Self::TaskOutput(_) => "task_output",
            Self::TaskStop(_) => "task_stop",
            Self::TeamCreate(_) => "team_create",
            Self::TeamDelete => "team_delete",
            Self::CronCreate(_) => "cron_create",
            Self::CronDelete(_) => "cron_delete",
            Self::CronList => "cron_list",
            Self::ToolSearch(_) => "tool_search",
            Self::ListMcpResources(_) => "list_mcp_resources",
            Self::ReadMcpResource(_) => "read_mcp_resource",
            Self::REPL(_) => "repl",
        }
    }

    /// Check if a tool name is registered (case-insensitive).
    /// Also normalizes `PascalCase` to `snake_case` for matching.
    pub fn contains(name: &str) -> bool {
        let normalized = name
            .to_lowercase()
            .replace("readfile", "read_file")
            .replace("writefile", "write_file")
            .replace("editfile", "edit_file")
            .replace("listdir", "list_dir")
            .replace("gitstatus", "git_status")
            .replace("gitdiff", "git_diff")
            .replace("gitlog", "git_log")
            .replace("gitcommit", "git_commit")
            .replace("webfetch", "web_fetch")
            .replace("websearch", "web_search")
            .replace("taskcreate", "task_create")
            .replace("taskupdate", "task_update")
            .replace("tasklist", "task_list")
            .replace("taskget", "task_get")
            .replace("plancreate", "plan_create")
            .replace("plansave", "plan_save")
            .replace("planload", "plan_load")
            .replace("planlist", "plan_list")
            .replace("runtests", "run_tests")
            .replace("runtest", "run_test")
            .replace("runbench", "run_bench")
            .replace("dockerbuild", "docker_build")
            .replace("dockerrun", "docker_run")
            .replace("dockerps", "docker_ps")
            .replace("dockerstop", "docker_stop")
            .replace("dockerlogs", "docker_logs")
            .replace("dockerinspect", "docker_inspect")
            .replace("dockerimages", "docker_images")
            .replace("multiedit", "multi_edit")
            .replace("applypatch", "apply_patch")
            .replace("claudetexteditor", "claude_text_editor")
            .replace("codesearch", "code_search")
            .replace("semanticsearch", "semantic_search")
            .replace("databasequery", "database_query")
            .replace("databaseschema", "database_schema")
            .replace("databasetransaction", "database_transaction")
            .replace("httpget", "http_get")
            .replace("httppost", "http_post")
            .replace("httpput", "http_put")
            .replace("httpdelete", "http_delete")
            .replace("lspdiagnostics", "lsp_diagnostics")
            .replace("lsphover", "lsp_hover")
            .replace("lspdefinition", "lsp_definition")
            .replace("lspcompletion", "lsp_completion")
            .replace("lspdocumentsymbols", "lsp_document_symbols")
            .replace("lspreferences", "lsp_references");
        Self::all_tool_names().iter().any(|n| *n == normalized)
    }

    /// Get all registered tool names, sorted alphabetically.
    pub fn all_tool_names() -> Vec<&'static str> {
        let mut names = vec![
            "read_file",
            "write_file",
            "edit_file",
            "list_dir",
            "bash",
            "glob",
            "grep",
            "git_status",
            "git_diff",
            "git_log",
            "git_commit",
            "lsp_diagnostics",
            "lsp_hover",
            "lsp_definition",
            "lsp_completion",
            "lsp_document_symbols",
            "lsp_references",
            "web_fetch",
            "web_search",
            "task_create",
            "task_update",
            "task_list",
            "task_get",
            "plan_create",
            "plan_save",
            "plan_load",
            "plan_list",
            "run_tests",
            "run_test",
            "run_bench",
            "coverage",
            "docker_build",
            "docker_run",
            "docker_ps",
            "docker_stop",
            "docker_logs",
            "docker_inspect",
            "docker_images",
            "multi_edit",
            "apply_patch",
            "claude_text_editor",
            "code_search",
            "semantic_search",
            "question",
            "database_query",
            "database_schema",
            "database_transaction",
            "http_get",
            "http_post",
            "http_put",
            "http_delete",
            "notebook_edit",
            "skill",
            "worktree_create",
            "worktree_list",
            "worktree_delete",
            "brief",
            "send_message",
            "task_output",
            "task_stop",
            "team_create",
            "team_delete",
            "cron_create",
            "cron_delete",
            "cron_list",
            "tool_search",
            "list_mcp_resources",
            "read_mcp_resource",
            "repl",
        ];
        names.sort_unstable();
        names
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::to_value;

    #[test]
    fn test_tool_contains_case_insensitive() {
        assert!(ToolCatalog::contains("read_file"));
        assert!(ToolCatalog::contains("ReadFile"));
        assert!(ToolCatalog::contains("READ_FILE"));
        assert!(!ToolCatalog::contains("unknown_tool"));
    }

    #[test]
    fn test_all_tool_names_sorted() {
        let names = ToolCatalog::all_tool_names();
        assert!(names.contains(&"read_file"));
        assert!(names.contains(&"bash"));
        assert!(names.contains(&"grep"));

        // Check sorted
        let mut sorted = names.clone();
        sorted.sort();
        assert_eq!(names, sorted);
    }

    #[test]
    fn test_tool_serialization() {
        let tool = ToolCatalog::ReadFile(ReadFileInput {
            file_path: "/path/to/file.txt".to_string(),
            offset: None,
            limit: None,
        });

        let json = to_value(&tool).unwrap();
        assert_eq!(json["name"], "read_file");
        assert_eq!(json["arguments"]["file_path"], "/path/to/file.txt");
    }

    #[test]
    fn test_tool_description() {
        let tool = ToolCatalog::ReadFile(ReadFileInput {
            file_path: "/path/to/file.txt".to_string(),
            offset: None,
            limit: None,
        });

        assert!(tool.description().contains("file"));
    }

    #[test]
    fn test_tool_permission() {
        let read_tool = ToolCatalog::ReadFile(ReadFileInput {
            file_path: "/path/to/file.txt".to_string(),
            offset: None,
            limit: None,
        });
        assert_eq!(read_tool.permission(), ToolPermission::Read);

        let write_tool = ToolCatalog::WriteFile(WriteFileInput {
            file_path: "/path/to/file.txt".to_string(),
            content: "content".to_string(),
            create_parents: None,
        });
        assert_eq!(write_tool.permission(), ToolPermission::Write);

        let bash_tool = ToolCatalog::Bash(BashInput {
            command: "echo test".to_string(),
            timeout_secs: None,
            restart: None,
        });
        assert_eq!(bash_tool.permission(), ToolPermission::Execute);

        let web_tool = ToolCatalog::WebFetch(WebFetchInput {
            url: "https://example.com".to_string(),
        });
        assert_eq!(web_tool.permission(), ToolPermission::Network);
    }

    #[test]
    fn test_tool_name() {
        let tool = ToolCatalog::ReadFile(ReadFileInput {
            file_path: "/path/to/file.txt".to_string(),
            offset: None,
            limit: None,
        });
        assert_eq!(tool.name(), "read_file");
    }

    #[test]
    fn test_input_serialization() {
        let input = ReadFileInput {
            file_path: "/path/to/file.txt".to_string(),
            offset: Some(10),
            limit: Some(100),
        };

        let json = to_value(&input).unwrap();
        assert_eq!(json["file_path"], "/path/to/file.txt");
        assert_eq!(json["offset"], 10);
        assert_eq!(json["limit"], 100);
    }

    #[test]
    fn test_input_skip_none() {
        let input = ReadFileInput {
            file_path: "/path/to/file.txt".to_string(),
            offset: None,
            limit: None,
        };

        let json = to_value(&input).unwrap();
        assert_eq!(json["file_path"], "/path/to/file.txt");
        assert!(json.get("offset").is_none());
        assert!(json.get("limit").is_none());
    }
}
