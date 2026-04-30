# rustycode-memory

Short-term memory and context management for RustyCode sessions.

## Purpose

Manages contextual memory during development sessions. Stores observations, patterns, and decisions with confidence scoring. Provides ranking and relevance filtering to keep context focused and cost-effective, especially when used with limited context windows.

## Key Types

- `MemoryEntry` — A single memory with confidence score, domain, scope, and evidence
- `MemoryEntryConfig` — Builder for creating new memory entries
- `MemoryDomain` — Categorization (CodeStyle, Testing, Git, Debugging, Workflow, Architecture, ProjectSpecific)
- `MemoryScope` — Scope level (Project or Global)
- `MemorySource` — Origin (SessionObservation, UserExplicit, ProjectAnalysis, ManualEntry)
- `Observation` — Evidence/pattern that contributed to a memory with timestamp and confidence boost

## Public API

```rust
use rustycode_memory::{MemoryEntry, MemoryEntryConfig, MemoryDomain, MemoryScope, MemorySource};

// Create a memory entry
let config = MemoryEntryConfig {
    id: "user_prefers_tdd".to_string(),
    trigger: "when implementing features".to_string(),
    confidence: 0.8,
    domain: MemoryDomain::Testing,
    source: MemorySource::UserExplicit,
    scope: MemoryScope::Project,
    project_id: Some("rustycode".to_string()),
    action: "always write tests first".to_string(),
};

let mut entry = MemoryEntry::new(config);

// Boost confidence through repeated use
entry.boost_confidence(0.1);
entry.use_count += 1;
```

## Confidence Scoring

- Range: 0.3 (minimum) to 0.9 (maximum)
- Initialized based on source and evidence
- Increases through `boost_confidence()` when memory is used
- Used for ranking relevance in context-limited scenarios

## Domains

Memory entries are categorized by domain for filtering:
- **CodeStyle** — Code organization, naming conventions, patterns
- **Testing** — Test structure, coverage expectations, test patterns
- **Git** — Commit message format, workflow preferences
- **Debugging** — Common issues, error patterns, diagnostics
- **Workflow** — Development process preferences, tool usage
- **Architecture** — Design decisions, module boundaries
- **ProjectSpecific** — Custom project-level observations

## Dependencies

- `serde` — Serialization for persistence
- `anyhow` — Error handling
- `tracing` — Logging

## Architecture Notes

Memory is stored with timestamps and use counts to enable:
- Relevance ranking (high-confidence, frequently-used entries first)
- Decay-based aging (detect stale memories)
- Evidence tracking (why a memory was created)
- Scope isolation (project vs. global memories)

Observations are immutable once recorded, providing a persistent audit trail of how memories evolved.

## Testing

Tests verify confidence clamping, serialization round-trips, and evidence tracking.

## See Also

- `rustycode-tui-memory` — TUI integration for memory display
- `rustycode-vector-memory` — Vector-based semantic memory (HNSW)
- `rustycode-session` — Session lifecycle (memory lives during session)
