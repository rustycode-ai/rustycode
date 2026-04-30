# RustyCode Tool Interface Specification

This document specifies how tools should format their responses, including truncation limits, intelligent tool selection, and output formatting standards.

## Table of Contents

1. [Overview](#1-overview)
2. [Truncation Specification](#2-truncation-specification)
3. [Tool Selection System](#3-tool-selection-system)
4. [Response Formatting](#4-response-formatting)
5. [Integration Guide](#5-integration-guide)
6. [Constants Reference](#6-constants-reference)
7. [Testing](#7-testing)

---

## 1. Overview

### 1.1 Goals

The tool interface system serves three primary goals:

1. **Token Efficiency**: Truncate large outputs to stay within context limits
2. **Intelligent Selection**: Provide relevant tools based on context and usage patterns
3. **Dense Formatting**: Present information compactly while preserving essential data

### 1.2 Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                    LLM Provider Layer                       │
│  (rustycode-llm: Anthropic, OpenAI, Gemini, etc.)          │
└────────────────────┬────────────────────────────────────────┘
                     │
                     │ Tool Call Request
                     ▼
┌─────────────────────────────────────────────────────────────┐
│                   Tool Selector                             │
│  - Profile-based filtering (Explore/Implement/Debug/Ops)   │
│  - Usage-based ranking                                      │
│  - Context-aware prediction                                 │
└────────────────────┬────────────────────────────────────────┘
                     │
                     │ Selected Tool
                     ▼
┌─────────────────────────────────────────────────────────────┐
│                   Tool Execution                            │
│  - Permission checks                                        │
│  - Parameter validation                                      │
│  - Result generation                                         │
└────────────────────┬────────────────────────────────────────┘
                     │
                     │ Raw Output
                     ▼
┌─────────────────────────────────────────────────────────────┐
│                  Truncation Layer                           │
│  - Tool-specific limits                                      │
│  - Smart detection (test/build summaries)                   │
│  - Metadata generation (counts, truncated status)           │
└────────────────────┬────────────────────────────────────────┘
                     │
                     │ Truncated + Structured
                     ▼
┌─────────────────────────────────────────────────────────────┐
│                    Response Formatting                       │
│  - Dense display                                            │
│  - Metadata envelope                                        │
│  - Markdown rendering                                        │
└─────────────────────────────────────────────────────────────┘
```

---

## 2. Truncation Specification

### 2.1 Tool-Specific Limits

| Tool | Limit Type | Value | Rationale |
|------|-----------|-------|-----------|
| **bash** | Lines | 30 | Most command output fits; prevents log spam |
| **bash** | Bytes | 50KB | Catches binary output or massive logs |
| **read_file** | Lines | 80 | Full context for most files; 2-3 screens |
| **grep** | Matches | 15 | Enough to see pattern; request more if needed |
| **glob** | Files | 30 | Manageable list; paginate for large repos |
| **list_dir** | Entries | 30 | Directory navigation use case |

### 2.2 Truncation Behavior

All truncation functions follow this pattern:

```rust
pub fn truncate_<type>(
    input: Input,
    max: usize,
    source_name: &str
) -> TruncatedOutput
```

**Properties**:
- Always preserves first and last N items (context preservation)
- Adds truncation notice with count
- Includes metadata (total count, shown count, truncated flag)
- Never returns empty output (shows at least 1 item)

**Example**:

```rust
// Input: 100 grep matches
let truncated = truncate_items(matches, 15, "grep results");

// Output:
// "**15 matches** for \"pattern\" (showing 15 of 100)\n\n\
//  file1.rs:10 → match line 1\n\
//  file1.rs:20 → match line 2\n\
//  ...\n\
//  file5.rs:50 → match line 15"
```

### 2.3 Smart Detection

The truncation layer includes smart detection for common patterns:

#### Test Summary Detection

```rust
fn detect_test_summary(output: &str) -> bool {
    // Looks for patterns like:
    // "test result: ok. 5 passed; 0 failed"
    // "Passed: 10, Failed: 2"
    // "Tests: 15 passed, 1 failed"
}
```

When detected:
- Shows only summary line: `✓ Test result: ok. 93 passed; 0 failed`
- Hides individual test output (can be requested with specific line range)

#### Build Summary Detection

```rust
fn detect_build_summary(output: &str) -> bool {
    // Looks for:
    // "Compiling... Finished"
    // "Build succeeded in 2.3s"
    // "error: aborting" (errors always shown)
}
```

When detected (success):
- Shows: `✓ Build succeeded in 2.3s`
- Hides compiler warnings (can be requested)

When errors present:
- Shows full error output
- Never truncates errors

---

## 3. Tool Selection System

### 3.1 Multi-Level Filtering

Tools are filtered at three levels:

```
Global Blacklist
    ↓
Agent Profile (Explore/Implement/Debug/Ops)
    ↓
Context Prediction (keyword-based)
    ↓
Usage Ranking (frequency-based)
```

### 3.2 Tool Profiles

#### Explore Profile

**Purpose**: Code discovery and understanding

**Tools**:
- `read_file` - Read source files
- `list_dir` - Navigate directory structure
- `grep` - Search for patterns
- `glob` - Find files by name
- `web_fetch` - Read documentation
- `lsp_hover` - Inspect symbols
- `lsp_definition` - Jump to definitions

**Trigger Keywords**:
- Phrases: "show me", "what is", "how does", "look at", "where is"
- Words: "find", "search", "list", "explore", "understand", "explain", "read", "check"

#### Implement Profile

**Purpose**: Code changes and implementation

**Tools**:
- `write_file` - Create new files
- `edit` - Modify existing files
- `bash` - Run commands
- `read_file` - Read before writing
- `test` - Run tests
- `grep` - Find related code

**Trigger Keywords**:
- "create", "write", "implement", "add", "build", "make", "generate"
- "refactor", "change", "update", "fix", "modify"

#### Debug Profile

**Purpose**: Diagnosis and troubleshooting

**Tools**:
- `lsp_diagnostics` - Get compiler errors
- `lsp_hover` - Inspect types
- `bash` - Run debugger
- `grep` - Search for error patterns
- `read_file` - Read error locations
- `test` - Run failing tests

**Trigger Keywords**:
- "debug", "error", "bug", "issue", "diagnostic", "broken", "fail", "crash"
- "investigate"

#### Ops Profile

**Purpose**: Operations and maintenance

**Tools**:
- `bash` - Run commands
- `git_commit` - Commit changes
- `git_diff` - Show diffs
- `git_status` - Show status
- `web_fetch` - Fetch resources
- `list_dir` - Navigate

**Trigger Keywords**:
- "deploy", "release", "git", "commit", "push", "run", "execute"

### 3.3 Usage Tracking

The system tracks:
- **Use count**: How often each tool is used
- **Last used**: Timestamp for recency ranking

Tools are ranked by frequency within the selected profile:

```rust
available.sort_by(|a, b| {
    let count_a = usage.usage_count(a);
    let count_b = usage.usage_count(b);
    count_b.cmp(&count_a)  // Most used first
});
```

### 3.4 Custom Overrides

#### Always Include

Force specific tools to always be available:

```rust
let selector = ToolSelector::new()
    .always_include("custom_tool")
    .always_include("experimental_feature");
```

#### Always Exclude

Block specific tools from selection:

```rust
let selector = ToolSelector::new()
    .always_exclude("dangerous_command")
    .always_exclude("deprecated_tool");
```

### 3.5 Global Blacklist

These tools never appear in suggestions:

```rust
const FILTERED_FROM_SUGGESTIONS: &[&str] = &[
    "invalid",      // Internal/debug
    "patch",        // Legacy
    "batch",        // Internal
    "internal",     // Implementation details
];
```

---

## 4. Response Formatting

### 4.1 Response Structure

All tool responses use this structure:

```rust
pub struct ToolOutput {
    /// Human-readable text (truncated, formatted)
    pub text: String,

    /// Structured metadata (counts, status, etc.)
    pub metadata: serde_json::Value,
}
```

### 4.2 Metadata Standard

All tools should include:

```json
{
  "tool": "tool_name",
  "truncated": true,
  "total_items": 100,
  "shown_items": 15,
  "source": "file_pattern or description"
}
```

Tool-specific additions:

#### Bash

```json
{
  "exit_code": 0,
  "command": "cargo test",
  "duration_ms": 2340,
  "transformed": { "title": "...", "short": "..." }
}
```

#### ReadFile

```json
{
  "path": "/path/to/file.rs",
  "total_bytes": 12345,
  "shown_bytes": 3456,
  "total_lines": 250,
  "shown_lines": 80
}
```

#### Grep

```json
{
  "pattern": "fn main",
  "total_matches": 42,
  "case_sensitive": false
}
```

### 4.3 Dense Formatting Examples

#### Before: Raw Bash Output

```
Running cargo test...
   Compiling myapp v0.1.0
    Finished dev [unoptimized + debuginfo] target(s) in 1.23s
     Running unittests src/lib.rs

running 15 tests
test tests::test_add ... ok
test tests::test_subtract ... ok
test tests::test_multiply ... ok
...
test result: ok. 15 passed; 0 failed
```

#### After: Smart Formatting

```
✓ Test result: ok. 15 passed; 0 failed (1.23s)
```

Full output available by requesting specific line range if needed.

#### Before: Raw Grep Output

```
20 matches for "async fn" in 12 files
src/api.rs:45: async fn handle_request() {
src/api.rs:78: async fn process_response() {
src/auth.rs:12: async fn login() {
...
```

#### After: Dense Grep Output

```
**20 matches** for "async fn" (showing 15 of 20)

src/api.rs:45 → async fn handle_request()
src/api.rs:78 → async fn process_response()
src/auth.rs:12 → async fn login()
src/auth.rs:34 → async fn logout()
src/db.rs:56 → async fn query()
src/db.rs:89 → async fn transaction()
...
(+ 5 more matches)
```

---

## 5. Integration Guide

### 5.1 For LLM Providers

When calling tools from your LLM provider integration:

```rust
use rustycode_tools::{ToolRegistry, ToolSelector, ToolProfile};

// 1. Get available tools for context
let selector = ToolSelector::new()
    .with_profile(ToolProfile::from_prompt(&user_prompt));

let tools = selector.select_tools();

// 2. Format tools for LLM
let tools_description = selector.format_tools_for_llm(&tools);

// 3. Include in system prompt or tools parameter
let system_prompt = format!(
    "You have access to these tools: {}\n\n\
     Use tools judiciously. Not every task requires a tool.",
    tools_description
);
```

### 5.2 Example: Anthropic Integration

```rust
use anthropic_rs::Client;

async fn call_with_tools(client: &Client, prompt: &str) -> Result<String> {
    let selector = ToolSelector::new()
        .with_profile(ToolProfile::from_prompt(prompt));

    let tools = selector.select_tools();
    let tool_defs = tools.iter()
        .map(|name| registry.get_tool(name).schema())
        .collect();

    let response = client
        .messages()
        .with_tools(tool_defs)
        .with_system("You are RustyCode, an AI coding assistant.")
        .with_user_prompt(prompt)
        .await?;

    // Handle tool calls...
    Ok(response.content)
}
```

### 5.3 Example: OpenAI Integration

```rust
use openai_rs::ChatCompletion;

async fn call_with_tools_openai(prompt: &str) -> Result<String> {
    let selector = ToolSelector::new()
        .with_profile(ToolProfile::from_prompt(prompt));

    let tools = selector.select_tools();

    let response = ChatCompletion::builder()
        .model("gpt-4")
        .tools(tools)  // Automatically formatted by selector
        .message(prompt)
        .call()
        .await?;

    Ok(response.content)
}
```

---

## 6. Constants Reference

### 6.1 Truncation Limits

```rust
// Maximum output sizes per tool
pub const BASH_MAX_LINES: usize = 30;
pub const BASH_MAX_BYTES: usize = 50_000;  // 50KB
pub const READ_MAX_LINES: usize = 80;
pub const GREP_MAX_MATCHES: usize = 15;
pub const LIST_MAX_ITEMS: usize = 30;
```

### 6.2 Tool Profiles

```rust
pub enum ToolProfile {
    Explore,    // Discovery: read, search, navigate
    Implement,  // Creation: write, edit, build
    Debug,      // Diagnosis: lsp, test, grep
    Ops,        // Operations: git, bash, deploy
    All,        // All tools available
}
```

### 6.3 Global Filters

```rust
// Tools never shown in suggestions
pub const FILTERED_FROM_SUGGESTIONS: &[&str] = &[
    "invalid",
    "patch",
    "batch",
    "internal",
];
```

---

## 7. Testing

### 7.1 Unit Tests

Run all tool interface tests:

```bash
cargo test -p rustycode-tools
```

Expected results:
- `truncation.rs`: 8/8 tests passing
- `tool_selector.rs`: 6/6 tests passing

### 7.2 Integration Tests

Test tool selection with real prompts:

```rust
#[test]
fn test_explore_profile_detection() {
    assert_eq!(
        ToolProfile::from_prompt("Show me the main function"),
        ToolProfile::Explore
    );
}

#[test]
fn test_implement_profile_detection() {
    assert_eq!(
        ToolProfile::from_prompt("Create a new user model"),
        ToolProfile::Implement
    );
}

#[test]
fn test_debug_profile_detection() {
    assert_eq!(
        ToolProfile::from_prompt("Debug this authentication error"),
        ToolProfile::Debug
    );
}
```

### 7.3 Manual Testing

Test truncation behavior:

```bash
# Should show 15 matches max
rustycode "grep async fn"

# Should show 30 files max
rustycode "glob *.rs"

# Should show 80 lines max
rustycode "read large_file.rs"
```

Test tool selection:

```bash
# Should select Explore profile tools
rustycode "Show me how authentication works"

# Should select Implement profile tools
rustycode "Add a new endpoint for user registration"

# Should select Debug profile tools
rustycode "Why is this test failing?"
```

---

## 8. Future Enhancements

### 8.1 Persistent Usage Tracking

Currently usage tracking is session-only. Future enhancement:

```rust
// Store usage stats in ~/.rustycode/usage.json
pub struct PersistentUsageTracker {
    file_path: PathBuf,
    session_stats: UsageTracker,
    historical_stats: UsageTracker,
}

impl PersistentUsageTracker {
    pub fn load() -> Result<Self> {
        let file_path = dirs::home_dir()
            .ok_or_else(|| anyhow!("no home dir"))?
            .join(".rustycode/usage.json");

        // Load or create...
    }

    pub fn save(&self) -> Result<()> {
        // Persist to disk
    }
}
```

### 8.2 MCP Integration

Model Context Protocol (MCP) tools can be integrated:

```rust
pub struct McpToolAdapter {
    server_name: String,
    tool_name: String,
}

impl Tool for McpToolAdapter {
    fn name(&self) -> &str {
        &format!("{}:{}", self.server_name, self.tool_name)
    }

    fn execute(&self, params: Value, ctx: &ToolContext) -> Result<ToolOutput> {
        // Call MCP server via stdio or WebSocket
        // Apply same truncation standards
    }
}
```

### 8.3 Advanced Context Analysis

Beyond keyword matching, use semantic analysis:

```rust
pub struct SemanticToolPredictor {
    embedding_model: EmbeddingModel,
    tool_embeddings: HashMap<String, Vec<f32>>,
}

impl SemanticToolPredictor {
    pub fn predict_tools(&self, prompt: &str) -> Vec<String> {
        let prompt_embedding = self.embedding_model.embed(prompt);

        // Find tools with similar embeddings
        self.tool_embeddings
            .iter()
            .map(|(tool, emb)| (tool, cosine_similarity(prompt_embedding, emb)))
            .filter(|(_, sim)| *sim > 0.7)
            .map(|(tool, _)| tool.clone())
            .collect()
    }
}
```

---

## 9. References

### 9.1 Related Documentation

- [OpenCode Tool Registry](https://github.com/smallcloudopensource/opencode) - Inspiration for tool filtering patterns
- [Anthropic Tool Use](https://docs.anthropic.com/claude/docs/tool-use) - Tool calling API
- [OpenAI Function Calling](https://platform.openai.com/docs/guides/function-calling) - Alternative tool calling approach

### 9.2 Code Locations

- Truncation: `crates/rustycode-tools/src/truncation.rs`
- Tool Selection: `crates/rustycode-tools/src/tool_selector.rs`
- Tool Implementations: `crates/rustycode-tools/src/*.rs`
- Integration: `crates/rustycode-llm/src/providers/*.rs`

---

## Appendix A: Full Example

### A.1 Complete Tool Call Flow

```rust
use rustycode_tools::{ToolRegistry, ToolSelector, ToolProfile};
use rustycode_llm::anthropic::AnthropicProvider;

#[tokio::main]
async fn main() -> Result<()> {
    // 1. User prompt
    let prompt = "Show me how authentication works in this codebase";

    // 2. Select appropriate tools
    let profile = ToolProfile::from_prompt(prompt);
    assert_eq!(profile, ToolProfile::Explore);

    let selector = ToolSelector::new()
        .with_profile(profile);

    let tools = selector.select_tools();
    // ["read_file", "grep", "list_dir", "lsp_hover", ...]

    // 3. Call LLM with tools
    let provider = AnthropicProvider::new()?;
    let response = provider
        .call_with_tools(prompt, &tools)
        .await?;

    // 4. LLM decides to use tools
    if response.tool_calls.is_empty() {
        println!("{}", response.text);
        return Ok(());
    }

    // 5. Execute tool calls (with truncation)
    for tool_call in response.tool_calls {
        let tool = registry.get_tool(&tool_call.name)?;
        let result = tool.execute(tool_call.params, &ctx)?;

        // Result is already truncated and formatted
        println!("{}", result.text);

        // Metadata available for structured use
        if result.metadata["truncated"].as_bool().unwrap_or(false) {
            println!(
                "[Showing {} of {} items]",
                result.metadata["shown_items"],
                result.metadata["total_items"]
            );
        }
    }

    // 6. Continue conversation with tool results...

    Ok(())
}
```

### A.2 Expected Output

```
**42 matches** for "auth" (showing 15 of 42)

src/auth/mod.rs:1 → mod authentication;
src/auth/mod.rs:2 → mod authorization;
src/auth/mod.rs:5 → pub use authentication::Authenticator;
src/auth/authentication.rs:10 → pub struct Authenticator {
src/auth/authentication.rs:15 → impl Authenticator {
src/auth/authentication.rs:20 → pub fn login(&mut self, user: &str, pass: &str) -> Result<bool> {
src/auth/authentication.rs:45 → pub fn logout(&mut self) -> Result<()> {
src/auth/middleware.rs:8 → pub async fn auth_middleware(req: Request) -> Result<Request> {
...
(+ 27 more matches)

📂 src/auth/ (3 items)
  authentication.rs - Core authentication logic
  authorization.rs - Permission checking
  middleware.rs - HTTP middleware

[Authentication System Overview]
The system uses JWT tokens with 24h expiration...
```

---

*Last updated: 2025-03-14*
*Version: 1.0.0*
