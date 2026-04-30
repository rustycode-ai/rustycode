# RustyCode Architecture

RustyCode is an AI-powered coding assistant built with a "Rust-First" philosophy, prioritizing compile-time safety, zero-cost abstractions, and fearless concurrency. The system is split into focused crates to ensure the core runtime remains small, observable, and reusable.

## Core Philosophy: The Rust Way

1. **Compile-Time Guarantees**: Leverage the type system to encode invariants and prevent invalid states.
2. **Structured Concurrency**: Native `async/await` with `tokio` for efficient resource management and cancellation support.
3. **Zero-Cost Abstractions**: Preference for monomorphization and compile-time registries over runtime polymorphism.
4. **Fearless Ownership**: Explicit resource lifetimes and RAII for automatic cleanup.
5. **Ergonomic Error Handling**: Results over exceptions, with `anyhow` for applications and `thiserror` for libraries.

## Crate Architecture

| Crate | Responsibility |
|-------|----------------|
| `rustycode-protocol` | Shared DTOs and event types |
| `rustycode-bus` | Type-safe, asynchronous event bus |
| `rustycode-id` | Time-sortable, compact identifiers |
| `rustycode-tools` | Compile-time and runtime tool systems |
| `rustycode-config` | Layered configuration discovery |
| `rustycode-storage` | SQLite persistence for sessions and events |
| `rustycode-runtime` | Async orchestration and session management |
| `rustycode-core` | Core logic and context assembly |
| `rustycode-cli` | Terminal entrypoint |
| `rustycode-tui` | Terminal User Interface |

## Key Subsystems

- **Event Bus**: Decoupled communication between crates using trait-based events.
- **Tool System**: Type-safe tool definitions with compile-time validation for arguments.
- **Persistence**: Hybrid storage using SQLite and typed session events for complete observability.
- **Git & LSP**: First-class integration with version control and language servers.

## Principles

- Prefer precise, explainable context over broad file inclusion.
- Record major runtime decisions as typed session events.
- Treat Git, LSP, memory, and skills as first-class subsystems.
- Keep config and storage formats inspectable by users.

---

*For current architecture review and P0 issues, see [ARCHITECTURE-REVIEW-2026-04-20.md](ARCHITECTURE-REVIEW-2026-04-20.md).*
