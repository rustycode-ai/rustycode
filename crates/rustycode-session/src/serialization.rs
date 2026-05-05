//! Session serialization with compression support
//!
//! This module provides efficient serialization and deserialization
//! of sessions with optional zstd compression and binary format.

use crate::session::Session;
use thiserror::Error;

/// Serialization format
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum SerializationFormat {
    /// JSON format
    Json,

    /// JSON with zstd compression
    CompressedJson,

    /// Binary format using bincode
    Binary,

    /// Binary format with zstd compression
    CompressedBinary,
}

/// Errors that can occur during serialization
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum SerializationError {
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Zstd error: {0}")]
    Zstd(String),

    #[error("Bincode error: {0}")]
    Bincode(String),
}

/// Session serializer
pub struct SessionSerializer;

impl SessionSerializer {
    /// Serialize a session to bytes
    pub fn serialize(
        session: &Session,
        format: SerializationFormat,
    ) -> Result<Vec<u8>, SerializationError> {
        match format {
            SerializationFormat::Json => Self::serialize_json(session),
            SerializationFormat::CompressedJson => Self::serialize_compressed_json(session),
            SerializationFormat::Binary => Self::serialize_binary(session),
            SerializationFormat::CompressedBinary => Self::serialize_compressed_binary(session),
            #[allow(unreachable_patterns)]
            _ => Self::serialize_json(session),
        }
    }

    /// Deserialize bytes to a session
    pub fn deserialize(
        data: &[u8],
        format: SerializationFormat,
    ) -> Result<Session, SerializationError> {
        match format {
            SerializationFormat::Json => Self::deserialize_json(data),
            SerializationFormat::CompressedJson => Self::deserialize_compressed_json(data),
            SerializationFormat::Binary => Self::deserialize_binary(data),
            SerializationFormat::CompressedBinary => Self::deserialize_compressed_binary(data),
            #[allow(unreachable_patterns)]
            _ => Self::deserialize_json(data),
        }
    }

    /// Serialize to JSON format
    fn serialize_json(session: &Session) -> Result<Vec<u8>, SerializationError> {
        let json = serde_json::to_vec_pretty(session)?;
        Ok(json)
    }

    /// Deserialize from JSON format
    fn deserialize_json(data: &[u8]) -> Result<Session, SerializationError> {
        // Prevent memory exhaustion from maliciously large session files
        const MAX_SESSION_SIZE: usize = 10 * 1024 * 1024; // 10MB limit
        if data.len() > MAX_SESSION_SIZE {
            return Err(SerializationError::Zstd(format!(
                "Session file too large: {} bytes (max: {} bytes)",
                data.len(),
                MAX_SESSION_SIZE
            )));
        }

        let session = serde_json::from_slice(data)?;
        Ok(session)
    }

    /// Serialize to compressed JSON format
    fn serialize_compressed_json(session: &Session) -> Result<Vec<u8>, SerializationError> {
        let json = Self::serialize_json(session)?;

        let compressed =
            zstd::bulk::compress(&json, 3).map_err(|e| SerializationError::Zstd(e.to_string()))?;

        Ok(compressed)
    }

    /// Deserialize from compressed JSON format
    fn deserialize_compressed_json(data: &[u8]) -> Result<Session, SerializationError> {
        let decompressed = zstd::bulk::decompress(data, 10_000_000) // 10MB max
            .map_err(|e| SerializationError::Zstd(e.to_string()))?;

        Self::deserialize_json(&decompressed)
    }

    /// Serialize to binary format using bincode (faster than JSON)
    fn serialize_binary(session: &Session) -> Result<Vec<u8>, SerializationError> {
        let binary =
            bincode::serialize(session).map_err(|e| SerializationError::Bincode(e.to_string()))?;
        Ok(binary)
    }

    /// Deserialize from binary format
    fn deserialize_binary(data: &[u8]) -> Result<Session, SerializationError> {
        let session =
            bincode::deserialize(data).map_err(|e| SerializationError::Bincode(e.to_string()))?;
        Ok(session)
    }

    /// Serialize to compressed binary format
    fn serialize_compressed_binary(session: &Session) -> Result<Vec<u8>, SerializationError> {
        let binary = Self::serialize_binary(session)?;

        let compressed = zstd::bulk::compress(&binary, 3)
            .map_err(|e| SerializationError::Zstd(e.to_string()))?;

        Ok(compressed)
    }

    /// Deserialize from compressed binary format
    fn deserialize_compressed_binary(data: &[u8]) -> Result<Session, SerializationError> {
        let decompressed = zstd::bulk::decompress(data, 10_000_000) // 10MB max
            .map_err(|e| SerializationError::Zstd(e.to_string()))?;

        Self::deserialize_binary(&decompressed)
    }

