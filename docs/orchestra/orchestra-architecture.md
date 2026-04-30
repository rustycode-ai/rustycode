# Orchestra Architecture

**Status:** Active architecture
**Last Updated:** 2026-03-19

## Purpose

Orchestra in RustyCode is a **Rust runtime kernel plus an LLM worker**.

We are not trying to make the LLM do orchestration, bookkeeping, parsing, timeout handling, retries, budgeting, or artifact persistence through prompts alone.
We are moving those deterministic responsibilities into Rust so the model can focus on reasoning and implementation.

## Core Boundary

### Rust owns

Rust owns deterministic, stateful, safety-critical behavior:

- derive state from `.orchestra/` files
- choose the next unit to execute
- load prompts/templates and build context
- enforce verification, retry, timeout, and budget policies
- persist summaries, evidence, metrics, and recovery state
- manage git/worktree mechanics
- detect stuck or inconsistent execution
- classify events and drive the runtime state machine

### LLM owns

The LLM owns judgment-heavy work:

- planning a slice or task
- deciding how to implement a task
- generating code changes
- interpreting failures and attempting repairs
- writing summaries and explanations
- making ambiguous tradeoff decisions

## Runtime Shape

```text
User / CLI / TUI
      |
      v
Orchestra Runtime Kernel (Rust)
  - state derivation
  - dispatch
  - verification
  - retry policy
  - timeout supervision
  - budget + metrics
  - artifact persistence
  - recovery
      |
      v
LLM Unit Runner
  - execute plan-slice / execute-task / complete-slice / validate-milestone
  - use tools
  - produce code and summaries
```

## Primary Runtime Loop

```text
1. Derive state from disk
2. Select active unit
3. Build focused prompt/context
4. Run LLM worker with tools
5. Persist summary/artifacts
6. Run deterministic verification
7. Decide continue / retry / pause / fail
8. Record metrics and budget impact
9. Update disk state
10. Loop
```

## Key Design Principles

### Disk is the durable source of truth

`.orchestra/` artifacts are authoritative.
`STATE.md` is a cache/display artifact, not the only real state.

### Fresh context per unit

Each unit should run in a fresh execution context with only the relevant slice, task, summaries, and verification context.

### Runtime control beats prompt discipline

If a behavior can be made deterministic in Rust, prefer that over “tell the model to remember to do it.”

### Evidence matters

Completion is not just “the model said done.”
A unit is only complete when the runtime has written the right artifacts and the verification policy passes.

## Major Runtime Subsystems

### State Runtime

Responsible for:

- reading roadmap/plan/task artifacts
- deriving current phase and active unit
- writing display cache state
- validating structural integrity of `.orchestra/`

### Prompt Runtime

Responsible for:

- prompt/template loading
- prompt caching
- assembling only the context required for the active unit
- failure-context injection for retries

### Verification Runtime

Responsible for:

- command discovery
- command execution
- evidence writing
- retry classification
- retry attempt tracking
- failure-context generation

### Timeout Runtime

Responsible for:

- soft timeout warning
- idle watchdog
- hard timeout stop
- progress markers
- in-flight tool awareness
- timeout recovery transitions

### Budget and Metrics Runtime

Responsible for:

- per-unit token/cost accounting
- ledger writes
- budget thresholds and enforcement
- project totals and reporting

### Recovery Runtime

Responsible for:

- crash detection
- lock lifecycle
- runtime record persistence
- stale execution cleanup
- remediation instructions for resumes

## Current Direction

The main architectural problem today is not missing helper modules.
It is that too much orchestration still lives directly inside `orchestra_executor.rs`.

The long-term direction is to reduce `Orchestra2Executor` into a coordinator over smaller runtime services.

## Near-Term Refactor Target

Split the current executor responsibilities into:

- `RuntimeKernel`-style control logic
- `LlmUnitRunner`-style prompt/tool execution logic

That keeps the system aligned with both:

- the public Orchestra philosophy
- the module structure already present in `orchestra-2`
