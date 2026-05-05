#![cfg_attr(test, allow(clippy::unwrap_used))]
#![allow(
    clippy::expect_used,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap
)]

// RustyCode Sortable ID System
//
// Implements time-sortable, compact identifiers using Base62 encoding.
// Format: [PREFIX][TIMESTAMP][RANDOMNESS]
// Example: sess_3w8qN5zX2yK9bF8pD3m
//
// Components:
// - PREFIX: 4-5 chars ending in '_' (sess_, evt_, mem_, skl_, etc.)
// - TIMESTAMP: 10 chars Base62 (milliseconds since epoch)
// - RANDOMNESS: 1-15 chars Base62 (prevents collisions)

use chrono::{DateTime, Utc};
use getrandom::fill as getrandom_sys;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;
use std::str::FromStr;
use thiserror::Error;

/// Base62 character set (0-9, A-Z, a-z)
const BASE62_CHARS: &[u8; 62] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";

/// Errors that can occur during ID operations
#[derive(Error, Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum IdError {
    #[error("Invalid ID format: {0}")]
    InvalidFormat(String),

    #[error("Invalid prefix: expected {expected}, got {found}")]
    InvalidPrefix { expected: String, found: String },

    #[error("Invalid timestamp: {0}")]
    InvalidTimestamp(String),

    #[error("Invalid Base62 encoding: {0}")]
    InvalidBase62(String),

    #[error("ID too short: {length} chars (minimum: {min})")]
    TooShort { length: usize, min: usize },

    #[error("ID too long: {length} chars (maximum: {max})")]
    TooLong { length: usize, max: usize },
}

/// Encode a u64 value to Base62 string
fn encode_base62(mut value: u64) -> String {
    if value == 0 {
        return String::from("0");
    }

    let mut chars = vec![];
    while value > 0 {
        chars.push(BASE62_CHARS[(value % 62) as usize]);
        value /= 62;
    }
    chars.reverse();
    // BASE62_CHARS contains only ASCII bytes, which are always valid UTF-8
    String::from_utf8(chars).expect("base62 chars are always valid UTF-8")
}

/// Decode a Base62 string to u64
fn decode_base62(s: &str) -> Result<u64, IdError> {
    if s.is_empty() {
        return Err(IdError::InvalidBase62("empty string".to_string()));
    }

    let mut result: u64 = 0;
    for (i, c) in s.chars().enumerate() {
        let digit = BASE62_CHARS
            .iter()
            .position(|&ch| ch as char == c)
            .ok_or_else(|| {
                IdError::InvalidBase62(format!("invalid character '{c}' at position {i}"))
            })?;

        result = result
            .checked_mul(62)
            .and_then(|value| value.checked_add(digit as u64))
            .ok_or_else(|| IdError::InvalidBase62(format!("value too large at position {i}")))?;
    }

    Ok(result)
}

/// Internal sortable ID representation
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SortableId {
    /// The prefix (e.g., "sess_", "evt_")
    prefix: String,
    /// Timestamp in milliseconds since Unix epoch
    timestamp_ms: u64,
    /// Random component to prevent collisions
    random: u64,
}

impl SortableId {
    /// Minimum ID length: prefix (4) + timestamp (10) + random (1)
    const MIN_LENGTH: usize = 15;
    /// Maximum ID length: prefix (5) + timestamp (10) + random (15)
    const MAX_LENGTH: usize = 30;

    pub fn new(prefix: impl Into<String>) -> Self {
        let prefix = prefix.into();

        // Validate prefix format (panics in dev, but that's OK for now)
        assert!(
            prefix.len() >= 4,
            "Prefix must be at least 4 characters, got: {prefix} ({})",
            prefix.len()
        );
        assert!(
            prefix.len() <= 5,
            "Prefix must be at most 5 characters, got: {prefix} ({})",
            prefix.len()
        );
        assert!(
            prefix.ends_with('_'),
            "Prefix must end with '_', got: {prefix}"
        );

        // Get current timestamp in milliseconds
        let now = Utc::now();
        let timestamp_ms = now.timestamp_millis() as u64;

        // Generate random bytes for uniqueness
        let mut random_bytes = [0u8; 8];
        getrandom_sys(&mut random_bytes).expect("getrandom failed");
        let random = u64::from_be_bytes(random_bytes);

        Self {
            prefix,
            timestamp_ms,
            random,
        }
    }

    /// Create a `SortableId` from components
    pub fn from_components(prefix: impl Into<String>, timestamp_ms: u64, random: u64) -> Self {
        let prefix = prefix.into();

        // Validate prefix format
        assert!(
            prefix.len() >= 4,
            "Prefix must be at least 4 characters, got: {prefix} ({})",
            prefix.len()
        );
        assert!(
            prefix.len() <= 5,
            "Prefix must be at most 5 characters, got: {prefix} ({})",
            prefix.len()
        );
        assert!(
            prefix.ends_with('_'),
            "Prefix must end with '_', got: {prefix}"
        );

        Self {
            prefix,
            timestamp_ms,
            random,
        }
    }

