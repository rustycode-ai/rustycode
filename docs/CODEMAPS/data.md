# Data: Storage, Protocol Types, Session Management

<!-- Generated: 2026-05-14 | Files scanned: 1601 | Token estimate: ~500 -->

## Storage (rustycode-storage, 10K LOC)

SQLite-backed persistence via `rusqlite`.

```
storage/src/
├── lib.rs         # StorageEngine, migrations
└── (tables)
    ├── Sessions     # Session metadata
    ├── Messages     # Conversation history
    ├── Plans        # Execution plans
    ├── Milestones   # Milestone tracking
    ├── Events       # Event log
    └── Config       # Persisted configuration
```

**Key types:** `StorageEngine`, `SessionRecord`, `MessageRecord`

## Session (rustycode-session, 5.4K LOC)

Session lifecycle management with compression.

```
session/src/
├── lib.rs         # Session manager
├── session.rs     # Session state machine
└── snapshot.rs    # Session serialization (bincode + zstd compression)
```

## Protocol Types (rustycode-protocol, 21.6K LOC)

Cross-crate shared types — the lingua franca of the system.

### Core Domain Types

| Type | File | Purpose |
|------|------|---------|
| `Message`, `ToolCall`, `ToolResult` | `message.rs` | LLM conversation |
| `Plan`, `PlanStep`, `PlanDependency` | `plan.rs` | Execution plans |
| `Milestone`, `MilestoneStatus` | `milestone.rs` | Milestone tracking |
| `CodeSymbol`, `FileOutline` | `code_symbol.rs` | Code structure |
| `ContextSection`, `ContextPlan` | `context.rs` | Context management |
| `AgentOutcome` | `agent_outcome.rs` | Agent result |
| `BudgetAllocation` | `budget.rs` | Resource budgets |
| `WorkingMode` | `modes.rs` | CLI/TUI/Headless modes |
| `Intent` | `intent.rs` | User intent classification |

### Agent Protocol (`agent_protocol.rs`)

Structured messages for multi-agent orchestration:
- `AgentAction` — actions an agent can take
- `AgentMessage<T>` — generic agent message envelope
- `AgentRole` — Architect, Builder, Skeptic, Judge, Scalpel
- `ArchitectMessage`, `BuilderMessage`, `SkepticMessage`, `JudgeMessage`, `ScalpelMessage`
- `EscalationRequest`, `EscalationTarget`
- `StructuralDeclaration`, `ModuleDeclaration`, `InterfaceDeclaration`

### Event System (`event.rs`)

`Event` enum for EventBus pub/sub — session events, tool events, agent events.

## ID Generation (rustycode-id, 1.4K LOC)

Deterministic + random ID generation with `getrandom`.

## Tasks (rustycode-tasks, 2.2K LOC)

Task definition, status tracking, dependency management.
