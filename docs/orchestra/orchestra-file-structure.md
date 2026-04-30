# Orchestra File Structure

**Status:** Active file structure
**Last Updated:** 2026-03-19

## Purpose

This document defines the `.orchestra/` layout that the Rust runtime should read and write.

The model may help create or update these files, but the runtime should treat them as durable system state.

## Core Layout

```text
.orchestra/
├── STATE.md
├── activity.logl
├── runtime/
│   ├── metrics.jsonl
│   └── ...
├── milestones/
│   └── M01/
│       ├── ROADMAP.md
│       ├── VALIDATION.md
│       └── slices/
│           └── S01/
│               ├── PLAN.md
│               ├── RESEARCH.md
│               ├── CONTEXT.md
│               ├── S01-SUMMARY.md
│               └── tasks/
│                   └── T01/
│                       ├── T01-PLAN.md
│                       ├── T01-SUMMARY.md
│                       └── T01-VERIFY.json
```

## Source of Truth Rules

### Authoritative artifacts

The runtime should derive state primarily from:

- milestone `ROADMAP.md`
- slice `PLAN.md`
- task plan/summary/evidence artifacts

### Display cache artifact

`STATE.md` is a display cache and operator aid.
It should not be the only authoritative source of state.

## Key Files

### `.orchestra/STATE.md`

Human-readable snapshot of:

- active phase
- active milestone/slice/task
- current progress overview
- recent status summary

### `.orchestra/activity.logl`

Append-only activity log for runtime and recovery visibility.

### `.orchestra/runtime/metrics.jsonl`

Per-unit metrics ledger.
Should record tokens, cost, duration, and success outcome.

### `ROADMAP.md`

Milestone-level source of truth for slices.
Contains slice ordering and done state.

### `PLAN.md`

Slice-level source of truth for tasks.
Contains task ordering, task titles, done state, and verification intent.

### `T##-PLAN.md`

Task contract for implementation.
Should define what to build and how success is evaluated.

### `T##-SUMMARY.md`

Task result artifact.
Should explain what changed, what was verified, and anything notable for future units.

### `T##-VERIFY.json`

Machine-readable verification evidence.
Should be written by the runtime, not improvised by the model.

## Naming Conventions

- milestone: `M01`, `M02`, ...
- slice: `S01`, `S02`, ...
- task: `T01`, `T02`, ...
- milestone roadmap: `ROADMAP.md`
- slice plan: `PLAN.md`
- task plan: `T01-PLAN.md`
- task summary: `T01-SUMMARY.md`
- task verification evidence: `T01-VERIFY.json`

## Notes

Older YAML-centric designs are superseded.
The active Orchestra runtime should assume a **markdown-first artifact model** with targeted JSON evidence where machine readability matters.
