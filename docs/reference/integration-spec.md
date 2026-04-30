# RustyCode Integration Spec: Agents, Skills, Tools, MCP, Hooks, and Orchestra

**Last Updated:** 2026-03-19
**Status:** Active integration overview

## Purpose

This document describes the major integration surfaces in RustyCode and where Orchestra fits among them.

## Orchestra Position In The System

Orchestra should be treated as a **runtime-driven workflow layer** on top of RustyCode primitives.

It is not just another slash command.
It depends on and interacts with:

- agents and skills for optional context augmentation
- tools for file/system operations
- hooks and recovery systems for runtime control
- MCP/tool ecosystems where relevant

## Current Orchestra Direction

The active direction is:

- Rust handles deterministic workflow execution mechanics
- the LLM handles planning and implementation reasoning
- `.orchestra/` artifacts provide durable workflow state

See:

- `docs/orchestra-architecture.md`
- `docs/orchestra-workflow.md`
- `docs/orchestra-implementation.md`

## Integration Areas

### Agents

Agents are useful for exploration, side investigations, and bounded delegated work.
They are not a substitute for the Orchestra runtime state machine.

### Skills

Skills can enrich prompts and execution context.
Orchestra should use them selectively via:

- discovery
- prompt inclusion
- telemetry
- activation policy

### Tools

Tools are the mechanism the LLM uses to act on the repo.
Orchestra depends heavily on reliable tool execution, but tool orchestration policy should remain runtime-owned.

### Hooks

Hooks are useful for post-unit or environment-specific behaviors.
They should extend the runtime, not replace core runtime guarantees.

### MCP

MCP integrations may extend available tools and systems, but Orchestra correctness should not depend on ad hoc MCP behavior.

## Status Guidance

When documenting Orchestra integration status, distinguish between:

- legacy Orchestra/CLI support
- `rustycode-orchestra` runtime support
- parity goals with `orchestra-2`

Avoid saying simply “Orchestra is implemented and working” without clarifying which layer or path that refers to.
