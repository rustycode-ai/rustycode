# rustycode-tools

Tool execution framework, security enforcement, and code intelligence for RustyCode.

## Purpose

This crate provides the complete tool layer that RustyCode agents use to interact with the outside world: reading and writing files, executing shell commands, querying LSP servers, searching codebases, managing plans, and more. It defines the `Tool` trait, implements 30+ concrete tools, and enforces security boundaries (path validation, sandboxing, threat detection, permission management) around every operation.

**Downstream consumers** include `rustycode-core`, `rustycode-tui`, `rustycode-orchestration`, `rustycode-llm`, `rustycode-cli`, `rustycode-mcp`, `rustycode-runtime`, `rustycode-plugins`, `rustycode-execution`, and `rustycode-acp`. Almost every crate that needs to execute a tool depends on this one.

## Current Architecture

The crate has **104 source files** and **~66K lines of code**, organized into five sub-module directories and ~50 root-level modules.

### Sub-module directories

| Directory | LOC | Purpose |
|-----------|-----|---------|
| `providers/` | ~19.8K | Concrete `Tool` trait implementations (bash, fs, edit, git, lsp, docker, database, search, etc.) |
| `security/` | ~5.2K | Path validation, permission management, sandboxing, threat scanning, directory trust, approval |
| `executor/` | ~5.0K | Tool dispatch, caching, batching, inspection pipeline, middleware, tool-shim extraction, sub-agent tasks |
| `indexing/` | ~5.0K | Tree-sitter repo map, code indexing, semantic search (optional) |
| `registry/` | ~3.5K | `ToolCatalog` enum, tool discovery/registration, selector, permissions mapping, audit logging |

### Root-level modules (~28K LOC)

These modules have not yet been moved into sub-directories. They cover a wide range of concerns:

- **Core utilities**: `truncation`, `compaction`, `line_endings`, `json_repair`, `yaml_format`, `text_summary`
- **Code intelligence**: `edit_format`, `code_review`, `diagnostics`, `token_counter`
- **Security and safety**: `security_patterns`, `egress_detector`, `doom_loop`, `smart_approve`, `directory_trust`
- **Lifecycle and hooks**: `hooks`, `lifecycle`, `streaming`, `markdown_stream`, `log_rotation`
- **File management**: `file_snapshot`, `file_formatter`, `file_reference`, `workspace_checkpoint`, `checkpoint`
- **Agent support**: `plan_management`, `plan_templates`, `recipes`, `task_retry`, `observation_layer`
- **Infrastructure**: `config_migration`, `plugin`, `plugin_manager`, `app_paths`, `executable_search`, `subprocess`, `testing`
- **UI and templates**: `prompt_template`, `hints_loader`, `slash_commands`, `skills`, `todo`, `todo_read`
- **Miscellaneous**: `commit_msg`, `image_detect`, `osv_check`, `project_tracker`, `transform`, `tool_arg_coercion`, `large_response`, `native_tools`, `shutdown`, `api`

## Key Types and Public API

### Core types (re-exported from `rustycode-tools-api`)

These types form the core tool interface and are available directly from this crate:

```rust
use rustycode_tools::{
    Tool, ToolContext, ToolOutput, ToolPermission, ToolInfo,
    ToolRegistry, ToolGate, ToolProfile, ToolSelector, CancellationToken,
};
```

- **`Tool`** -- Trait every tool implements (`name`, `description`, `parameters_schema`, `permission`, `execute`).
- **`ToolContext`** -- Execution context carrying working directory, agent role, and plan gate.
- **`ToolOutput`** -- Result of a tool execution (text output plus metadata).
- **`ToolPermission`** -- Enum: `None`, `Read`, `Write`, `Execute`, `Network`.
- **`ToolRegistry`** -- Registry that maps tool names to `Tool` implementations and dispatches calls.

### Tool executor

```rust
use rustycode_tools::ToolExecutor;

let executor = ToolExecutor::from_cwd(std::env::current_dir()?);

// List available tools
for tool in executor.list() {
    println!("{}: {}", tool.name, tool.description);
}

// Execute a tool call
let call = ToolCall {
    name: "bash".into(),
    arguments: json!({"command": "ls"}),
    id: "1".into(),
};
let result = executor.execute(&call);
```

### Tool catalog

```rust
use rustycode_tools::registry::catalog::ToolCatalog;

// Exhaustive enum of all tools -- compile-time guaranteed coverage
let catalog = ToolCatalog::Bash(BashInput {
    command: "ls -la".into(),
    timeout: None,
});

// Case-insensitive lookup
assert!(ToolCatalog::contains("bash"));
assert!(ToolCatalog::contains("Bash"));
```

