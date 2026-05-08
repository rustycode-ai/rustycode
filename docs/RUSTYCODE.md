# RustyCode — Comprehensive Project Documentation

> Rust-native AI-powered autonomous development framework
> 48 crates · 1,102 source files · ~525K LOC · 10,000+ tests

## Table of Contents

1. [Overview](#overview)
2. [Quick Start](#quick-start)
3. [Architecture](#architecture)
4. [Crate Reference](#crate-reference)
5. [LLM Provider System](#llm-provider-system)
6. [Tool System](#tool-system)
7. [Skill System](#skill-system)
8. [Orchestration Engine](#orchestration-engine)
9. [TUI (Terminal User Interface)](#tui-terminal-user-interface)
10. [CLI Reference](#cli-reference)
11. [Event Bus](#event-bus)
12. [Security Model](#security-model)
13. [Session & Storage](#session--storage)
14. [Benchmarking](#benchmarking)
15. [Configuration](#configuration)
16. [Testing](#testing)
17. [Building & Deployment](#building--deployment)
18. [Project History & Milestones](#project-history--milestones)

---

## Overview

RustyCode is a full-stack AI coding agent built entirely in Rust. It provides three interaction modes:

- **TUI** — Interactive terminal UI (ratatui-based) for pair programming with an AI
- **CLI** — Command-line interface for scripted/automated workflows
- **Headless Agent** — Fully autonomous execution mode for benchmarking and CI

### Key Capabilities

- Multi-provider LLM support (Anthropic, OpenAI, Gemini, Bedrock, Azure, Ollama, LiteRT, Mistral, Cohere, HuggingFace, OpenRouter, Copilot)
- 20+ built-in tools (file I/O, bash, grep, glob, LSP, web fetch, notebook editing, git operations)
- YAML frontmatter-based skill system with brace expansion and path-based activation
- Autonomous orchestration with structured reasoning, quality gates, and AST pipelines
- Type-safe async event bus for decoupled inter-crate communication
- Terminal Bench 2.0 compatible benchmark runner with native and Docker modes
- Context compression and token budget management
- Git worktree-based parallel development
- SWE-bench evaluation support

---

## Quick Start

```bash
# Build
cargo build --release

# Launch TUI
cargo run -- tui

# Run a task directly
cargo run -- "fix the authentication bug in src/auth.rs"

# Autonomous agent mode
cargo run -- agent new "implement user registration endpoint"

# List available LLM providers
cargo run -- provider list

# List installed skills
cargo run -- skills list

# Run benchmarks
cargo run -- bench run --dataset terminal-bench-v2
```

---

## Architecture

### Layer Diagram

```
┌──────────────────────────────────────────────────────────────────┐
│  rustycode-cli (binary)                                          │
│  rustycode-tui (ratatui TUI)  │  rustycode-ws-server (web API)   │
├──────────────────────────────────────────────────────────────────┤
│  rustycode-core (session mgmt, headless runtime)                 │
│  rustycode-orchestration (autonomous execution engine)           │
│  rustycode-runtime (task scheduler, negotiation, resource pool)  │
├──────────────────────────────────────────────────────────────────┤
│  rustycode-llm (providers)     │  rustycode-tools (execution)    │
│  rustycode-bus (events)        │  rustycode-guard (security)     │
│  rustycode-skill (discovery)   │  rustycode-mcp (MCP protocol)   │
├──────────────────────────────────────────────────────────────────┤
│  rustycode-protocol (shared types)   │  rustycode-config          │
│  rustycode-storage (persistence)     │  rustycode-auth            │
│  rustycode-session (session state)   │  rustycode-memory          │
│  rustycode-prompt (templates)        │  rustycode-lsp (LSP client)│
└──────────────────────────────────────────────────────────────────┘
```

### Dependency Direction Rules

1. **Downward only**: Upper layers depend on lower layers, never the reverse
2. **Protocol as lingua franca**: Cross-crate types live in `rustycode-protocol`
3. **Orchestration is standalone**: Never depends on CLI, TUI, or session crates
4. **No circular dependencies**: `rustycode-tool-integration` provides shared traits (`ToolExecutorApi`, `TokenCounter`, `CostTracker`) that both `rustycode-llm` and `rustycode-tools` depend on — no edge between them
5. **Event bus for decoupling**: Crates communicate events via `rustycode-bus`, not direct calls

### Crate Size Distribution

| Category | Crates | LOC |
|----------|--------|-----|
| UI (TUI + CLI) | 3 | 134K |
| Core & Orchestration | 5 | 217K |
| LLM & Providers | 4 | 55K |
| Tools & Security | 5 | 87K |
| Infrastructure | 8 | 32K |
| Domain (skill, bus, storage, etc.) | 16 | 62K |

---

## Crate Reference

### Top-Level (by LOC)

| Crate | LOC | Purpose |
|-------|-----|---------|
| `rustycode-tui` | 125K | Terminal UI — event loop, rendering, input handling, streaming |
| `rustycode-tools` | 83K | Tool execution framework, 20+ tools, permissions, bash sandbox |
| `rustycode-orchestration` | 76K | Autonomous execution, reasoning, quality gates, AST pipeline |
| `rustycode-llm` | 51K | LLM provider abstractions, 12+ provider implementations |
| `rustycode-core` | 44K | Session management, headless runtime, checkpoint recovery |
| `rustycode-runtime` | 42K | Task scheduler, resource pool, negotiation, agent lifecycle |
| `rustycode-protocol` | 20K | Cross-crate shared types, agent protocol, frontmatter parser |
| `rustycode-mcp` | 16K | Model Context Protocol client/server implementation |
| `rustycode-storage` | 11K | Persistence layer (SQLite-backed) |
| `rustycode-bench` | 11K | Benchmark runner (Terminal Bench compatible, native + Docker) |
| `rustycode-cli` | 9K | CLI binary with 20+ subcommands |
| `rustycode-skill` | 9K | Skill discovery, caching, YAML frontmatter, path activation |

### Infrastructure Crates

| Crate | LOC | Purpose |
|-------|-----|---------|
| `rustycode-bus` | 7K | Async type-safe event bus with wildcards and hooks |
| `rustycode-connector` | 6K | Provider connector framework |
| `rustycode-session` | 5K | Session state management |
| `rustycode-config` | 5K | Configuration loading and validation |
| `rustycode-executable` | 5K | Executable/task representation |
| `rustycode-lsp` | 4K | LSP client for diagnostics, hover, definition, references |
| `rustycode-ws-server` | 4K | WebSocket server for web interface |
| `rustycode-git` | 4K | Git operations wrapper (libgit2) |
| `rustycode-acp` | 4K | Agent Communication Protocol |
| `rustycode-memory` | 4K | Agent memory system |
| `rustycode-tools-api` | 4K | Tool trait definitions (`RustyCodeTool`) |
| `rustycode-learning` | 4K | Learning/feedback collection |
| `rustycode-providers` | 3K | Provider registry and catalog |
| `rustycode-agent-runtime` | 3K | Headless agent runtime |
| `rustycode-prompt` | 3K | Prompt templates and model-specific routing |
| `rustycode-tasks` | 2K | Task representation and state machine |
| `rustycode-observability` | 2K | Tracing and metrics |
| `rustycode-vector-memory` | 2K | Vector-based semantic memory |
| `rustycode-guard` | 2K | Security guard (path/command validation) |
| `rustycode-auth` | 2K | Authentication |
| `rustycode-agents` | 2K | Agent definitions |
| `rustycode-tools-registry` | 2K | Tool registration and lookup |
| `rustycode-id` | 1K | ID generation |
| `rustycode-litert` | 1K | LiteRT-LM local inference runtime |
| `rustycode-execution` | 1K | Execution primitives |
| `rustycode-classification` | 1K | Task classification |
| `rustycode-ui-core` | 2K | Shared UI types |
| `rustycode-ui-model` | 1K | UI data models |
| `rustycode-macros` | 1K | Proc macros |
| `rustycode-thread-guard` | 1K | Thread safety utilities |
| `rustycode-tool-integration` | 1K | Shared tool/LLM traits: `ToolExecutorApi`, `TokenCounter`, `CostTracker` |
| `rustycode-sandbox` | <1K | Sandbox execution environment |
| `rustycode-tool-server` | <1K | Tool server for external process execution |
| `rustycode-shared-runtime` | <1K | Shared tokio runtime |

---

## LLM Provider System

### Architecture

The LLM layer (`rustycode-llm`) provides a unified provider abstraction with 12+ implementations:

```
rustycode-llm
├── lib.rs              # Unified LLMProvider trait
├── anthropic.rs        # Claude (direct API)
├── openai.rs           # GPT-4, GPT-3.5 (direct API)
├── gemini.rs           # Google Gemini
├── bedrock.rs          # AWS Bedrock (Claude on AWS)
├── azure.rs            # Azure OpenAI
├── ollama.rs           # Local Ollama models
├── litert_lm.rs        # On-device LiteRT inference
├── mistral.rs          # Mistral AI
├── cohere.rs           # Cohere
├── huggingface.rs      # HuggingFace Inference API
├── openrouter.rs       # OpenRouter multi-model gateway
├── copilot.rs          # GitHub Copilot
├── openai_compatible.rs # Generic OpenAI-compatible endpoints
├── mock.rs             # Mock provider for testing
├── caching.rs          # Response caching layer
├── circuit_breaker.rs  # Fault tolerance
├── cost_tracker.rs     # Token usage and cost tracking
├── client_pool.rs      # Connection pooling
└── conversation.rs     # Conversation history management
```

### Provider Trait

All providers implement the unified `LLMProvider` trait, enabling transparent switching:

```rust
// All providers implement this trait
// Conversation turn: send messages, get streaming response
async fn chat_stream(&self, messages: &[Message]) -> Result<ChatStream>;
```

### Resilience Features

- **Circuit breaker**: Trips after N failures, recovers with exponential backoff
- **Client pooling**: Reuses connections across requests
- **Graceful degradation**: Falls back to simpler models on timeout/cost
- **Offline mode**: Routes to local providers (Ollama, LiteRT) when cloud is unavailable
- **Cost tracking**: Per-request token accounting with budget enforcement

### Provider Commands

```bash
rustycode provider list        # List all configured providers
rustycode provider models anthropic  # Show models for a provider
rustycode provider info claude-sonnet-4-6  # Model details
rustycode provider tiers       # Models by cost tier
rustycode provider catalog     # Unified model catalog
rustycode provider install     # Install LiteRT local runtime
```

---

## Tool System

### Tool Architecture

```
rustycode-tools-api (trait definitions)
         ↓
rustycode-tools (concrete implementations)
    ├── read_file       # Read with offset/limit, line-numbered output
    ├── edit_file       # 3-strategy matching (exact → normalized → trimmed)
    ├── write_file      # Write with diff output on overwrite
    ├── bash            # Shell execution with timeout and background support
    ├── grep            # Search with type filter, case-insensitive, context
    ├── glob            # File pattern matching
    ├── web_fetch       # URL content fetching with prompt extraction
    ├── notebook_edit   # Jupyter notebook cell editing (replace/insert/delete)
    ├── lsp_*           # 8 LSP tools (diagnostics, hover, definition, references, etc.)
    ├── todo_write      # Task tracking
    ├── memory_*        # Agent memory operations
    └── git_*           # Git status, diff, log
```

### Tool Trait

```rust
pub trait RustyCodeTool: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn input_schema(&self) -> Value;
    fn execute(&self, input: Value, context: &ToolContext) -> Result<ToolResult>;
    fn annotations(&self) -> ToolAnnotations { ... }
    fn tags(&self) -> Vec<ToolTag> { ... }
}
```

### Tool Tags (Compile-Time Safe)

```rust
pub enum ToolTag {
    Explore,    // Read-only discovery
    Implement,  // Write/edit/execute
    Debug,      // Diagnostics, inspection
    Refactor,   // LSP rename/extract
    Ops,        // git, bash, docker
}
```

### Edit Tool Matching Strategies

The edit tool uses 3 fallback strategies for robustness against LLM whitespace variations:

1. **Exact match** — character-for-character
2. **Line-ending normalized** — LF↔CRLF tolerant
3. **Trimmed** — whitespace-insensitive matching

### Tool Activation Tiers

Tools load progressively based on task demands:

| Tier | Tools | Activation |
|------|-------|------------|
| Default | read, edit, write, bash, grep, glob | Always |
| Extended | web_fetch, LSP tools, notebook_edit, todo, memory, git | On demand |
| Full | All registered tools including MCP | When needed |

The `ToolActivationManager` starts at **Extended** tier — LSP and advanced tools are available from session start. Promotion is one-way (never demotes).

---

## Skill System

### Overview

Skills are YAML-frontmatter-driven capabilities discovered from `SKILL.md` files. They provide domain-specific knowledge, tool scoping, and path-based activation.

### Skill File Format

```yaml
---
name: my-skill
description: What this skill does
when-to-use: When to activate it
version: "1.0"
paths:
  - "src/**/*.{ts,tsx}"
  - "*.json"
user-invocable: true
model-invocable: true
allowed-tools:
  - read_file
  - edit_file
  - bash
effort: high
model: sonnet
agent: executor
categories:
  - frontend
excludes:
  - "node_modules/**"
gotchas:
  - "Don't modify generated files"
---

# Skill Instructions

The body content after the second `---` contains the skill's
instructions, which are injected into the LLM context when activated.
```

### Path Activation

Skills activate based on file paths matching glob patterns. The frontmatter parser supports:

- **Brace expansion**: `src/*.{ts,tsx}` → `src/*.ts`, `src/*.tsx`
- **Comma-separated**: `"*.rs, *.toml"` → `*.rs`, `*.toml`
- **Nested braces**: `{src,lib}/**/*.{rs,ts}` → 4 patterns
- **Mixed**: `"src/*.{ts,tsx}, *.json"` → 3 patterns

### Activation Modes

| Mode | Trigger | Description |
|------|---------|-------------|
| Always | No paths | Always active in session |
| Conditional | Has paths | Activated when matching file is touched |
| Manual | user-invocable: false | Only activated by explicit user request |

### Skill Discovery Flow

```
1. Scan skill directories (project-local, user-global)
2. Parse YAML frontmatter from each SKILL.md
3. Normalize paths (brace expansion + comma split)
4. Classify as Always/Conditional/Manual
5. Cache metadata (TTL-based)
6. On file touch → check conditional skills → promote if match
```

---

## Orchestration Engine

The orchestration crate (`rustycode-orchestration`, 76K LOC) is the brain of autonomous execution.

### Module Map

```
rustycode-orchestration
├── autonomous/         # Autonomous execution loop
├── thinking/           # Structured reasoning (DAG thought graph, confidence scoring)
├── ast/                # Adaptive Structured Thinking pipeline
├── conductor/          # Multi-model orchestration
├── delegation/         # Agent delegation and routing
├── bootstrap/          # Task bootstrapping and planning
├── context/            # Context window management
├── cache/              # Prompt/response caching
├── cost_table/         # Cost estimation per model
├── quality/            # Quality detection and gates
├── compaction/         # Context compression
├── autonomy/           # Autonomy level management
├── tool_tiers/         # Progressive tool activation
├── isolation/          # Tool capability classification
├── recovery/           # Checkpoint recovery
├── structured_thinking/ # Step-by-step reasoning with metacognition
├── ask_user_tool/      # LLM-initiated clarification requests
├── stuck_detector/     # Confidence stagnation and repetition detection
└── agent_executor/     # Agent execution with streaming
```

### Structured Thinking

The structured thinking module provides multi-step reasoning:

1. **DAG Thought Graph** — Thoughts as nodes, dependencies as edges
2. **Multi-factor Confidence Scoring** — Not just "confident or not"
3. **Metacognitive Stuck Detection** — Detects when the agent is spinning
4. **Strategy Preemption** — Switches approach with cooldown (3 iterations minimum)

### Quality Gates

Autonomous execution passes through quality gates:

- **Code compiles** — cargo check must pass
- **Tests pass** — cargo test must succeed
- **No clippy warnings** — cargo clippy must be clean
- **LSP diagnostics clear** — Zero errors on affected files

### Context Compression

When the context window fills, the compaction module:

1. Identifies low-value messages (old tool outputs, verbose logs)
2. Summarizes conversation history into compact representation
3. Preserves recent tool calls and user instructions
4. Maintains a "compaction context" for continuity

---

## TUI (Terminal User Interface)

The TUI (`rustycode-tui`, 125K LOC) is a ratatui-based interactive interface.

### Features

- Split-pane layout: conversation + context panel
- Streaming response rendering with syntax highlighting
- Mouse support: text selection, drag scrolling, click navigation
- Image paste pipeline (clipboard → LLM vision)
- Workspace-aware: git branch, project directory display
- Skill activation UI
- Todo/task sync from LLM into workspace tasks
- Deep thinking visualization
- Workspace boundary control during streaming
- tmux-aware input handling

### Event Loop

```
┌─────────────────────────────────────┐
│         Terminal Events              │
│  (keyboard, mouse, resize)          │
└──────────┬──────────────────────────┘
           ↓
┌─────────────────────────────────────┐
│      TUI Event Loop                 │
│  - Route input to focused pane      │
│  - Handle LLM streaming chunks      │
│  - Update workspace state            │
│  - Render ratatui frames             │
└──────────┬──────────────────────────┘
           ↓
┌─────────────────────────────────────┐
│      LLM Provider (streaming)       │
│  - Anthropic / OpenAI / etc.        │
│  - Tool calls → Tool Executor       │
│  - Response chunks → UI render      │
└─────────────────────────────────────┘
```

### Configuration Wizard

First launch runs a configuration wizard:

```bash
rustycode tui --reconfigure   # Force reconfiguration
rustycode tui --workspace .   # Override workspace
rustycode tui --resume         # Resume last session
rustycode tui --model sonnet   # Override model
```

---

## CLI Reference

### Top-Level Commands

```
rustycode [TASK]              # Execute a task directly
rustycode doctor               # Health check
rustycode config               # Configuration management
rustycode context              # Context management
rustycode run [TASK]           # Run a task
rustycode tools                # Tool management
rustycode sessions             # Session management
rustycode events               # Event bus monitoring
rustycode plan                 # Plan mode (create/list/show/approve/reject)
rustycode agent                # Autonomous agent (new/step/reset)
rustycode harness              # Long-running agent with persistence
rustycode omo                  # Multi-agent orchestration
rustycode worktree             # Git worktree management
rustycode provider             # LLM provider management
rustycode history              # Conversation history
rustycode skills               # Skill management
rustycode learnings            # Team learnings/memory
rustycode tui                  # Launch interactive TUI
rustycode web                  # Web interface
rustycode serve                # Web UI + API server
rustycode swebench             # SWE-bench evaluation
rustycode checkpoint           # Repository checkpoint/rewind
rustycode bench                # Benchmark runner
rustycode ast                  # AST pipeline commands
```

### Global Options

```
--yes              Auto-approve all prompts
--color <auto|always|never>   Color output
--format <human|json>         Output format
--model <MODEL>               Override LLM model
--effort <LEVEL>              Effort level (low/medium/high/xhigh/max)
--verbose                     Verbose logging
--debug                       Debug logging
```

---

## Event Bus

The event bus (`rustycode-bus`) provides type-safe, async pub/sub for decoupled crate communication.

### Features

- **Type safety**: Compile-time guarantee publishers/subscribers match
- **Wildcards**: Subscribe to `session.*`, `git.*`, etc.
- **Hooks**: Pre/post processing for logging, metrics, error handling
- **Thread-safe**: All operations safe across tokio tasks

### Event Trait

```rust
pub trait Event: Send + Sync + 'static {
    fn event_type(&self) -> &'static str;
    fn timestamp(&self) -> DateTime<Utc>;
    fn as_any(&self) -> &dyn Any;
    fn clone_box(&self) -> Box<dyn Event>;
    fn serialize(&self) -> serde_json::Value;
}
```

### Subscription Filters

```rust
pub enum SubscriptionFilter {
    Exact(String),         // "session.created"
    Prefix(String),        // "session.*"
    Regex(Regex),          // custom pattern
}
```

---

## Security Model

### Layered Security

```
rustycode-guard
├── Path validation        # Blocks .env, credentials, sensitive files
├── Command validation     # Validates shell commands before execution
├── Filename blocking      # Checks BLOCKED_FILENAMES (id_rsa, tfstate, etc.)
└── Extension blocking     # Blocks .pem, .key, .secret, etc.

rustycode-tools
├── Bash sandbox           # Timeout enforcement, output size limits
├── Eval flag blocking     # Blocks -c/-e flags for ALL interpreters
├── Smart approval         # Auto-classifies gh CLI, curl/wget, jq/yq
└── Permission classifier  # Role-based tool access (read/write/exec)

rustycode-tools-api
├── ToolAnnotations        # read-only/destructive/idempotent/open-world hints
├── ToolPermission         # Allow/block with reason tracking
└── ToolBlockedReason      # Structured rejection explanations
```

### Secret Management

- API keys stored as `secrecy::SecretString` — never logged or displayed
- `sanitize_for_log()` strips API key patterns from all log output
- `.gitleaks.toml` for pre-commit secret scanning
- `.gitignore` blocks `.env`, `credentials.json`, `config.json`

---

## Session & Storage

### Session Lifecycle

```
1. Create session (new or resume)
2. Configure LLM provider + model
3. Load skills (always-active + conditional)
4. Initialize tool activation manager
5. Run conversation loop (user input → LLM → tool calls → response)
6. Persist session state (messages, tool results, costs)
7. Close / checkpoint for recovery
```

### Storage Layer

- SQLite-backed persistence via `rustycode-storage`
- Session state, conversation history, tool results
- Cost tracking per session
- Learning/feedback collection

### Checkpoint Recovery

```bash
rustycode checkpoint <hash>                    # Rewind repo to checkpoint
rustycode checkpoint <hash> --restore <files>  # Restore specific files
```

---

## Benchmarking

### rtk-bench (`rustycode-bench`)

Terminal Bench 2.0 compatible benchmark runner with native and Docker modes.

### Architecture

```
rustycode-bench
├── runner/              # NativeRunner, DockerRunner
├── config.rs            # JSON/TOML config file support
├── retry.rs             # Exception-based retry filtering
├── history.rs           # Result history store + diff
├── agent_registry.rs    # Extensible agent factory pattern
├── composite.rs         # Dataset merging with dedup
├── report.rs            # Pretty/Json/Csv/Markdown formatters
└── dataset/             # Dataset loading and validation
```

### Commands

```bash
rustycode bench run --dataset terminal-bench-v2   # Run TB2 tasks
rustycode bench run --timeout 120                  # Per-task timeout
rustycode bench results                            # Show results
rustycode bench list-datasets                      # Available datasets
```

### Native vs Docker Mode

- **Native**: Runs tasks directly on host (macOS arm64). Fast, no virtualization overhead.
- **Docker**: Full container isolation. Required for tasks with Dockerfile-built dependencies.
- **Static binary**: Musl-linked for fresh container compatibility (no glibc dependency).

---

## Configuration

### Config Locations

```
~/.rustycode/config.toml        # Global configuration
.rustycode/config.toml          # Project-local configuration
RUSTYCODE_* env vars            # Environment overrides
```

### Key Configuration

```toml
[provider]
default = "anthropic"
model = "claude-sonnet-4-6"

[agent]
max_iterations = 3
max_tool_turns = 25
min_tool_calls_to_stop = 5
wall_clock_timeout_secs = 900

[tools]
bash_timeout_secs = 120
max_output_bytes = 1048576

[skills]
cache_ttl_secs = 300
```

---

## Testing

### Test Statistics

- **10,000+ tests** across all crates
- **Zero clippy warnings** (pedantic + nursery enforced)
- **CI enforced**: `cargo clippy -D warnings`, `cargo test`, `cargo fmt --check`

### Test Types

| Type | Location | Framework |
|------|----------|-----------|
| Unit | `#[cfg(test)] mod tests` in source files | `#[test]` |
| Integration | `tests/` directory per crate | `#[test]` + `#[tokio::test]` |
| Property | Scattered | proptest |
| Benchmark | `benches/` | Criterion |

### Test Commands

```bash
cargo test --workspace                    # All tests
cargo test -p rustycode-orchestration     # Single crate
cargo test -p rustycode-core --test integration_test_name  # Specific test
cargo test test_name                      # By name pattern
cargo clippy --workspace -- -D warnings   # Lint check
cargo fmt --check                         # Format check
```

### Testing Conventions

- `#![allow(clippy::unwrap_used)]` in test modules only
- `tempfile::tempdir()` for filesystem test isolation
- Mock provider (`rustycode-llm::mock`) for LLM-free testing
- Event bus subscribers for integration testing without direct coupling

---

## Building & Deployment

### Development Build

```bash
cargo build                          # CLI binary (default member)
cargo build --workspace --all-targets # Everything
```

### Release Build

```bash
cargo build --release                # Optimized CLI binary
./scripts/build-release.sh linux-amd64  # Cross-compile (musl static)
```

### Cross-Compilation

For TB2 benchmarks (x86_64 Linux):

```bash
# Musl target produces fully static binary (no glibc dependency)
cargo zigbuild --release -p rustycode-cli --target x86_64-unknown-linux-musl
```

### Workspace Members

48 crates in workspace (plus `examples/` and `tests/`). Default member: `rustycode-cli`.

Excluded: `crates/rustycode-web/` (WASM/web build), `mcp-test-server/` (test utility).

---

## Project History & Milestones

| Date | Milestone |
|------|-----------|
| 2026-04-10 | All 4 plan features complete (Repo Map, Architect Mode, Parallel Worktrees, SWE-bench) |
| 2026-04-13 | Benchmark crate complete (Harbor-compatible pipeline) |
| 2026-04-15 | Dependency optimization (reqwest 0.12, ~76 deps removed) |
| 2026-04-17 | Edit tool enhancement (flexible matching + diff), musl static binary fix |
| 2026-04-18 | Production readiness review (10,880 tests, zero clippy) |
| 2026-04-20 | Orchestration consolidation, structured thinking module, P0 architecture fixes |
| 2026-04-23 | Tool definition improvements (grep types, LSP tools, eval security fix) |
| 2026-04-24 | Security hardening (filename blocking, notebook_edit, strategy preemption) |
| 2026-04-25 | rtk-bench Harbor replacement (157 tests, native + Docker runners) |
| 2026-04-26 | AST production wiring (composable structured thinking tool) |
| 2026-04-28 | Structured thinking unification, AskUser tool, stuck detection |
| 2026-05-05 | Comprehensive documentation, frontmatter fix, skill integration tests |
| 2026-05-06 | Dead code cleanup, runtime module wiring, ratzilla-wasm removal, 48 crate workspace |

---

## License

MIT License. See [LICENSE](../LICENSE) for details.

## Repository

https://github.com/luengnat/rustycode
