# Multi-Agent Session Architecture

This directory contains the architecture specification for RustyCode's multi-agent system,
organized by subsystem. Start with `01-hierarchy-overview.md` for the big picture.

## Document Index

| # | Document | Scope |
|---|----------|-------|
| 01 | [Hierarchy Overview](01-hierarchy-overview.md) | Problem statement, nesting model, dependency diagram |
| 02 | [Single Agent](02-single-agent.md) | AgentSession, AgentConfig, plugins, event system |
| 03 | [Structured Thinking](03-structured-thinking.md) | ReasoningGraph, strategies, executor, hybrid model |
| 04 | [Context Forwarding](04-context-forwarding.md) | AgentContext, AgentOutcome, HandoffPackage, SharedWorkspace |
| 05 | [Orchestration](05-orchestration.md) | StepOrchestrator, tiers, TaskContext, ModelRouter, AST |
| 06 | [Teams & Ensembles](06-teams-and-ensembles.md) | TeamOrchestrator, Coordinator, ConvergenceView, ensembles |
| 07 | [Session Persistence](07-session-persistence.md) | Session types, compaction, SessionManager |
| 08 | [Cross-Cutting Concerns](08-cross-cutting.md) | Doom loops, error hardening, state boundaries |
| 09 | [Implementation Plan](09-implementation-plan.md) | Phased plan with goals, validation, and tests |

## Key Crate Map

| Crate | Layer |
|-------|-------|
| `rustycode-agent-runtime` | Single agent execution |
| `rustycode-orchestration` | Tier escalation, structured thinking, task context |
| `rustycode-team` | Team coordination, ensembles |
| `rustycode-session` | Persistent session storage |
| `rustycode-protocol` | Cross-crate shared types |
| `rustycode-llm` | LLM provider abstractions |
| `rustycode-tools` | Tool execution framework |

## Conventions

- Struct definitions show *current* code state (not aspirational)
- `#[serde(skip)]` fields are marked as ephemeral
- File paths are relative to repository root
