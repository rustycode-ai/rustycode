# RustyCode Re-Architecture Roadmap

> Based on comparative analysis of OpenAI Codex CLI architecture (codex-rs, 113 crates).

## Executive Summary

RustyCode has the building blocks for a Codex-class architecture — `Op`, `EventMsg`, `EventBus`, and `SyncEvent` types already exist in `rustycode-protocol` — but they're not wired as the primary data flow. Instead, 4 parallel event systems carry data through the stack, and the TUI directly depends on 26 workspace crates.

This roadmap describes 4 phases to close the gap:

1. **Unify Events** — Consolidate 4 event channels into 1 `EventMsg` system
2. **Event Sourcing** — JSONL rollouts as source of truth, SQLite as derived index
3. **App-Server Daemon** — JSON-RPC protocol separating UI from core
4. **Crate Decomposition** — Reduce coupling in hotspot crates by 50%+

**Total estimated effort**: 20-24 weeks across all phases (Phases 3+4 can overlap).

## Current State Assessment

### Crate Coupling Scores (deps × LOC)

| Crate | Workspace Deps | LOC | Coupling Score | Status |
|-------|---------------|-----|---------------|--------|
| rustycode-tui | 26 | 109,248 | 2,840,448 | Critical |
| rustycode-tools | 12 | 68,094 | 817,128 | Critical |
| rustycode-cli | 16 | 44,569 | 697,104 | High |
| rustycode-core | 19 | 26,122 | 496,318 | High |
| rustycode-runtime | 11 | 26,129 | 287,419 | Medium |

### Event System Fragmentation

| System | Location | Role | Used As Primary? |
|--------|----------|------|-------------------|
| EventBus | rustycode-bus | Domain pub/sub | Supplemental only |
| StreamEvent | rustycode-protocol | Agent callbacks | Yes (via AgentEvents trait) |
| OrchestrationEvent | rustycode-orchestration | Orchestration layer | Yes (in orchestration) |
| EventMsg | rustycode-protocol | Core→TUI protocol | No (defined but underused) |

### What We Have (Already Good)

- `Op` enum with 12 typed variants (SendMessage, StopStream, ApproveTool, etc.)
- `EventMsg` enum with 40+ typed variants (TextDelta, TurnStarted, ToolExecCompleted, etc.)
- `SyncEvent` with monotonic sequence numbers for ordering
- `EventBus` with type-safe pub/sub and wildcard subscriptions
- `AgentEvents` callback trait for agent loop events
- `LLMProvider` unified trait across all providers
- `ToolRegistry` with tool discovery and dispatch
- Sandboxing (Seatbelt + landlock)

### What's Missing (Codex Has, We Don't)

- Single unified event channel (SQ/EQ pattern)
- Event-sourced persistence (JSONL rollouts)
- App-server daemon (JSON-RPC client/server)
- Session replay from event log
- Thread forking
- Plugin system with marketplace
- Per-thread config overrides
- Cursor pagination for list APIs

---

## Phase 1: Unify Event System

**Goal**: Consolidate 4 event channels into a single `EventMsg` system.

**Status**: In Progress (40%) | **Estimated Duration**: 6-8 weeks

**Full Document**: [PHASE1-UNIFY-EVENTS.md](./PHASE1-UNIFY-EVENTS.md) (816 lines)

### Implementation Update (2026-05-12)

- `EventMsg` and `Op` types implemented in `rustycode-protocol`.
- Basic `EventMsg` variants for streaming, tool execution, and planning added.
- `AgentSession` still uses legacy `AgentEvents` callback trait (refactor pending).

### Architecture

```
TUI/CLI ──submit(Op)──→ Runtime ──broadcast(EventMsg)──→ TUI/CLI
                              │
                        AgentSession
                        (no callback trait)
```

### Key Changes

- **AgentSession** emits `EventMsg` into `tokio::sync::broadcast` channel (replaces `AgentEvents` callback trait)
- **Runtime** exposes `submit(Op)` for all inbound, `next_event()` for all outbound
- **OrchestrationEvent** variants merge into `EventMsg` (with new variants: StrategyChanged, QualityGateResult, MilestoneProgress)
- **EventBus** demoted to infrastructure-only notifications (metrics, health, worker pools)
- **EventMsg gains ~15 new variants** covering orchestration and tool events currently on separate channels
- **Op gains ~6 new variants**: ResumeFromCheckpoint, RetryLastTurn, SetStrategy, SetToolProfile

