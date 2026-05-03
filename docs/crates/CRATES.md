# RustyCode Crate Catalog

Comprehensive reference for all major crates in the RustyCode workspace, organized by architectural layers with public APIs, dependencies, and cross-crate usage patterns.

---

## Layer 1: Binaries & Main Entry Points (3 crates)

### rustycode-tui
**Purpose:** Interactive terminal UI with streaming responses, syntax highlighting, tool visualization  
**Key Interfaces:** Message display, input area, tool panel, status bar, memory browser  
**Entry Point:** `cargo run -p rustycode-cli -- tui`  
**Features:** Streaming responses, code highlighting, memory management, keyboard navigation  
**Dependencies:** ratatui, tokio, rustycode-core
**Use:** Interactive development with Claude  
**Note:** The TUI is a subcommand of `rustycode-cli`, not a separate binary.  
**See Also:** [README](../../crates/rustycode-tui/README.md)

### rustycode-cli
**Purpose:** Command-line interface for automated workflows, CI/CD, batch processing  
**Key Commands:** plan, auto, eval, search, refactor, test, debug  
**Entry Point:** `cargo run -p rustycode-cli -- "task"`  
**Features:** Streaming output, JSON mode, token limits, session loading  
**Dependencies:** clap, rustycode-core, rustycode-session, rustycode-config, tokio  
**Use:** Scripts, CI/CD pipelines, batch task execution  
**See Also:** [README](../../crates/rustycode-cli/README.md)

### rustycode-orchestration
**Purpose:** Structured reasoning and execution strategy for complex task solving  
**Key Concepts:** Phases, quality gates, iterative refinement, canonical thinking  
**Execution Flow:** Analyze → Plan → Implement → Test → Verify → Iterate  
**Features:** Risk awareness, context management, quality enforcement  
**Dependencies:** rustycode-llm, rustycode-tools, rustycode-prompt, rustycode-runtime  
**Use:** Production reasoning and orchestration for the shell/TUI stack  
**See Also:** [README](../../crates/rustycode-orchestration/README.md)

---

## Layer 2: Core Infrastructure (10 crates)

### rustycode-protocol
**Purpose:** Core types and protocol definitions for all cross-crate communication  
**Key Types:** SessionId, PlanId, Message, ToolCall, ToolResult, CompletionRequest  
**ID System:** Time-sortable, human-readable, collision-free identifiers  
**Principles:** No circular deps, immutable types, comprehensive serialization  
**Dependencies:** serde, chrono, uuid, sha2, base64  
**Used By:** Every crate  
**See Also:** [README](../../crates/rustycode-protocol/README.md)

### rustycode-core
**Purpose:** Session management and headless execution engine  
**Key Types:** Session, SessionConfig, SessionManager, HeadlessExecutor, Checkpoint  
**Lifecycle:** Create → Add Message → Process → Checkpoint → Complete  
**Modes:** TUI, CLI, Headless, ACP  
**Dependencies:** rustycode-protocol, rustycode-llm, rustycode-tools, rustycode-storage, rustycode-memory, tokio  
**Used By:** All frontends  
**See Also:** [README](../../crates/rustycode-core/README.md)

### rustycode-llm
**Purpose:** LLM provider abstraction and implementations  
**Providers:** Anthropic (Claude), OpenAI (GPT), Google Gemini, OpenRouter, Ollama, Kimi, Qwen, Vertex AI  
**Key Types:** LLMProvider, CompletionRequest, CompletionResponse, StreamingChunk  
**Features:** Streaming, vision, tool use, cost tracking, caching, fallback chains  
**Dependencies:** reqwest, tokio, serde_json, rustycode-protocol, rustycode-providers, rustycode-auth  
**Used By:** rustycode-core, rustycode-agents, all executors  
**See Also:** [README](../../crates/rustycode-llm/README.md)

### rustycode-tools-api
**Purpose:** Tool trait definitions (decoupled from implementation)  
**Key Traits:** Tool, ToolExecutor, ToolRegistry, ToolSelector  
**Types:** ToolProfile, ToolParameter, ToolError, ToolResult, Permission  
**Design:** Prevents circular dependencies, enables custom implementations  
**Dependencies:** serde, anyhow  
**Used By:** Consumers of tools (agents, orchestrators)  
**See Also:** [README](../../crates/rustycode-tools-api/README.md)

### rustycode-tools
**Purpose:** Tool execution engine and built-in tool implementations  
**Built-in Tools:** bash, git, file_read, file_write, ls, grep, find  
**Key Types:** ToolExecutor, BashTool, GitTool  
**Security:** Integrated with rustycode-guard for validation  
**Dependencies:** rustycode-tools-api, rustycode-protocol, rustycode-guard, rustycode-observability, tokio  
**Used By:** Core session, ACP server, orchestrators  
**See Also:** [README](../../crates/rustycode-tools/README.md)

