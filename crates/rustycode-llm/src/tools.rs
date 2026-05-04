//! Tool definitions for LLM providers
//!
//! This module defines common tools that can be exposed to LLMs (Claude, GPT, etc.)
//! to enable them to execute actions like running commands, reading files, etc.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::tool_annotations::anthropic_annotations_for_tool_name;

/// A tool definition that can be sent to LLM providers
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
    /// Anthropic server tool type, when this tool is hosted by Anthropic.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub anthropic_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub examples: Option<Vec<serde_json::Value>>,
    /// Whether this is a server-side tool (executed by Anthropic)
    #[serde(skip_serializing)]
    pub is_server_tool: bool,
    /// Enable fine-grained tool streaming (Anthropic-specific)
    /// When true, tool parameters stream without buffering or JSON validation
    #[serde(skip_serializing_if = "Option::is_none")]
    pub eager_input_streaming: Option<bool>,
    /// Anthropic tool annotations for custom tools.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub annotations: Option<Value>,
}

impl ToolDefinition {
    /// Create a new tool definition
    pub fn new(
        name: impl Into<String>,
        description: impl Into<String>,
        input_schema: serde_json::Value,
    ) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
            input_schema,
            anthropic_type: None,
            examples: None,
            is_server_tool: false,
            eager_input_streaming: None,
            annotations: None,
        }
    }

    /// Add examples to the tool definition
    pub fn with_examples(mut self, examples: Vec<serde_json::Value>) -> Self {
        self.examples = Some(examples);
        self
    }

    /// Mark this as a server-side tool (executed by Anthropic, not locally)
    pub fn server_tool(mut self) -> Self {
        self.is_server_tool = true;
        self
    }

    /// Set the Anthropic server tool type for this tool.
    pub fn with_anthropic_type(mut self, anthropic_type: impl Into<String>) -> Self {
        self.anthropic_type = Some(anthropic_type.into());
        self
    }

    /// Enable eager input streaming for this tool (Anthropic-specific)
    /// When enabled, tool parameters will stream without buffering or JSON validation
    /// This results in faster streaming with longer chunks and fewer word breaks
    pub fn with_eager_streaming(mut self) -> Self {
        self.eager_input_streaming = Some(true);
        self
    }

    /// Attach Anthropic tool annotations.
    pub fn with_annotations(mut self, annotations: Value) -> Self {
        self.annotations = Some(annotations);
        self
    }
}

/// Bash/command execution tool
fn bash_tool() -> ToolDefinition {
    ToolDefinition::new(
        "bash",
        "Executes a given bash command and returns its output. \
         The working directory persists between commands, but shell state does not. \
         The shell environment is initialized from the user's profile (bash or zsh). \
         IMPORTANT: Avoid using this tool to run find, grep, cat, head, tail, sed, awk, or echo commands, \
         unless explicitly instructed or after you have verified that a dedicated tool cannot accomplish your task. \
         Instead, use the appropriate dedicated tool: \
         File search: Use glob (NOT find or ls) \
         Content search: Use grep (NOT grep or rg) \
         Read files: Use read_file (NOT cat/head/tail) \
         Edit files: Use edit (NOT sed/awk) \
         Write files: Use write_file (NOT echo/cat <<EOF) \
         Communication: Output text directly (NOT echo/printf) \
         These built-in tools provide a better user experience and make it easier to review tool calls.",
        json!({
            "type": "object",
            "properties": {
                "command": {
                    "type": "string",
                    "description": "The command to execute. If commands depend on each other, chain with &&. Use ; only when earlier failure doesn't matter. DO NOT use newlines to separate commands. Always quote file paths containing spaces."
                },
                "timeout": {
                    "type": "integer",
                    "description": "Optional timeout in milliseconds (up to 600000ms / 10 minutes). Defaults to 120000ms (2 minutes). Use longer timeout for commands known to take minutes."
                },
                "run_in_background": {
                    "type": "boolean",
                    "description": "Set to true to run this command in the background. Only use this when you don't need the result immediately and are OK being notified when the command completes later. You do not need to use '&' at the end of the command when using this parameter. You will be notified when it finishes — do not poll."
                },
                "dangerouslyDisableSandbox": {
                    "type": "boolean",
                    "description": "Set this to true to dangerously override sandbox mode and run commands without sandboxing. Only use when the command requires access to resources outside the sandbox."
                },
                "description": {
                    "type": "string",
                    "description": "Clear, concise description of what this command does (5-10 words for simple commands)"
                }
            },
            "required": ["command"]
        }),
    ).with_examples(vec![
        json!({"command": "ls -la"}),
        json!({"command": "grep -r pattern src/"}),
        json!({"command": "cargo test"}),
        json!({"command": "npm run build", "run_in_background": true}),
    ])
    .with_eager_streaming()
}

/// File reading tool
fn read_file_tool() -> ToolDefinition {
    ToolDefinition::new(
        "read_file",
        "Reads a file from the local filesystem. You can access any file directly by using this tool. \
         The file_path parameter must be an absolute path, not a relative path. \
         By default, it reads up to 2000 lines starting from the beginning of the file. \
         Results are returned with line numbers starting at 1. \
         This tool can read images (PNG, JPG, etc) — contents are presented visually. \
         This tool can read PDF files (.pdf). For large PDFs (more than 10 pages), you MUST provide the pages parameter to read specific page ranges. Maximum 20 pages per request. \
         This tool can read Jupyter notebooks (.ipynb files) and returns all cells with their outputs. \
         This tool can only read files, not directories. To read a directory, use an ls command via the bash tool. \
         If you read a file that exists but has empty contents you will receive a system reminder warning in place of file contents.",
        json!({
            "type": "object",
            "properties": {
                "file_path": {
                    "type": "string",
                    "description": "The absolute path of the file to read"
                },
                "offset": {
                    "type": "integer",
                    "description": "The line number to start reading from. Only provide if the file is too large to read at once."
                },
                "limit": {
                    "type": "integer",
                    "description": "The number of lines to read. Only provide if the file is too large to read at once."
                },
                "pages": {
                    "type": "string",
                    "description": "Page range for PDF files (e.g., \"1-5\", \"3\", \"10-20\"). Only applicable to PDF files. Maximum 20 pages per request."
                }
            },
            "required": ["file_path"]
        }),
    ).with_examples(vec![
        json!({"file_path": "README.md"}),
        json!({"file_path": "src/main.rs"}),
        json!({"file_path": "Cargo.toml"}),
        json!({"file_path": "src/large_file.rs", "offset": 100, "limit": 50}),
    ])
    .with_eager_streaming()
}

/// File writing tool
fn write_file_tool() -> ToolDefinition {
    ToolDefinition::new(
        "write_file",
        "Write content to a file. Creates the file if it doesn't exist, overwrites if it does. \
         Prefer the edit tool for modifying existing files — it only sends the diff. \
         Only use this tool to create new files or for complete rewrites. \
         If this is an existing file, you MUST read it first to understand its current contents. \
         NEVER create documentation files (*.md) or README files unless explicitly requested by the user. \
         Only use emojis if the user explicitly requests it. Avoid writing emojis to files unless asked.",
        json!({
            "type": "object",
            "properties": {
                "file_path": {
                    "type": "string",
                    "description": "The absolute path to the file to write (must be absolute, not relative)"
                },
                "content": {
                    "type": "string",
                    "description": "The content to write to the file"
                }
            },
            "required": ["file_path", "content"]
        }),
    ).with_examples(vec![
        json!({"file_path": "output.txt", "content": "Hello, world!"}),
        json!({"file_path": "src/config.rs", "content": "pub struct Config {\n    pub debug: bool,\n}"}),
    ])
    .with_eager_streaming()
}

