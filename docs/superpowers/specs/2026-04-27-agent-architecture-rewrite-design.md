# Agent Architecture Rewrite

**Date**: 2026-04-27
**Status**: Implemented (2026-04-27)
**Owner**: RustyCode Core Team
**Version**: 2.0
**Supersedes**: `~/.claude/plans/sequential-coalescing-pie.md` (draft), v1 of this doc

---

## Executive Summary

RustyCode has **three independent LLM↔tool loops** (headless 4,672 lines · TUI ~3,850 lines · bench 7,089 lines) totaling ~15,600 lines of agent code. The codebase also has **rich code intelligence infrastructure** (tree-sitter, code index, semantic search, full LSP client) that is **completely disconnected** from the agent loops.

This design:
1. Replaces all three loops with a single `AgentSession` in a new `rustycode-agent` crate
2. Integrates code intelligence as the agent's "brain" — structural understanding that makes the LLM smarter, replacing heuristic hacks
3. Deletes ~10,000+ lines of behavioral heuristics
4. Trusts the model with a ≤40-line system prompt

**Core principle**: Inform, don't control. The model is smart. The loop is thin. Code intelligence replaces babysitting.

---

## 1. The Right Mental Model

Four layers, cleanly separated:

```
┌───────────────────────────────────────────────────┐
│                  Interface                         │
│   TUI · CLI · Bench · API                          │
│   renders events, injects user input               │
└──────────────────┬────────────────────────────────┘
                   │ AgentEvents stream
┌──────────────────▼────────────────────────────────┐
│                   Agent                            │
│   LLM ↔ tool loop                                 │
│   no heuristics · no nudges · no tracking state    │
│                                                    │
│   Informed by: CodeIntelligence service            │
│   "auth.rs has 12 dependents" — not "verify now!"  │
└──────────┬───────────────────────┬────────────────┘
           │ tool calls            │ queries
┌──────────▼──────────┐  ┌────────▼──────────────────┐
│      Runtime        │  │   CodeIntelligence          │
│   resolves tools    │  │   tree-sitter (RepoMap)     │
│   manages context   │  │   code index (trigram/sym)  │
│   MCP · skills      │  │   semantic search (BGE)     │
│   cost tracking     │  │   LSP (def/refs/diagnostics)│
└─────────────────────┘  └────────────────────────────┘
```

**Agent** — the protocol. Receives task + context + tools. Produces event stream + result. Drives behavior through reasoning, informed by structural code understanding. No knowledge of TUI, CLI, or tool implementation.

**CodeIntelligence** — the brain. Existing infrastructure repurposed as a live service the agent queries. Not injected prompts — on-demand structural analysis. "What depends on auth.rs?" is a query, not a rule.

**Runtime** — the environment. Resolves tool calls (built-ins, MCP, skills — all identical to the agent). Manages context budget, cost tracking. Agent never knows tool source.

**Interface** — pure display. Implements `AgentEvents` to render. TUI renders + injects user input. CLI prints. Bench collects metrics. All observe the same event stream from the same agent.

---

## 2. What's Wrong Today — Complete Inventory

### 2.1 Headless Loop (4,672 lines)

**File**: `crates/rustycode-core/src/headless/mod.rs`

| Responsibility | Lines | % |
|---|---|---|
| Core LLM↔tool loop (stream, execute, message construction) | ~650 | 14% |
| Heuristic/behavioral injection (10 stop-prevention guards, urgency nudges, stagnation, loop detection) | ~1,330 | 28% |
| Tool result post-processing (hints, error enrichment, truncation) | ~650 | 14% |
| State variable initialization (~25 mutable tracking vars) | ~270 | 6% |
| Text cleanup (repetition detection, prefix stripping) | ~260 | 6% |
| System prompts + prompt construction | ~150 | 3% |
| `is_modifying`/`is_code_write` closures | ~165 | 4% |
| Streaming callbacks | ~90 | 2% |
| Checkpoint/recovery | ~130 | 3% |
| Constants/config | ~35 | 1% |
| Tests | ~935 | 20% |

Plus `hints.rs` (894 lines) and `utils.rs` (581 lines).

**Key types exported**: `HeadlessTaskResult`, `run_headless_task()`, `run_headless_task_with_iteration()`, `dispatch_agent_action()`, `summarize_tool_args()`.