### Security

```rust
use rustycode_tools::security::validation::{validate_read_path, validate_write_path};
use rustycode_tools::security::sandbox::{Sandbox, SandboxLevel};
use rustycode_tools::security::patterns::ThreatScanner;
use rustycode_tools::security::trust::DirectoryTrust;

// Validate a path before reading
validate_read_path(&path, &workspace_root)?;

// Create a sandbox
let sandbox = Sandbox::new(cwd, allowed, denied, SandboxLevel::Strict)?;
sandbox.enforce()?;
```

### Session-mode permission check

```rust
use rustycode_tools::check_tool_permission;
use rustycode_protocol::SessionMode;

// Planning mode only allows read-only tools
assert!(check_tool_permission("read_file", SessionMode::Planning));
assert!(!check_tool_permission("bash", SessionMode::Planning));

// Executing mode allows all tools
assert!(check_tool_permission("bash", SessionMode::Executing));
```

### Repo map (code indexing)

```rust
use rustycode_tools::indexing::repo_map::RepoMap;

let map = RepoMap::build(Path::new("."), 4000)?;
println!("{}", map.to_map_string());
```

### Tool shim (extract tool calls from text)

```rust
use rustycode_tools::executor::tool_shim::ToolCallExtractor;

let text = r#"{"name": "bash", "arguments": {"command": "ls"}}"#;
let calls = ToolCallExtractor::extract(text);
assert_eq!(calls[0].name, "bash");
```

## Features

### Tool implementations (`providers/`)

30+ concrete tools implementing the `Tool` trait:

| Category | Tools |
|----------|-------|
| File I/O | `read_file`, `write_file`, `edit`, `multiedit`, `apply_patch`, `claude_text_editor`, `search_replace` |
| Shell | `bash` (with timeout, streaming, error detection) |
| Search | `grep`, `glob`, `codesearch`, `web_search`, `web_fetch` |
| Version control | `git_status`, `git_diff`, `git_log`, `git_commit` |
| LSP | `lsp_diagnostics`, `lsp_hover`, `lsp_definitions`, `lsp_references`, `lsp_completion`, `lsp_document_symbols` |
| Docker | `docker_build`, `docker_run`, `docker_ps`, `docker_stop`, `docker_logs`, `docker_inspect`, `docker_images` |
| Database | `database_query`, `database_schema`, `database_transaction` |
| Code intelligence | `symbol`, `question`, `compile_time` |
| Sub-agents | `task` (delegates to nested LLM conversation) |

### Security subsystem (`security/`)

- **Path validation** -- Blocks traversal attacks, symlinks, blocked extensions (`.env`, `.key`, `.pem`), and access outside workspace.
- **Sandboxing** -- Platform-specific: Landlock (Linux 5.13+), macOS sandbox, path-based ACL (fallback).
- **Threat scanning** -- Regex-based detection of dangerous commands (filesystem destruction, remote code execution, data exfiltration, privilege escalation).
- **Permission management** -- `PermissionManager` with risk-level classification, interactive prompts, and persistent permission storage.
- **Directory trust** -- Hierarchical trust that inherits to subdirectories but stops at git boundaries.
- **Smart approval** -- Heuristic classification of tool calls into read-only, write, and destructive categories.

### Execution infrastructure (`executor/`)

- **Batch execution** -- Parallel execution of independent tool calls for 2-5x speedup.
- **Caching** -- LRU cache with file-based invalidation, TTL expiration, and memory tracking.
- **Inspection pipeline** -- Composable inspectors: repetition detection, permission enforcement, rate limiting, budget tracking.
- **Middleware** -- Hook execution (pre/post tool use), plan mode validation, cost tracking.
- **Tool shim** -- Extracts structured tool calls from plain-text LLM output (XML, JSON, function-call, markdown formats).
- **Sub-agent tasks** -- `TaskTool` spawns nested LLM conversations for focused subtasks (max 10 turns, 5 min timeout).

### Code indexing (`indexing/`)

- **Repo map** -- Tree-sitter based structural summary for LLM context efficiency (Rust, Python, JavaScript, Go, TypeScript).
- **Code index** -- File-level symbol extraction and search.
- **Semantic search** -- Vector-based code search (gated behind `vector-memory` feature).

### Registry and discovery (`registry/`)

- **ToolCatalog** -- Exhaustive enum of all tools with tagged serialization and case-insensitive lookup.
- **Selector** -- Context-aware tool selection with usage profiles (Explore, Implement, Debug, Ops).
- **Audit logger** -- Per-tool timing, success/failure rates, and slow-tool detection.

### Agent support utilities