/// Web fetch tool
fn web_fetch_tool() -> ToolDefinition {
    ToolDefinition::new(
        "web_fetch",
        "Fetches content from a URL and converts it to markdown for analysis. \
         Takes a URL and an optional prompt describing what to extract. \
         HTTP URLs are automatically upgraded to HTTPS. \
         The tool is read-only and does not modify any files. \
         Results may be summarized if the content is very large. \
         Includes a 15-minute cache for faster responses when repeatedly accessing the same URL. \
         When a URL redirects to a different host, the tool will inform you and provide the redirect URL. \
         For GitHub URLs, prefer using the gh CLI via Bash instead (e.g., gh pr view, gh issue view). \
         IMPORTANT: If an MCP-provided web fetch tool is available, prefer using that instead, \
         as it may have fewer restrictions.",
        json!({
            "type": "object",
            "properties": {
                "url": {
                    "type": "string",
                    "description": "The URL to fetch content from (must be a fully-formed valid URL)"
                },
                "prompt": {
                    "type": "string",
                    "description": "Optional prompt describing what information to extract from the page. If provided, the content will be processed and a focused response returned."
                }
            },
            "required": ["url"]
        }),
    ).with_examples(vec![
        json!({"url": "https://docs.anthropic.com/en/docs/tool-use"}),
        json!({"url": "https://github.com/rust-lang/rust/blob/main/README.md"}),
        json!({"url": "https://www.anthropic.com/engineering/advanced-tool-use"}),
        json!({"url": "https://example.com/api/documentation"}),
    ])
    .with_eager_streaming()
}

/// File editing tool — performs exact string replacement in files
fn edit_file_tool() -> ToolDefinition {
    ToolDefinition::new(
        "edit_file",
        "Performs exact string replacements in files. \
         You MUST read the file first before editing it. \
         The edit will FAIL if old_string is not unique in the file — \
         provide more surrounding context (2-4 lines is usually sufficient) or use replace_all to change every occurrence. \
         ALWAYS prefer editing existing files over creating new ones. \
         When editing text from Read tool output, preserve the exact indentation AFTER the line number prefix. \
         The line number prefix format is: line number + tab. Everything after that is the actual file content to match. \
         NEVER include any part of the line number prefix in old_string or new_string.",
        json!({
            "type": "object",
            "properties": {
                "file_path": {
                    "type": "string",
                    "description": "Path to the file to edit"
                },
                "old_string": {
                    "type": "string",
                    "description": "The exact text to replace (must be unique in the file unless replace_all is true)"
                },
                "new_string": {
                    "type": "string",
                    "description": "The text to replace it with"
                },
                "replace_all": {
                    "type": "boolean",
                    "description": "Replace all occurrences of old_string (default false)"
                }
            },
            "required": ["file_path", "old_string", "new_string"]
        }),
    ).with_examples(vec![
        json!({"file_path": "src/main.rs", "old_string": "fn hello() {", "new_string": "fn greet() {"}),
        json!({"file_path": "src/lib.rs", "old_string": "use old_module", "new_string": "use new_module", "replace_all": true}),
    ])
    .with_eager_streaming()
}

/// Content search tool — search file contents with regex
fn grep_tool() -> ToolDefinition {
    ToolDefinition::new(
        "grep",
        "Search file contents using regex patterns (built on ripgrep). \
         ALWAYS use this tool instead of running grep/rg as a bash command. \
         Supports full regex syntax (e.g., \"log.*Error\", \"function\\s+\\w+\"). \
         Filter by glob pattern or file type. \
         Output modes: 'content' (matching lines), 'files_with_matches' (file paths, default), 'count' (match counts). \
         Use -i for case-insensitive search. Use context lines for surrounding code. \
         Multiline patterns: set multiline to true for cross-line matching.",
        json!({
            "type": "object",
            "properties": {
                "pattern": {
                    "type": "string",
                    "description": "Regular expression pattern to search for"
                },
                "path": {
                    "type": "string",
                    "description": "Directory or file to search in (defaults to current directory)"
                },
                "glob": {
                    "type": "string",
                    "description": "File pattern filter (e.g., '*.rs', '**/*.tsx')"
                },
                "type": {
                    "type": "string",
                    "description": "File type filter (e.g., 'rust', 'js', 'py', 'go')"
                },
                "output_mode": {
                    "type": "string",
                    "enum": ["content", "files_with_matches", "count"],
                    "description": "Output format: content shows matching lines, files_with_matches shows file paths, count shows match counts"
                },
                "-i": {
                    "type": "boolean",
                    "description": "Case-insensitive search (default: false)"
                },
                "-B": {
                    "type": "integer",
                    "description": "Number of lines to show before each match (content mode only)"
                },
                "-A": {
                    "type": "integer",
                    "description": "Number of lines to show after each match (content mode only)"
                },
                "-C": {
                    "type": "integer",
                    "description": "Alias for context — number of lines before and after matches"
                },
                "context": {
                    "type": "integer",
                    "description": "Number of context lines before and after matches (content mode only)"
                },
                "head_limit": {
                    "type": "integer",
                    "description": "Maximum number of results to return (limits output size)"
                },
                "offset": {
                    "type": "integer",
                    "description": "Skip first N entries before applying head_limit. Defaults to 0"
                },
                "limit": {
                    "type": "integer",
                    "description": "Maximum number of permitted matches. -1 means no limit. If exceeded, a shortened result is returned"
                },
                "multiline": {
                    "type": "boolean",
                    "description": "Enable multiline mode where . matches newlines. Use for patterns that span lines (e.g., struct \\{[\\s\\S]*?field\\})"
                }
            },
            "required": ["pattern"]
        }),
    ).with_examples(vec![
        json!({"pattern": "fn main", "glob": "*.rs"}),
        json!({"pattern": "TODO|FIXME", "output_mode": "content"}),
        json!({"pattern": "error", "type": "rust", "output_mode": "content", "context": 2}),
    ])
}

/// File pattern matching tool — find files by name patterns
fn glob_tool() -> ToolDefinition {
    ToolDefinition::new(
        "glob",
        "Find files matching glob patterns. Fast file pattern matching that works with any codebase size. \
         Returns matching file paths sorted by modification time. \
         Use this when you need to find files by name patterns. \
         When doing open-ended searches requiring multiple rounds, prefer the Agent tool.",
        json!({
            "type": "object",
            "properties": {
                "pattern": {
                    "type": "string",
                    "description": "Glob pattern (e.g., '**/*.rs', 'src/**/*.ts', 'tests/**/*test*.py')"
                },
                "path": {
                    "type": "string",
                    "description": "The directory to search in. If not specified, the current working directory will be used. IMPORTANT: Omit this field to use the default directory. Do not enter \"undefined\" or \"null\" — simply omit it for the default behavior. Must be a valid directory path if provided."
                }
            },
            "required": ["pattern"]
        }),
    ).with_examples(vec![
        json!({"pattern": "**/*.rs"}),
        json!({"pattern": "src/**/mod.rs"}),
        json!({"pattern": "tests/**/*.py"}),
    ])
}

/// Web search tool (server-side - executed by Anthropic)
fn web_search_tool() -> ToolDefinition {
    ToolDefinition::new(
        "web_search",
        "Search the web and use the results to inform responses. \
         Provides up-to-date information for current events and recent data. \
         Returns search result information formatted as search result blocks, including links as markdown hyperlinks. \
         Use this tool for accessing information beyond Claude's knowledge cutoff. \
         Searches are performed automatically within a single API call. \
         Domain filtering is supported to include or block specific websites. \
         Web search is only available in the US. \
         CRITICAL REQUIREMENT: After answering the user's question, you MUST include a \"Sources:\" section at the end of your response. \
         In the Sources section, list all relevant URLs from the search results as markdown hyperlinks: [Title](URL). \
         This is MANDATORY - never skip including sources. \
         IMPORTANT: Use the correct year in search queries. \
         Example format: [Your answer here]\n\nSources:\n- [Source Title 1](https://example.com/1)\n- [Source Title 2](https://example.com/2)",
        json!({
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "Search query to use. Be specific for better results (e.g., 'Rust tokio spawn best practices 2026')"
                },
                "allowed_domains": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Only include search results from these websites"
                },
                "blocked_domains": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Never include search results from these websites"
                }
            },
            "required": ["query"]
        }),
    ).with_examples(vec![
        json!({"query": "Rust async spawn best practices 2026"}),
        json!({"query": "Anthropic Claude tool use documentation"}),
        json!({"query": "how to fix cannot borrow as mutable"}),
        json!({"query": "Rust 2024 edition new features"}),
    ])
    .server_tool()
    .with_anthropic_type("web_search_20260209")
}

/// Tool search tool (server-side - executed by Anthropic)
fn tool_search_tool() -> ToolDefinition {
    ToolDefinition::new(
        "tool_search",
        "Search for available tools by name, description, or functionality. Use this when you need to discover a smaller, relevant subset of tools instead of loading every available tool definition into context.",
        json!({
            "type": "object",
            "properties": {},
            "additionalProperties": false
        }),
    )
    .server_tool()
    .with_anthropic_type("tool_search_tool_bm25_20251119")
}