### rustycode-bus
**Purpose:** Event bus for inter-module pub/sub communication  
**Key Types:** EventBus, EventChannel, EventHandler, EventListener  
**Event Types:** SessionEvent, MessageEvent, ToolEvent, LLMEvent, StorageEvent, ContextEvent  
**Features:** Priority-based execution, fire-and-forget publishing, async handlers  
**Dependencies:** tokio, parking_lot, serde, anyhow  
**Used By:** All modules for loose coupling  
**See Also:** [README](../../crates/rustycode-bus/README.md)

### rustycode-storage
**Purpose:** Session persistence with SQLite  
**Key Types:** SessionStore, StorageConfig, Session record, Message record  
**Tables:** sessions, messages, checkpoints, tool_calls, api_calls  
**Features:** ACID transactions, connection pooling, migrations, backup/restore  
**Dependencies:** sqlx, tokio, rustycode-protocol, anyhow  
**Used By:** rustycode-core, rustycode-session  
**See Also:** [README](../../crates/rustycode-storage/README.md)

### rustycode-config
**Purpose:** Configuration loading from files, env, CLI with validation  
**Sources:** CLI flags (highest) → env vars → config files → defaults (lowest)  
**Key Types:** Config, ConfigBuilder, ConfigSource, ValidationError  
**Support:** TOML config files, environment variables, API key secrets  
**Dependencies:** toml, serde, validator, anyhow  
**Used By:** All crates on startup  
**See Also:** [README](../../crates/rustycode-config/README.md)

### rustycode-git
**Purpose:** Git operations and worktree management  
**Key Types:** GitRepository, WorktreeManager, Worktree, GitStatus, CommitMessage  
**Operations:** status, commit, push, pull, branch, merge, rebase, log, blame, diff  
**Worktrees:** Parallel branch work in isolated directories  
**Dependencies:** git2, tempfile, anyhow  
**Used By:** tool execution, session recovery, autonomous workflows  
**See Also:** [README](../../crates/rustycode-git/README.md)

### rustycode-session
**Purpose:** Session lifecycle management (create, load, resume, archive)  
**Key Types:** SessionManager, SessionBuilder, SessionMode, SessionMetadata, SessionRecovery  
**Lifecycle Phases:** Create → Init → Run → Checkpoint → Complete → Archive  
**Features:** Multi-mode support, recovery from crashes, metadata tracking  
**Dependencies:** rustycode-storage, rustycode-config, rustycode-core, rustycode-protocol, tokio  
**Used By:** All frontends, orchestrators  
**See Also:** [README](../../crates/rustycode-session/README.md)

---

## Layer 3: Agent & Execution (7 crates)

### rustycode-agents
**Purpose:** Agent implementations for specialized development tasks  
**Agent Types:** CodeAgent, ReviewAgent, TestAgent, DebugAgent  
**Key Types:** Agent trait, AgentConfig, AgentResult  
**Features:** TDD patterns, iterative refinement, structured output  
**Dependencies:** rustycode-llm, rustycode-tools, rustycode-protocol, tokio, anyhow  
**Used By:** rustycode-core, rustycode-orchestration, orchestrators  
**See Also:** [README](../../crates/rustycode-agents/README.md)

### rustycode-execution
**Purpose:** Plan execution engine with orchestration  
**Key Types:** Executor, ExecutionConfig, ExecutionResult, PlanExecutor, StepExecutor  
**Features:** Step-by-step execution, result collection, error recovery  
**Dependencies:** anyhow, tokio, rustycode-protocol, rustycode-session, rustycode-observability  
**Used By:** CLI, orchestrators, automation  
**See Also:** [README](../../crates/rustycode-execution/README.md)

### rustycode-skill
**Purpose:** Skill discovery and workflow management  
**Skill Types:** workflow, tool-bundle, pattern, guide, custom  
**Key Types:** Skill, SkillManager, SkillRegistry, SkillWorkflow, SkillActivation  
**Features:** Metadata-only loading, TTL caching, relevance scoring, YAML frontmatter parsing  
**Dependencies:** tokio, serde, regex, tracing  
**Used By:** Workflow enforcement, agent execution  
**See Also:** [README](../../crates/rustycode-skill/README.md)