    /// Parse a `SortableId` from a string
    pub fn parse(s: impl AsRef<str>) -> Result<Self, IdError> {
        let s = s.as_ref();

        // Validate length
        if s.len() < Self::MIN_LENGTH {
            return Err(IdError::TooShort {
                length: s.len(),
                min: Self::MIN_LENGTH,
            });
        }
        if s.len() > Self::MAX_LENGTH {
            return Err(IdError::TooLong {
                length: s.len(),
                max: Self::MAX_LENGTH,
            });
        }

        // Find underscore to determine prefix length
        let underscore_pos = s
            .find('_')
            .ok_or_else(|| IdError::InvalidFormat("no underscore found in prefix".to_string()))?;

        let prefix = &s[..=underscore_pos];
        if !prefix.ends_with('_') {
            return Err(IdError::InvalidFormat(format!(
                "prefix must end with '_', got: {prefix}"
            )));
        }

        // Extract timestamp (next 10 chars after prefix)
        let timestamp_start = underscore_pos + 1;
        let timestamp_end = timestamp_start + 10;

        if timestamp_end > s.len() {
            return Err(IdError::InvalidFormat(
                "ID too short for timestamp".to_string(),
            ));
        }

        let timestamp_str = &s[timestamp_start..timestamp_end];

        // Validate timestamp before decoding to catch overflow early
        if timestamp_str.len() > 10 {
            return Err(IdError::InvalidTimestamp(format!(
                "timestamp too long: {} chars",
                timestamp_str.len()
            )));
        }

        let timestamp_ms = decode_base62(timestamp_str)
            .map_err(|e| IdError::InvalidTimestamp(format!("{timestamp_str}: {e}")))?;

        // Extract random component (remaining chars)
        let random_str = &s[timestamp_end..];

        // Validate random length before decoding
        if random_str.len() > 15 {
            return Err(IdError::InvalidBase62(format!(
                "random component too long: {} chars",
                random_str.len()
            )));
        }

        let random = decode_base62(random_str)
            .map_err(|e| IdError::InvalidBase62(format!("in random component: {e}")))?;

        Ok(Self {
            prefix: prefix.to_string(),
            timestamp_ms,
            random,
        })
    }

    /// Get the prefix
    pub fn prefix(&self) -> &str {
        &self.prefix
    }

    /// Get the timestamp as milliseconds since Unix epoch
    pub const fn timestamp_ms(&self) -> u64 {
        self.timestamp_ms
    }

    /// Get the timestamp as a `DateTime<Utc>`
    pub fn timestamp(&self) -> DateTime<Utc> {
        DateTime::from_timestamp(self.timestamp_ms as i64 / 1000, 0).unwrap_or_else(|| {
            DateTime::from_timestamp(0, 0).expect("unix epoch timestamp is always valid")
        })
    }

    /// Get the random component
    pub const fn random(&self) -> u64 {
        self.random
    }

    /// Convert to string representation
    fn to_string_internal(&self) -> String {
        let timestamp_encoded = encode_base62(self.timestamp_ms);
        let random_encoded = encode_base62(self.random);

        // Pad timestamp to exactly 10 chars by left-padding with '0'
        let timestamp_padded = if timestamp_encoded.len() >= 10 {
            timestamp_encoded
        } else {
            format!("{timestamp_encoded:0>10}")
        };

        format!("{}{}{}", self.prefix, timestamp_padded, random_encoded)
    }
}

impl fmt::Display for SortableId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_string_internal())
    }
}

impl Serialize for SortableId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for SortableId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Self::parse(&s).map_err(serde::de::Error::custom)
    }
}

// Type-Safe ID Wrappers

macro_rules! define_id_wrapper {
    ($name:ident, $prefix:expr) => {
        #[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub struct $name(SortableId);

        impl $name {
            pub fn new() -> Self {
                Self(SortableId::new($prefix))
            }

            /// Parse from string
            pub fn parse(s: impl AsRef<str>) -> Result<Self, IdError> {
                let s = s.as_ref();
                let candidate_prefix = s.find('_').map(|index| &s[..=index]).ok_or_else(|| {
                    IdError::InvalidFormat("no underscore found in prefix".to_string())
                })?;
                if candidate_prefix != $prefix {
                    return Err(IdError::InvalidPrefix {
                        expected: $prefix.to_string(),
                        found: candidate_prefix.to_string(),
                    });
                }
                let id = SortableId::parse(s)?;
                if id.prefix() != $prefix {
                    return Err(IdError::InvalidPrefix {
                        expected: $prefix.to_string(),
                        found: id.prefix().to_string(),
                    });
                }
                Ok(Self(id))
            }

            /// Get the inner `SortableId`
            pub const fn inner(&self) -> &SortableId {
                &self.0
            }

            /// Get timestamp
            pub fn timestamp(&self) -> DateTime<Utc> {
                self.0.timestamp()
            }

            /// Convert to string
            pub fn to_string(&self) -> String {
                self.0.to_string()
            }
        }

        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}", self.0)
            }
        }

        impl FromStr for $name {
            type Err = IdError;

            fn from_str(s: &str) -> Result<Self, Self::Err> {
                Self::parse(s)
            }
        }

        impl Serialize for $name {
            fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
            where
                S: Serializer,
            {
                self.0.serialize(serializer)
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
            where
                D: Deserializer<'de>,
            {
                let id = SortableId::deserialize(deserializer)?;
                if id.prefix() != $prefix {
                    return Err(serde::de::Error::custom(format!(
                        "Invalid prefix: expected {}, got {}",
                        $prefix,
                        id.prefix()
                    )));
                }
                Ok(Self(id))
            }
        }
    };
}