/// Code execution tool (server-side — required for Agent Skills API)
fn code_execution_tool() -> ToolDefinition {
    ToolDefinition::new(
        "code_execution",
        "Execute code in a sandboxed environment. Required for Agent Skills to generate files (presentations, spreadsheets, documents, PDFs).",
        json!({
            "type": "object",
            "properties": {},
            "additionalProperties": false
        }),
    )
    .server_tool()
    .with_anthropic_type("code_execution_20250825")
}

/// LSP diagnostics tool
fn lsp_diagnostics_tool() -> ToolDefinition {
    ToolDefinition::new(
        "lsp_diagnostics",
        "Get diagnostics (errors, warnings, hints) for a source file using the Language Server Protocol. Use this when user asks about errors, warnings, code quality, or 'check this file'. Requires an LSP server like rust-analyzer to be installed.",
        json!({
            "type": "object",
            "properties": {
                "file_path": {
                    "type": "string",
                    "description": "Path to the file to analyze (e.g., 'src/main.rs', 'crates/rustycode-tui/src/lib.rs')"
                }
            },
            "required": ["file_path"]
        }),
    ).with_examples(vec![
        json!({"file_path": "src/main.rs"}),
        json!({"file_path": "crates/rustycode-tui/src/lib.rs"}),
        json!({"file_path": "/Users/nat/dev/rustycode/Cargo.toml"}),
    ])
}

/// LSP hover tool
fn lsp_hover_tool() -> ToolDefinition {
    ToolDefinition::new(
        "lsp_hover",
        "Get hover information (type signature, documentation) for a symbol at a specific position. Use this when user asks 'what is this', 'what type is', 'how does this work', or 'show me documentation for'.",
        json!({
            "type": "object",
            "properties": {
                "file_path": {
                    "type": "string",
                    "description": "Path to the file (e.g., 'src/main.rs')"
                },
                "line": {
                    "type": "integer",
                    "description": "Line number (1-based, as shown in editors)"
                },
                "character": {
                    "type": "integer",
                    "description": "Character offset (1-based, as shown in editors)"
                }
            },
            "required": ["file_path", "line", "character"]
        }),
    ).with_examples(vec![
        json!({"file_path": "src/main.rs", "line": 5, "character": 10}),
        json!({"file_path": "crates/rustycode-tui/src/lib.rs", "line": 15, "character": 8}),
    ])
}

/// LSP go to definition tool
fn lsp_definition_tool() -> ToolDefinition {
    ToolDefinition::new(
        "lsp_definition",
        "Find the definition of a symbol at a specific position. Use this when user asks 'where is this defined', 'find the definition', 'go to definition', or 'show me where this comes from'.",
        json!({
            "type": "object",
            "properties": {
                "file_path": {
                    "type": "string",
                    "description": "Path to the file (e.g., 'src/main.rs')"
                },
                "line": {
                    "type": "integer",
                    "description": "Line number (1-based, as shown in editors)"
                },
                "character": {
                    "type": "integer",
                    "description": "Character offset (1-based, as shown in editors)"
                }
            },
            "required": ["file_path", "line", "character"]
        }),
    ).with_examples(vec![
        json!({"file_path": "src/main.rs", "line": 10, "character": 5}),
        json!({"file_path": "crates/rustycode-tui/src/lib.rs", "line": 20, "character": 12}),
    ])
}

/// LSP completion tool
fn lsp_completion_tool() -> ToolDefinition {
    ToolDefinition::new(
        "lsp_completion",
        "Get code completions at a specific position. Use this when user asks for completions, autocomplete, 'what can I use here', or 'show me available methods/functions'.",
        json!({
            "type": "object",
            "properties": {
                "file_path": {
                    "type": "string",
                    "description": "Path to the file (e.g., 'src/main.rs')"
                },
                "line": {
                    "type": "integer",
                    "description": "Line number (1-based, as shown in editors)"
                },
                "character": {
                    "type": "integer",
                    "description": "Character offset (1-based, as shown in editors)"
                }
            },
            "required": ["file_path", "line", "character"]
        }),
    ).with_examples(vec![
        json!({"file_path": "src/main.rs", "line": 15, "character": 20}),
        json!({"file_path": "crates/rustycode-tui/src/lib.rs", "line": 25, "character": 15}),
    ])
}

/// LSP find references tool
fn lsp_references_tool() -> ToolDefinition {
    ToolDefinition::new(
        "lsp_references",
        "Find all references to a symbol at a specific position across the workspace. \
         Use this when user asks 'where is this used', 'find all usages', or 'show me references to'.",
        json!({
            "type": "object",
            "properties": {
                "file_path": {
                    "type": "string",
                    "description": "Path to the file containing the symbol"
                },
                "line": {
                    "type": "integer",
                    "description": "Line number (1-based, as shown in editors)"
                },
                "character": {
                    "type": "integer",
                    "description": "Character offset (1-based, as shown in editors)"
                }
            },
            "required": ["file_path", "line", "character"]
        }),
    ).with_examples(vec![
        json!({"file_path": "src/main.rs", "line": 10, "character": 5}),
    ])
}

/// LSP document symbols tool
fn lsp_document_symbols_tool() -> ToolDefinition {
    ToolDefinition::new(
        "lsp_document_symbols",
        "Get all symbols (functions, classes, variables) in a document. \
         Use this to quickly understand the structure of a file.",
        json!({
            "type": "object",
            "properties": {
                "file_path": {
                    "type": "string",
                    "description": "Path to the file to analyze"
                }
            },
            "required": ["file_path"]
        }),
    )
    .with_examples(vec![json!({"file_path": "src/main.rs"})])
}

/// LSP go to implementation tool
fn lsp_implementation_tool() -> ToolDefinition {
    ToolDefinition::new(
        "lsp_implementation",
        "Find implementations of an interface or abstract method at a specific position. \
         Use this when user asks 'what implements this', 'find implementors', or 'show me implementations'.",
        json!({
            "type": "object",
            "properties": {
                "file_path": {
                    "type": "string",
                    "description": "Path to the file containing the interface/trait"
                },
                "line": {
                    "type": "integer",
                    "description": "Line number (1-based, as shown in editors)"
                },
                "character": {
                    "type": "integer",
                    "description": "Character offset (1-based, as shown in editors)"
                }
            },
            "required": ["file_path", "line", "character"]
        }),
    ).with_examples(vec![
        json!({"file_path": "src/traits.rs", "line": 5, "character": 10}),
    ])
}

/// LSP call hierarchy tool — incoming calls
fn lsp_incoming_calls_tool() -> ToolDefinition {
    ToolDefinition::new(
        "lsp_incoming_calls",
        "Find all functions/methods that call the function at a specific position. \
         Use this to understand what code depends on a function.",
        json!({
            "type": "object",
            "properties": {
                "file_path": {
                    "type": "string",
                    "description": "Path to the file"
                },
                "line": {
                    "type": "integer",
                    "description": "Line number (1-based, as shown in editors)"
                },
                "character": {
                    "type": "integer",
                    "description": "Character offset (1-based, as shown in editors)"
                }
            },
            "required": ["file_path", "line", "character"]
        }),
    )
    .with_examples(vec![
        json!({"file_path": "src/lib.rs", "line": 20, "character": 5}),
    ])
}

/// LSP call hierarchy tool — outgoing calls
fn lsp_outgoing_calls_tool() -> ToolDefinition {
    ToolDefinition::new(
        "lsp_outgoing_calls",
        "Find all functions/methods called by the function at a specific position. \
         Use this to understand what a function depends on.",
        json!({
            "type": "object",
            "properties": {
                "file_path": {
                    "type": "string",
                    "description": "Path to the file"
                },
                "line": {
                    "type": "integer",
                    "description": "Line number (1-based, as shown in editors)"
                },
                "character": {
                    "type": "integer",
                    "description": "Character offset (1-based, as shown in editors)"
                }
            },
            "required": ["file_path", "line", "character"]
        }),
    )
    .with_examples(vec![
        json!({"file_path": "src/lib.rs", "line": 20, "character": 5}),
    ])
}

