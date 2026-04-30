# Sortable ID System Design

## Overview

This document specifies the design for a time-sortable, compact ID system for RustyCode. The new IDs replace UUIDs while maintaining backward compatibility with the existing protocol.

## Motivation

### Current State (UUID v4)
- **Length**: 36 characters (e.g., `550e8400-e29b-41d4-a716-446655440000`)
- **Sortability**: Random - not time-ordered
- **Human readability**: Poor - not memorable or distinguishable
- **Storage**: Inefficient for indexing and sorting

### Desired State (Sortable IDs)
- **Length**: ~20-26 characters (compact Base62 encoding)
- **Sortability**: Time-ordered in both directions (asc/desc)
- **Human readability**: Good - recognizable prefixes, sortable timestamps
- **Storage**: Efficient for indexing and natural sorting

## ID Format Specification

### Structure

```
[PREFIX][TIMESTAMP][RANDOMNESS]
```

**Components:**
1. **Prefix** (2-5 chars): Entity type identifier
2. **Timestamp** (10-12 chars): Milliseconds since Unix epoch in Base62
3. **Randomness** (8-12 chars): Collision resistance

### Base62 Character Set

```
0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz
```

- Ordered: 0-9, A-Z, a-z
- No special characters (URL-safe, filesystem-safe)
- Case-sensitive but visually distinct

### Timestamp Encoding

**Unix epoch milliseconds in Base62:**
- Current timestamp (2026): ~1.7B milliseconds
- Base62 encoding: ~10 characters
- Precision: Millisecond-level ordering
- Range: Supports years 1970-2286 (comfortable headroom)

**Example encoding:**
```
1704067200000 (2024-01-01 00:00:00 UTC)
→ Base62: "1a2b3c4d5e"
```

### Randomness Component

- **Length**: 8-12 characters Base62
- **Entropy**: ~48-72 bits (sufficient for collision resistance)
- **Distribution**: Cryptographically secure random
- **Purpose**: Prevent collisions within same millisecond

## Prefix Registry

| Prefix | Entity Type        | Example ID              |
|--------|-------------------|-------------------------|
| `sess_` | Session           | `sess_1a2b3c4d5eXyZ123` |
| `evt_`  | Event             | `evt_1a2b3c4d5eAbC987`  |
| `mem_`  | Memory Entry      | `mem_1a2b3c4d5eDeF456`  |
| `skl_`  | Skill             | `skl_1a2b3c4d5eFeD789`  |
| `ctx_`  | Context Section   | `ctx_1a2b3c4d5eCaB321`  |
| `run_`  | Execution Run     | `run_1a2b3c4d5eBaC654`  |
| `msg_`  | Message/Turn      | `msg_1a2b3c4d5eAbC987`  |

**Prefix rules:**
- 2-5 alphanumeric characters
- Must end with underscore (`_`) for readability
- Lowercase preferred (but case-sensitive for sorting)
- Registered in central prefix registry

## ID Length Comparison

| ID Type          | Format           | Length | Example                              |
|------------------|------------------|--------|--------------------------------------|
| UUID v4          | 8-4-4-4-12 hex   | 36     | `550e8400-e29b-41d4-a716-446655440000` |
| Sortable (short) | prefix+10+8      | 20     | `sess_1a2b3c4d5eXyZ12`               |
| Sortable (long)  | prefix+12+12     | 26     | `sess_1a2b3c4d5e6fXyZ123AbCd`        |
| **Savings**:     |                  | **28%** |                                      |

## Core Type Definitions

### New Crate: `rustycode-id`

**Location:** `/crates/rustycode-id/`

**Dependencies:**
- `serde` (optional)
- `chrono` (for timestamp conversion)
- `rand` (for randomness)
- `thiserror` (for error types)

### Core Types