// Define type-safe ID wrappers
define_id_wrapper!(SessionId, "sess_");
define_id_wrapper!(EventId, "evt_");
define_id_wrapper!(MemoryId, "mem_");
define_id_wrapper!(SkillId, "skl_");
define_id_wrapper!(PlanId, "plan_");
define_id_wrapper!(ToolId, "tool_");
define_id_wrapper!(FileId, "file_");

// Orchestra methodology ID types
define_id_wrapper!(MilestoneId, "mile_");
define_id_wrapper!(SliceId, "slic_");
define_id_wrapper!(TaskId, "task_");

// Tests

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_base62_encoding() {
        assert_eq!(encode_base62(0), "0");
        assert_eq!(encode_base62(1), "1");
        assert_eq!(encode_base62(61), "z");
        assert_eq!(encode_base62(62), "10");
        assert_eq!(encode_base62(3844), "100"); // 62^2
    }

    #[test]
    fn test_base62_decoding() {
        assert_eq!(decode_base62("0").unwrap(), 0);
        assert_eq!(decode_base62("1").unwrap(), 1);
        assert_eq!(decode_base62("z").unwrap(), 61);
        assert_eq!(decode_base62("10").unwrap(), 62);
        assert_eq!(decode_base62("100").unwrap(), 3844);
    }

    #[test]
    fn test_base62_roundtrip() {
        let values = vec![0, 1, 61, 62, 3844, 1_000_000, u64::MAX / 2];
        for v in values {
            let encoded = encode_base62(v);
            let decoded = decode_base62(&encoded).unwrap();
            assert_eq!(decoded, v, "Roundtrip failed for {v}");
        }
    }

    #[test]
    fn test_invalid_base62() {
        assert!(decode_base62("").is_err());
        assert!(decode_base62("  ").is_err());
        assert!(decode_base62("@#$").is_err());
    }

    #[test]
    fn test_sortable_id_creation() {
        let id = SortableId::new("test_");
        assert_eq!(id.prefix(), "test_");
        assert!(id.timestamp_ms() > 0);
        assert!(id.random() > 0);
    }

    #[test]
    fn test_sortable_id_parsing() {
        let id = SortableId::new("sess_");
        let s = id.to_string();
        let parsed = SortableId::parse(&s).unwrap();
        assert_eq!(parsed, id);
    }

    #[test]
    fn test_sortable_id_format() {
        let id = SortableId::from_components("evt_", 1_234_567_890_000, 9_876_543_210);
        let s = id.to_string();

        assert!(s.starts_with("evt_"));
        assert!(s.len() >= 16); // prefix (5) + timestamp (10) + at least 1 char random
    }

    #[test]
    fn test_sortable_id_timestamp_extraction() {
        let ts_ms = 1_234_567_890_000;
        let id = SortableId::from_components("mem_", ts_ms, 123);
        assert_eq!(id.timestamp_ms(), ts_ms);
    }

    #[test]
    fn test_sortable_id_ordering() {
        let earlier = SortableId::from_components("sess_", 1000, 999);
        let later = SortableId::from_components("sess_", 2000, 111);
        assert!(earlier < later);
    }

    #[test]
    fn test_invalid_prefix() {
        let result = SortableId::parse("invalid1234567890123456");
        assert!(matches!(result, Err(IdError::InvalidFormat(_))));
    }

    #[test]
    fn test_too_short() {
        let result = SortableId::parse("sess_123");
        assert!(matches!(result, Err(IdError::TooShort { .. })));
    }

    #[test]
    fn test_id_length() {
        let id = SessionId::new();
        let s = id.to_string();
        // Should be 15-26 chars: prefix (4-5) + timestamp (10) + random (1-11)
        assert!(s.len() >= 15);
        assert!(s.len() <= 26);
    }

    #[test]
    fn test_session_id() {
        let id = SessionId::new();
        let s = id.to_string();
        assert!(s.starts_with("sess_"));
        // Length varies: prefix (5) + timestamp (10) + random (1-11)
        assert!(s.len() >= 16 && s.len() <= 27);

        let parsed = SessionId::parse(&s).unwrap();
        assert_eq!(parsed.to_string(), s);
    }

    #[test]
    fn test_session_id_wrong_prefix() {
        // Valid format but wrong prefix - use smaller numbers to avoid overflow
        let result = SessionId::parse("evt_0000000001111");
        match &result {
            Err(IdError::InvalidPrefix { .. }) => {}
            other => panic!("Expected InvalidPrefix error, got: {other:?}"),
        }
    }

    #[test]
    fn test_event_id() {
        let id = EventId::new();
        let s = id.to_string();
        assert!(s.starts_with("evt_"));

        let parsed = EventId::parse(&s).unwrap();
        assert_eq!(parsed.to_string(), s);
    }

    #[test]
    fn test_memory_id() {
        let id = MemoryId::new();
        assert!(id.to_string().starts_with("mem_"));
    }

    #[test]
    fn test_skill_id() {
        let id = SkillId::new();
        assert!(id.to_string().starts_with("skl_"));
    }

    #[test]
    fn test_tool_id() {
        let id = ToolId::new();
        assert!(id.to_string().starts_with("tool_"));
    }

    #[test]
    fn test_file_id() {
        let id = FileId::new();
        assert!(id.to_string().starts_with("file_"));
    }

    #[test]
    fn test_milestone_id() {
        let id = MilestoneId::new();
        assert!(id.to_string().starts_with("mile_"));
    }

    #[test]
    fn test_slice_id() {
        let id = SliceId::new();
        assert!(id.to_string().starts_with("slic_"));
    }

    #[test]
    fn test_task_id() {
        let id = TaskId::new();
        assert!(id.to_string().starts_with("task_"));
    }

    #[test]
    fn test_sortable_id_display() {
        let id = SortableId::new("test_");
        let s = format!("{id}");
        assert!(s.starts_with("test_"));
    }

    #[test]
    fn test_session_id_display() {
        let id = SessionId::new();
        let s = format!("{id}");
        assert!(s.starts_with("sess_"));
    }

    #[test]
    fn test_session_id_from_str() {
        let id = SessionId::new();
        let s = id.to_string();
        let parsed: Result<SessionId, _> = s.parse();
        assert!(parsed.is_ok());
        assert_eq!(parsed.unwrap().to_string(), s);
    }

    #[test]
    fn test_session_id_serde() {
        let id = SessionId::new();
        let json = serde_json::to_string(&id).unwrap();
        let parsed: SessionId = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.to_string(), id.to_string());
    }

    #[test]
    fn test_sortable_id_serde() {
        let id = SortableId::new("sess_");
        let json = serde_json::to_string(&id).unwrap();
        let parsed: SortableId = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.to_string(), id.to_string());
    }

    #[test]
    fn test_multiple_ids_are_unique() {
        let mut ids = std::collections::HashSet::new();
        for _ in 0..1000 {
            let id = SessionId::new();
            assert!(ids.insert(id.to_string()), "Duplicate ID generated!");
        }
    }

    #[test]
    fn test_ids_are_time_sortable() {
        use std::thread;
        use std::time::Duration;

        let id1 = SessionId::new();
        thread::sleep(Duration::from_millis(10));
        let id2 = SessionId::new();
        thread::sleep(Duration::from_millis(10));
        let id3 = SessionId::new();

        assert!(id1.to_string() < id2.to_string());
        assert!(id2.to_string() < id3.to_string());
    }

    #[test]
    fn test_id_compactness() {
        let id = SessionId::new();
        let id_str = id.to_string();

        // Sortable ID should be much shorter than UUID (36 chars)
        assert!(id_str.len() < 36, "Sortable ID should be < 36 chars");
        assert!(id_str.len() >= 15, "Sortable ID should be >= 15 chars");
    }

    #[test]
    fn test_default_impl() {
        let id1 = SessionId::default();
        let id2 = SessionId::default();
        assert_ne!(id1.to_string(), id2.to_string());
    }

    #[test]
    fn test_timestamp_extraction() {
        let id = SessionId::new();
        let ts = id.timestamp();

        // The timestamp should be recent (within last 10 seconds)
        let now = Utc::now();
        let diff = now - ts;
        assert!(diff.num_seconds() < 10, "Timestamp should be recent");
    }

    #[test]
    fn test_inner_accessor() {
        let id = SessionId::new();
        let inner = id.inner();
        assert_eq!(inner.prefix(), "sess_");
    }

    #[test]
    fn test_all_id_types() {
        let sess = SessionId::new();
        let evt = EventId::new();
        let mem = MemoryId::new();
        let skl = SkillId::new();
        let plan = PlanId::new();
        let tool = ToolId::new();
        let file = FileId::new();

        assert!(sess.to_string().starts_with("sess_"));
        assert!(evt.to_string().starts_with("evt_"));
        assert!(mem.to_string().starts_with("mem_"));
        assert!(skl.to_string().starts_with("skl_"));
        assert!(plan.to_string().starts_with("plan_"));
        assert!(tool.to_string().starts_with("tool_"));
        assert!(file.to_string().starts_with("file_"));

        // Orchestra methodology ID types
        let mile = MilestoneId::new();
        let slic = SliceId::new();
        let task = TaskId::new();

        assert!(mile.to_string().starts_with("mile_"));
        assert!(slic.to_string().starts_with("slic_"));
        assert!(task.to_string().starts_with("task_"));
    }

    // ========================================================================
    // Additional coverage: serialization roundtrips, edge cases, Display/Debug,
    // Clone/Eq/Hash, Default values, error formatting
    // ========================================================================

    // --- Serialization roundtrips for all wrapper types ---

    #[test]
    fn test_event_id_serde_roundtrip() {
        let id = EventId::new();
        let json = serde_json::to_string(&id).unwrap();
        assert!(json.starts_with('"'));
        assert!(json.ends_with('"'));
        let parsed: EventId = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.to_string(), id.to_string());
    }

    #[test]
    fn test_memory_id_serde_roundtrip() {
        let id = MemoryId::new();
        let json = serde_json::to_string(&id).unwrap();
        let parsed: MemoryId = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.to_string(), id.to_string());
    }

    #[test]
    fn test_skill_id_serde_roundtrip() {
        let id = SkillId::new();
        let json = serde_json::to_string(&id).unwrap();
        let parsed: SkillId = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.to_string(), id.to_string());
    }

    #[test]
    fn test_plan_id_serde_roundtrip() {
        let id = PlanId::new();
        let json = serde_json::to_string(&id).unwrap();
        let parsed: PlanId = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.to_string(), id.to_string());
    }

    #[test]
    fn test_tool_id_serde_roundtrip() {
        let id = ToolId::new();
        let json = serde_json::to_string(&id).unwrap();
        let parsed: ToolId = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.to_string(), id.to_string());
    }

    #[test]
    fn test_file_id_serde_roundtrip() {
        let id = FileId::new();
        let json = serde_json::to_string(&id).unwrap();
        let parsed: FileId = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.to_string(), id.to_string());
    }

    #[test]
    fn test_milestone_id_serde_roundtrip() {
        let id = MilestoneId::new();
        let json = serde_json::to_string(&id).unwrap();
        let parsed: MilestoneId = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.to_string(), id.to_string());
    }

    #[test]
    fn test_slice_id_serde_roundtrip() {
        let id = SliceId::new();
        let json = serde_json::to_string(&id).unwrap();
        let parsed: SliceId = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.to_string(), id.to_string());
    }

    #[test]
    fn test_task_id_serde_roundtrip() {
        let id = TaskId::new();
        let json = serde_json::to_string(&id).unwrap();
        let parsed: TaskId = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.to_string(), id.to_string());
    }

    #[test]
    fn test_serde_in_struct() {
        #[derive(Serialize, Deserialize)]
        struct Record {
            id: SessionId,
            event: EventId,
        }
        let record = Record {
            id: SessionId::new(),
            event: EventId::new(),
        };
        let json = serde_json::to_string(&record).unwrap();
        let parsed: Record = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.id.to_string(), record.id.to_string());
        assert_eq!(parsed.event.to_string(), record.event.to_string());
    }

    #[test]
    fn test_serde_deserialize_wrong_prefix() {
        let id = EventId::new();
        let json = serde_json::to_string(&id).unwrap();
        let result: Result<SessionId, _> = serde_json::from_str(&json);
        assert!(result.is_err());
    }

    #[test]
    fn test_serde_deserialize_invalid_string() {
        let result: Result<SessionId, _> = serde_json::from_str("\"garbage\"");
        assert!(result.is_err());
    }

    #[test]
    fn test_serde_deserialize_non_string() {
        let result: Result<SessionId, _> = serde_json::from_str("42");
        assert!(result.is_err());
    }

    // --- Edge cases: parsing, base62, length limits ---

    #[test]
    fn test_too_long_id() {
        let long_id = "sess_".to_string() + &"A".repeat(50);
        let result = SortableId::parse(&long_id);
        assert!(matches!(result, Err(IdError::TooLong { .. })));
    }

    #[test]
    fn test_parse_no_underscore() {
        let result = SortableId::parse("noslash1234567890");
        assert!(matches!(result, Err(IdError::InvalidFormat(_))));
    }

    #[test]
    fn test_parse_invalid_base62_chars() {
        // Valid length, valid prefix, but invalid characters in timestamp
        let result = SortableId::parse("sess_!!!!!!!!!z");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_with_special_characters() {
        let result = SortableId::parse("sess_0000000000!");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_with_spaces() {
        let result = SortableId::parse("sess_00000000 00");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_with_unicode() {
        let result = SortableId::parse("sess_00000000\u{00e9}0");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_minimal_valid_length() {
        // prefix(5) + timestamp(10) + random(1) = 16 chars for sess_
        let id = SortableId::from_components("sess_", 0, 0);
        let s = id.to_string();
        let parsed = SortableId::parse(&s).unwrap();
        assert_eq!(parsed.timestamp_ms(), 0);
        assert_eq!(parsed.random(), 0);
    }

    #[test]
    fn test_parse_exactly_min_length_boundary() {
        // 14 chars = 1 less than MIN_LENGTH of 15 (ses_ is 4 chars, then 10 chars)
        let result = SortableId::parse("ses_0000000000");
        assert!(matches!(result, Err(IdError::TooShort { .. })));
    }

    #[test]
    fn test_base62_decode_invalid_char_at_position() {
        let result = decode_base62("ab!c");
        match result {
            Err(IdError::InvalidBase62(msg)) => {
                assert!(
                    msg.contains('!'),
                    "Error should mention invalid char: {msg}"
                );
            }
            other => panic!("Expected InvalidBase62, got: {other:?}"),
        }
    }

    #[test]
    fn test_base62_roundtrip_boundary_values() {
        // u64::MAX itself may overflow encode, but near-max should work
        let values = vec![0u64, 1, 62, 62 * 62, 999_999_999_999, u64::MAX / 10];
        for v in values {
            let encoded = encode_base62(v);
            let decoded = decode_base62(&encoded).unwrap();
            assert_eq!(decoded, v, "Roundtrip failed for {v}");
        }
    }

    #[test]
    fn test_base62_encode_zero() {
        assert_eq!(encode_base62(0), "0");
    }

    #[test]
    fn test_base62_all_single_chars() {
        for i in 0..62 {
            let encoded = encode_base62(i);
            assert_eq!(encoded.len(), 1, "Single char expected for {i}");
            let decoded = decode_base62(&encoded).unwrap();
            assert_eq!(decoded, i);
        }
    }

    #[test]
    fn test_from_components_roundtrip() {
        let id = SortableId::from_components("sess_", 1_700_000_000_000, 42);
        let s = id.to_string();
        let parsed = SortableId::parse(&s).unwrap();
        assert_eq!(parsed.prefix(), "sess_");
        assert_eq!(parsed.timestamp_ms(), 1_700_000_000_000);
        assert_eq!(parsed.random(), 42);
    }

    #[test]
    fn test_from_components_zero_random() {
        let id = SortableId::from_components("test_", 1000, 0);
        let s = id.to_string();
        assert!(s.starts_with("test_"));
        let parsed = SortableId::parse(&s).unwrap();
        assert_eq!(parsed.random(), 0);
    }

    #[test]
    fn test_from_components_zero_timestamp() {
        let id = SortableId::from_components("test_", 0, 12345);
        let s = id.to_string();
        let parsed = SortableId::parse(&s).unwrap();
        assert_eq!(parsed.timestamp_ms(), 0);
        assert_eq!(parsed.random(), 12345);
    }

    #[test]
    fn test_timestamp_method_returns_valid_datetime() {
        let id = SortableId::from_components("test_", 1_700_000_000_000, 0);
        let dt = id.timestamp();
        assert_eq!(dt.timestamp_millis(), 1_700_000_000_000);
    }

    #[test]
    fn test_timestamp_method_zero() {
        let id = SortableId::from_components("test_", 0, 0);
        let dt = id.timestamp();
        // Zero milliseconds => Unix epoch
        assert!(dt.timestamp_millis() >= 0);
    }

    #[test]
    fn test_wrapper_parse_wrong_prefix_each_type() {
        let session_str = SessionId::new().to_string();
        assert!(EventId::parse(&session_str).is_err());
        assert!(MemoryId::parse(&session_str).is_err());
        assert!(SkillId::parse(&session_str).is_err());
        assert!(PlanId::parse(&session_str).is_err());
        assert!(ToolId::parse(&session_str).is_err());
        assert!(FileId::parse(&session_str).is_err());
        assert!(MilestoneId::parse(&session_str).is_err());
        assert!(SliceId::parse(&session_str).is_err());
        assert!(TaskId::parse(&session_str).is_err());
    }

    #[test]
    fn test_wrapper_from_str_roundtrip() {
        let id = EventId::new();
        let s = id.to_string();
        let parsed: EventId = s.parse().unwrap();
        assert_eq!(parsed.to_string(), s);
    }

    #[test]
    fn test_wrapper_from_str_invalid() {
        let result: Result<SessionId, _> = "not_valid".parse();
        assert!(result.is_err());
    }

    #[test]
    fn test_plan_id_creation() {
        let id = PlanId::new();
        assert!(id.to_string().starts_with("plan_"));
        let parsed = PlanId::parse(id.to_string()).unwrap();
        assert_eq!(parsed.to_string(), id.to_string());
    }

    // --- Display and Debug formatting ---

    #[test]
    fn test_sortable_id_debug_format() {
        let id = SortableId::from_components("test_", 1000, 42);
        let debug = format!("{id:?}");
        assert!(debug.contains("test_"));
    }

    #[test]
    fn test_session_id_debug_format() {
        let id = SessionId::new();
        let debug = format!("{id:?}");
        assert!(debug.contains("sess_"));
    }

    #[test]
    fn test_event_id_debug_format() {
        let id = EventId::new();
        let debug = format!("{id:?}");
        assert!(!debug.is_empty());
    }

    #[test]
    fn test_wrapper_display_matches_to_string() {
        let id = SessionId::new();
        let display = format!("{id}");
        let to_string = id.to_string();
        assert_eq!(display, to_string);
    }

    #[test]
    fn test_sortable_id_display_matches_to_string() {
        let id = SortableId::from_components("test_", 999, 7);
        let display = format!("{id}");
        let to_string = id.to_string();
        assert_eq!(display, to_string);
    }

    #[test]
    fn test_id_error_debug_format() {
        let err = IdError::InvalidFormat("test error".to_string());
        let debug = format!("{err:?}");
        assert!(debug.contains("InvalidFormat"));
    }

    #[test]
    fn test_id_error_display_format() {
        let err = IdError::InvalidFormat("test error".to_string());
        let display = format!("{err}");
        assert!(display.contains("test error"));
    }

    #[test]
    fn test_id_error_invalid_prefix_display() {
        let err = IdError::InvalidPrefix {
            expected: "sess_".to_string(),
            found: "evt_".to_string(),
        };
        let display = format!("{err}");
        assert!(display.contains("sess_"));
        assert!(display.contains("evt_"));
    }

    #[test]
    fn test_id_error_too_short_display() {
        let err = IdError::TooShort { length: 5, min: 15 };
        let display = format!("{err}");
        assert!(display.contains('5'));
        assert!(display.contains("15"));
    }

    #[test]
    fn test_id_error_too_long_display() {
        let err = IdError::TooLong {
            length: 50,
            max: 30,
        };
        let display = format!("{err}");
        assert!(display.contains("50"));
        assert!(display.contains("30"));
    }

    #[test]
    fn test_id_error_invalid_base62_display() {
        let err = IdError::InvalidBase62("bad char".to_string());
        let display = format!("{err}");
        assert!(display.contains("bad char"));
    }

    #[test]
    fn test_id_error_invalid_timestamp_display() {
        let err = IdError::InvalidTimestamp("overflow".to_string());
        let display = format!("{err}");
        assert!(display.contains("overflow"));
    }

    // --- Clone / Eq / Hash ---

    #[test]
    fn test_sortable_id_clone() {
        let id = SortableId::from_components("test_", 12345, 67);
        let cloned = id.clone();
        assert_eq!(id, cloned);
    }

    #[test]
    fn test_session_id_clone() {
        let id = SessionId::new();
        let cloned = id.clone();
        assert_eq!(id, cloned);
        assert_eq!(id.to_string(), cloned.to_string());
    }

    #[test]
    fn test_event_id_clone() {
        let id = EventId::new();
        let cloned = id.clone();
        assert_eq!(id, cloned);
    }

    #[test]
    fn test_id_equality_same_components() {
        let a = SortableId::from_components("sess_", 1000, 42);
        let b = SortableId::from_components("sess_", 1000, 42);
        assert_eq!(a, b);
    }

    #[test]
    fn test_id_inequality_different_timestamp() {
        let a = SortableId::from_components("sess_", 1000, 42);
        let b = SortableId::from_components("sess_", 2000, 42);
        assert_ne!(a, b);
    }

    #[test]
    fn test_id_inequality_different_random() {
        let a = SortableId::from_components("sess_", 1000, 42);
        let b = SortableId::from_components("sess_", 1000, 99);
        assert_ne!(a, b);
    }

    #[test]
    fn test_id_inequality_different_prefix() {
        let a = SortableId::from_components("sess_", 1000, 42);
        let b = SortableId::from_components("test_", 1000, 42);
        assert_ne!(a, b);
    }

    #[test]
    fn test_sortable_id_hash_equal() {
        use std::collections::HashSet;
        let a = SortableId::from_components("sess_", 1000, 42);
        let b = SortableId::from_components("sess_", 1000, 42);
        let mut set = HashSet::new();
        assert!(set.insert(a));
        assert!(!set.insert(b)); // same hash/equality => not re-inserted
    }

    #[test]
    fn test_session_id_hash_in_set() {
        use std::collections::HashSet;
        let id = SessionId::new();
        let mut set = HashSet::new();
        assert!(set.insert(id.clone()));
        assert!(!set.insert(id));
    }

    #[test]
    fn test_id_error_clone_eq() {
        let err1 = IdError::InvalidFormat("test".to_string());
        let err2 = err1.clone();
        assert_eq!(err1, err2);
    }

    #[test]
    fn test_id_error_neq_different_variants() {
        let err1 = IdError::InvalidFormat("a".to_string());
        let err2 = IdError::TooShort { length: 1, min: 15 };
        assert_ne!(err1, err2);
    }

    #[test]
    fn test_wrapper_ordering_same_prefix() {
        let earlier = SortableId::from_components("sess_", 1000, 0);
        let later = SortableId::from_components("sess_", 2000, 0);
        assert!(earlier < later);
        assert!(later > earlier);
        assert!(earlier != later);
    }

    #[test]
    fn test_wrapper_ordering_same_timestamp_different_random() {
        let a = SortableId::from_components("sess_", 1000, 10);
        let b = SortableId::from_components("sess_", 1000, 20);
        assert!(a < b);
    }

    // --- Default for all wrapper types ---

    #[test]
    fn test_default_event_id() {
        let id = EventId::default();
        assert!(id.to_string().starts_with("evt_"));
    }

    #[test]
    fn test_default_memory_id() {
        let id = MemoryId::default();
        assert!(id.to_string().starts_with("mem_"));
    }

    #[test]
    fn test_default_skill_id() {
        let id = SkillId::default();
        assert!(id.to_string().starts_with("skl_"));
    }

    #[test]
    fn test_default_plan_id() {
        let id = PlanId::default();
        assert!(id.to_string().starts_with("plan_"));
    }

    #[test]
    fn test_default_tool_id() {
        let id = ToolId::default();
        assert!(id.to_string().starts_with("tool_"));
    }

    #[test]
    fn test_default_file_id() {
        let id = FileId::default();
        assert!(id.to_string().starts_with("file_"));
    }

    #[test]
    fn test_default_milestone_id() {
        let id = MilestoneId::default();
        assert!(id.to_string().starts_with("mile_"));
    }

    #[test]
    fn test_default_slice_id() {
        let id = SliceId::default();
        assert!(id.to_string().starts_with("slic_"));
    }

    #[test]
    fn test_default_task_id() {
        let id = TaskId::default();
        assert!(id.to_string().starts_with("task_"));
    }

    #[test]
    fn test_default_produces_unique_ids() {
        // Default generates a new random ID each time
        let a = SessionId::default();
        let b = SessionId::default();
        assert_ne!(a, b);
    }

    // --- Prefix validation panics ---

    #[test]
    #[should_panic(expected = "Prefix must be at least 4 characters")]
    fn test_prefix_too_short() {
        SortableId::new("ab_");
    }

    #[test]
    #[should_panic(expected = "Prefix must be at most 5 characters")]
    fn test_prefix_too_long() {
        SortableId::new("toolong_");
    }

    #[test]
    #[should_panic(expected = "Prefix must end with '_'")]
    fn test_prefix_no_underscore() {
        SortableId::new("test!");
    }

    #[test]
    #[should_panic(expected = "Prefix must be at least 4 characters")]
    fn test_from_components_prefix_too_short() {
        SortableId::from_components("ab_", 0, 0);
    }

    #[test]
    #[should_panic(expected = "Prefix must be at most 5 characters")]
    fn test_from_components_prefix_too_long() {
        SortableId::from_components("toolong_", 0, 0);
    }

    #[test]
    #[should_panic(expected = "Prefix must end with '_'")]
    fn test_from_components_prefix_no_underscore() {
        SortableId::from_components("test!", 0, 0);
    }

    // --- Prefix edge: exactly 4-char and 5-char prefixes ---

    #[test]
    fn test_prefix_exactly_four_chars() {
        let id = SortableId::new("ab__");
        assert_eq!(id.prefix(), "ab__");
    }

    #[test]
    fn test_prefix_exactly_five_chars() {
        let id = SortableId::new("abc__");
        assert_eq!(id.prefix(), "abc__");
    }

    // --- SortableId timestamp_ms accessor ---

    #[test]
    fn test_timestamp_ms_accessor() {
        let ts = 1_700_000_000_123_u64;
        let id = SortableId::from_components("test_", ts, 0);
        assert_eq!(id.timestamp_ms(), ts);
    }

    // --- Inner accessor returns correct prefix for each type ---

    #[test]
    fn test_inner_accessors_all_types() {
        assert_eq!(EventId::new().inner().prefix(), "evt_");
        assert_eq!(MemoryId::new().inner().prefix(), "mem_");
        assert_eq!(SkillId::new().inner().prefix(), "skl_");
        assert_eq!(PlanId::new().inner().prefix(), "plan_");
        assert_eq!(ToolId::new().inner().prefix(), "tool_");
        assert_eq!(FileId::new().inner().prefix(), "file_");
        assert_eq!(MilestoneId::new().inner().prefix(), "mile_");
        assert_eq!(SliceId::new().inner().prefix(), "slic_");
        assert_eq!(TaskId::new().inner().prefix(), "task_");
    }

    // --- Serde JSON value embedded in a map ---

    #[test]
    fn test_serde_embedded_in_json_value() {
        let id = SessionId::new();
        let json = serde_json::json!({ "session_id": id });
        let parsed: serde_json::Value = serde_json::from_str(&json.to_string()).unwrap();
        assert!(parsed["session_id"].is_string());
        let recovered: SessionId = serde_json::from_value(parsed["session_id"].clone()).unwrap();
        assert_eq!(recovered.to_string(), id.to_string());
    }
}