    /// Serialize session to a file
    pub fn to_file(
        session: &Session,
        path: impl AsRef<std::path::Path>,
        format: SerializationFormat,
    ) -> Result<(), SerializationError> {
        let data = Self::serialize(session, format)?;
        let path = path.as_ref();
        let tmp_path = path.with_extension("tmp");
        std::fs::write(&tmp_path, &data).map_err(|e| {
            SerializationError::Io(std::io::Error::new(
                e.kind(),
                format!("Failed to write session to {}: {e}", tmp_path.display()),
            ))
        })?;
        std::fs::rename(&tmp_path, path).map_err(|e| {
            SerializationError::Io(std::io::Error::new(
                e.kind(),
                format!("Failed to rename session file: {e}"),
            ))
        })?;
        Ok(())
    }

    /// Deserialize session from a file
    pub fn from_file(
        path: impl AsRef<std::path::Path>,
        format: SerializationFormat,
    ) -> Result<Session, SerializationError> {
        let data = std::fs::read(path)?;
        Self::deserialize(&data, format)
    }

    /// Get compression ratio (compressed / uncompressed)
    pub fn compression_ratio(session: &Session) -> Result<f64, SerializationError> {
        let json = Self::serialize_json(session)?;
        let compressed = Self::serialize_compressed_json(session)?;

        if json.is_empty() {
            return Ok(0.0);
        }

        Ok(compressed.len() as f64 / json.len() as f64)
    }

    /// Get format size comparison
    pub fn format_sizes(session: &Session) -> Result<FormatSizes, SerializationError> {
        let json = Self::serialize_json(session)?;
        let compressed_json = Self::serialize_compressed_json(session)?;
        let binary = Self::serialize_binary(session)?;
        let compressed_binary = Self::serialize_compressed_binary(session)?;

        Ok(FormatSizes {
            json_size: json.len(),
            compressed_json_size: compressed_json.len(),
            binary_size: binary.len(),
            compressed_binary_size: compressed_binary.len(),
        })
    }
}

/// Size comparison for different formats
#[derive(Debug, Clone)]
pub struct FormatSizes {
    pub json_size: usize,
    pub compressed_json_size: usize,
    pub binary_size: usize,
    pub compressed_binary_size: usize,
}

impl FormatSizes {
    /// Calculate compression ratio for JSON
    pub fn json_compression_ratio(&self) -> f64 {
        if self.json_size == 0 {
            return 0.0;
        }
        self.compressed_json_size as f64 / self.json_size as f64
    }

    /// Calculate compression ratio for binary
    pub fn binary_compression_ratio(&self) -> f64 {
        if self.binary_size == 0 {
            return 0.0;
        }
        self.compressed_binary_size as f64 / self.binary_size as f64
    }

