# Orchestra Implementation Status

> **⚠️ DEPRECATED**: `rustycode-orchestra` has been deleted. All functionality now lives in `crates/rustycode-orchestration/`. See `crates/rustycode-orchestration/README.md` for the current module map.

**Status:** Superseded by `rustycode-orchestration`
**Last Updated:** 2026-03-19

## Purpose

This document described the implementation direction for Orchestra in RustyCode.

It is intentionally focused on the **Rust runtime kernel** rather than older wave/YAML prototypes.

## Current Implementation Model

The implementation now lives in `crates/rustycode-orchestration/`.

Many key modules already exist for:

- state derivation
- prompt loading
- verification gate logic
- verification evidence writing
- timeout supervision
- budget tracking
- metrics recording
- skill discovery/telemetry
- crash recovery and runtime records

The important distinction now is:

- the canonical path is `Orchestra2Executor`
- much of the deterministic runtime has been extracted into focused modules
- the remaining work is mostly around polishing behavior and tightening the few still-mixed orchestration seams

## What Works Today

- bootstrap a runnable `.orchestra/` project with `orchestra init` or the auto-bootstrap path
- derive the active unit from `.orchestra/` files
- execute units with an LLM provider and tools through `Orchestra2Executor`
- run verification with retry-aware failure context
- persist verification evidence and retry state
- track tool activity for timeout supervision
- record per-unit metrics and budget usage
- write task summaries and canonical state artifacts
- exercise the canonical path with offline mock-provider smoke tests

## Verified Integration Gaps

The remaining gaps are now mostly “finish the runtime” gaps rather than “make the feature real” gaps:

- crash/startup recovery can still be tightened on the canonical path
- startup crash recovery is now active for stale locks, with room to deepen the session briefing and resume behavior
- some post-unit orchestration branches are still intentionally thin
- there is still room to simplify the executor further by pushing more live control flow into runtime helpers
- the broader crate still has warning backlog outside the Orchestra v2 slice

## Main Gap

The biggest remaining gap is still **integration depth**, not helper-module existence.

Most of the core runtime now exists and is wired.
The work left is to keep shrinking the amount of orchestration that lives directly in the executor and keep moving deterministic behavior into dedicated runtime helpers.

## Implementation Priorities

### Priority 1: Verification control loop

Move post-unit verification into a runtime-owned control loop that:

- discovers commands deterministically
- runs checks
- writes `T##-VERIFY.json`
- tracks retry attempts
- injects failure context into retries
- decides retry vs fail vs continue

This priority matters first because the code for most of the ingredients already exists; the missing work is primarily executor integration and state transitions.

### Priority 2: Timeout supervision

Bring timeout behavior closer to `orchestra-2` semantics:

- soft warning
- idle watchdog
- hard timeout
- recovery state transitions
- in-flight tool awareness

### Priority 3: Real metrics and budget

Replace placeholder accounting with real runtime accounting where possible.

### Priority 4: Secrets/config runtime

Resolve runtime prerequisites before dispatch instead of relying on prompts or manual environment assumptions.

### Priority 5: Git/worktree lifecycle

Move milestone/task isolation mechanics into deterministic runtime services.

## Suggested Code Shape

### Runtime side

```rust
struct RuntimeKernel {
    state: StateRuntime,
    verification: VerificationRuntime,
    timeout: TimeoutRuntime,
    budget: BudgetRuntime,
    metrics: MetricsRuntime,
    recovery: RecoveryRuntime,
}
```

### LLM side

```rust
struct LlmUnitRunner {
    provider: Arc<dyn LLMProvider>,
    tool_registry: ToolRegistry,
    model: String,
}
```

## Immediate Next Refactor

The next concrete refactor should be:

1. keep extracting any remaining mixed control flow from `orchestra_executor.rs`
2. keep tightening startup/recovery behavior on the canonical path
3. let the executor coordinate rather than own all control logic directly

## Anti-Goals

Do not treat older designs based on `STATE.yaml`, `PLAN.yaml`, or wave-first orchestration as the active implementation plan.
Those are historical concepts, not the current target architecture.