/// LSP full diagnostics tool
fn lsp_full_diagnostics_tool() -> ToolDefinition {
    ToolDefinition::new(
        "lsp_full_diagnostics",
        "Get comprehensive diagnostics (errors, warnings, hints) with full context for a file. More detailed than lsp_diagnostics. Use this when you need complete error information including related diagnostics.",
        json!({
            "type": "object",
            "properties": {
                "file_path": {
                    "type": "string",
                    "description": "Path to the file to analyze (e.g., 'src/main.rs')"
                }
            },
            "required": ["file_path"]
        }),
    ).with_examples(vec![
        json!({"file_path": "src/main.rs"}),
    ])
}

/// LSP code actions tool
fn lsp_code_actions_tool() -> ToolDefinition {
    ToolDefinition::new(
        "lsp_code_actions",
        "Get available code actions (quick fixes, refactorings, source actions) for a range. Use this to find automated fixes for diagnostics or to apply refactorings.",
        json!({
            "type": "object",
            "properties": {
                "file_path": {
                    "type": "string",
                    "description": "Path to the file (e.g., 'src/main.rs')"
                },
                "line": {
                    "type": "integer",
                    "description": "Start line number (1-based, as shown in editors)"
                },
                "character": {
                    "type": "integer",
                    "description": "Start character offset (1-based, as shown in editors)"
                },
                "end_line": {
                    "type": "integer",
                    "description": "End line number (1-based, as shown in editors)"
                },
                "end_character": {
                    "type": "integer",
                    "description": "End character offset (1-based, as shown in editors)"
                }
            },
            "required": ["file_path", "line", "character", "end_line", "end_character"]
        }),
    ).with_examples(vec![
        json!({"file_path": "src/main.rs", "line": 10, "character": 1, "end_line": 10, "end_character": 20}),
    ])
}

/// LSP rename tool
fn lsp_rename_tool() -> ToolDefinition {
    ToolDefinition::new(
        "lsp_rename",
        "Rename a symbol at a position across all references in the workspace. Use this to safely rename variables, functions, types, etc.",
        json!({
            "type": "object",
            "properties": {
                "file_path": {
                    "type": "string",
                    "description": "Path to the file (e.g., 'src/main.rs')"
                },
                "line": {
                    "type": "integer",
                    "description": "Line number (1-based, as shown in editors)"
                },
                "character": {
                    "type": "integer",
                    "description": "Character offset (1-based, as shown in editors)"
                },
                "new_name": {
                    "type": "string",
                    "description": "The new name for the symbol"
                }
            },
            "required": ["file_path", "line", "character", "new_name"]
        }),
    ).with_examples(vec![
        json!({"file_path": "src/main.rs", "line": 15, "character": 8, "new_name": "process_request"}),
    ])
}

/// LSP formatting tool
fn lsp_formatting_tool() -> ToolDefinition {
    ToolDefinition::new(
        "lsp_formatting",
        "Format a file or range using the language server's formatter. Use this to auto-format code according to project style.",
        json!({
            "type": "object",
            "properties": {
                "file_path": {
                    "type": "string",
                    "description": "Path to the file to format (e.g., 'src/main.rs')"
                },
                "line": {
                    "type": "integer",
                    "description": "Start line for range formatting (optional, formats whole file if omitted)"
                },
                "character": {
                    "type": "integer",
                    "description": "Start character for range formatting"
                },
                "end_line": {
                    "type": "integer",
                    "description": "End line for range formatting"
                },
                "end_character": {
                    "type": "integer",
                    "description": "End character for range formatting"
                }
            },
            "required": ["file_path"]
        }),
    ).with_examples(vec![
        json!({"file_path": "src/main.rs"}),
        json!({"file_path": "src/main.rs", "line": 10, "character": 1, "end_line": 20, "end_character": 1}),
    ])
}

/// Get symbols overview tool
fn lsp_get_symbols_overview_tool() -> ToolDefinition {
    ToolDefinition::new(
        "lsp_get_symbols_overview",
        "Get a compact hierarchical overview of symbols in a file grouped by kind (functions, structs, impls, etc.). Use this to quickly understand a file's structure without reading the entire content.",
        json!({
            "type": "object",
            "properties": {
                "file_path": {
                    "type": "string",
                    "description": "Path to the file (e.g., 'src/main.rs')"
                },
                "depth": {
                    "type": "integer",
                    "description": "Depth of symbol hierarchy to return (default: 0, immediate children only)"
                }
            },
            "required": ["file_path"]
        }),
    ).with_examples(vec![
        json!({"file_path": "src/main.rs"}),
        json!({"file_path": "src/lib.rs", "depth": 1}),
    ])
}

/// Find symbol tool
fn lsp_find_symbol_tool() -> ToolDefinition {
    ToolDefinition::new(
        "lsp_find_symbol",
        "Find symbols by name path pattern in a file. Use this to locate specific functions, structs, or methods. Supports qualified names like 'MyClass/my_method'.",
        json!({
            "type": "object",
            "properties": {
                "file_path": {
                    "type": "string",
                    "description": "Path to the file (e.g., 'src/main.rs')"
                },
                "name_path": {
                    "type": "string",
                    "description": "Symbol name path (e.g., 'MyClass/my_method' or 'process_data')"
                },
                "include_body": {
                    "type": "boolean",
                    "description": "Whether to include the symbol's source code (default: false)"
                }
            },
            "required": ["file_path", "name_path"]
        }),
    ).with_examples(vec![
        json!({"file_path": "src/main.rs", "name_path": "App/run"}),
        json!({"file_path": "src/lib.rs", "name_path": "Config", "include_body": true}),
    ])
}

/// Replace symbol body tool
fn lsp_replace_symbol_body_tool() -> ToolDefinition {
    ToolDefinition::new(
        "lsp_replace_symbol_body",
        "Replace a symbol's entire body with new content. Use this to rewrite a function, method, or other definition. The symbol is identified by name path.",
        json!({
            "type": "object",
            "properties": {
                "file_path": {
                    "type": "string",
                    "description": "Path to the source file (e.g., 'src/main.rs')"
                },
                "name_path": {
                    "type": "string",
                    "description": "Symbol name path to identify the symbol (e.g., 'MyClass/my_method')"
                },
                "body": {
                    "type": "string",
                    "description": "New body content to replace the symbol with"
                }
            },
            "required": ["file_path", "name_path", "body"]
        }),
    ).with_examples(vec![
        json!({"file_path": "src/main.rs", "name_path": "process", "body": "fn process() -> Result<()> {\n    todo!()\n}"}),
    ])
}

/// Insert before symbol tool
fn lsp_insert_before_symbol_tool() -> ToolDefinition {
    ToolDefinition::new(
        "lsp_insert_before_symbol",
        "Insert text before a symbol's definition. Use this to add new items (functions, imports, fields) above an existing symbol.",
        json!({
            "type": "object",
            "properties": {
                "file_path": {
                    "type": "string",
                    "description": "Path to the source file (e.g., 'src/main.rs')"
                },
                "name_path": {
                    "type": "string",
                    "description": "Symbol name path to insert before (e.g., 'MyClass/my_method')"
                },
                "body": {
                    "type": "string",
                    "description": "Content to insert before the symbol"
                }
            },
            "required": ["file_path", "name_path", "body"]
        }),
    )
}

/// Insert after symbol tool
fn lsp_insert_after_symbol_tool() -> ToolDefinition {
    ToolDefinition::new(
        "lsp_insert_after_symbol",
        "Insert text after a symbol's definition. Use this to add new items after an existing symbol.",
        json!({
            "type": "object",
            "properties": {
                "file_path": {
                    "type": "string",
                    "description": "Path to the source file (e.g., 'src/main.rs')"
                },
                "name_path": {
                    "type": "string",
                    "description": "Symbol name path to insert after (e.g., 'MyClass/my_method')"
                },
                "body": {
                    "type": "string",
                    "description": "Content to insert after the symbol"
                }
            },
            "required": ["file_path", "name_path", "body"]
        }),
    )
}

/// Safe delete symbol tool
fn lsp_safe_delete_symbol_tool() -> ToolDefinition {
    ToolDefinition::new(
        "lsp_safe_delete_symbol",
        "Safely delete a symbol after checking for references. Will report if the symbol is still used elsewhere. Use this instead of text-based deletion to avoid breaking references.",
        json!({
            "type": "object",
            "properties": {
                "file_path": {
                    "type": "string",
                    "description": "Path to the source file (e.g., 'src/main.rs')"
                },
                "name_path": {
                    "type": "string",
                    "description": "Symbol name path to delete (e.g., 'old_function')"
                }
            },
            "required": ["file_path", "name_path"]
        }),
    ).with_examples(vec![
        json!({"file_path": "src/main.rs", "name_path": "deprecated_helper"}),
    ])
}