### Migration Strategy (5 sub-phases)

| Phase | Duration | What Changes |
|-------|----------|-------------|
| 1A | 1 week | Add new EventMsg variants, don't remove old ones |
| 1B | 2 weeks | Dual emission: AgentSession emits both callback AND EventMsg |
| 1C | 2 weeks | TUI switches from callback consumption to EventMsg broadcast |
| 1D | 1 week | Remove callback trait, old StreamEvent usage |
| 1E | 1 week | EventBus domain events removed, infrastructure-only |

### Success Criteria

- AgentSession uses no callback trait — only broadcasts EventMsg
- Runtime processes all inbound via Op, all outbound via EventMsg
- Zero direct StreamEvent or OrchestrationEvent usage outside protocol crate
- All existing tests pass

---

## Phase 2: Event Sourcing with JSONL Rollouts

**Goal**: JSONL rollouts as source of truth, SQLite as derived index.

**Status**: In Progress (15%) | **Estimated Duration**: 6-8 weeks

**Depends on**: Phase 1 (unified EventMsg)

**Full Document**: [PHASE2-EVENT-SOURCING.md](./PHASE2-EVENT-SOURCING.md) (1,462 lines)

### Implementation Update (2026-05-12)

- MVP `RolloutRecorder` implemented in `rustycode-core/src/rollout.rs`.
- JSONL append-only logging functional for session events.
- Session replay and SQLite derived indexing (StateRuntime) still in design/prototype phase.

### Architecture

```
Runtime ──EventMsg──→ RolloutRecorder ──append──→ JSONL file
                            │
                            └──apply──→ StateRuntime ──index──→ SQLite
```

### Key Components

- **RolloutRecorder**: Appends each EventMsg as a `RolloutItem` to JSONL files at `~/.rustycode/sessions/YYYY/MM/DD/rollout-{timestamp}-{uuid}.jsonl`
- **RolloutItem enum**: 25+ variants extending EventMsg with persistence metadata (timestamps, sequence numbers)
- **StateRuntime**: SQLite with `threads` table, derived indexes, FTS search, token tracking
- **Session Replay**: Rebuild complete session state from JSONL (target: <100ms for 1000 items)
- **Thread Forking**: `ForkedFrom` marker with parent reference, `InitialHistory::Forked` for compressed prefix
- **Backfill**: Watermark-based batch processing (200 files/batch, 900s lease, crash recovery)
- **Compaction**: Groups consecutive items, reduces rollout size by >50%

### Storage Estimates

| Session Size | Raw Rollout | Compacted |
|-------------|-------------|-----------|
| Short (10 turns) | ~50 KB | ~25 KB |
| Medium (100 turns) | ~500 KB | ~100 KB |
| Long (1000 turns) | ~5 MB | ~500 KB |

### Migration Strategy (6 sub-phases)

| Phase | Duration | What Changes |
|-------|----------|-------------|
| 2A | 1 week | Add RolloutRecorder, write-only (no reads) |
| 2B | 2 weeks | Add StateRuntime with SQLite schema |
| 2C | 2 weeks | Dual-write: existing storage + JSONL/SQLite |
| 2D | 1 week | Migrate reads to use SQLite indexes |
| 2E | 1 week | Session replay and thread forking |
| 2F | 1 week | Remove old snapshot storage |

### Success Criteria

- Session state recoverable from JSONL after crash
- Thread fork produces child with parent's prefix history
- SQLite indexes match filesystem rollouts (verified by read repair)
- Backfill completes for existing sessions on upgrade

---

## Phase 3: App-Server Daemon

**Goal**: JSON-RPC protocol separating UI from core.

**Status**: Design Complete | **Estimated Duration**: 8 weeks

**Depends on**: Phase 1, Phase 2

**Full Document**: [PHASE3-APP-SERVER.md](./PHASE3-APP-SERVER.md) (645 lines)

### Architecture