- **Compaction** -- Progressive context compaction (middle-out removal of tool responses when token thresholds are exceeded).
- **Doom loop detector** -- Detects when an LLM agent is stuck repeating the same tool calls with similar arguments.
- **Observation layer** -- `tracing_subscriber::Layer` implementation for Langfuse/Jaeger observability.
- **File snapshots** -- Undo system tracking file changes per tool execution, with snapshot groups for atomic rollback.
- **Egress detector** -- Network destination extraction from shell commands (URLs, git remotes, S3 buckets, SSH targets).
- **Plan management** -- CRUD for execution plans with templates.
- **Hooks** -- Configurable lifecycle event hooks (`SessionStart`, `SessionEnd`, `PreToolUse`, `PostToolUse`, `PreCompact`, `PostCompact`, `Error`).

## Known Limitations and God Object Status

This crate is documented as a **god object** in the architecture review. It has grown to ~66K LOC across 104 files and is significantly over-connected.

### Specific problems

1. **Circular dependency with `rustycode-llm`** (P0 issue). The `rustycode-llm` crate depends on `rustycode-tools` for tool type definitions and the shim, while `rustycode-tools` depends on `rustycode-llm` features. This prevents independent testing and refactoring. A mitigation crate (`rustycode-tool-integration`) has been created, but full cleanup is still in progress.

2. **Scope creep** -- The crate contains modules unrelated to tool execution: `compaction`, `code_review`, `commit_msg`, `diagnostics`, `egress_detector`, `markdown_stream`, `observation_layer`, `osv_check`, `prompt_template`, `recipes`, `streaming`, `telemetry_limiter`, `todo`, `token_counter`, and `transform`. These should live in their own crates or in the crates that consume them.

3. **Security is tightly coupled to providers** -- Tool implementations in `providers/` directly call `security::validation` functions. This makes it impossible to use the security subsystem independently or to swap security implementations.

4. **Massive allow block** -- The crate root carries ~70 clippy lint suppressions, a clear indicator that the codebase needs structural cleanup.

5. **Duplicate modules** -- `smart_approve` exists both as a root module and inside `security/approve`, with nearly identical types. `security_patterns` (root) duplicates `security::patterns`.

6. **LSP provider at 115K LOC** -- The `providers/lsp.rs` file alone is the largest in the crate and should be split into per-operation modules.

7. **Bash provider at 84K LOC** -- The `providers/bash.rs` file handles shell detection, command validation, error detection, timeout management, and streaming -- each of which could be its own module.

## Intended Future Architecture

The proposed modular breakdown separates concerns into distinct crates with clear boundaries:

```
rustycode-tools-api/       (unchanged) -- Tool trait, ToolContext, ToolOutput, ToolPermission
rustycode-tools-security/  (new crate) -- validation, sandbox, patterns, trust, approve
rustycode-tools-registry/  (exists)    -- catalog, selector, loader, permissions mapping, audit
rustycode-tools-indexing/  (new crate) -- repo_map, code_index, semantic_search
rustycode-tools/           (slimmed)   -- executor dispatch, providers, middleware, caching
```

### Migration plan

1. **Extract `rustycode-tools-security`** -- Move `security/` to its own crate. Providers depend on it rather than the monolith. This breaks the tight coupling between validation and tool implementations.

2. **Extract `rustycode-tools-indexing`** -- Move `indexing/` (repo map, code index, semantic search) to its own crate. Tree-sitter dependencies (5 grammars) only need to be compiled when indexing is used.

3. **Merge duplicate modules** -- Consolidate `smart_approve` (root) into `security/approve`, and `security_patterns` (root) into `security/patterns`.

4. **Split large providers** -- Break `lsp.rs` into `lsp_diagnostics.rs`, `lsp_hover.rs`, `lsp_definitions.rs`, etc. Break `bash.rs` into `bash_core.rs`, `bash_validation.rs`, `bash_streaming.rs`.

5. **Relocate misplaced modules** -- Move `compaction` to `rustycode-core`, `observation_layer` to `rustycode-observability`, `token_counter` to `rustycode-llm`, `commit_msg` to `rustycode-git`.

6. **Resolve circular dependency** -- The `rustycode-tool-integration` shim crate already exists. Complete the migration so `rustycode-llm` and `rustycode-tools` communicate only through it.

## Dependencies

### External crates