/// Rename symbol (by name path) tool
fn lsp_rename_symbol_tool() -> ToolDefinition {
    ToolDefinition::new(
        "lsp_rename_symbol",
        "Rename a symbol across the codebase by name path. Use this when you know the symbol name but not its exact position.",
        json!({
            "type": "object",
            "properties": {
                "file_path": {
                    "type": "string",
                    "description": "Path to the source file containing the symbol (e.g., 'src/main.rs')"
                },
                "name_path": {
                    "type": "string",
                    "description": "Symbol name path to rename (e.g., 'MyClass/old_name')"
                },
                "new_name": {
                    "type": "string",
                    "description": "The new name for the symbol"
                }
            },
            "required": ["file_path", "name_path", "new_name"]
        }),
    ).with_examples(vec![
        json!({"file_path": "src/main.rs", "name_path": "old_func", "new_name": "new_func"}),
    ])
}

/// Analyze symbol tool
fn lsp_analyze_symbol_tool() -> ToolDefinition {
    ToolDefinition::new(
        "lsp_analyze_symbol",
        "Analyze a symbol to get its references, implementations, and complexity metrics. Use this for deep code understanding before refactoring.",
        json!({
            "type": "object",
            "properties": {
                "file_path": {
                    "type": "string",
                    "description": "Path to the source file (e.g., 'src/main.rs')"
                },
                "name_path": {
                    "type": "string",
                    "description": "Symbol name path to analyze (e.g., 'MyClass/process')"
                },
                "include_info": {
                    "type": "boolean",
                    "description": "Include hover-like info (type signature, documentation) (default: false)"
                }
            },
            "required": ["file_path", "name_path"]
        }),
    )
}

/// Extract symbol tool
fn lsp_extract_symbol_tool() -> ToolDefinition {
    ToolDefinition::new(
        "lsp_extract_symbol",
        "Extract a symbol definition to a new file or module. Use this to refactor code by moving functions, structs, or traits to their own files.",
        json!({
            "type": "object",
            "properties": {
                "file_path": {
                    "type": "string",
                    "description": "Path to the source file (e.g., 'src/main.rs')"
                },
                "name_path": {
                    "type": "string",
                    "description": "Symbol name path to extract (e.g., 'MyStruct')"
                },
                "target_path": {
                    "type": "string",
                    "description": "Target file path for the extracted symbol (optional, auto-generated if omitted)"
                }
            },
            "required": ["file_path", "name_path"]
        }),
    )
}

/// Inline symbol tool
fn lsp_inline_symbol_tool() -> ToolDefinition {
    ToolDefinition::new(
        "lsp_inline_symbol",
        "Inline a symbol definition at all its usage sites. Use this to remove unnecessary abstraction by replacing calls with the actual implementation.",
        json!({
            "type": "object",
            "properties": {
                "file_path": {
                    "type": "string",
                    "description": "Path to the source file (e.g., 'src/main.rs')"
                },
                "name_path": {
                    "type": "string",
                    "description": "Symbol name path to inline (e.g., 'trivial_wrapper')"
                }
            },
            "required": ["file_path", "name_path"]
        }),
    )
}

/// Notebook editing tool — replace, insert, or delete cells in Jupyter notebooks
fn notebook_edit_tool() -> ToolDefinition {
    ToolDefinition::new(
        "notebook_edit",
        "Completely replaces the contents of a specific cell in a Jupyter notebook (.ipynb file) with new source. \
         Jupyter notebooks are interactive documents that combine code, text, and visualizations, \
         commonly used for data analysis and scientific computing. \
         The notebook_path parameter must be an absolute path, not a relative path. \
         The cell_number is 0-indexed. Use edit_mode=insert to add a new cell at the index specified by cell_number. \
         Use edit_mode=delete to delete the cell at the index specified by cell_number.",
        json!({
            "type": "object",
            "properties": {
                "notebook_path": {
                    "type": "string",
                    "description": "The absolute path to the Jupyter notebook file to edit (must be absolute, not relative)"
                },
                "cell_id": {
                    "type": "string",
                    "description": "The ID of the cell to edit. When inserting a new cell, the new cell will be inserted after the cell with this ID, or at the beginning if not specified."
                },
                "new_source": {
                    "type": "string",
                    "description": "The new source for the cell"
                },
                "cell_type": {
                    "type": "string",
                    "enum": ["code", "markdown"],
                    "description": "The type of the cell (code or markdown). If not specified, it defaults to the current cell type. If using edit_mode=insert, this is required."
                },
                "edit_mode": {
                    "type": "string",
                    "enum": ["replace", "insert", "delete"],
                    "description": "The type of edit to make (replace, insert, delete). Defaults to replace."
                }
            },
            "required": ["notebook_path", "new_source"]
        }),
    ).with_examples(vec![
        json!({"notebook_path": "/path/to/notebook.ipynb", "cell_id": "cell-1", "new_source": "print('Hello, world!')"}),
        json!({"notebook_path": "/path/to/notebook.ipynb", "cell_id": "cell-2", "new_source": "# Markdown cell", "cell_type": "markdown"}),
        json!({"notebook_path": "/path/to/notebook.ipynb", "cell_id": "cell-3", "edit_mode": "delete", "new_source": ""}),
    ])
}

/// Todo list management tool for tracking session tasks
fn todo_write_tool() -> ToolDefinition {
    ToolDefinition::new(
        "todo_write",
        "Update the todo list for the current session. Use this to create and manage a structured task list \
         for tracking progress on complex, multi-step tasks. \
         Each task has a content (imperative description), status (pending/in_progress/completed), \
         and activeForm (present continuous form shown during execution). \
         Keep exactly ONE task in_progress at a time. \
         Mark tasks completed IMMEDIATELY after finishing. \
         Use proactively for tasks with 3+ steps. \
         Skip for single trivial tasks.",
        json!({
            "type": "object",
            "properties": {
                "todos": {
                    "type": "array",
                    "description": "The updated todo list. Each item has content (what to do), status (pending/in_progress/completed), and activeForm (present continuous form). Replaces the entire list on each call.",
                    "items": {
                        "type": "object",
                        "properties": {
                            "content": {
                                "type": "string",
                                "description": "A brief, actionable task description in imperative form (e.g., 'Fix authentication bug')"
                            },
                            "status": {
                                "type": "string",
                                "enum": ["pending", "in_progress", "completed"],
                                "description": "Task status: pending (not started), in_progress (currently working), completed (done)"
                            },
                            "activeForm": {
                                "type": "string",
                                "description": "Present continuous form shown during execution (e.g., 'Fixing authentication bug')"
                            }
                        },
                        "required": ["content", "status", "activeForm"]
                    }
                }
            },
            "required": ["todos"]
        }),
    ).with_examples(vec![
        json!({"todos": [
            {"content": "Add dark mode toggle", "status": "in_progress", "activeForm": "Adding dark mode toggle"},
            {"content": "Update existing components", "status": "pending", "activeForm": "Updating existing components"},
            {"content": "Run tests and build", "status": "pending", "activeForm": "Running tests and build"}
        ]}),
        json!({"todos": [
            {"content": "Fix authentication bug", "status": "completed", "activeForm": "Fixing authentication bug"},
            {"content": "Add unit tests for auth", "status": "in_progress", "activeForm": "Adding unit tests for auth"}
        ]}),
    ])
}

/// Get all available tools for the TUI
pub fn get_tui_tools() -> Vec<ToolDefinition> {
    vec![
        bash_tool(),
        read_file_tool(),
        edit_file_tool(),
        write_file_tool(),
        grep_tool(),
        glob_tool(),
        web_fetch_tool(),
        web_search_tool(),  // Server tool
        tool_search_tool(), // Server tool
        lsp_diagnostics_tool(),
        lsp_hover_tool(),
        lsp_definition_tool(),
        lsp_references_tool(),
        lsp_document_symbols_tool(),
        lsp_implementation_tool(),
        lsp_completion_tool(),
        lsp_incoming_calls_tool(),
        lsp_outgoing_calls_tool(),
        lsp_full_diagnostics_tool(),
        lsp_code_actions_tool(),
        lsp_rename_tool(),
        lsp_formatting_tool(),
        lsp_get_symbols_overview_tool(),
        lsp_find_symbol_tool(),
        lsp_replace_symbol_body_tool(),
        lsp_insert_before_symbol_tool(),
        lsp_insert_after_symbol_tool(),
        lsp_safe_delete_symbol_tool(),
        lsp_rename_symbol_tool(),
        lsp_analyze_symbol_tool(),
        lsp_extract_symbol_tool(),
        lsp_inline_symbol_tool(),
        notebook_edit_tool(),
        todo_write_tool(),
    ]
}