```rust
// crates/rustycode-id/src/lib.rs
use chrono::{DateTime, Utc};
use rand::Rng;
use serde::{Deserialize, Serialize};
use std::fmt;

/// Error types for ID operations
#[derive(Debug, thiserror::Error)]
pub enum IdError {
    #[error("invalid ID format: {0}")]
    InvalidFormat(String),

    #[error("unknown prefix: {0}")]
    UnknownPrefix(String),

    #[error("timestamp conversion failed: {0}")]
    TimestampError(String),
}

/// Prefix identifier for entity types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Prefix {
    Session,
    Event,
    Memory,
    Skill,
    Context,
    Run,
    Message,
    Custom(&'static str),
}

impl Prefix {
    pub fn as_str(&self) -> &str {
        match self {
            Prefix::Session => "sess_",
            Prefix::Event => "evt_",
            Prefix::Memory => "mem_",
            Prefix::Skill => "skl_",
            Prefix::Context => "ctx_",
            Prefix::Run => "run_",
            Prefix::Message => "msg_",
            Prefix::Custom(s) => s,
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "sess_" => Some(Prefix::Session),
            "evt_" => Some(Prefix::Event),
            "mem_" => Some(Prefix::Memory),
            "skl_" => Some(Prefix::Skill),
            "ctx_" => Some(Prefix::Context),
            "run_" => Some(Prefix::Run),
            "msg_" => Some(Prefix::Message),
            _ => Some(Prefix::Custom(s)), // Accept unknown prefixes
        }
    }
}

/// Core sortable ID type
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SortableId {
    raw: String,
    prefix_len: usize,
}

impl SortableId {
    /// Generate a new sortable ID with given prefix
    pub fn new(prefix: Prefix) -> Self {
        let timestamp_ms = Utc::now().timestamp_millis();
        let timestamp_encoded = encode_base62(timestamp_ms as u64);

        // 12 chars of randomness (72 bits entropy)
        let randomness: u64 = rand::thread_rng().gen();
        let random_encoded = encode_base62(randomness);

        let raw = format!("{}{}{}", prefix.as_str(), timestamp_encoded, random_encoded);
        let prefix_len = prefix.as_str().len();

        Self { raw, prefix_len }
    }

    /// Parse from string
    pub fn parse(s: impl AsRef<str>) -> Result<Self, IdError> {
        let s = s.as_ref();

        // Validate minimum length
        if s.len() < 12 {
            return Err(IdError::InvalidFormat(
                "ID too short".to_string()
            ));
        }

        // Extract prefix (up to and including underscore)
        let prefix_end = s.find('_')
            .ok_or_else(|| IdError::InvalidFormat("missing prefix underscore".into()))?;

        let prefix_len = prefix_end + 1;

        // Validate Base62 characters after prefix
        let body = &s[prefix_len..];
        if !body.chars().all(|c| is_base62(c)) {
            return Err(IdError::InvalidFormat(
                "invalid Base62 characters".to_string()
            ));
        }

        Ok(Self {
            raw: s.to_string(),
            prefix_len,
        })
    }

    /// Get the prefix component
    pub fn prefix(&self) -> &str {
        &self.raw[..self.prefix_len]
    }

    /// Get the timestamp component (as encoded string)
    pub fn timestamp_encoded(&self) -> &str {
        let start = self.prefix_len;
        &self.raw[start..start+10] // First 10 chars after prefix
    }

    /// Get the randomness component
    pub fn randomness(&self) -> &str {
        let start = self.prefix_len + 10;
        &self.raw[start..]
    }

    /// Extract timestamp as DateTime
    pub fn timestamp(&self) -> Result<DateTime<Utc>, IdError> {
        let encoded = self.timestamp_encoded();
        let timestamp_ms = decode_base62(encoded)
            .map_err(|e| IdError::TimestampError(e.to_string()))?;

        DateTime::from_timestamp_millis(timestamp_ms as i64)
            .ok_or_else(|| IdError::TimestampError("invalid timestamp".to_string()))
    }

    /// Check if this ID comes before another (by time)
    pub fn is_before(&self, other: &Self) -> bool {
        self.timestamp_encoded() < other.timestamp_encoded()
    }

    /// Check if this ID comes after another (by time)
    pub fn is_after(&self, other: &Self) -> bool {
        self.timestamp_encoded() > other.timestamp_encoded()
    }

    /// Get raw string representation
    pub fn as_str(&self) -> &str {
        &self.raw
    }
}

impl fmt::Display for SortableId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.raw)
    }
}

impl From<SortableId> for String {
    fn from(id: SortableId) -> Self {
        id.raw
    }
}

impl<'a> TryFrom<&'a str> for SortableId {
    type Error = IdError;

    fn try_from(s: &'a str) -> Result<Self, Self::Error> {
        Self::parse(s)
    }
}

// Serialization support (optional feature)
#[cfg(feature = "serde")]
impl Serialize for SortableId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.raw)
    }
}

#[cfg(feature = "serde")]
impl<'de> Deserialize<'de> for SortableId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        SortableId::parse(s).map_err(serde::de::Error::custom)
    }
}

/// Base62 encoding utilities
const BASE62_CHARS: &[u8; 62] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";

fn encode_base62(mut value: u64) -> String {
    if value == 0 {
        return "0".to_string();
    }

    let mut buf = [0u8; 12]; // Max 12 chars for u64
    let mut pos = buf.len();

    while value > 0 {
        pos -= 1;
        buf[pos] = BASE62_CHARS[(value % 62) as usize];
        value /= 62;
    }

    std::str::from_utf8(&buf[pos..]).unwrap().to_string()
}

fn decode_base62(s: &str) -> Result<u64, Box<dyn std::error::Error>> {
    let mut value = 0u64;

    for byte in s.bytes() {
        let digit = if byte.is_ascii_digit() {
            byte - b'0'
        } else if byte.is_ascii_uppercase() {
            byte - b'A' + 10
        } else if byte.is_ascii_lowercase() {
            byte - b'a' + 36
        } else {
            return Err("invalid Base62 character".into());
        };

        value = value * 62 + (digit as u64);
    }

    Ok(value)
}

fn is_base62(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_' || c == '-'
}

// Strongly-typed ID wrappers
pub type SessionId = TypedId<Prefix::Session>;
pub type EventId = TypedId<Prefix::Event>;
pub type MemoryId = TypedId<Prefix::Memory>;
pub type SkillId = TypedId<Prefix::Skill>;
pub type ContextId = TypedId<Prefix::Context>;
pub type RunId = TypedId<Prefix::Run>;
pub type MessageId = TypedId<Prefix::Message>;

/// Typed ID wrapper for compile-time prefix enforcement
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TypedId<const PREFIX: Prefix> {
    inner: SortableId,
}

impl<const PREFIX: Prefix> TypedId<PREFIX> {
    pub fn new() -> Self {
        Self {
            inner: SortableId::new(PREFIX),
        }
    }

    pub fn parse(s: impl AsRef<str>) -> Result<Self, IdError> {
        let id = SortableId::parse(s)?;

        // Validate prefix matches
        if id.prefix() != PREFIX.as_str() {
            return Err(IdError::InvalidFormat(format!(
                "expected prefix {}, got {}",
                PREFIX.as_str(),
                id.prefix()
            )));
        }

        Ok(Self { inner: id })
    }

    pub fn as_str(&self) -> &str {
        self.inner.as_str()
    }

    pub fn timestamp(&self) -> Result<DateTime<Utc>, IdError> {
        self.inner.timestamp()
    }

    pub fn into_inner(self) -> SortableId {
        self.inner
    }
}

impl<const PREFIX: Prefix> fmt::Display for TypedId<PREFIX> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.inner)
    }
}

impl<const PREFIX: Prefix> Default for TypedId<PREFIX> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(feature = "serde")]
impl<const PREFIX: Prefix> Serialize for TypedId<PREFIX> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.inner.serialize(serializer)
    }
}

#[cfg(feature = "serde")]
impl<'de, const PREFIX: Prefix> Deserialize<'de> for TypedId<PREFIX> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let id = SortableId::deserialize(deserializer)?;

        if id.prefix() != PREFIX.as_str() {
            return Err(serde::de::Error::custom(format!(
                "expected prefix {}, got {}",
                PREFIX.as_str(),
                id.prefix()
            )));
        }

        Ok(Self { inner: id })
    }
}
```

