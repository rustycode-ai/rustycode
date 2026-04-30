#[derive(Clone, Copy, Debug, PartialEq, Hash, Default)]
#[non_exhaustive]
pub enum AiMode {
    /// Default mode - ask before destructive actions
    #[default]
    Ask,
    /// Plan mode - only describe what would be done, don't execute
    Plan,
    /// Act mode - execute but summarize before destructive actions
    Act,
    /// Yolo mode - fully autonomous, no confirmation
    Yolo,
}

/// Specialized agent mode for different types of tasks
///
/// These modes configure the AI's behavior, available tools, and system prompts
/// for specific types of work.
#[derive(Clone, Copy, Debug, PartialEq, Hash, Default)]
#[non_exhaustive]
pub enum AgentMode {
    /// Default code mode - full coding capabilities
    #[default]
    Code,
    /// Architecture mode - design and planning, no file edits
    Architect,
    /// Debug mode - focused on troubleshooting and diagnosis
    Debug,
    /// Review mode - code review and analysis only
    Review,
    /// Test mode - focused on writing and running tests
    Test,
    /// Refactor mode - code improvements without functional changes
    Refactor,
    /// Documentation mode - focused on docs and comments
    Docs,
}

impl AgentMode {
    /// Get the display name for this mode
    pub fn display_name(&self) -> &'static str {
        match self {
            AgentMode::Code => "Code",
            AgentMode::Architect => "Architect",
            AgentMode::Debug => "Debug",
            AgentMode::Review => "Review",
            AgentMode::Test => "Test",
            AgentMode::Refactor => "Refactor",
            AgentMode::Docs => "Docs",
        }
    }

    /// Get the description for this mode
    pub fn description(&self) -> &'static str {
        match self {
            AgentMode::Code => "Full coding capabilities - write, edit, and refactor code",
            AgentMode::Architect => "Design and planning - no direct file edits",
            AgentMode::Debug => "Troubleshooting and diagnosis - read-only + diagnostic tools",
            AgentMode::Review => "Code review and analysis - read-only with review comments",
            AgentMode::Test => "Test focused - write and run tests",
            AgentMode::Refactor => "Code improvements - maintain behavior, improve structure",
            AgentMode::Docs => "Documentation - add and improve docs and comments",
        }
    }

    /// Get the system prompt suffix for this mode
    pub fn system_prompt_suffix(&self) -> &'static str {
        match self {
            AgentMode::Code => {
                "
You are in Code mode. You have full capabilities to:
- Write new code and features
- Edit existing code
- Refactor and improve code structure
- Run tests and diagnostics
- Execute commands

Focus on writing clean, idiomatic code that follows best practices.
"
            }
            AgentMode::Architect => {
                "
You are in Architect mode. Your role is to:
- Design system architecture and components
- Plan implementation approaches
- Identify technical risks and trade-offs
- Suggest patterns and abstractions

You should NOT directly edit files. Instead, describe:
- What needs to be built
- How components should interact
- Key design decisions and rationale
- Implementation steps for developers

Use read_file and list_dir to understand the codebase, but describe changes rather than making them.
"
            }
            AgentMode::Debug => {
                "
You are in Debug mode. Your role is to:
- Diagnose issues and bugs
- Analyze error messages and stack traces
- Identify root causes
- Suggest fixes and workarounds

Focus on:
- Understanding what's happening in the code
- Finding the source of problems
- Explaining why issues occur
- Proposing minimal, targeted fixes

Use read_file, grep, and diagnostic tools heavily. Suggest fixes but be conservative about changes.
"
            }
            AgentMode::Review => {
                "
You are in Review mode. Your role is to:
- Review code for quality and correctness
- Identify potential bugs and issues
- Suggest improvements
- Check adherence to best practices

You are READ-ONLY. Do not make any changes. Instead:
- Point out specific issues with line numbers
- Explain why something might be problematic
- Suggest better approaches
- Highlight security or performance concerns

Provide constructive, actionable feedback.
"
            }
            AgentMode::Test => {
                "
You are in Test mode. Your role is to:
- Write comprehensive tests
- Improve test coverage
- Find edge cases and boundary conditions
- Verify existing functionality

Focus on:
- Unit tests for individual functions
- Integration tests for component interactions
- Edge cases and error conditions
- Clear, descriptive test names

You can write test files and run tests, but avoid modifying production code unless it's to make it testable.
"
            }
            AgentMode::Refactor => {
                "
You are in Refactor mode. Your role is to:
- Improve code structure and organization
- Enhance readability and maintainability
- Reduce complexity and duplication
- Apply design patterns appropriately

IMPORTANT: Maintain the exact same behavior.
- Do not change functionality
- Do not add new features
- Ensure tests still pass

Focus on:
- Extracting and naming functions well
- Reducing nesting and complexity
- Improving names and organization
- Eliminating dead code and duplication
"
            }
            AgentMode::Docs => {
                "
You are in Docs mode. Your role is to:
- Add and improve documentation
- Write clear comments
- Create usage examples
- Document APIs and interfaces

Focus on:
- Module and function documentation
- Inline comments for complex logic
- Usage examples
- API documentation

You can add comments and documentation, but avoid changing code logic.
"
            }
        }
    }

    /// Check if a tool should be available in this mode
    pub fn allows_tool(&self, tool_name: &str) -> bool {
        match self {
            // Code mode - all tools available
            AgentMode::Code => true,

            // Architect - read-only tools only
            AgentMode::Architect => matches!(
                tool_name,
                "read_file"
                    | "list_dir"
                    | "glob"
                    | "grep"
                    | "lsp_document_symbols"
                    | "lsp_references"
                    | "lsp_definition"
                    | "codesearch"
                    | "question"
            ),

            // Debug - read-only + diagnostic tools
            AgentMode::Debug => matches!(
                tool_name,
                "read_file"
                    | "list_dir"
                    | "glob"
                    | "grep"
                    | "bash"
                    | "lsp_document_symbols"
                    | "lsp_references"
                    | "lsp_definition"
                    | "codesearch"
                    | "question"
            ),

            // Review - read-only only
            AgentMode::Review => matches!(
                tool_name,
                "read_file"
                    | "list_dir"
                    | "glob"
                    | "grep"
                    | "lsp_document_symbols"
                    | "lsp_references"
                    | "lsp_definition"
                    | "codesearch"
            ),

            // Test - test-focused tools
            AgentMode::Test => matches!(
                tool_name,
                "read_file" | "write_file" | "list_dir" | "glob" | "grep" | "bash"
            ),

            // Refactor - all tools except destructive ones
            AgentMode::Refactor => !matches!(tool_name, "apply_patch" | "checkpoint"),

            // Docs - all tools for reading, writing comments/docs
            AgentMode::Docs => true,
        }
    }

    /// Get all available modes
    pub fn all() -> &'static [AgentMode] {
        &[
            AgentMode::Code,
            AgentMode::Architect,
            AgentMode::Debug,
            AgentMode::Review,
            AgentMode::Test,
            AgentMode::Refactor,
            AgentMode::Docs,
        ]
    }

    /// Cycle to the next mode
    pub fn next_mode(&self) -> Self {
        match self {
            AgentMode::Code => AgentMode::Architect,
            AgentMode::Architect => AgentMode::Debug,
            AgentMode::Debug => AgentMode::Review,
            AgentMode::Review => AgentMode::Test,
            AgentMode::Test => AgentMode::Refactor,
            AgentMode::Refactor => AgentMode::Docs,
            AgentMode::Docs => AgentMode::Code,
        }
    }

    /// Cycle to the previous mode
    pub fn prev(&self) -> Self {
        match self {
            AgentMode::Code => AgentMode::Docs,
            AgentMode::Architect => AgentMode::Code,
            AgentMode::Debug => AgentMode::Architect,
            AgentMode::Review => AgentMode::Debug,
            AgentMode::Test => AgentMode::Review,
            AgentMode::Refactor => AgentMode::Test,
            AgentMode::Docs => AgentMode::Refactor,
        }
    }
}