**Consumers**: CLI (`main.rs:888`, `main.rs:1133`), harness (`harness_cmd.rs:528`), runtime (`runtime::AsyncRuntime::run_headless*`).

### 2.2 TUI Loop (~3,850 lines)

**Files**: `crates/rustycode-tui/src/app/streaming/response.rs` (1,638) + `tool_execution.rs` (855) + `handlers.rs` (~1,000 loop portion)

**Zero shared code with headless.** Completely independent:
- Custom SSE parsing (does NOT use shared `SseEventProcessor`)
- Own tool name normalization, message pruning, stall detection
- Different constants: 50 turns (vs headless's 25), 25KB tool results (vs 8KB)
- Own system prompt construction (~100 lines with workspace context)

**UI-specific responsibilities** (not candidates for shared core):
- Streaming markdown rendering (safe boundary detection)
- Tool approval flow (risk → channel → user decision)
- Interactive questions (question + options → channel)
- Undo snapshots (before file writes)
- Thinking block rendering
- Auto-scroll management
- File change detection between turns
- Tab title updates

### 2.3 Bench Loop (7,089 lines)

**File**: `crates/rustycode-bench/src/agent/code_agent.rs`

| Responsibility | Lines | % |
|---|---|---|
| Config + constructors + 175-line system prompt | ~287 | 4% |
| Tool schemas (6 hand-crafted JSON) | ~168 | 2% |
| Response parsing (4 formats — doesn't use structured tool_use API) | ~179 | 3% |
| Path/traceback/output helper heuristics | ~353 | 5% |
| **Custom tool execution (all via shell commands)** | **~1,068** | **15%** |
| Error enrichment (40+ patterns) | ~205 | 3% |
| Context management (workspace discovery, pruning) | ~733 | 10% |
| **Task analysis heuristics** | **~633** | **9%** |
| **Main loop with inline heuristics** | **~2,715** | **38%** |
| Tests | ~597 | 8% |

**Key difference**: Bench uses `BenchEnvironment::exec()` (shell-based container execution), NOT `ToolRegistry`. Only ~624 lines (9%) is generic LLM loop; the remaining 83% is bench-specific.

### 2.4 Disconnected Code Intelligence

The codebase has rich code analysis infrastructure that **no agent loop uses**:

| Infrastructure | Location | Capability |
|---|---|---|
| **Tree-sitter (RepoMap)** | `tools/indexing/repo_map.rs` | Structural summaries: functions, structs, traits, imports for 5 languages + regex fallback |
| **Code Index** | `tools/indexing/code_index.rs` | Trigram O(1) search, symbol index, dependency graph (1,358 lines) |
| **Semantic Search** | `tools/indexing/semantic_search.rs` | BGE-Small embeddings, intent-based queries (1,608 lines) |
| **LSP Client** | `lsp/src/client.rs` | Full async LSP: hover, goto-def, references, completion, diagnostics, rename, code actions (8 language servers) |
| **Context Pipeline** | `orchestration/context/` | 8 sub-modules: budget, chunking, cache optimization, compression, distillation |

The orchestration crate's "AST pipeline" (`orchestration/ast/`) is actually "Adaptive Structured Thinking" — a 6-phase task planner that does **no actual code parsing**. Its research phase walks directories and matches filenames. None of the real code intelligence feeds into it.

---

## 3. Target Architecture

### 3.1 New Crate: `rustycode-agent`

**Dependency graph after rewrite**:

```
rustycode-cli ────→ rustycode-agent ──→ rustycode-llm
rustycode-tui ────→ rustycode-agent ──→ rustycode-tools
rustycode-bench ──→ rustycode-agent ──→ rustycode-protocol
                                      ──→ rustycode-lsp (for CodeIntelligence)

orchestration ──→ rustycode-agent (for tiered execution)
agent ──→ orchestration (for context pipeline)
```

**Why new crate, not in core**: `rustycode-core` is 40K+ LOC (god object). Adding more goes wrong direction. Clean crate with one job.

### 3.2 AgentSession — the thin loop

```rust
// crates/rustycode-agent/src/lib.rs

pub struct AgentConfig {
    pub max_turns: usize,       // hard limit (default: 25)
    pub timeout_secs: u64,      // wall clock (default: 900)
    pub context_budget: usize,  // tokens before pruning
}

pub struct AgentResult {
    pub output: String,
    pub turns: usize,
    pub tool_calls: usize,
    pub cost_usd: f64,
    pub stopped_reason: StoppedReason,
}

pub enum StoppedReason {
    NoToolCalls,              // model stopped → task done
    MaxTurnsReached,
    TimeoutExceeded,
    ContextBudgetExceeded,
}

/// The only interface between agent and display layer.
pub trait AgentEvents: Send {
    // Streaming
    fn on_text_delta(&mut self, delta: &str);
    fn on_thinking_delta(&mut self, delta: &str);

    // Tool lifecycle
    fn on_tool_call(&mut self, id: &str, name: &str, input: &serde_json::Value);
    fn on_tool_result(&mut self, id: &str, name: &str, output: &str, is_error: bool);

    // User interaction — interface injects decisions
    fn on_approval_needed(&mut self, tool_name: &str, input: &serde_json::Value) -> ApprovalDecision;
    fn on_question(&mut self, question: &str, options: &[String]) -> Option<String>;

    // Lifecycle
    fn on_turn_start(&mut self, turn: usize);
    fn on_done(&mut self, result: &AgentResult);
}

pub enum ApprovalDecision {
    Approve,
    Reject(String),
    AutoApproved,
}

pub struct AgentSession { config: AgentConfig }

impl AgentSession {
    pub fn new(config: AgentConfig) -> Self;

    pub async fn run(
        &self,
        provider: &dyn LLMProvider,
        system: &str,
        messages: Vec<Message>,
        tools: &ToolRegistry,
        intelligence: &dyn CodeIntelligence,  // ← the brain
        events: &mut dyn AgentEvents,
    ) -> Result<AgentResult>;
}
```

### 3.3 CodeIntelligence — the brain

```rust
// crates/rustycode-agent/src/intelligence.rs

/// Structural code understanding the agent queries on demand.
/// Replaces heuristic nudges with real analysis.
pub trait CodeIntelligence: Send + Sync {
    /// "What does this codebase look like?" — structural summary for system prompt
    fn repo_map(&self, budget_tokens: usize) -> String;

    /// "What depends on this file/function?" — replaces stagnation detection
    fn dependents(&self, path: &str) -> Vec<SymbolRef>;

    /// "What calls this function?" — replaces grep-then-stop heuristic
    fn callers(&self, symbol: &str) -> Vec<SymbolRef>;

    /// "What's the definition of X?" — LSP go-to-def
    fn definition(&self, symbol: &str, file: &Path, line: usize) -> Option<Location>;

    /// "What's wrong with this file?" — LSP diagnostics
    fn diagnostics(&self, file: &Path) -> Vec<Diagnostic>;

    /// "Find code related to X" — semantic search
    fn search(&self, query: &str, limit: usize) -> Vec<CodeLocation>;

    /// "What changed?" — diff analysis for post-edit context
    fn changes(&self) -> Vec<FileChange>;
}

pub struct SymbolRef {
    pub name: String,
    pub file: PathBuf,
    pub line: usize,
    pub kind: SymbolKind,
}

pub struct FileChange {
    pub path: PathBuf,
    pub change_type: ChangeType,
    pub affected_symbols: Vec<String>,
}
```

**Implementation**: Wraps existing infrastructure:
- `RepoMap` (tree-sitter) → `repo_map()`
- `CodeIndex` → `dependents()`, `callers()`
- `LspClient` → `definition()`, `diagnostics()`
- `SemanticIndex` → `search()`
- Git diff → `changes()`

**How it replaces heuristics**:

| Heuristic (deleted) | What CodeIntelligence does instead |
|---|---|
| "URGENT: N reads without writing!" | After reads, agent gets `dependents()` — sees what needs editing |
| "You forgot to verify!" | After edits, agent gets `diagnostics()` — sees if anything broke |
| "Same file edited 15 times!" | After 2nd edit, `diagnostics()` shows if the approach is working |
| "CRITICAL: build failed 3 times!" | `diagnostics()` gives structured error info, not raw output |
| 370-line stop-prevention chain | Agent sees `changes()` — knows what's done and what isn't |
| 240-line system prompt | `repo_map()` gives real codebase context, not generic rules |

The model doesn't need to be told "verify your changes" — it sees diagnostics automatically after edits. It doesn't need "you're stuck" — it sees that the same diagnostics keep appearing.

### 3.4 The Loop — Complete

```
AgentSession::run():
  initial_context = intelligence.repo_map(budget=2000)
  prepend initial_context to system prompt

  for turn in 0..max_turns:
    if timeout exceeded → return TimeoutExceeded

    // Enrich context with live intelligence
    if turn > 0:
      changes = intelligence.changes()
      if changes.not_empty():
        append changes summary to messages
      if changes.has_edits():
        diagnostics = intelligence.diagnostics(changed_files)
        if diagnostics.not_empty():
          append diagnostic summary to messages

    // Build request
    response = provider.complete_stream(request, tools)
    stream response → fire on_text_delta / on_thinking_delta

    if no tool calls → return NoToolCalls (task done)

    for tool_call in response.tool_calls:
      fire on_tool_call
      if needs_approval → ask via on_approval_needed
      result = tools.execute(tool_call)
      fire on_tool_result
      append result to messages

    fire on_turn_start(turn + 1)

    // Prune if over budget
    if messages.token_count > context_budget:
      prune messages (keep recent, summarize old)

    continue
```

**No state between turns** except message history. No tracking variables. No nudges. The model sees structural reality and decides what to do.

### 3.5 What Gets Deleted

| Source | Lines deleted | What's removed |
|---|---|---|
| `headless/mod.rs` heuristics | ~1,330 | All nudge injection, stop-prevention, stagnation, loop detection |
| `headless/mod.rs` state tracking | ~270 | ~25 mutable tracking vars and their init |
| `headless/mod.rs` closures | ~165 | `is_modifying`, `is_code_write` |
| `headless/mod.rs` prompts | ~150 | HEADLESS_SYSTEM_PROMPT (240 lines), RETRY_SYSTEM_PROMPT |
| `headless/mod.rs` text cleanup | ~260 | Repetition detection (model + context pipeline handles this) |
| `headless/hints.rs` | 894 | Error→advice mappings (model + diagnostics handles this) |
| `headless/mod.rs` tests | ~935 | Tests for deleted heuristics |
| `tui/response.rs` loop portion | ~800 | TUI's own LLM↔tool loop (replaced by AgentSession) |
| `tui/tool_execution.rs` | ~855 | TUI's own tool dispatch (replaced by shared) |
| `tui/streaming/events.rs` | ~200 | Custom SSE parsing (replaced by shared SseEventProcessor) |
| `tui/streaming/tool_detection.rs` | ~150 | Custom tool detection |
| `bench/code_agent.rs` generic loop | ~624 | LLM call, response parsing, context pruning |
| **Total** | **~6,633** | |

Plus bench-specific heuristics that can be simplified or removed over time: ~2,000+ lines of nudges, auto-install, auto-verify.

---

## 4. System Prompt

The entire prompt:

```
You are an expert software engineer with full access to the codebase.

Complete the task using the tools available. Read what you need, make changes,
verify they work. Stop when the task is done and verified.

- Use tools to read files, write files, run commands, and check results.
- After making changes, verify: run tests, check compilation, confirm behavior.
- If something fails, fix it. Don't stop on the first error.
- When done and verified, say what you did and what was changed.
```

Plus the `repo_map()` output appended dynamically — real structural context, not generic rules.

---

## 5. Interface Implementations

### 5.1 CLI

```rust
struct CliEvents { stdout: Stdout }
impl AgentEvents for CliEvents {
    fn on_text_delta(&mut self, delta: &str) { write!(self.stdout, "{delta}"); }
    fn on_tool_call(&mut self, id: &str, name: &str, input: &Value) {
        println!("→ {name}: {}", summarize(input));
    }
    fn on_tool_result(&mut self, id: &str, name: &str, output: &str, is_error: bool) {
        if is_error { eprintln!("✗ {name}: {output}"); }
    }
    fn on_approval_needed(&mut self, ..) -> ApprovalDecision {
        // Prompt on stdin
    }
    fn on_done(&mut self, result: &AgentResult) { /* summary */ }
}
```

**Migration**: Replace `runtime.run_headless_with_prior_messages()` → `AgentSession::run()`.

### 5.2 TUI

The TUI implementation needs richer events:
- Streaming markdown rendering (incremental, safe boundary detection)
- Tool approval flow (risk → channel → user decision)
- Interactive questions (question + options → channel)
- Undo snapshots (before file writes)
- Thinking block rendering
- Auto-scroll, file change detection, tab titles

```rust
struct TuiEvents {
    tx: mpsc::Sender<StreamChunk>,  // existing channel pattern
    approval_rx: mpsc::Receiver<ApprovalDecision>,
    question_rx: mpsc::Receiver<String>,
}
impl AgentEvents for TuiEvents {
    fn on_text_delta(&mut self, delta: &str) {
        self.tx.send(StreamChunk::Text(delta.into())).ok();
    }
    fn on_approval_needed(&mut self, tool_name: &str, input: &Value) -> ApprovalDecision {
        self.tx.send(StreamChunk::ApprovalRequest { tool_name, input }).ok();
        self.approval_rx.recv().unwrap_or(ApprovalDecision::Reject("timeout".into()))
    }
    // ... etc
}
```

**Migration**: Replace `stream_llm_response()` (response.rs:292) with `AgentSession::run()`. Delete custom SSE parsing — `SseEventProcessor` handles it internally.

### 5.3 Bench

Bench is different — it uses shell-based `BenchEnvironment`, not `ToolRegistry`. The adapter pattern:

```rust
struct BenchToolAdapter { env: BenchEnvironment }
impl Tool for BenchToolAdapter { /* wraps env.exec() as tools */ }

struct BenchEvents { metrics: AgentMetrics }
impl AgentEvents for BenchEvents { /* collect turn counts, costs */ }
```

**Migration**: Bench's 7,089 lines break into:
- ~624 lines: generic loop → **replace** with AgentSession
- ~1,068 lines: tool execution → **keep** as BenchToolAdapter
- ~633 lines: task analysis heuristics → **keep** (bench-specific optimization)
- ~205 lines: error enrichment → **keep** as post-processing hook
- ~2,715 lines: main loop with inline heuristics → **refactor** to emit events
- ~597 lines: tests → **adapt**

---

## 6. API Surface — What the Agent Crate Imports

### LLM (from `rustycode-llm`)

```rust
use rustycode_llm::provider::{
    LLMProvider,           // trait: complete_stream() → Pin<Box<dyn Stream<Item = StreamChunk>>>
    CompletionRequest,     // { model, messages, tools, stream, max_tokens, temperature }
    CompletionResponse,    // { content, usage, stop_reason }
    ChatMessage,           // role + content constructors (user, assistant, tool_result)
    MessageRole,           // User, Assistant, System, Tool(name)
    SSEEvent,              // Text, ContentBlockStart/Delta/Stop, MessageDelta/Stop, Thinking
    ContentBlockType,      // Text, ToolUse { id, name, input }, Thinking
    ContentDelta,          // Text { text }, PartialJson { partial_json }
    Usage,                 // { input_tokens, output_tokens, reasoning_tokens }
    ProviderError,         // Auth, RateLimited, ContextLengthExceeded, etc.
    ThinkingConfig,        // extended thinking configuration
};
```

### Streaming (from `rustycode-core`)

```rust
use rustycode_core::streaming::{
    SseEventProcessor,      // process_event(event, callbacks) → continue/stop
    StreamingCallbacks,     // trait: on_text, on_thinking, on_tool_start, on_tool_complete
    ToolAccumulator,        // { id, name, partial_json } — accumulates tool call from stream
};
```

### Tools (from `rustycode-tools-api`)

```rust
use rustycode_tools_api::{
    Tool,                   // trait: name, description, execute(params, ctx)
    ToolContext,            // { cwd, sandbox, max_permission, session_id }
    ToolOutput,             // tool execution result
    ToolRegistry,           // register(tool), execute(call, ctx) → ToolResult
    ToolPermission,         // None, Read, Write, Execute
    CancellationToken,
};
```

### Protocol (from `rustycode-protocol`)

```rust
use rustycode_protocol::{
    ToolCall,               // { call_id, name, arguments }
    ToolResult,             // { call_id, output, error, success }
    ContentBlock,           // Text, ToolUse, ToolResult, Thinking
    MessageContent,         // Simple(String) or Blocks(Vec<ContentBlock>)
};
```

**Important**: Two `LLMProvider` traits exist. Use `rustycode_llm::LLMProvider` (has streaming). The `rustycode_protocol::llm::LLMProvider` lacks streaming and uses simpler request format.

### Code Intelligence (from existing crates)

```rust
use rustycode_tools::indexing::repo_map::RepoMap;        // tree-sitter summaries
use rustycode_tools::indexing::code_index::CodeIndex;     // trigram, symbol, dependency
use rustycode_lsp::client::LspClient;                     // full LSP client
```

---

## 7. Naming

| Old | New | Why |
|---|---|---|
| `headless` | `agent` | headless implies a mode; agent is the concept |
| `HeadlessRunner` | `AgentSession` | a running instance |
| `run_headless_task` | `AgentSession::run` | clear ownership |
| `HeadlessTaskResult` | `AgentResult` | consistent naming |
| `HEADLESS_SYSTEM_PROMPT` | (deleted) | replaced by 6-line prompt + repo_map |
| `HeadlessStreamCallbacks` | internal | no longer public |

---

## 8. Current Dependency Chain

```
CLI ──→ runtime ──→ core(headless) ──→ llm, tools, protocol
TUI ──→ core(streaming only), llm, tools, protocol (independent loop)
Bench ──→ llm, protocol (independent, shell-based)
Orchestration ──→ runtime (NOT core)
```

### Target Dependency Chain

```
CLI ──→ agent ──→ llm, tools, protocol, core(streaming), lsp
TUI ──→ agent ──→ (same)
Bench ──→ agent ──→ (same) + bench-specific adapter
Orchestration ──→ agent (for tiered execution)
Agent ──→ orchestration::context (for prompt pipeline)
```

`rustycode-core` loses `headless/` (4,672 lines + 1,475 in hints/utils). `rustycode-runtime` loses `run_headless*` methods.

---

## 9. Migration Phases

### Phase 1 — Create `rustycode-agent` with `AgentSession` + `CodeIntelligence` trait

**Scope**: New crate, no existing code changes.

1. Create `crates/rustycode-agent/` (depends on `rustycode-llm`, `rustycode-tools`, `rustycode-protocol`, `rustycode-core` for streaming)
2. Define all types: `AgentConfig`, `AgentResult`, `StoppedReason`, `AgentEvents`, `ApprovalDecision`
3. Define `CodeIntelligence` trait with `NoopIntelligence` (empty impl for testing)
4. Implement `AgentSession::run()` using `SseEventProcessor`
5. Wire `ToolRegistry` for tool execution
6. Wire context pruning (adapt from `headless/utils.rs`)
7. Wire checkpoint/recovery (adapt from `headless/mod.rs:3601-3734`)
8. Write tests: mock provider → verify event sequence

**Verification**: `cargo test -p rustycode-agent` green. No other crate touched.

### Phase 2 — Implement `CodeIntelligence` with existing infrastructure

**Scope**: `rustycode-agent`, `rustycode-tools`, `rustycode-lsp`.

1. Implement `CodeIntelligence` trait wrapping `RepoMap`, `CodeIndex`, `LspClient`
2. `repo_map()` → delegates to `RepoMap::build()` (already token-budgeted)
3. `dependents()` / `callers()` → delegates to `CodeIndex`
4. `definition()` / `diagnostics()` → delegates to `LspClient`
5. `changes()` → wraps git diff
6. LSP server lifecycle management (start rust-analyzer, etc.)
7. Write integration tests with sample codebase

**Verification**: `intelligence.repo_map(2000)` produces meaningful output. `intelligence.diagnostics(path)` returns real errors.

### Phase 3 — Wire CLI to `AgentSession`

**Scope**: `rustycode-cli`, `rustycode-runtime`.

1. Implement `CliEvents`
2. Replace `runtime.run_headless()` → `AgentSession::run()`
3. Replace `runtime.run_headless_with_prior_messages()` → `AgentSession::run()` with prior messages
4. Update harness_cmd
5. Keep old headless as fallback (don't delete yet)

**Verification**: `cargo run -p rustycode-cli -- run --auto "fix the typo in README.md"` works end-to-end.

### Phase 4 — Wire TUI to `AgentSession`

**Scope**: `rustycode-tui`.

1. Implement `TuiEvents` adapter (maps `AgentEvents` → `StreamChunk` channel)
2. Replace `stream_llm_response()` with `AgentSession::run()`
3. Delete custom SSE parsing, tool detection, tool name normalization
4. Wire approval/question flows through `on_approval_needed` / `on_question`

**Verification**: TUI launches, accepts input, streams responses, handles approvals.

### Phase 5 — Wire bench to `AgentSession`

**Scope**: `rustycode-bench`.

1. Implement `BenchToolAdapter` (wraps `BenchEnvironment::exec()` as `Tool` trait)
2. Implement `BenchEvents` (metric collection)
3. Keep bench-specific heuristics (task analysis, error enrichment, auto-verify) as post-processing hooks
4. Refactor main loop to use `AgentSession::run()` + `BenchEvents`

**Verification**: Bench runs produce equivalent metrics.

### Phase 6 — Delete old headless

**Scope**: `rustycode-core`, `rustycode-runtime`.

1. Delete `crates/rustycode-core/src/headless/` (entire directory)
2. Remove headless re-exports from `core/src/lib.rs`
3. Remove `run_headless*` from `rustycode-runtime`
4. Remove `run_headless*` from `core::runtime`
5. Verify: zero references to `headless` in codebase

**Verification**: `cargo check --workspace` clean. All consumers use `AgentSession`.

### Phase 7 (Future) — Orchestration integration

- Tiered execution uses `AgentSession` with different models per tier
- AST pipeline's research phase uses `CodeIntelligence.repo_map()` instead of directory walking
- `ContextLoader` backed by `CodeIntelligence` instead of heuristic briefs

---

## 10. Risks and Mitigations

| Risk | Likelihood | Impact | Mitigation |
|---|---|---|---|
| Model performs worse without heuristics | Medium | High | CLI first. If patterns regress, add ONE sentence to prompt — not a new guard. |
| TUI migration breaks existing behavior | Medium | High | Feature flag during migration. Old loop available as fallback. |
| CodeIntelligence slow (LSP startup) | Medium | Low | Lazy init. Start LSP on first query, not at agent creation. Cache results. |
| Bench heuristic loss reduces solve rate | Medium | Medium | Keep bench-specific hooks (error enrichment, auto-install) as post-processing. |
| Context budget management differs from TUI's simpler approach | Low | Low | Use 3-phase pruning from headless/utils.rs — strictly better. |
| Two `LLMProvider` traits cause confusion | Low | Medium | Agent crate re-exports the correct one with a doc comment. |

---

## 11. Success Criteria

1. `headless/mod.rs`, `headless/hints.rs` deleted
2. `code_agent.rs` main loop replaced with `AgentSession::run()` + `BenchEvents`
3. TUI and CLI call the same `AgentSession::run()`
4. System prompt ≤ 40 lines (+ dynamic `repo_map()` output)
5. Zero mid-loop nudge injection anywhere in the codebase
6. `CodeIntelligence` trait implemented with at least `repo_map()` and `diagnostics()`
7. Adding a new tool source (MCP server) only touches `ToolRegistry`, not any loop
8. `cargo test --workspace` passes
9. `cargo run -p rustycode-cli -- run --auto "fix the typo in README.md"` works
10. TUI launches, streams, handles approvals — all via `AgentSession`

---

## 12. Open Questions

| Question | Default if unresolved |
|---|---|
| `CodeIntelligence` — eagerly init or lazy? | Lazy (first query starts LSP, caches index) |
| Checkpoint/recovery — part of AgentSession or wrapper? | Wrapper (`CheckpointingSession`) |
| Bench's error enrichment (40+ patterns) — keep as hook or delete? | Keep as optional `ToolResultPostProcessor` hook |
| `AgentEvents::on_approval_needed` — sync or async? | Sync with channel (matches TUI pattern) |
| `orchestration/context/` — move to agent crate or keep separate? | Keep separate (agent imports from orchestration) |
| How does `CodeIntelligence` integrate with AST pipeline? | Phase 7 — `repo_map()` replaces heuristic research |