## API Design

### Core Operations

```rust
use rustycode_id::{SortableId, SessionId, EventId, Prefix};

// Generate new IDs
let session_id = SessionId::new();
// => "sess_1a2b3c4d5eXyZ123AbC"

let event_id = EventId::new();
// => "evt_1a2b3c4d5fXyZ456DeF"

// Parse from string
let id = SortableId::parse("sess_1a2b3c4d5eXyZ123")?;
let typed_id = SessionId::parse("sess_1a2b3c4d5eXyZ123")?;

// Extract components
let prefix = id.prefix();           // => "sess_"
let timestamp = id.timestamp()?;    // => 2024-01-01T12:00:00Z
let encoded_ts = id.timestamp_encoded(); // => "1a2b3c4d5e"
let random = id.randomness();        // => "XyZ123"

// Comparison
let id1 = SessionId::new();
let id2 = SessionId::new();
assert!(id1.is_before(&id2));

// Convert to string
let s: String = id.as_str().to_string();
let s: String = id.into();

// Display
println!("Session: {}", session_id);
```

### Time-Based Queries

```rust
use chrono::{Utc, Duration};

let session_id = SessionId::new();
let created = session_id.timestamp()?;

// Time range queries
let yesterday = Utc::now() - Duration::days(1);
let is_recent = session_id.timestamp()? > yesterday;

// Sorting
let mut ids = vec![
    SessionId::new(),
    SessionId::new(),
    SessionId::new(),
];
ids.sort(); // Sorts by timestamp (natural string sort)
ids.sort_by(|a, b| b.timestamp().cmp(&a.timestamp())); // Reverse
```

