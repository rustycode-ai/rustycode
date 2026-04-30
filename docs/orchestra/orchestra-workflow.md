# Orchestra Workflow

**Status:** Active workflow model
**Last Updated:** 2026-03-19

## Overview

Orchestra is a spec-driven development workflow built to reduce context rot during long-running AI coding sessions.

In RustyCode, the workflow is implemented as a combination of:

- durable `.orchestra/` artifacts on disk
- a Rust runtime that chooses and validates work
- an LLM worker that performs planning and implementation

## High-Level Flow

```text
1. Define milestone and slices
2. Capture context and plans on disk
3. Derive active unit from `.orchestra/`
4. Execute one unit in fresh context
5. Verify and persist evidence
6. Retry, continue, or pause
7. Advance to the next unit
```

## Unit Types

The runtime currently centers around these unit types:

- `plan-slice`
- `execute-task`
- `complete-slice`
- `validate-milestone`

These are selected from derived state, not from ad hoc chat memory.

## Practical Workflow Stages

### 1. Project and milestone setup

Create the `.orchestra/` structure, roadmap, slices, and task plans.
This gives the runtime the durable artifacts it needs.

### 2. Planning work

The system can plan a slice by reading its context and writing a `PLAN.md` that defines tasks and verification expectations.

### 3. Task execution

For each active task:

- build focused context
- execute with tools in fresh session context
- write summary and evidence
- verify deterministically
- decide retry or completion

### 4. Slice completion

Once all tasks in a slice are done, the runtime writes slice-level completion artifacts and updates the roadmap.

### 5. Milestone validation

Once all slices in a milestone are complete, the runtime validates milestone completion and writes validation artifacts.

## Important Workflow Rules

### Fresh context per unit

Each unit should run with a focused prompt and bounded context.
The runtime should not accumulate arbitrary prior conversation state forever.

### Durable artifacts before state advancement

A task is not complete just because the model says it is complete.
The required artifacts must exist and verification must pass.

### Verification is part of the workflow

Verification is not an optional afterthought.
It is part of normal execution and should influence retry/stop behavior.

### Retry is runtime policy

If verification fails, the runtime should decide whether to:

- retry with failure context
- pause for human review
- fail the unit

## Current vs Target Workflow

### Current

- state derivation is in place
- unit execution loop exists
- task and slice artifacts exist
- verification/timeout/budget subsystems exist but need deeper integration

### Target

- deterministic post-unit control loop in Rust
- robust retry semantics
- strong timeout and recovery behavior
- real metrics and budget accounting
- cleaner separation between runtime kernel and LLM worker

## What This Document Is

This is the **workflow truth** for how Orchestra should operate conceptually.

It is not a promise that every command or phase from the public `get-shit-done` installer is already implemented in RustyCode.
For implementation status, see `docs/orchestra-implementation.md`.