/// Get tools required for the Anthropic Agent Skills API.
/// Callers should append these to their tool list when using `container.skills`.
pub fn get_skills_tools() -> Vec<ToolDefinition> {
    vec![code_execution_tool()]
}

/// Get Anthropic beta headers required for Agent Skills API.
pub fn get_anthropic_skills_beta_headers() -> Vec<&'static str> {
    vec!["code-execution-2025-08-25", "skills-2025-10-02"]
}

/// Convert tool definitions to the format expected by Anthropic API
pub fn to_anthropic_tools(tools: &[ToolDefinition]) -> Vec<serde_json::Value> {
    tools
        .iter()
        .map(|tool| {
            if tool.is_server_tool {
                let anthropic_type = tool.anthropic_type.as_deref().unwrap_or_else(|| {
                    debug_assert!(false, "server tool '{}' missing anthropic_type", tool.name);
                    "unknown"
                });
                json!({
                    "type": anthropic_type,
                    "name": tool.name,
                })
            } else {
                let mut tool_json = json!({
                    "name": tool.name,
                    "description": tool.description,
                    "input_schema": tool.input_schema
                });

                if let Some(annotations) = tool
                    .annotations
                    .clone()
                    .or_else(|| anthropic_annotations_for_tool_name(&tool.name))
                {
                    tool_json["annotations"] = annotations;
                }

                // Include examples if present
                if let Some(ref examples) = tool.examples {
                    if !examples.is_empty() {
                        tool_json["examples"] = json!(examples);
                    }
                }

                // Include eager_input_streaming if enabled
                if let Some(eager) = tool.eager_input_streaming {
                    if eager {
                        tool_json["eager_input_streaming"] = json!(true);
                    }
                }

                tool_json
            }
        })
        .collect()
}

/// Convert tool definitions to the format expected by OpenAI API
pub fn to_openai_tools(tools: &[ToolDefinition]) -> Vec<serde_json::Value> {
    tools
        .iter()
        .map(|tool| {
            json!({
                "type": "function",
                "function": {
                    "name": tool.name,
                    "description": tool.description,
                    "parameters": tool.input_schema
                }
            })
        })
        .collect()
}

/// Normalize tool definitions to OpenAI-compatible format.
///
/// Callers may provide tools in Anthropic format (`{name, description, input_schema}`)
/// or already in OpenAI format (`{type: "function", function: {name, description, parameters}}`).
/// This function converts any format to the correct OpenAI shape:
/// ```json
/// { "type": "function", "function": { "name": "...", "description": "...", "parameters": {...} } }
/// ```
pub fn normalize_tools_for_openai(tools: &[serde_json::Value]) -> Vec<serde_json::Value> {
    tools
        .iter()
        .map(|tool| {
            if tool.get("type").and_then(|t| t.as_str()).is_some() {
                return tool.clone();
            }

            let name = tool
                .get("name")
                .and_then(|v| v.as_str())
                .or_else(|| {
                    tool.get("function")
                        .and_then(|f| f.get("name"))
                        .and_then(|v| v.as_str())
                })
                .unwrap_or("unknown");

            let description = tool
                .get("description")
                .and_then(|v| v.as_str())
                .or_else(|| {
                    tool.get("function")
                        .and_then(|f| f.get("description"))
                        .and_then(|v| v.as_str())
                })
                .unwrap_or("");

            let parameters = tool
                .get("parameters")
                .cloned()
                .or_else(|| tool.get("input_schema").cloned())
                .or_else(|| {
                    tool.get("function")
                        .and_then(|f| f.get("parameters"))
                        .cloned()
                })
                .unwrap_or(json!({"type": "object", "properties": {}}));

            json!({
                "type": "function",
                "function": {
                    "name": name,
                    "description": description,
                    "parameters": parameters
                }
            })
        })
        .collect()
}

/// Normalize tools into the flat Responses API format.
///
/// The Responses API uses `{type, name, description, parameters}` instead of
/// the Chat Completions `{type, function: {name, description, parameters}}`.
///
/// ```json
/// { "type": "function", "name": "...", "description": "...", "parameters": {...} }
/// ```
pub fn normalize_tools_for_responses(tools: &[serde_json::Value]) -> Vec<serde_json::Value> {
    tools
        .iter()
        .map(|tool| {
            let name = tool
                .get("name")
                .and_then(|v| v.as_str())
                .or_else(|| {
                    tool.get("function")
                        .and_then(|f| f.get("name"))
                        .and_then(|v| v.as_str())
                })
                .unwrap_or("unknown");

            let description = tool
                .get("description")
                .and_then(|v| v.as_str())
                .or_else(|| {
                    tool.get("function")
                        .and_then(|f| f.get("description"))
                        .and_then(|v| v.as_str())
                });

            let parameters = tool
                .get("parameters")
                .cloned()
                .or_else(|| tool.get("input_schema").cloned())
                .or_else(|| {
                    tool.get("function")
                        .and_then(|f| f.get("parameters"))
                        .cloned()
                });

            let mut obj = serde_json::Map::new();
            obj.insert("type".to_string(), json!("function"));
            obj.insert("name".to_string(), json!(name));
            if let Some(desc) = description {
                obj.insert("description".to_string(), json!(desc));
            }
            if let Some(params) = parameters {
                obj.insert("parameters".to_string(), params);
            }
            serde_json::Value::Object(obj)
        })
        .collect()
}

/// Strip JSON Schema keywords that some providers (notably Zhipu) reject.
///
/// Removes: `minimum`, `maximum`, `enum`, `additionalProperties`, and recurses
/// into nested `properties` and `items` objects.
///
/// The base schema is left intact for providers that support full JSON Schema
/// (OpenAI, Anthropic, etc.). Call this *after* `normalize_tools_for_openai`
/// only for providers known to have strict validation.
pub fn sanitize_tools_for_strict_providers(tools: &[serde_json::Value]) -> Vec<serde_json::Value> {
    tools
        .iter()
        .map(|tool| {
            let mut tool = tool.clone();
            if let Some(func) = tool.get_mut("function") {
                if let Some(params) = func.get_mut("parameters") {
                    strip_unsupported_keywords(params);
                    whitelist_params_top_level(params);
                }
            }
            tool
        })
        .collect()
}

const ALLOWED_PARAM_KEYS: &[&str] = &[
    "type",
    "description",
    "properties",
    "required",
    "items",
    "default",
];

fn whitelist_params_top_level(params: &mut serde_json::Value) {
    let Some(obj) = params.as_object_mut() else {
        return;
    };
    let keys_to_remove: Vec<String> = obj
        .keys()
        .filter(|k| !ALLOWED_PARAM_KEYS.contains(&k.as_str()))
        .cloned()
        .collect();
    for key in keys_to_remove {
        obj.remove(&key);
    }
}