### Prefix Filtering

```rust
use rustycode_id::SortableId;

let ids: Vec<SortableId> = vec![
    // ... various IDs
];

// Filter by prefix
let sessions: Vec<_> = ids
    .iter()
    .filter(|id| id.prefix() == "sess_")
    .collect();

let events: Vec<_> = ids
    .iter()
    .filter(|id| id.prefix() == "evt_")
    .collect();
```

## Migration Strategy

### Phase 1: Add New Crate (Non-Breaking)

1. **Add `rustycode-id` crate**
   - No changes to existing code
   - New crate is completely independent

2. **Update workspace dependencies**
   ```toml
   [workspace.dependencies]
   rustycode-id = { path = "../rustycode-id" }
   ```

### Phase 2: Internal Migration (Non-Breaking)

1. **Update `rustycode-protocol`**
   - Add new types alongside existing UUID types
   - Keep UUID types for backward compatibility

   ```rust
   // crates/rustycode-protocol/src/lib.rs
   use rustycode_id::{SessionId as SortableSessionId, EventId as SortableEventId};
   use uuid::Uuid;

   // Legacy type (keep for backward compatibility)
   #[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
   pub struct SessionIdLegacy(pub Uuid);

   // New type (internal use)
   pub type SessionId = SortableSessionId;
   pub type EventId = SortableEventId;
   ```

2. **Update `rustycode-storage`**
   - Store sortable IDs as TEXT (same as UUID strings)
   - No schema changes needed (both are TEXT)
   - Add migration helper for existing data

   ```rust
   // crates/rustycode-storage/src/lib.rs
   impl Storage {
       // Migrate existing UUID sessions to sortable IDs
       pub fn migrate_session_ids(&self) -> Result<usize> {
           // Fetch all sessions with UUIDs
           // Generate new sortable IDs with same timestamp
           // Update in place
           // Return count of migrated sessions
       }
   }
   ```

### Phase 3: External API Compatibility

**Option A: Dual Protocol Support**
- Accept both UUID and sortable ID formats in API
- Convert internally to sortable IDs
- Serialize back to format client expects

```rust
// Accept both formats
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum SessionIdCompat {
    Legacy(Uuid),
    Modern(SortableId),
}

impl From<SessionIdCompat> for SortableId {
    fn from(id: SessionIdCompat) -> Self {
        match id {
            SessionIdCompat::Legacy(uuid) => {
                // Convert UUID to sortable ID using UUID timestamp
                // (UUID v7 would be ideal, but v4 has no timestamp)
                // Fall back to current time with warning
                SortableId::new(Prefix::Session)
            }
            SessionIdCompat::Modern(id) => id,
        }
    }
}
```

**Option B: Accept New Format Only (Breaking Change)**
- Version 2.0 of protocol
- Clear migration path documented
- Migration tool provided

### Phase 4: Deprecate UUID Types

1. **Soft deprecation**
   - Mark UUID types as `#[deprecated]`
   - Add compiler warnings
   - Documentation encourages migration

2. **Hard removal**
   - Major version bump
   - Remove legacy types
   - Clean up conversion code

## Storage Implementation

### SQLite Schema

**Current schema (unchanged):**
```sql
CREATE TABLE sessions (
    id TEXT PRIMARY KEY,        -- Works for both UUID and sortable ID
    task TEXT NOT NULL,
    created_at TEXT NOT NULL
);

CREATE TABLE events (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id TEXT NOT NULL,   -- Works for both UUID and sortable ID
    at TEXT NOT NULL,
    kind TEXT NOT NULL,
    detail TEXT NOT NULL
);
```

**Benefits of sortable IDs for storage:**
- Natural string sort = time sort
- No separate timestamp index needed for ID-based queries
- Prefix-based filtering without joins
- Smaller storage footprint (20 vs 36 bytes)

