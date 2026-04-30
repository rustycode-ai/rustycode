# rustycode-id

Sortable ID generation and management system.

## Purpose

Provides implementation of RustyCode's sortable ID system. Generates human-readable, time-sortable, collision-resistant identifiers for all entities. IDs can be sorted chronologically and include meaningful prefixes.

## Key Types

- `SortableId` — Generic sortable ID with custom prefix
- `IdGenerator` — Generates IDs for a given prefix
- `IdParser` — Parses and validates IDs

## ID Format

```
prefix_timestamp_random

Example: sess_2026042210305412_k9zxP5qM
├─ Prefix: sess (entity type)
├─ Timestamp: 2026042210305412 (microseconds since epoch)
└─ Random: k9zxP5qM (8 chars base32, prevents collisions)
```

## Public API

```rust
use rustycode_id::{SortableId, IdGenerator};

// Generate ID with custom prefix
let id = SortableId::new("task").to_string();
println!("{}", id);  // task_2026042210305412_k9zxP5qM

// Create generator for a prefix
let gen = IdGenerator::new("evt");
let evt1 = gen.generate();
let evt2 = gen.generate();
assert!(evt1 < evt2);  // IDs are sortable

// Parse and extract info
let parsed = SortableId::parse("sess_2026042210305412_k9zxP5qM")?;
println!("Prefix: {}", parsed.prefix());
println!("Timestamp: {:?}", parsed.timestamp());
```

## Properties

- **Sortable** — IDs from earlier times sort before later times
- **Human-readable** — Prefix indicates entity type
- **Collision-resistant** — Random component prevents duplicates
- **Compact** — 27-32 chars vs 36 for UUID
- **Fast** — No network/database lookup needed

## Built-in Prefixes

- `sess_` — Session ID
- `plan_` — Plan ID
- `evt_` — Event ID
- `mem_` — Memory entry ID
- `skl_` — Skill ID
- `tool_` — Tool ID
- `file_` — File ID

## Implementation Details

Timestamp uses microsecond precision. Random component is 8 characters of base32 (40 bits of entropy).

Collision probability very low due to:
- Random component (40 bits)
- Microsecond timestamp (unlikely to repeat)

## Dependencies

- `chrono` — Timestamp generation
- `rand` — Random component
- `base32` — Encoding
- `serde` — Serialization
- `anyhow` — Error handling

## Architecture Notes

IDs are generated locally without network calls. Sortability comes from timestamp ordering.

Base32 encoding chosen for readability (avoids confusing characters like 0/O, I/l/1).

## Testing

Tests verify ID generation, parsing, sorting properties, and collision resistance.

## See Also

- `rustycode-protocol` — Uses SortableId extensively
- All crates — ID generation throughout
