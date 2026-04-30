# RustyCode Orchestration: Module Architecture

> **Note**: This document was originally written for `rustycode-orchestra`, which has been deleted. The module architecture described here now applies to `crates/rustycode-orchestration/`. Some module paths may have changed during the migration.

The `rustycode-orchestration` crate implements autonomous development orchestration through a carefully modularized architecture of focused modules.

## Architecture Principles

1. **Single Responsibility**: Each module has one clear purpose
2. **Encapsulation**: Internal implementation hidden, explicit public API
3. **Loose Coupling**: Modules communicate through well-defined interfaces
4. **High Cohesion**: Related functionality grouped together
5. **No Circular Dependencies**: Clean dependency flow from bottom-up

## Layer Model

```
┌─────────────────────────────────────────────────────────────┐
│  Service Layer (service, coordinator)                       │
│  - Orchestrates autonomous development workflows             │
├─────────────────────────────────────────────────────────────┤
│  Execution Layer (execution, planning, recovery, worktree)  │
│  - Manages plan execution, recovery, isolated environments  │
├─────────────────────────────────────────────────────────────┤
│  Support Layer (llm, tools, cache, config, verification)    │
│  - Provides domain services and infrastructure               │
├─────────────────────────────────────────────────────────────┤
│  Foundation Layer (state, phases, detection, files, paths)  │
│  - Core data structures and type system                      │
├─────────────────────────────────────────────────────────────┤
│  Context Layer (context, thinking, discovery, convention)   │
│  - Prompt context, reasoning, and convention discovery      │
└─────────────────────────────────────────────────────────────┘
```

## Core Modules (7)

### Tier 1: Type System & Fundamentals

**error** — Error types, complexity levels, risk assessment
- Types: `Complexity`, `RiskLevel`, `Unit`, `UnitType`
- No dependencies (foundational)

**phases** — Phase lifecycle for orchestration
- Types: `Phase` lifecycle model
- Depends on: error

**state** — Execution state tracking and persistence
- Types: State structures for workflow tracking
- Depends on: phases, error

### Tier 2: Support & Detection

**detection** — State derivation and complexity classification
- Types: `StateDeriver`, complexity metrics
- Depends on: state, phases, models
- Provides: Complexity analysis, state inference

**files** — File system operations
- Functions: File I/O utilities
- Depends on: paths

**paths** — Path management and resolution
- Functions: `agent_dir()`, `app_root()`, path utilities
- No dependencies (foundational)

### Tier 3: Services

**service** — Main orchestration coordinator
- Types: `OrchestraService`
- Depends on: execution, planning, state, context

**test_lock** — Concurrent test prevention
- Types: `CRATE_TEST_LOCK`
- No dependencies

## Execution Layer (4)

**execution** — Task execution lifecycle
- Types: `TaskContext`, execution state
- Depends on: tools, context, recovery, state

**planning** — Plan generation and scheduling
- Types: Plan, schedule structures
- Depends on: state, detection, context, models

**recovery** — Recovery and resilience mechanisms
- Types: Recovery strategies
- Depends on: cache, state, verification

**worktree** — Isolated environment management
- Types: Worktree types
- Depends on: git, paths

## Infrastructure (11)

**llm** — LLM provider integration
- Traits: Provider abstractions
- Depends on: config, models, tools

**tools** — Tool execution framework
- Types: Tool registry, execution context
- Depends on: config, cache

**cache** — Caching infrastructure
- Types: LRU-TTL cache, request deduplication
- No domain dependencies

**config** — Configuration management
- Types: Config structures
- No dependencies

**git** — Git operations and worktree
- Functions: Git utilities
- Depends on: paths

**cli** — Command-line interface utilities
- Types: CLI command types
- No dependencies

**verification** — Quality verification gates
- Types: `VerificationGate`, gate criteria
- Depends on: state, context

**json_persistence** — JSON file persistence
- Functions: Serialization utilities
- No dependencies

**plan_mode** — Plan mode abstractions
- Types: Mode types and operations
- Depends on: state

**migration** — Schema and migration management
- Types: Migration types
- Depends on: state

**swebench** — SWE-Bench integration
- Types: Prediction, runner types
- No dependencies

## Context & Reasoning (4)

**context** — Prompt context and compression
- Types: Context budget, compressor
- Depends on: config, models

**thinking** — Reasoning graphs and metacognition
- Types: Reasoning structures
- Depends on: detection, context

**discovery** — Extension registry and discovery
- Types: Discovery types
- Depends on: config

**convoy** — Convoy plan execution
- Types: Convoy execution types
- Depends on: planning, execution

## Observability & Coordination (3)

**observability** — Telemetry and logging
- Types: Metrics, tracing types
- Depends on: models

**models** — Model cost tracking and routing
- Types: Cost, routing types
- Depends on: config

**coordinator** — Autonomous development coordination
- Types: Coordinator types
- Depends on: planning, execution, recovery, worktree, session

**session** — Session lifecycle management
- Types: Session types
- Depends on: state, config

## Fixture (1)

**fixture** — Test fixtures and mock data
- Types: Fixture builders
- No dependencies (test utilities)

## Module Organization Rules

### Public vs Private

- **Public types** are explicitly listed in module README
- **Public functions** are exported via `pub use` in `lib.rs`
- **Internal types** stay private within modules (not re-exported)
- **Submodule structure** is private (users interact with public API)

### Dependency Guidelines

1. **No cycles**: Dependencies flow downward through layers
2. **Explicit imports**: Avoid `use super::*` across modules
3. **Minimal exports**: Re-export only the public API
4. **Type definitions**: Put shared types in `error` or `state` modules

### Testing

Each module includes:
- Unit tests inline with `#[cfg(test)]`
- Integration tests in `tests/` directory
- Example usage in module README.md
- Property-based tests where applicable

## Common Patterns

### Adding a New Endpoint

1. Add type to relevant module (e.g., config, state)
2. Add handler to execution or service module
3. Wire through coordinator or service
4. Add verification gate
5. Update observability (metrics, tracing)

### Adding a New LLM Provider

1. Implement `Provider` trait in llm module
2. Register in llm provider registry
3. Add config in config module
4. Update llm/README.md with provider details

### Adding a New Tool

1. Define in tools module
2. Implement tool trait from `rustycode-tools-api`
3. Register in tool registry
4. Add security validation if needed
5. Update tools/README.md

## Dependency Graph

```
service
  ├── coordinator
  │   ├── planning
  │   │   ├── state
  │   │   ├── detection
  │   │   ├── models
  │   │   └── context
  │   ├── execution
  │   │   ├── tools
  │   │   ├── context
  │   │   └── recovery
  │   ├── recovery
  │   │   ├── cache
  │   │   ├── state
  │   │   └── verification
  │   ├── worktree
  │   │   └── git
  │   └── session
  │       └── state
  ├── planning
  └── execution

detection
  ├── state
  ├── phases
  └── models

context
  ├── config
  └── models

llm
  ├── config
  ├── models
  └── tools
```

## Verification & Health Checks

Run these regularly to maintain architecture integrity:

```bash
# Check compilation
cargo build -p rustycode-orchestration --all-features

# Verify no circular dependencies
cargo tree -p rustycode-orchestration

# Run all tests
cargo test -p rustycode-orchestration

# Check coverage
cargo tarpaulin -p rustycode-orchestration
```

---

**Last Updated:** 2026-04-22  
**Status:** S04 Complete - 27 modules, zero circular dependencies, 100% documented