**Indexing optimization:**
```sql
-- Primary key is automatically indexed
-- Sorting by ID is sorting by time (no extra index needed)
SELECT * FROM sessions ORDER BY id DESC; -- Newest first

-- Prefix filtering
SELECT * FROM sessions WHERE id LIKE 'sess_%';
```

## Performance Analysis

### Encoding Performance

**Benchmark results (estimated):**
- **UUID generation**: ~50ns (single syscall to getrandom)
- **Sortable ID generation**: ~200ns (timestamp + encoding + randomness)
- **Sortable ID parsing**: ~100ns (string validation + base62 decode)
- **Comparison**: O(1) string comparison (same as UUID)

**Acceptable tradeoffs:**
- 4x slower generation (negligible in practice)
- Same comparison speed (critical for sorting)
- Faster queries (natural sort vs timestamp joins)

### Storage Performance

**Space savings:**
- UUID: 36 bytes per ID
- Sortable ID: 20-26 bytes per ID
- **Savings**: 28-44% per ID

**Impact on 1M sessions:**
- UUID storage: 36 MB
- Sortable ID storage: 20 MB
- **Savings**: 16 MB (44% reduction)

**Query performance:**
- Time-based queries: No separate timestamp sort needed
- Prefix queries: Simple LIKE filter (indexed via LIKE optimization)
- Range queries: Natural string range = time range

## Example Usage

### Complete Session Lifecycle

```rust
use chrono::Utc;
use rustycode_id::{SessionId, EventId};
use rustycode_protocol::{Session, SessionEvent, EventKind};

// Create session with sortable ID
let session = Session {
    id: SessionId::new(),
    task: "inspect workspace".to_string(),
    created_at: Utc::now(),
};

println!("Session ID: {}", session.id);
// => "sess_1a2b3c4d5eXyZ123AbC"

// Create event linked to session
let event = SessionEvent {
    session_id: session.id.clone(),
    at: Utc::now(),
    kind: EventKind::SessionStarted,
    detail: "task=inspect workspace".to_string(),
};

// Store in database
storage.insert_session(&session)?;
storage.insert_event(&event)?;

// Query latest sessions (natural sort)
let recent_sessions = storage.query_sessions(
    "SELECT * FROM sessions ORDER BY id DESC LIMIT 10"
)?;

// Extract timestamp from ID
let created = session.id.timestamp()?;
println!("Session created at: {}", created);
// => "2024-03-12 16:30:45 UTC"
```

### Time Range Queries

```rust
use chrono::{Utc, Duration};
use rustycode_id::SessionId;

// Generate time-based ID for range queries
let now = Utc::now();
let hour_ago = now - Duration::hours(1);

// Create boundary IDs for efficient range queries
let start_id = SessionId::parse(&format!(
    "sess_{}",
    encode_base62(hour_ago.timestamp_millis() as u64)
))?;

let end_id = SessionId::parse(&format!(
    "sess_{}",
    encode_base62(now.timestamp_millis() as u64)
))?;

// Query: SELECT * FROM sessions WHERE id >= ? AND id <= ?
let sessions = storage.query_sessions_by_range(&start_id, &end_id)?;
```

## Backward Compatibility

### Protocol Version 1.x (Current)

```rust
// Public API still uses UUID
pub use uuid::Uuid;

#[derive(Clone)]
pub struct SessionId(pub Uuid);

impl SessionId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}
```

### Protocol Version 2.0 (Future)

```rust
// Public API uses sortable IDs
pub use rustycode_id::SessionId;

// Internal conversion from old format
impl From<Uuid> for SessionId {
    fn from(_uuid: Uuid) -> Self {
        // Generate new sortable ID
        // Log migration event
        Self::new()
    }
}

// Serialization accepts both formats
impl Serialize for SessionId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}
```

## Testing Strategy