```
┌─ TUI ──────────────────────┐
│  depends on: ~5 crates      │
│  ClientHandle (mpsc/WS)     │
└──────────┬──────────────────┘
           │ JSON-RPC 2.0
┌──────────▼──────────────────┐
│  rustycode-server            │
│  Router → Runtime.submit(Op)│
│  Broadcaster ← EventMsg     │
└──────────┬──────────────────┘
           │
┌──────────▼──────────────────┐
│  Core / Agent-Runtime        │
└─────────────────────────────┘
```

### New Crates

| Crate | Purpose | Deps |
|-------|---------|------|
| `rustycode-server-protocol` | Typed requests/responses/notifications, JSON-RPC envelope | 0 async deps |
| `rustycode-server` | Message processor, router, approval handler | protocol, core, runtime |
| `rustycode-server-client` | InProcessClient (mpsc) + future RemoteClient (WebSocket) | server-protocol |

### MVP Method Set (~25 methods)

- **session/***: create, stop, list, get, delete
- **turn/***: submit, cancel, list, get
- **tool/***: approve, list, toggle
- **config/***: get, set, model/list
- **fs/***: read, list (read-only for TUI)
- **history/***: search
- **plan/***: create, approve, status

### Bidirectional Approval Flow

Server sends `approval/requested` notification to client → user sees dialog → client sends `tool/approve` with decision (Accept, AcceptForSession, Decline, Cancel).

### Streaming Guarantees

- **Lossless**: TextDelta, ThinkingDelta, ToolCallCompleted (every message delivered)
- **Best-effort**: CommandOutputDelta (lagged messages emit `Lagged { skipped: N }` marker)

### Migration Strategy

| Phase | Duration | What Changes |
|-------|----------|-------------|
| 3A | 2 weeks | Scaffold 3 new crates, InProcessClient wires to server |
| 3B | 2 weeks | Event tunneling: server broadcasts EventMsg as notifications |
| 3C | 2 weeks | Command tunneling: TUI submits via client.request() |
| 3D | 2 weeks | Remove direct deps from TUI (26 → ~5) |

### Success Criteria

- TUI workspace deps reduced from 26 to ~5
- InProcessClient works without serialization
- Approval flow works over protocol
- Streaming token deltas delivered in <50ms latency
- All existing TUI functionality preserved

---

## Phase 4: Crate Decomposition

**Goal**: Reduce coupling scores by 50%+ in hotspot crates.

**Status**: In Progress (pre-work done) | **Estimated Duration**: 8 weeks

**Can run in parallel with**: Phase 3

**Full Document**: [PHASE4-CRATE-DECOMPOSITION.md](./PHASE4-CRATE-DECOMPOSITION.md)

### Implementation Update (2026-05-12)

Intra-crate restructuring completed out of order as pre-work:
- TUI `App` struct: 60+ flat fields → 11 typed sub-structs (`state_model.rs`)
- `rustycode-core` runtime: `runtime.rs` monolith → `runtime/` with 6 domain files
- `rustycode-core` context: `context_management/` + `context_prio/` → `context/`
- `rustycode-core` recovery: flat checkpoint files → `recovery/` submodule
- TUI file renames: `input/event_loop.rs` → `input/handler.rs`; `render/event_loop.rs` → `render/viewport.rs`

Crate extraction (the main Phase 4 work) remains pending.

### Target Coupling Reduction

| Crate | Current Score | Target Score | Reduction |
|-------|-------------|-------------|-----------|
| rustycode-tui | 2,840,448 | ~450,000 | 84% |
| rustycode-core | 496,318 | ~96,000 | 81% |
| rustycode-tools | 817,128 | ~192,000 | 76% |

### TUI Decomposition (26 → 5 deps)

After Phase 3's app-server layer, TUI depends only on:
`rustycode-server-client`, `rustycode-server-protocol`, `rustycode-protocol`, `rustycode-config`

### Core Decomposition (19 → 8 deps)

Extract focused crates:
- `rustycode-session-manager` — Session lifecycle, state machine
- `rustycode-thread-manager` — Thread/conversation management
- `rustycode-tool-dispatch` — Tool routing and execution
- `rustycode-edit-history` — Edit tracking and undo

### Tools Decomposition (12 → 4 deps)

Split by category:
- `rustycode-tools-fs` — File read/write/edit/list
- `rustycode-tools-bash` — Shell command execution
- `rustycode-tools-lsp` — Language server protocol tools
- `rustycode-tools-mcp` — MCP server integration
- `rustycode-tools-indexing` — Code indexing, repo map, semantic search

### New Crates (~20 total)

Average 2,350 LOC each, all under 500 LOC per module.

### Circular Dependency Resolution

| Cycle | Resolution |
|-------|-----------|
| core ↔ orchestration | Extract session/plan-executor to new crate |
| tools ↔ execution | Extract tools-executor |
| llm ↔ tools | Already resolved via tool-integration shim |

### Migration Strategy (4 phases, 8 weeks)

1. Extract independent modules first (no circular deps)
2. Extract cross-cutting concerns (session, thread, tool-dispatch)
3. Resolve remaining circular dependencies
4. Verify all coupling targets met

### Success Criteria

- No crate has >10 workspace dependencies
- No circular dependency paths (even via shims)
- All crates have clear single responsibilities
- Target module size <500 LOC (per Codex convention)

---

## Implementation Order

```
Phase 1 (Unify Events) ──→ Phase 2 (Event Sourcing) ──→ Phase 5 (Plugins)
                                    │
                                    ▼
                           Phase 3 (App-Server)
                                    │
                                    ▼
                           Phase 4 (Decomposition)
```

Phases 3 and 4 can overlap. Phase 5 (plugin system) is future work after the foundation is solid.

### Timeline Overview

| Phase | Weeks | Dependencies | Key Deliverable |
|-------|-------|-------------|----------------|
| 1 | 6-8 | None | Single EventMsg channel, no callback traits |
| 2 | 6-8 | Phase 1 | JSONL rollouts, SQLite indexes, session replay |
| 3 | 8 | Phase 1+2 | TUI deps 26→5, JSON-RPC protocol |
| 4 | 8 | None (parallel with 3) | No crate >10 deps, <500 LOC modules |

## Risk Assessment

| Risk | Likelihood | Impact | Mitigation |
|------|-----------|--------|------------|
| EventMsg migration breaks TUI | Medium | High | Dual emission during transition, feature flag for rollout |
| JSONL rollout performance | Low | Medium | Benchmark with large sessions, async writes |
| App-server adds latency | Low | High | Start with in-process (no serialization), benchmark before adding remote |
| Crate decomposition causes merge conflicts | High | Low | Small PRs, one extraction at a time |
| Circular dep resolution requires new shim crates | Medium | Low | Accept shims as transitional, remove when boundaries improve |
| Rate limiting during background agent work | Low | Low | Retry or write docs directly |

## Codex Patterns Worth Adopting

From the AGENTS.md analysis:

1. **"Resist adding code to codex-core"** — RustyCode should adopt the same stance for `rustycode-core`
2. **Target modules under 500 LoC** — Current god objects are 1,000-2,000+ LOC per file
3. **Avoid bool/ambiguous Option parameters** — Use enums, named methods, newtypes
4. **Prefer private modules with explicit public API** — Reduce accidental coupling
5. **Exhaustive match, avoid wildcard arms** — For business-critical enums
6. **Prefer RPITIT trait methods with Send bounds** — Over async_trait

## Document Index

| Document | Lines | Content |
|----------|-------|---------|
| [PHASE1-UNIFY-EVENTS.md](./PHASE1-UNIFY-EVENTS.md) | 819 | EventMsg consolidation, AgentSession migration, 5-phase rollout |
| [PHASE2-EVENT-SOURCING.md](./PHASE2-EVENT-SOURCING.md) | 1,465 | JSONL rollouts, SQLite schema, replay, forking, compaction |
| [PHASE3-APP-SERVER.md](./PHASE3-APP-SERVER.md) | 645 | JSON-RPC protocol, 25 methods, approval flow, TUI migration |
| [PHASE4-CRATE-DECOMPOSITION.md](./PHASE4-CRATE-DECOMPOSITION.md) | 637 | 20 new crates, coupling targets, circular dep resolution |

## Source Analysis

- Codex codebase: ~/dev/codex/codex-rs (113 crates, analyzed 2026-05-12)
- RustyCode codebase: ~/dev/rustycode (~35 crates, v0.4.0)