| Crate | Purpose |
|-------|---------|
| `tokio` | Async runtime for concurrent tool execution |
| `serde` / `serde_json` / `serde_yaml` | Serialization for tool parameters and results |
| `regex` | Pattern matching in search, validation, threat detection |
| `tree-sitter` + grammars (5) | Code parsing for repo map (Rust, Python, JS, Go, TS) |
| `similar` | Diff generation for edit tool output |
| `walkdir` / `ignore` | Directory traversal respecting `.gitignore` |
| `lru` / `dashmap` | Caching and concurrent data structures |
| `governor` | Rate limiting for tool calls |
| `handlebars` | Prompt template rendering |
| `reqwest` | HTTP client for web tools |
| `sha2` | Content hashing for cache keys |
| `lsp-types` | Language Server Protocol type definitions |
| `anyhow` / `thiserror` | Error handling (application and library) |
| `tracing` / `tracing-subscriber` | Structured logging and observation layer |

### Cross-crate dependencies

| Dependency | Purpose |
|------------|---------|
| `rustycode-tools-api` | Core `Tool` trait, `ToolContext`, `ToolOutput`, `ToolPermission` |
| `rustycode-protocol` | Shared types (`ToolCall`, `ToolResult`, `Message`, `SessionMode`) |
| `rustycode-config` | Configuration loading |
| `rustycode-lsp` | LSP client for language server tools |
| `rustycode-bus` | Event bus for inter-module communication |
| `rustycode-storage` | Session persistence |
| `rustycode-shared-runtime` | Shared tokio runtime |
| `rustycode-thread-guard` | Thread safety utilities |
| `rustycode-tool-integration` | Shim to break circular dependency with `rustycode-llm` |
| `rustycode-vector-memory` | Vector search (optional, `vector-memory` feature) |

### Downstream consumers

`rustycode-core`, `rustycode-tui`, `rustycode-orchestration`, `rustycode-llm`, `rustycode-cli`, `rustycode-mcp`, `rustycode-runtime`, `rustycode-plugins`, `rustycode-macros`, `rustycode-execution`, `rustycode-acp`

## Architecture Notes

### Design patterns

- **Strategy pattern** -- Each tool implements the `Tool` trait, allowing the registry to dispatch uniformly.
- **Inspector pipeline** -- Composable `Inspector` implementations that run before tool execution (repetition, permission, rate-limit, budget). Each inspector returns `InspectionAction` (Allow, Deny, or RequireApproval).
- **Builder pattern** -- `ToolExecutor` and `ToolContext` use builder methods (`with_role`, `with_plan_gate`).
- **Observer pattern** -- `LifecycleEvent` hooks allow cross-cutting concerns without tight coupling.
- **Enum dispatch** -- `ToolCatalog` provides exhaustive matching over all tools for compile-time safety.

### Design rationale

- **Tool shim** extracts tool calls from plain-text LLM output using regex rather than a secondary LLM call, making it zero-cost and instant. This enables tool use with models that lack native function calling.
- **Progressive compaction** removes tool responses from the middle of conversation history first, preserving recent context and early system messages.
- **Flexible edit matching** (exact, line-ending-normalized, trimmed) handles common LLM output issues where models normalize whitespace or line endings.
- **Batch execution** runs independent tool calls in parallel threads rather than sequential dispatch, providing 2-5x speedup for common multi-file operations.

### How to extend

To add a new tool:

1. Create a new file in `src/providers/` implementing the `Tool` trait.
2. Register it in `src/providers/mod.rs`.
3. Add a variant to `ToolCatalog` in `src/registry/catalog.rs`.
4. Add the corresponding input type with serde derive.
5. Update `check_tool_permission` in `lib.rs` if the tool should be available in Planning mode.

## Testing

```bash
# Run all tests
cargo test -p rustycode-tools

# Run a specific module's tests
cargo test -p rustycode-tools -- bash
cargo test -p rustycode-tools -- security
cargo test -p rustycode-tools -- repo_map

# Run with optional features
cargo test -p rustycode-tools --features vector-memory
```

Tests are organized as inline `#[cfg(test)] mod tests` blocks within each source file. Integration tests for tool execution patterns live in the workspace `tests/` directory.

The crate uses `tempfile` for filesystem tests, mock HTTP responses for web tools, and mock LSP servers for language server tests. Security tests verify path and command validation across edge cases.

## See Also

- `rustycode-tools-api` -- Core `Tool` trait and context types (the interface this crate implements)
- `rustycode-tools-registry` -- Standalone tool registry crate (partial extraction)
- `rustycode-tool-integration` -- Shim crate for the `rustycode-llm` circular dependency
- `rustycode-tool-server` -- Standalone tool server binary
- `rustycode-llm` -- LLM provider abstraction (has circular dependency with this crate; see Known Limitations)
- `rustycode-protocol` -- Shared types used across crates
- `/docs/architecture/ARCHITECTURE-REVIEW-2026-04-20.md` -- Full architecture review with P0 issue details