### Unit Tests

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    #[test]
    fn test_id_generation() {
        let id1 = SessionId::new();
        let id2 = SessionId::new();

        assert_eq!(id1.prefix(), "sess_");
        assert_eq!(id2.prefix(), "sess_");
        assert_ne!(id1.as_str(), id2.as_str()); // Unique
    }

    #[test]
    fn test_id_sorting() {
        std::thread::sleep(std::time::Duration::from_millis(10));

        let id1 = SessionId::new();
        let id2 = SessionId::new();

        assert!(id1.is_before(&id2));
        assert!(id2.is_after(&id1));
    }

    #[test]
    fn test_id_parsing() {
        let id = SessionId::new();
        let s = id.as_str();

        let parsed = SessionId::parse(s).unwrap();
        assert_eq!(parsed.as_str(), s);
    }

    #[test]
    fn test_timestamp_extraction() {
        let before = Utc::now();
        let id = SessionId::new();
        let after = Utc::now();

        let timestamp = id.timestamp().unwrap();
        assert!(timestamp >= before && timestamp <= after);
    }

    #[test]
    fn test_invalid_prefix() {
        let result = SessionId::parse("evt_1a2b3c4d5eXyZ");
        assert!(result.is_err());
    }

    #[test]
    fn test_base62_encoding_roundtrip() {
        let values = vec![0, 1, 61, 62, 3844, u64::MAX];

        for value in values {
            let encoded = encode_base62(value);
            let decoded = decode_base62(&encoded).unwrap();
            assert_eq!(decoded, value);
        }
    }
}
```

### Integration Tests

```rust
#[test]
fn test_storage_with_sortable_ids() {
    let storage = Storage::open(&temp_db_path()).unwrap();

    let session = Session {
        id: SessionId::new(),
        task: "test".to_string(),
        created_at: Utc::now(),
    };

    storage.insert_session(&session).unwrap();

    // Query by ID
    let retrieved = storage.get_session(session.id.as_str()).unwrap();
    assert_eq!(retrieved.id.as_str(), session.id.as_str());
}
```

### Performance Tests

```rust
#[bench]
fn bench_id_generation(b: &mut Bencher) {
    b.iter(|| {
        SessionId::new();
    });
}

#[bench]
fn bench_id_parsing(b: &mut Bencher) {
    let id = SessionId::new();
    let s = id.as_str();

    b.iter(|| {
        SessionId::parse(s).unwrap();
    });
}

#[bench]
fn bench_id_comparison(b: &mut Bencher) {
    let id1 = SessionId::new();
    let id2 = SessionId::new();

    b.iter(|| {
        id1.is_before(&id2);
    });
}
```

## Implementation Checklist

- [ ] Create `crates/rustycode-id/Cargo.toml`
- [ ] Implement core `SortableId` type
- [ ] Implement Base62 encoding/decoding
- [ ] Implement typed ID wrappers
- [ ] Add comprehensive unit tests
- [ ] Add benchmarks
- [ ] Update workspace `Cargo.toml`
- [ ] Add `rustycode-id` to `rustycode-protocol` dependencies
- [ ] Add migration helpers to `rustycode-storage`
- [ ] Update documentation
- [ ] Create migration guide
- [ ] Run integration tests
- [ ] Performance validation

## Future Enhancements

### UUID v7 Compatibility

Future versions could align with UUID v7 spec:
- UUID v7 is time-sortable (similar design)
- Standard format (128-bit)
- Better tooling support

```rust
// Potential future enhancement
pub struct UuidV7 {
    bytes: [u8; 16],
}

impl From<SortableId> for UuidV7 {
    fn from(id: SortableId) -> Self {
        // Convert sortable ID to UUID v7 format
    }
}
```

### Distributed ID Generation

For distributed systems, consider:
- Machine ID component (instead of pure randomness)
- Sequence counter for high-throughput scenarios
- Snowflake-like architecture

```rust
pub struct DistributedSortableId {
    prefix: String,
    timestamp_ms: u64,
    machine_id: u16,
    sequence: u16,
}
```

### Custom Prefixes

Allow user-defined prefixes:
```rust
let custom_id = SortableId::with_prefix("custom_");
```

## References

- **Base62 Encoding**: Standard character set for URL-safe encoding
- **UUID v7**: RFC 4122 (draft) - Time-ordered UUIDs
- **Snowflake IDs**: Twitter's distributed ID generation
- **ULID**: Universally Unique Lexicographically Sortable Identifier
- **KSUID**: K-Sortable Unique ID (segment.io)

## Conclusion

The sortable ID system provides:
- **28-44% space savings** over UUIDs
- **Time-based sorting** without extra indexes
- **Human-readable prefixes** for easy identification
- **Collision resistance** with cryptographic randomness
- **Backward compatibility** through gradual migration
- **Zero schema changes** in storage layer

The design prioritizes practical efficiency over theoretical optimality, making it ideal for RustyCode's session management, event tracking, and long-term storage needs.