    /// Get best format (smallest size)
    pub fn best_format(&self) -> SerializationFormat {
        let min_size = *[
            self.json_size,
            self.compressed_json_size,
            self.binary_size,
            self.compressed_binary_size,
        ]
        .iter()
        .filter(|&&x| x > 0)
        .min()
        .unwrap_or(&self.json_size);

        if min_size == self.compressed_binary_size {
            SerializationFormat::CompressedBinary
        } else if min_size == self.binary_size {
            SerializationFormat::Binary
        } else if min_size == self.compressed_json_size {
            SerializationFormat::CompressedJson
        } else {
            SerializationFormat::Json
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::message::{Message, MessagePart};

    fn create_test_session() -> Session {
        let mut session = Session::new("Test Session");
        session.add_message(Message::user("Hello, world!"));
        session.add_message(Message::assistant("Hi there!"));
        session.add_message(Message::user("How are you?"));
        session.add_message(Message::assistant("I'm doing great!"));
        session
    }

    #[test]
    fn test_serialize_deserialize_json() {
        let session = create_test_session();

        let serialized = SessionSerializer::serialize(&session, SerializationFormat::Json).unwrap();
        let deserialized =
            SessionSerializer::deserialize(&serialized, SerializationFormat::Json).unwrap();

        assert_eq!(deserialized.name, session.name);
        assert_eq!(deserialized.message_count(), session.message_count());
    }

    #[test]
    fn test_serialize_deserialize_compressed_json() {
        let session = create_test_session();

        let serialized =
            SessionSerializer::serialize(&session, SerializationFormat::CompressedJson).unwrap();
        let deserialized =
            SessionSerializer::deserialize(&serialized, SerializationFormat::CompressedJson)
                .unwrap();

        assert_eq!(deserialized.name, session.name);
        assert_eq!(deserialized.message_count(), session.message_count());
    }

    #[test]
    fn test_compression_ratio() {
        let session = create_test_session();

        let ratio = SessionSerializer::compression_ratio(&session).unwrap();

        assert!(
            ratio > 0.0 && ratio < 1.0,
            "Compression ratio should be between 0 and 1"
        );
    }

    #[test]
    fn test_format_sizes() {
        let session = create_test_session();

        let sizes = SessionSerializer::format_sizes(&session).unwrap();

        assert!(sizes.json_size > 0);
        assert!(sizes.compressed_json_size > 0);

        // Compressed should be smaller
        assert!(sizes.compressed_json_size < sizes.json_size);
    }

    #[test]
    fn test_best_format() {
        let session = create_test_session();
        let sizes = SessionSerializer::format_sizes(&session).unwrap();

        let best_format = sizes.best_format();

        // For a small test session, compressed formats should be best
        // Accept either compressed format as they're both valid optimizations
        assert!(
            matches!(
                best_format,
                SerializationFormat::CompressedJson | SerializationFormat::CompressedBinary
            ),
            "Expected compressed format, got: {:?}",
            best_format
        );

        // Verify that compressed is actually smaller than uncompressed
        match best_format {
            SerializationFormat::CompressedJson => {
                assert!(sizes.compressed_json_size < sizes.json_size);
            }
            SerializationFormat::CompressedBinary => {
                assert!(sizes.compressed_binary_size < sizes.binary_size);
            }
            _ => panic!("Expected compressed format"),
        }
    }

    #[test]
    fn test_file_roundtrip() {
        let session = create_test_session();
        let temp_file = tempfile::NamedTempFile::new().unwrap();

        SessionSerializer::to_file(
            &session,
            temp_file.path(),
            SerializationFormat::CompressedJson,
        )
        .unwrap();
        let loaded =
            SessionSerializer::from_file(temp_file.path(), SerializationFormat::CompressedJson)
                .unwrap();

        assert_eq!(loaded.name, session.name);
        assert_eq!(loaded.message_count(), session.message_count());
    }

    #[test]
    fn test_large_session_compression() {
        let mut session = Session::new("Large Session");
        for i in 0..1000 {
            session.add_message(Message::user(format!("Message {}", i)));
            session.add_message(Message::assistant(format!("Response {}", i)));
        }

        let json = SessionSerializer::serialize(&session, SerializationFormat::Json).unwrap();
        let compressed =
            SessionSerializer::serialize(&session, SerializationFormat::CompressedJson).unwrap();

        // Compressed should be significantly smaller
        assert!(compressed.len() < json.len() / 2);
    }

    // --- SerializationFormat tests ---

    #[test]
    fn test_serialization_format_eq() {
        assert_eq!(SerializationFormat::Json, SerializationFormat::Json);
        assert_ne!(SerializationFormat::Json, SerializationFormat::Binary);
        assert_ne!(
            SerializationFormat::CompressedJson,
            SerializationFormat::CompressedBinary
        );
    }

    #[test]
    fn test_serialization_format_clone_copy() {
        let fmt = SerializationFormat::Json;
        let fmt2 = fmt; // Copy
        assert_eq!(fmt, fmt2);
        let fmt3 = fmt;
        assert_eq!(fmt, fmt3);
    }

    // --- SerializationError display tests ---

    #[test]
    fn test_serialization_error_json() {
        let bad_json: Result<Session, SerializationError> =
            serde_json::from_slice(b"not json").map_err(SerializationError::from);
        let err = bad_json.unwrap_err();
        assert!(err.to_string().starts_with("JSON error:"));
    }

    #[test]
    fn test_serialization_error_io() {
        let err = SerializationError::Io(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "file missing",
        ));
        assert!(err.to_string().contains("IO error:"));
    }

    #[test]
    fn test_serialization_error_zstd() {
        let err = SerializationError::Zstd("decompress failed".to_string());
        assert!(err.to_string().contains("Zstd error:"));
        assert!(err.to_string().contains("decompress failed"));
    }

    #[test]
    fn test_serialization_error_bincode() {
        let err = SerializationError::Bincode("bad data".to_string());
        assert!(err.to_string().contains("Bincode error:"));
        assert!(err.to_string().contains("bad data"));
    }

    // --- Binary format serialize (deserialize may fail due to SystemTime) ---

    #[test]
    fn test_serialize_binary_succeeds() {
        let session = create_test_session();
        let serialized =
            SessionSerializer::serialize(&session, SerializationFormat::Binary).unwrap();
        assert!(!serialized.is_empty());
    }

    #[test]
    fn test_serialize_compressed_binary_succeeds() {
        let session = create_test_session();
        let serialized =
            SessionSerializer::serialize(&session, SerializationFormat::CompressedBinary).unwrap();
        assert!(!serialized.is_empty());
    }

    // --- FormatSizes tests ---

    #[test]
    fn test_format_sizes_json_compression_ratio() {
        let session = create_test_session();
        let sizes = SessionSerializer::format_sizes(&session).unwrap();
        let ratio = sizes.json_compression_ratio();
        assert!(ratio > 0.0 && ratio < 1.0);
    }

    #[test]
    fn test_format_sizes_binary_compression_ratio() {
        let session = create_test_session();
        let sizes = SessionSerializer::format_sizes(&session).unwrap();
        let ratio = sizes.binary_compression_ratio();
        assert!(ratio > 0.0 && ratio < 1.0);
    }

    #[test]
    fn test_format_sizes_debug() {
        let session = create_test_session();
        let sizes = SessionSerializer::format_sizes(&session).unwrap();
        let debug = format!("{:?}", sizes);
        assert!(debug.contains("json_size"));
        assert!(debug.contains("binary_size"));
    }

    // --- Invalid deserialization data ---

    #[test]
    fn test_deserialize_invalid_json() {
        let result = SessionSerializer::deserialize(b"invalid", SerializationFormat::Json);
        assert!(result.is_err());
    }

    #[test]
    fn test_deserialize_invalid_compressed_json() {
        let result =
            SessionSerializer::deserialize(b"invalid", SerializationFormat::CompressedJson);
        assert!(result.is_err());
    }

    // --- File roundtrip for all formats ---

    #[test]
    fn test_file_roundtrip_json() {
        let session = create_test_session();
        let temp_file = tempfile::NamedTempFile::new().unwrap();

        SessionSerializer::to_file(&session, temp_file.path(), SerializationFormat::Json).unwrap();
        let loaded =
            SessionSerializer::from_file(temp_file.path(), SerializationFormat::Json).unwrap();

        assert_eq!(loaded.name, session.name);
        assert_eq!(loaded.message_count(), session.message_count());
    }

    #[test]
    fn test_from_file_missing_path() {
        let result = SessionSerializer::from_file(
            "/nonexistent/path/session.json",
            SerializationFormat::Json,
        );
        assert!(result.is_err());
    }

    // --- Session with rich content roundtrip ---

    #[test]
    fn test_json_roundtrip_with_tool_calls() {
        let mut session = Session::new("Tool Test");
        session.add_message(Message::user("Run this"));
        let mut msg = Message::assistant("I'll run it");
        msg.add_part(MessagePart::ToolCall {
            id: "call_1".to_string(),
            name: "bash".to_string(),
            input: serde_json::json!({"command": "ls -la"}),
        });
        session.add_message(msg);
        session.add_message(Message::tool_result("call_1", "file1.rs\nfile2.rs", false));

        let serialized = SessionSerializer::serialize(&session, SerializationFormat::Json).unwrap();
        let deserialized =
            SessionSerializer::deserialize(&serialized, SerializationFormat::Json).unwrap();

        assert_eq!(deserialized.message_count(), 3);
        // Assistant message should have tool call
        let assistant_msgs: Vec<_> = deserialized
            .messages
            .iter()
            .filter(|m| m.role == crate::message::MessageRole::Assistant)
            .collect();
        assert!(assistant_msgs[0].has_tool_calls());
    }

    #[test]
    fn test_compressed_json_roundtrip_preserves_content() {
        let mut session = Session::new("Content Test");
        session.add_message(Message::user("Hello"));
        session.add_message(Message::assistant("World with special chars: <>&\"'"));

        let serialized =
            SessionSerializer::serialize(&session, SerializationFormat::CompressedJson).unwrap();
        let deserialized =
            SessionSerializer::deserialize(&serialized, SerializationFormat::CompressedJson)
                .unwrap();

        let user_msg: Vec<_> = deserialized
            .messages
            .iter()
            .filter(|m| m.role == crate::message::MessageRole::User)
            .collect();
        assert_eq!(user_msg[0].text(), "Hello");

        let asst_msg: Vec<_> = deserialized
            .messages
            .iter()
            .filter(|m| m.role == crate::message::MessageRole::Assistant)
            .collect();
        assert_eq!(asst_msg[0].text(), "World with special chars: <>&\"'");
    }

    #[test]
    fn test_atomic_write_no_tmp_leftover() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("session.json");
        let session = Session::new("test-atomic");

        SessionSerializer::to_file(&session, &path, SerializationFormat::Json).unwrap();

        assert!(path.exists(), "Session file should exist");
        assert!(
            !dir.path().join("session.tmp").exists(),
            "No tmp file should remain"
        );
    }
}
