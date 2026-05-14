# Architecture Overview

<!-- Generated: 2026-05-14 | Files scanned: 1601 | Token estimate: ~600 -->

## Stats

57 crates, 1601 .rs files, ~614K LOC

## Layer Diagram

```
┌──────────────────────────────────────────────────────────────────┐
│  Binaries                                                        │
│  rustycode-cli (CLI+TUI) │ rustycode-guard (sandbox binary)      │
├──────────────────────────────────────────────────────────────────┤
│  Application Layer                                               │
│  rustycode-core (session, headless, recovery, 26K LOC)           │
│  rustycode-runtime (execution engine, 44K LOC)                   │
│  rustycode-tui (terminal UI, 110K LOC)                           │
│  rustycode-orchestration (autonomous strategies, 72K LOC)        │
├──────────────────────────────────────────────────────────────────┤
│  Agent & Team Layer                                              │
│  rustycode-agent-runtime (headless agent)                        │
│  rustycode-team (multi-agent coordination, 15K LOC)              │
│  rustycode-bench (benchmark runner, 12K LOC)                     │
│  rustycode-integration (CI/CD integration)                       │
├──────────────────────────────────────────────────────────────────┤
│  Service Layer                                                   │
│  rustycode-llm (providers, 42K LOC) │ rustycode-tools (69K LOC)  │
│  rustycode-bus (event pub/sub)      │ rustycode-storage (SQLite) │
│  rustycode-session                  │ rustycode-skill (YAML)      │
│  rustycode-mcp (browser/MCP)        │ rustycode-prompt (templates)│
│  rustycode-memory                   │ rustycode-search            │
├──────────────────────────────────────────────────────────────────┤
│  Foundation Layer                                                │
│  rustycode-protocol (shared types) │ rustycode-config            │
│  rustycode-tools-api (traits)      │ rustycode-tool-integration  │
│  rustycode-tools-security          │ rustycode-tools-registry    │
│  rustycode-sandbox (Seatbelt/landlock) │ rustycode-auth           │
│  rustycode-id │ rustycode-executable │ rustycode-tasks            │
├──────────────────────────────────────────────────────────────────┤
│  Server/Web (excluded from default workspace)                    │
│  rustycode-server │ rustycode-ws-server │ rustycode-server-client│
│  rustycode-server-protocol │ rustycode-web (WASM)                │
└──────────────────────────────────────────────────────────────────┘
```

## Dependency Flow (top-down)

```
cli → core, tui, runtime, orchestration, bus, llm, tools, ...
tui → core, orchestration, bus, llm, tools, session, memory, ...
core → orchestration, bus, llm, tools, storage, git, lsp, ...
orchestration → llm, tools, tools-api, tools-security, skill, storage
llm → tools, tools-api, tool-integration, protocol, config, auth
tools → tools-api, tools-security, tool-integration, bus, config, lsp
```

**Circular dependency breaker:** `rustycode-tool-integration` shim crate
provides `ToolExecutorApi`, `ToolInfo`, `TokenCounter`, `CostTracker` —
both `llm` and `tools` depend on it instead of each other.

## Key Traits

| Trait | Crate | Purpose |
|-------|-------|---------|
| `LLMProvider` | rustycode-llm | Unified LLM provider interface |
| `RustyCodeTool` | rustycode-tools-api | Tool execution contract |
| `Tool` | rustycode-tools-api | Low-level tool dispatch |
| `ToolRouter` | rustycode-tools-api | Tool routing by name |
| `Event` | rustycode-bus | Event bus subscription |
| `Hook` | rustycode-bus | Lifecycle hooks |
| `RunController` | rustycode-ui-model | TUI ↔ engine control |
| `Transport` | rustycode-llm | HTTP/WS transport layer |
| `AuthMethod` | rustycode-llm | API key auth strategies |

## Inter-Crate Communication

- **Types:** `rustycode-protocol` (Milestone, Plan, Message, ToolCall, CodeSymbol, ...)
- **Events:** `rustycode-bus::EventBus` (pub/sub with filters)
- **Tools:** `rustycode-tools-api` trait → `rustycode-tools` implementation
- **LLM:** `rustycode-llm::LLMProvider` trait → per-provider impls
- **Config:** `rustycode-config` (TOML/JSON, schema validation)
- **Skills:** `rustycode-skill` (YAML frontmatter discovery)