### rustycode-runtime
**Purpose:** Async runtime utilities and task coordination  
**Key Types:** RuntimeConfig, TaskPool, TaskHandle, CancellationToken  
**Features:** Multi-threaded runtime, I/O optimization, bounded execution, metrics  
**Dependencies:** tokio, num_cpus, parking_lot, anyhow  
**Used By:** Core async infrastructure  
**See Also:** [README](../../crates/rustycode-runtime/README.md)

### rustycode-shared-runtime
**Purpose:** Global tokio runtime for preventing allocator/TLS growth  
**Key API:** SHARED_RUNTIME (LazyLock), spawn_on_shared(), block_on_shared()  
**Usage:** All async components that need efficient runtime usage  
**Dependencies:** tokio, num_cpus  
**Used By:** All async components  
**See Also:** [README](../../crates/rustycode-shared-runtime/README.md)

### rustycode-bench
**Purpose:** Harbor-compatible benchmark runner framework  
**Key Types:** BenchEnvironment, BenchAgent, OracleAgent, CodeAgent, Trial, Job  
**Features:** Containerized execution, multiple agent types, dataset registry  
**Dependencies:** tokio, serde, rustycode-protocol, docker API  
**Used By:** Performance measurement, Harbor integration  
**See Also:** [README](../../crates/rustycode-bench/README.md)

---

## Layer 4: UI Components (1 crate)

### rustycode-ui-core
**Purpose:** Shared UI types for web and terminal frontends  
**Key Types:** FrontendMessage, FrontendMessageKind, FrontendSession, SubmittedInput, RunController  
**Input Parsing:** /command → SlashCommand, !command → BangCommand, text → ChatMessage  
**Dependencies:** serde, rustycode-protocol  
**Used By:** All frontends  
**See Also:** [README](../../crates/rustycode-ui-core/README.md)

---

## Layer 5: Observability & Context (4 crates)

### rustycode-observability
**Purpose:** Metrics, tracing, and logging infrastructure  
**Key Types:** SessionMetrics, Counter, Gauge, Histogram, MetricsStore, ExecutionContext, LogContext  
**Metrics:** Requests, tokens, costs, LLM usage, tool invocations, resources  
**Logging Levels:** Trace, Debug, Info, Warn, Error  
**Dependencies:** tracing, tracing-subscriber, serde, tokio, parking_lot  
**Used By:** All instrumented components  
**See Also:** [README](../../crates/rustycode-observability/README.md)

### rustycode-memory
**Purpose:** Short-term context and memory management  
**Key Types:** MemoryEntry, MemoryDomain, MemoryScope, MemorySource, Observation  
**Domains:** CodeStyle, Testing, Git, Debugging, Workflow, Architecture, ProjectSpecific  
**Features:** Confidence scoring (0.3–0.9), relevance ranking, evidence tracking  
**Dependencies:** serde, anyhow, tracing  
**Used By:** TUI, core session  
**See Also:** [README](../../crates/rustycode-memory/README.md)

### rustycode-lsp
**Purpose:** Language Server Protocol client integration  
**Key Types:** LspClient, LspClientConfig, ProjectDetector, LanguageId  
**Operations:** Hover, goto definition, completions, diagnostics, symbol search, references, rename  
**Providers:** rust-analyzer, pyright, typescript-language-server, gopls, clangd  
**Dependencies:** lsp-types, tokio, serde_json, anyhow, tracing  
**Used By:** Code generation, refactoring, analysis  
**See Also:** [README](../../crates/rustycode-lsp/README.md)