/// Recursively strip unsupported JSON Schema keywords from a single value.
///
/// Some providers (Zhipu/GLM) reject tool schemas containing keywords beyond
/// the basic `type`/`description`/`properties`/`required`/`items`/`default` set.
/// This removes: `$schema`, `minimum`, `maximum`, `enum`, `additionalProperties`,
/// `oneOf`, `anyOf`, `allOf`, `format`, `patternProperties`, `not`, `if`/`then`/`else`.
fn strip_unsupported_keywords(value: &mut serde_json::Value) {
    let Some(obj) = value.as_object_mut() else {
        return;
    };

    obj.remove("$schema");
    obj.remove("minimum");
    obj.remove("maximum");
    obj.remove("exclusiveMinimum");
    obj.remove("exclusiveMaximum");
    obj.remove("minLength");
    obj.remove("maxLength");
    obj.remove("pattern");
    obj.remove("enum");
    obj.remove("const");
    obj.remove("additionalProperties");
    obj.remove("oneOf");
    obj.remove("anyOf");
    obj.remove("allOf");
    obj.remove("format");
    obj.remove("patternProperties");
    obj.remove("not");
    obj.remove("if");
    obj.remove("then");
    obj.remove("else");

    if let Some(props) = obj.get_mut("properties") {
        if let Some(props_obj) = props.as_object_mut() {
            for (_key, prop) in props_obj.iter_mut() {
                strip_unsupported_keywords(prop);
            }
        }
    }

    // Recurse into array items
    if let Some(items) = obj.get_mut("items") {
        strip_unsupported_keywords(items);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bash_tool_definition() {
        let tools = get_tui_tools();
        let bash = tools.iter().find(|t| t.name == "bash").unwrap();

        assert_eq!(bash.name, "bash");
        assert!(bash.description.contains("bash"));
        assert!(bash.input_schema["required"]
            .as_array()
            .unwrap()
            .contains(&json!("command")));
    }

    #[test]
    fn test_anthropic_conversion() {
        let tools = get_tui_tools();
        let anthropic_tools = to_anthropic_tools(&tools);

        assert!(!anthropic_tools.is_empty());
        let read_file_tool = anthropic_tools
            .iter()
            .find(|t| t["name"] == "read_file")
            .unwrap();
        assert_eq!(read_file_tool["annotations"]["readOnlyHint"], true);
        let bash_tool = anthropic_tools
            .iter()
            .find(|t| t["name"] == "bash")
            .unwrap();
        assert!(bash_tool["description"].is_string());
        assert!(bash_tool["input_schema"].is_object());
    }

    #[test]
    fn test_server_tools_included_with_type() {
        let tools = get_tui_tools();
        let anthropic_tools = to_anthropic_tools(&tools);

        // web_search is a server tool, so it should appear with an Anthropic tool type
        let web_search_in_anthropic = anthropic_tools.iter().find(|t| t["name"] == "web_search");
        assert!(
            web_search_in_anthropic.is_some(),
            "Server tools should be included in the Anthropic tools array"
        );

        let web_search = web_search_in_anthropic.unwrap();
        assert_eq!(web_search["type"], "web_search_20260209");
        assert_eq!(web_search["name"], "web_search");

        let tool_search_in_anthropic = anthropic_tools.iter().find(|t| t["name"] == "tool_search");
        assert!(
            tool_search_in_anthropic.is_some(),
            "tool_search should be included in the Anthropic tools array"
        );

        let tool_search = tool_search_in_anthropic.unwrap();
        assert_eq!(tool_search["type"], "tool_search_tool_bm25_20251119");
    }

    #[test]
    fn test_web_search_is_server_tool() {
        let tools = get_tui_tools();
        let web_search = tools.iter().find(|t| t.name == "web_search").unwrap();
        assert!(
            web_search.is_server_tool,
            "web_search should be marked as server tool"
        );

        // Local tools should not be marked as server tools
        let bash = tools.iter().find(|t| t.name == "bash").unwrap();
        assert!(!bash.is_server_tool, "bash should not be a server tool");
    }

    #[test]
    fn test_normalize_anthropic_format_to_openai() {
        let anthropic_tools = vec![json!({
            "name": "bash",
            "description": "Run a bash command",
            "input_schema": {"type": "object", "properties": {"command": {"type": "string"}}}
        })];

        let normalized = normalize_tools_for_openai(&anthropic_tools);
        assert_eq!(normalized.len(), 1);

        let tool = &normalized[0];
        assert_eq!(tool["type"], "function");
        assert_eq!(tool["function"]["name"], "bash");
        assert_eq!(tool["function"]["description"], "Run a bash command");
        assert!(tool["function"]["parameters"]["properties"]["command"].is_object());
    }

    #[test]
    fn test_normalize_already_openai_format_passes_through() {
        let openai_tools = vec![json!({
            "type": "function",
            "function": {
                "name": "read_file",
                "description": "Read a file",
                "parameters": {"type": "object", "properties": {"path": {"type": "string"}}}
            }
        })];

        let normalized = normalize_tools_for_openai(&openai_tools);
        assert_eq!(normalized.len(), 1);
        assert_eq!(normalized[0], openai_tools[0]);
    }

    #[test]
    fn test_normalize_nested_function_format() {
        let nested_tools = vec![json!({
            "function": {
                "name": "write_file",
                "description": "Write a file",
                "parameters": {"type": "object", "properties": {"content": {"type": "string"}}}
            }
        })];

        let normalized = normalize_tools_for_openai(&nested_tools);
        assert_eq!(normalized.len(), 1);

        let tool = &normalized[0];
        assert_eq!(tool["type"], "function");
        assert_eq!(tool["function"]["name"], "write_file");
        assert_eq!(tool["function"]["description"], "Write a file");
    }

    #[test]
    fn test_normalize_parameters_field_preferred_over_input_schema() {
        let tools = vec![json!({
            "name": "bash",
            "description": "Run command",
            "parameters": {"type": "object", "properties": {"cmd": {"type": "string"}}},
            "input_schema": {"type": "object", "properties": {"ignored": {"type": "string"}}}
        })];

        let normalized = normalize_tools_for_openai(&tools);
        let params = &normalized[0]["function"]["parameters"];
        assert!(params["properties"]["cmd"].is_object());
        assert!(params["properties"].get("ignored").is_none());
    }

    #[test]
    fn test_normalize_empty_tools_list() {
        let normalized = normalize_tools_for_openai(&[]);
        assert!(normalized.is_empty());
    }

    /// Verify that a tool already in OpenAI format passes through WITHOUT modification.
    /// This is the critical "no double-wrapping" test.
    #[test]
    fn test_openai_format_passes_through_unchanged() {
        let openai_tool = json!({
            "type": "function",
            "function": {
                "name": "read_file",
                "description": "Read file contents",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "path": { "type": "string", "description": "File path" }
                    },
                    "required": ["path"]
                }
            }
        });

        let normalized = normalize_tools_for_openai(std::slice::from_ref(&openai_tool));
        assert_eq!(normalized.len(), 1);
        assert_eq!(
            normalized[0], openai_tool,
            "OpenAI-format tool must pass through unchanged"
        );
    }

    /// Verify structured_thinking tool schema passes through normalize without double-wrapping.
    /// This is the exact bug we hit: structured_thinking caused Zhipu 400 because its
    /// parameters got nested as { "function": { "name": ..., "parameters": ... } }.
    #[test]
    fn test_structured_thinking_schema_no_double_wrap() {
        let schema = json!({
            "type": "function",
            "function": {
                "name": "structured_thinking",
                "description": "Use this tool to record each step of your structured reasoning.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "thought": { "type": "string", "description": "The current thought" },
                        "phase": { "type": "integer", "description": "Phase number" },
                        "type": {
                            "type": "string",
                            "enum": ["decision", "constraint", "validation", "learning", "hypothesis"],
                            "description": "Type of thought"
                        },
                        "confidence": {
                            "type": "integer",
                            "minimum": 0,
                            "maximum": 100,
                            "description": "Confidence (0-100)"
                        },
                        "next_thought_needed": {
                            "type": "boolean",
                            "description": "Whether to think further"
                        }
                    },
                    "required": ["thought", "phase", "type", "confidence", "next_thought_needed"]
                }
            }
        });

        let normalized = normalize_tools_for_openai(std::slice::from_ref(&schema));
        assert_eq!(normalized.len(), 1);

        let tool = &normalized[0];

        // Must still have type: "function" at top level
        assert_eq!(tool["type"], "function");

        // function must be a flat object with name/description/parameters
        let func = &tool["function"];
        assert_eq!(func["name"], "structured_thinking");
        assert!(func["description"].is_string());

        // parameters must be the JSON Schema directly — NOT a nested function object
        let params = &func["parameters"];
        assert_eq!(params["type"], "object", "parameters.type must be 'object'");
        assert!(
            params["properties"]["thought"].is_object(),
            "parameters must have 'thought' property directly"
        );

        // The critical assertion: parameters must NOT contain a "function" key
        assert!(
            params.get("function").is_none(),
            "parameters must NOT contain a nested 'function' — that's double-wrapping"
        );
        assert!(
            params.get("name").is_none(),
            "parameters must NOT contain 'name' — that's double-wrapping"
        );
    }

    /// Verify that multiple tools in mixed formats all normalize correctly.
    #[test]
    fn test_normalize_mixed_format_tools() {
        let tools = vec![
            // OpenAI format (has "type" key)
            json!({
                "type": "function",
                "function": {
                    "name": "bash",
                    "description": "Run bash",
                    "parameters": {"type": "object", "properties": {"command": {"type": "string"}}}
                }
            }),
            // Anthropic format (has "input_schema" instead of "parameters")
            json!({
                "name": "read_file",
                "description": "Read a file",
                "input_schema": {"type": "object", "properties": {"path": {"type": "string"}}}
            }),
            // Flat format (name/description/parameters at top level)
            json!({
                "name": "write_file",
                "description": "Write a file",
                "parameters": {"type": "object", "properties": {"content": {"type": "string"}}}
            }),
            // Nested function format (no "type" key)
            json!({
                "function": {
                    "name": "search",
                    "description": "Search codebase",
                    "parameters": {"type": "object", "properties": {"query": {"type": "string"}}}
                }
            }),
        ];

        let normalized = normalize_tools_for_openai(&tools);
        assert_eq!(normalized.len(), 4, "all tools must be preserved");

        // Every tool must have type: "function"
        for (i, tool) in normalized.iter().enumerate() {
            assert_eq!(
                tool["type"], "function",
                "tool {} must have type 'function'",
                i
            );
            assert!(
                tool["function"]["name"].is_string(),
                "tool {} must have function.name",
                i
            );
            assert!(
                tool["function"]["parameters"]["type"] == "object",
                "tool {} must have function.parameters.type = 'object'",
                i
            );
        }

        // Verify names
        let names: Vec<&str> = normalized
            .iter()
            .map(|t| t["function"]["name"].as_str().unwrap())
            .collect();
        assert_eq!(names, &["bash", "read_file", "write_file", "search"]);
    }

    /// Verify that tools with empty parameters get a default empty-object schema.
    #[test]
    fn test_normalize_tool_with_no_parameters() {
        let tools = vec![json!({
            "name": "ping",
            "description": "Health check"
        })];

        let normalized = normalize_tools_for_openai(&tools);
        assert_eq!(normalized.len(), 1);

        let params = &normalized[0]["function"]["parameters"];
        assert_eq!(params["type"], "object");
        assert!(params["properties"].is_object());
    }

    /// Verify that parameters with required fields are preserved through normalization.
    #[test]
    fn test_normalize_preserves_required_array() {
        let tools = vec![json!({
            "name": "edit_file",
            "description": "Edit a file",
            "parameters": {
                "type": "object",
                "properties": {
                    "path": {"type": "string"},
                    "old": {"type": "string"},
                    "new": {"type": "string"}
                },
                "required": ["path", "old", "new"]
            }
        })];

        let normalized = normalize_tools_for_openai(&tools);
        let required = normalized[0]["function"]["parameters"]["required"]
            .as_array()
            .unwrap();
        assert_eq!(required.len(), 3);
        assert!(required.contains(&json!("path")));
        assert!(required.contains(&json!("old")));
        assert!(required.contains(&json!("new")));
    }

    /// Verify that nested properties (like metadata objects) survive normalization.
    #[test]
    fn test_normalize_preserves_nested_properties() {
        let tools = vec![json!({
            "type": "function",
            "function": {
                "name": "structured_thinking",
                "description": "Record reasoning",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "metadata": {
                            "type": "object",
                            "properties": {
                                "rationale": {"type": "string"},
                                "alternatives_rejected": {
                                    "type": "array",
                                    "items": {"type": "string"}
                                }
                            }
                        }
                    }
                }
            }
        })];

        let normalized = normalize_tools_for_openai(&tools);
        // Since it has "type", it passes through unchanged
        assert_eq!(normalized[0], tools[0]);

        let metadata = &normalized[0]["function"]["parameters"]["properties"]["metadata"];
        assert_eq!(metadata["type"], "object");
        assert!(metadata["properties"]["rationale"].is_object());
        assert!(metadata["properties"]["alternatives_rejected"]["items"]["type"].is_string());
    }

    /// Verify that parameter constraints (minimum, maximum, enum) survive normalization.
    #[test]
    fn test_normalize_preserves_parameter_constraints() {
        let tools = vec![json!({
            "type": "function",
            "function": {
                "name": "set_confidence",
                "description": "Set confidence",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "level": {
                            "type": "integer",
                            "minimum": 0,
                            "maximum": 100
                        },
                        "category": {
                            "type": "string",
                            "enum": ["low", "medium", "high"]
                        }
                    }
                }
            }
        })];

        let normalized = normalize_tools_for_openai(&tools);
        let props = &normalized[0]["function"]["parameters"]["properties"];

        assert_eq!(props["level"]["minimum"], 0);
        assert_eq!(props["level"]["maximum"], 100);
        assert_eq!(props["category"]["enum"].as_array().unwrap().len(), 3);
    }

    #[test]
    fn test_sanitize_strips_minimum_maximum_enum() {
        let tools = vec![json!({
            "type": "function",
            "function": {
                "name": "structured_thinking",
                "description": "Record reasoning",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "thought": { "type": "string", "description": "The current thought" },
                        "type": {
                            "type": "string",
                            "enum": ["decision", "constraint", "validation"],
                            "description": "Type of thought"
                        },
                        "confidence": {
                            "type": "integer",
                            "minimum": 0,
                            "maximum": 100,
                            "description": "Confidence level"
                        }
                    },
                    "required": ["thought", "type", "confidence"]
                }
            }
        })];

        let sanitized = sanitize_tools_for_strict_providers(&tools);
        assert_eq!(sanitized.len(), 1);

        let props = &sanitized[0]["function"]["parameters"]["properties"];

        assert!(props["type"].get("enum").is_none());
        assert!(props["confidence"].get("minimum").is_none());
        assert!(props["confidence"].get("maximum").is_none());

        assert_eq!(props["type"]["type"], "string");
        assert_eq!(props["type"]["description"], "Type of thought");
        assert_eq!(props["confidence"]["type"], "integer");
        assert_eq!(props["thought"]["type"], "string");

        let required = sanitized[0]["function"]["parameters"]["required"]
            .as_array()
            .unwrap();
        assert_eq!(required.len(), 3);
    }

    #[test]
    fn test_sanitize_handles_nested_properties() {
        let tools = vec![json!({
            "type": "function",
            "function": {
                "name": "test_tool",
                "description": "Test",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "metadata": {
                            "type": "object",
                            "additionalProperties": false,
                            "properties": {
                                "score": {
                                    "type": "integer",
                                    "minimum": 0,
                                    "maximum": 10
                                }
                            }
                        }
                    }
                }
            }
        })];

        let sanitized = sanitize_tools_for_strict_providers(&tools);
        let meta = &sanitized[0]["function"]["parameters"]["properties"]["metadata"];

        assert!(meta.get("additionalProperties").is_none());
        assert!(meta["properties"]["score"].get("minimum").is_none());
        assert!(meta["properties"]["score"].get("maximum").is_none());
        assert_eq!(meta["properties"]["score"]["type"], "integer");
    }

    #[test]
    fn test_sanitize_handles_array_items() {
        let tools = vec![json!({
            "type": "function",
            "function": {
                "name": "test_tool",
                "description": "Test",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "tags": {
                            "type": "array",
                            "items": {
                                "type": "object",
                                "properties": {
                                    "value": {
                                        "type": "integer",
                                        "minimum": 1,
                                        "maximum": 100
                                    }
                                }
                            }
                        }
                    }
                }
            }
        })];

        let sanitized = sanitize_tools_for_strict_providers(&tools);
        let items = &sanitized[0]["function"]["parameters"]["properties"]["tags"]["items"];

        assert!(items["properties"]["value"].get("minimum").is_none());
        assert!(items["properties"]["value"].get("maximum").is_none());
        assert_eq!(items["properties"]["value"]["type"], "integer");
    }

    #[test]
    fn test_sanitize_empty_tools() {
        let sanitized = sanitize_tools_for_strict_providers(&[]);
        assert!(sanitized.is_empty());
    }

    #[test]
    fn test_sanitize_preserves_tool_without_function_key() {
        let tools = vec![json!({
            "type": "function",
            "name": "raw_tool"
        })];

        let sanitized = sanitize_tools_for_strict_providers(&tools);
        assert_eq!(sanitized[0]["type"], "function");
        assert_eq!(sanitized[0]["name"], "raw_tool");
    }
}
