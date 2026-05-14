# 01 — Hierarchy Overview

## Problem Statement

The original `AgentSession` model was monolithic and tightly coupled to a single provider and
model configuration. This created four blocking issues:

1. **Provider lock-in** — `SessionMetadata` assumed one `(provider, model)` pair, making it
   impossible to route different tasks to different models within the same session.

2. **Execution-state entanglement** — Persistent message history was mixed with transient
   execution state. A failing agent or orchestrator restart left the session in an indeterminate
   state with no clean recovery path.

3. **Implicit lifecycle** — Agents had no formal onboarding/offboarding contracts. Context was
   never reliably transferred between agents, and partial work was routinely lost on failure.

4. **No structured reasoning** — Agents had no way to decompose complex problems, track
   confidence scores, or carry reasoning state across escalation boundaries or agent handoffs.

---

## Nesting Model

The system is organized as a hierarchy of increasing coordination complexity. Each level wraps
the level below it, adding coordination semantics.

```
┌─────────────────────────────────────────────────────────────────────────┐
│  Ensemble                                                               │
│  Multiple teams with shared ConvergenceView and consensus mechanisms    │
│  ┌───────────────────────────────────────────────────────────────────┐  │
│  │  Team (TeamOrchestrator)                                         │  │
│  │  Builder → Skeptic → Judge loop via Coordinator                  │  │
│  │  Scalpel for targeted fixes, Architect for plan review           │  │
│  │  Team-level ConvergenceView aggregates agent confidence          │  │
│  │  ┌─────────────────────────────────────────────────────────────┐│  │
│  │  │  Orchestrated Agent (StepOrchestrator)                     ││  │
│  │  │  5-tier escalation: Conductor → Musician → Editor →        ││  │
│  │  │  Composer → Thinking                                       ││  │
│  │  │  TaskContext tracks per-task execution state                ││  │
│  │  │  ┌─────────────────────────────────────────────────────────┐││  │
│  │  │  │  Single Agent (AgentSession)                           │││  │
│  │  │  │  Live execution engine, provider/model injected         │││  │
│  │  │  │  Own ReasoningGraph for local reasoning                 │││  │
│  │  │  │  AgentPlugins observe/mutate at lifecycle hooks         │││  │
│  │  │  │  ┌─────────────────────────────────────────────────────┐│││  │
│  │  │  │  │  Sub-Agent (scoped child AgentSession)              ││││  │
│  │  │  │  │  Inherits workspace, scoped tools                   ││││  │
│  │  │  │  │  Own ReasoningGraph, reported back to parent        ││││  │
│  │  │  │  └─────────────────────────────────────────────────────┘│││  │
│  │  │  └─────────────────────────────────────────────────────────┘││  │
│  │  └─────────────────────────────────────────────────────────────┘│  │
│  └───────────────────────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────────────────┘

Cross-cutting at all levels:
  • Session Persistence (compaction, snapshots)
  • Context Forwarding (AgentContext → AgentOutcome, HandoffPackage)
  • SharedWorkspace (file sharing, conflict detection)
  • Event System (EventMsg broadcast, Op commands)
  • Doom Loop Prevention (two independent mechanisms)
```

---

## Dependency Direction

```
CLI / TUI
  → rustycode-core (session management)
    → rustycode-orchestration (autonomous execution)
      → rustycode-agent-runtime (single agent engine)
        → rustycode-llm (provider trait)
        → rustycode-tools (tool execution)
    → rustycode-team (multi-agent coordination)
      → rustycode-orchestration (reuse tier escalation)

Shared foundations (never depend upward):
  rustycode-protocol — cross-crate types
  rustycode-bus — event pub/sub
  rustycode-session — persistent storage
```

**Rule:** Orchestration never depends on CLI/TUI. Team depends on orchestration, not vice versa.
If code knows about both terminals and reasoning, it needs to be split.

---

## Structured Thinking: Hybrid Model

Each agent owns a `ReasoningGraph` (DAG of scored thoughts) for local problem-solving.
At team level, a lightweight `ConvergenceView` aggregates confidence scores and key insights
from all agents without merging full graphs. Ensembles share one `ConvergenceView` across
multiple teams.

```
Agent A ─── ReasoningGraph ───┐
                               ├─→ ConvergenceView (team-level aggregation)
Agent B ─── ReasoningGraph ───┘     • max/mean confidence
                                     • key insights (top-N by confidence)
                                     • dissenting opinions
```

See [03-structured-thinking.md](03-structured-thinking.md) and
[04-context-forwarding.md](04-context-forwarding.md) for details.

---

## Key File Map

| Component | File |
|-----------|------|
| Live agent engine | `crates/rustycode-agent-runtime/src/session.rs` |
| Agent plugins | `crates/rustycode-agent-runtime/src/plugins/mod.rs` |
| Structured thinking | `crates/rustycode-orchestration/src/thinking/` |
| Tier orchestrator | `crates/rustycode-orchestration/src/orchestrator.rs` |
| Task context | `crates/rustycode-orchestration/src/task_context.rs` |
| Cross-tier handoff | `crates/rustycode-orchestration/src/handoff.rs` |
| Agent catalog | `crates/rustycode-orchestration/src/agent_registry.rs` |
| Model routing | `crates/rustycode-orchestration/src/routing/model_router.rs` |
| Team engine | `crates/rustycode-team/src/orchestrator.rs` |
| Team coordination | `crates/rustycode-team/src/coordinator.rs` |
| Persistent session | `crates/rustycode-session/src/session.rs` |
| Compaction | `crates/rustycode-session/src/compaction.rs` |
| Session storage | `crates/rustycode-session/src/session_manager.rs` |
| Protocol session | `crates/rustycode-protocol/src/session.rs` |