### rustycode-prompt
**Purpose:** Handlebars-based prompt templating  
**Key Types:** TemplateManager, PromptBuilder, PromptLayer, EnvironmentContext  
**Built-in Templates:** system/coding_assistant, system/code_review, system/debug, user/* variants  
**Features:** Inline rendering, context macros, smart defaults  
**Dependencies:** handlebars, serde, tokio::fs  
**Used By:** LLM integration, agent execution  
**See Also:** [README](../../crates/rustycode-prompt/README.md)

---

## Layer 6: Security & Provider Integration (4 crates)

### rustycode-guard
**Purpose:** Hook-based security and operation gating  
**Security Rules:** 15+ built-in rules (R01-R15) covering sudo, protected paths, rm -rf, force push, secrets, path traversal  
**Key Types:** HookInput, HookResult, ToolGate  
**Hooks:** Pre-tool validation, post-tool checks, permission gating  
**Dependencies:** regex, serde, anyhow  
**Used By:** Tool execution pipeline  
**See Also:** [README](../../crates/rustycode-guard/README.md)

### rustycode-auth
**Purpose:** OAuth 2.0 and API key authentication  
**Auth Methods:** API key, OAuth code flow, OAuth implicit, GitHub Copilot device code  
**Key Types:** AuthType, OAuthClient, TokenStore, StoredToken  
**Features:** SecretString wrappers, PKCE support, CSRF protection, automatic refresh  
**Dependencies:** secrecy, serde, tokio, reqwest, anyhow  
**Used By:** LLM initialization, service authentication  
**See Also:** [README](../../crates/rustycode-auth/README.md)

### rustycode-providers
**Purpose:** LLM provider registry with metadata and pricing  
**Key Types:** ModelRegistry, ProviderMetadata, ModelInfo, PricingInfo, CostTracker  
**Providers:** Anthropic, OpenAI, Gemini, OpenRouter, Ollama, Kimi, Qwen, Vertex AI  
**Auto-discovery:** Environment variables (ANTHROPIC_API_KEY, OPENAI_API_KEY, etc.)  
**Dependencies:** serde, anyhow, regex  
**Used By:** rustycode-llm, rustycode-observability  
**See Also:** [README](../../crates/rustycode-providers/README.md)

### rustycode-macros
**Purpose:** Procedural macros for tool definition and generation  
**Macros:** #[tool], #[derive(ToolDescription)]  
**Features:** Doc extraction, external documentation, name conversion, signature generation  
**Dependencies:** proc_macro, syn, quote  
**Used By:** Tool implementations  
**See Also:** [README](../../crates/rustycode-macros/README.md)

---

## Layer 7: Tool Registry & Discovery (4 crates)

### rustycode-tools-registry
**Purpose:** Tool registry and discovery system  
**Key Types:** ToolRegistry, ToolMetadata, ToolDiscovery, MetadataProvider, RegistryConfig  
**Discovery Sources:** Built-in tools, plugins, skill frontmatter, custom providers  
**Features:** Metadata caching, category filtering, O(1) lookup  
**Dependencies:** rustycode-tools-api, rustycode-skill, serde, anyhow  
**Used By:** Agent execution, tool selection, CLI  
**See Also:** [README](../../crates/rustycode-tools-registry/README.md)

### rustycode-tool-server
**Purpose:** Standalone HTTP/WebSocket server for remote tool execution  
**API:** POST /call (HTTP), /ws (WebSocket), GET /cache/:call_id, / (Web UI)  
**Features:** Multi-threaded execution, result caching, bidirectional WebSocket  
**Use Cases:** Remote IDE integration, CI/CD, distributed execution  
**Dependencies:** axum, tokio, rustycode-tools, rustycode-protocol  
**See Also:** [README](../../crates/rustycode-tool-server/README.md)

### rustycode-task-integration (not yet documented)
**Purpose:** Bridge between tool registry and execution  
**See Also:** [README](../../crates/rustycode-tools/README.md)

### rustycode-tasks
**Purpose:** Task management and lifecycle (skeleton)  
**Intended Types:** Task, TaskManager, TaskFilter, TaskDependency, TaskStatus  
**Status:** Skeleton with architecture documented  
**Dependencies (planned):** tokio, serde, sqlx, anyhow  
**See Also:** [README](../../crates/rustycode-tasks/README.md)

---

## Layer 8: Protocol & Integration (2 crates)

### rustycode-acp
**Purpose:** Agent Client Protocol server for IDE integration  
**Protocol Support:** initialize ✅, session/new ✅, session/load ✅, session/prompt ✅, streaming 🔄  
**Clients:** Zed, VS Code, other IDE extensions  
**Communication:** JSON-RPC 2.0 over stdin/stdout  
**Dependencies:** serde_json, tokio, rustycode-core, rustycode-llm, rustycode-tools  
**Spec:** https://agentclientprotocol.com/  
**See Also:** [README](../../crates/rustycode-acp/README.md)

### rustycode-mcp
**Purpose:** Model Context Protocol server for Claude integration  
**Resources:** Code (file://, codebase://, search://), Context (memory://, history://, plan://)  
**Tools:** Exposes all RustyCode tools via MCP  
**Use Cases:** Claude + RustyCode integration, IDE extensions, multi-tool workflows  
**Dependencies:** tokio, serde_json, rustycode-core, rustycode-lsp, rustycode-tools  
**Protocol:** https://modelcontextprotocol.io/  
**See Also:** [README](../../crates/rustycode-mcp/README.md)

---

## Cross-Crate Dependency Overview

```
┌─────────────────────────────────────────────────────┐
│ Layer 1: Binaries                                   │
│  - rustycode-tui, rustycode-cli, rustycode-orchestration│
├─────────────────────────────────────────────────────┤
│ Layer 2: Core Infrastructure (protocol, core, llm) │
│  - rustycode-protocol, rustycode-core, rustycode-llm
│  - rustycode-tools-api, rustycode-tools            │
│  - rustycode-bus, rustycode-storage, rustycode-config
│  - rustycode-git, rustycode-session                │
├─────────────────────────────────────────────────────┤
│ Layer 3: Execution & Agents                         │
│  - rustycode-agents, rustycode-execution           │
│  - rustycode-skill                                 │
│  - rustycode-runtime, rustycode-shared-runtime     │
│  - rustycode-bench                                 │
├─────────────────────────────────────────────────────┤
│ Layer 4: UI Components                              │
│  - rustycode-ui-core                               │
├─────────────────────────────────────────────────────┤
│ Layer 5: Observability & Context                    │
│  - rustycode-observability, rustycode-memory       │
│  - rustycode-lsp, rustycode-prompt                 │
├─────────────────────────────────────────────────────┤
│ Layer 6: Security & Auth                            │
│  - rustycode-guard, rustycode-auth                 │
│  - rustycode-providers, rustycode-macros           │
├─────────────────────────────────────────────────────┤
│ Layer 7: Tool Discovery & Registry                  │
│  - rustycode-tools-registry, rustycode-tool-server │
│  - rustycode-tasks                                 │
├─────────────────────────────────────────────────────┤
│ Layer 8: Protocol Integration                       │
│  - rustycode-acp, rustycode-mcp                    │
└─────────────────────────────────────────────────────┘
```

---

## Common Integration Patterns

### Tool Execution Pipeline
1. `rustycode-tools-registry` — Discover available tools
2. `rustycode-guard` — Validate tool safety (hooks)
3. `rustycode-tools` — Execute tool implementation
4. `rustycode-observability` — Track execution metrics

### LLM Provider Integration
1. `rustycode-providers` — Discover and configure providers
2. `rustycode-auth` — Obtain/refresh authentication tokens
3. `rustycode-llm` — Call provider API
4. `rustycode-prompt` — Template prompts
5. `rustycode-observability` — Track tokens and costs

### Agent Execution
1. `rustycode-agents` — Create agent instance
2. `rustycode-llm` — Call LLM for decisions
3. `rustycode-tools-registry` + `rustycode-tools` — Execute tools
4. `rustycode-memory` — Track decisions and patterns
5. `rustycode-observability` — Collect metrics

### Session Lifecycle
1. `rustycode-config` — Load configuration
2. `rustycode-session` — Create session
3. `rustycode-core` — Main execution loop
4. `rustycode-storage` — Persist session
5. `rustycode-observability` — Track costs/metrics

---

## Crate Maturity & Status

**Stable (production-ready):**
- Core: protocol, core, config, storage, session, execution
- Providers: Anthropic, OpenAI, Google Gemini
- Tools: bash, git, file operations
- Observability: metrics, logging, tracing

**Beta (feature-complete, ongoing refinement):**
- Agents: CodeAgent, ReviewAgent, TestAgent, DebugAgent
- UI: TUI components, event loop
- Plugins: plugin loading and discovery
- Skills: workflow system
- LLM: all providers, streaming, tool use

**Active Development:**
- Orchestra: autonomous framework
- MCP/ACP: protocol implementations
- Tools: expanding tool ecosystem

**Skeleton (intended but incomplete):**
- Tasks: task management (needs implementation)

---

## Total Documentation Coverage

**Documented Crates:** 35 of ~50 total workspace crates  
**Completion:** 70% of major crates documented

**Not yet documented (future):**
- rustycode-vector-memory — Vector-based semantic memory
- rustycode-learning — Conversation learning and extraction
- ~~rustycode-deep-thinker~~ — Deleted; thinking module lives in `rustycode-orchestration`
- ~~rustycode-load~~ — Archived to `_archived/`
- ~~rustycode-plugins~~ — Archived to `_archived/`
- ~~rustycode-tui-agents~~ — Archived to `_archived/`
- ~~rustycode-tui-core~~ — Archived to `_archived/`
- ~~rustycode-tui-memory~~ — Archived to `_archived/`
- ~~rustycode-tui-widgets~~ — Archived to `_archived/`
- ~~rustycode-web-native~~ — Archived to `_archived/`
- rustycode-id — ID system internals
- rustycode-thread-guard — Thread safety utilities
- rustycode-tool-integration — Tool-LLM integration layer
- rustycode-connector — Terminal connector abstraction
- rustycode-web — Web UI (WASM)
- And others...

---

**Last Updated:** 2026-04-22  
**Total Documented Crates:** 35  
**Layers:** 8  
**Major Components:** 40+
