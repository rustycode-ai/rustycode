//! Shared Working Memory for Multi-Agent Collaboration
//!
//! This module provides a shared memory system that enables agents to
//! collaborate by sharing intermediate results, analysis, and findings.
//!
//! # Features
//!
//! - **Type-Safe Storage**: Different data types with proper serialization
//! - **Access Control**: Read/write permissions and conflict resolution
//! - **Versioning**: Track changes and maintain history
//! - **Conflict Resolution**: Automatic and manual conflict handling
//! - **Memory Management**: Size limits and automatic cleanup
//! - **Agent Awareness**: Track which agent contributed what

use crate::workflow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

/// Memory entry with metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryEntry {
    /// Unique identifier
    pub id: String,

    /// Agent who created this entry
    pub agent_id: String,

    /// Entry type for routing
    pub entry_type: MemoryType,

    /// Data payload
    pub data: MemoryData,

    /// Creation timestamp
    pub created_at: DateTime<Utc>,

    /// Last modification timestamp
    pub modified_at: DateTime<Utc>,

    /// Version number
    pub version: u32,

    /// Access control
    pub access: AccessLevel,

    /// Dependencies (other entries this depends on)
    pub dependencies: Vec<String>,

    /// Confidence score (0.0-1.0)
    pub confidence: f64,

    pub metadata: HashMap<String, String>,
}

/// Types of memory entries
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum MemoryType {
    /// Analysis results
    Analysis,

    /// Code findings
    CodeFinding,

    /// Bug report
    BugReport,

    /// Performance metrics
    PerformanceMetrics,

    /// Test results
    TestResults,

    /// Configuration data
    Configuration,

    /// Intermediate result
    IntermediateResult,

    /// Final conclusion
    Conclusion,

    /// Custom type
    Custom(String),
}

/// Data payload variants
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub enum MemoryData {
    /// Text data
    Text(String),

    /// JSON data
    Json(serde_json::Value),

    /// Code snippet
    Code { language: String, code: String },

    /// Structured data
    Structured {
        data_type: String,
        content: HashMap<String, serde_json::Value>,
    },

    /// Binary data (base64 encoded)
    Binary { mime_type: String, data: String },

    /// Reference to external resource
    Reference { url: String, description: String },
}

/// Access control level
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[non_exhaustive]
pub enum AccessLevel {
    /// Everyone can read and write
    Public,

    /// Specified agents can read, anyone can write
    ProtectedRead { readers: Vec<String> },

    /// Specified agents can write, anyone can read
    ProtectedWrite { writers: Vec<String> },

    /// Specified agents can read and write
    Private {
        readers: Vec<String>,
        writers: Vec<String>,
    },

    /// Only creator can access
    Secret,
}

/// Memory conflict
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryConflict {
    pub entry_id: String,

    /// Conflicting versions
    pub versions: Vec<MemoryEntry>,

    pub conflict_type: ConflictType,

    pub detected_at: DateTime<Utc>,
}

/// Types of conflicts
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[non_exhaustive]
pub enum ConflictType {
    /// Concurrent writes
    ConcurrentWrite,

    /// Dependency cycle
    DependencyCycle,

    /// Access violation
    AccessViolation,

    /// Data inconsistency
    DataInconsistency,
}

/// Shared working memory
pub struct SharedWorkingMemory {
    /// Memory storage
    storage: HashMap<String, MemoryEntry>,

    /// Maximum entries
    max_entries: usize,

    /// Maximum size per entry (bytes)
    max_entry_size: usize,

    enable_conflict_detection: bool,

    pending_conflicts: Vec<MemoryConflict>,

    access_log: Vec<AccessLogEntry>,

    /// Memory statistics
    stats: MemoryStats,
}

/// Access log entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccessLogEntry {
    pub timestamp: DateTime<Utc>,

    pub agent_id: String,

    pub entry_id: String,

    /// Operation type
    pub operation: AccessOperation,

    pub success: bool,
}

/// Access operation type
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[non_exhaustive]
pub enum AccessOperation {
    Read,
    Write,
    Delete,
    Query,
}

/// Memory statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryStats {
    pub total_reads: usize,

    pub total_writes: usize,

    pub total_conflicts: usize,

    pub current_entries: usize,

    /// Memory usage (approximate bytes)
    pub memory_usage: usize,

    pub last_update: DateTime<Utc>,
}

impl Default for SharedWorkingMemory {
    fn default() -> Self {
        Self::new()
    }
}

impl SharedWorkingMemory {
    pub fn new() -> Self {
        Self {
            storage: HashMap::new(),
            max_entries: 1000,
            max_entry_size: 1024 * 1024, // 1MB
            enable_conflict_detection: true,
            pending_conflicts: Vec::new(),
            access_log: Vec::new(),
            stats: MemoryStats {
                total_reads: 0,
                total_writes: 0,
                total_conflicts: 0,
                current_entries: 0,
                memory_usage: 0,
                last_update: Utc::now(),
            },
        }
    }

    /// Write data to shared memory
    pub fn write(
        &mut self,
        agent_id: &str,
        entry_type: MemoryType,
        data: MemoryData,
        access: AccessLevel,
    ) -> Result<String> {
        let id = Uuid::new_v4().to_string();
        let now = Utc::now();

        let entry = MemoryEntry {
            id: id.clone(),
            agent_id: agent_id.to_string(),
            entry_type,
            data,
            created_at: now,
            modified_at: now,
            version: 1,
            access,
            dependencies: Vec::new(),
            confidence: 1.0,
            metadata: HashMap::new(),
        };

        // Check size limits
        let entry_size = serde_json::to_vec(&entry)
            .map_err(|e| {
                crate::workflow::WorkflowError::Validation(format!(
                    "entry serialization failed: {}",
                    e
                ))
            })?
            .len();
        if entry_size > self.max_entry_size {
            return Err(crate::workflow::WorkflowError::Validation(format!(
                "Entry size {} exceeds maximum {}",
                entry_size, self.max_entry_size
            )));
        }

        // Check entry limits
        if self.storage.len() >= self.max_entries {
            self.evict_oldest()?;
        }

        // Store entry
        self.storage.insert(id.clone(), entry.clone());

        // Update stats
        self.stats.total_writes = self.stats.total_writes.saturating_add(1);
        self.stats.current_entries = self.storage.len();
        self.stats.memory_usage = self.stats.memory_usage.saturating_add(entry_size);
        self.stats.last_update = now;

        // Log access
        self.log_access(agent_id, &id, AccessOperation::Write, true);

        Ok(id)
    }

    /// Read data from shared memory
    pub fn read(&mut self, agent_id: &str, entry_id: &str) -> Result<MemoryEntry> {
        // Check access permissions
        if !self.check_access(agent_id, entry_id, AccessOperation::Read) {
            self.log_access(agent_id, entry_id, AccessOperation::Read, false);
            return Err(crate::workflow::WorkflowError::Validation(
                "Access denied".to_string(),
            ));
        }

        let entry = self
            .storage
            .get(entry_id)
            .ok_or_else(|| crate::workflow::WorkflowError::NotFound(entry_id.to_string()))?
            .clone();

        // Update stats
        self.stats.total_reads = self.stats.total_reads.saturating_add(1);
        self.stats.last_update = Utc::now();

        // Log access
        self.log_access(agent_id, entry_id, AccessOperation::Read, true);

        Ok(entry)
    }

    /// Update existing entry
    pub fn update(&mut self, agent_id: &str, entry_id: &str, data: MemoryData) -> Result<()> {
        // Check access permissions
        if !self.check_access(agent_id, entry_id, AccessOperation::Write) {
            self.log_access(agent_id, entry_id, AccessOperation::Write, false);
            return Err(crate::workflow::WorkflowError::Validation(
                "Access denied".to_string(),
            ));
        }

        // Check for conflicts if enabled (need to clone data for conflict detection)
        if self.enable_conflict_detection {
            if let Some(existing_entry) = self.storage.get(entry_id) {
                if let Some(conflict) = self.detect_conflict(existing_entry, &data) {
                    self.pending_conflicts.push(conflict);
                    self.stats.total_conflicts = self.stats.total_conflicts.saturating_add(1);
                }
            }
        }

        let entry = self
            .storage
            .get_mut(entry_id)
            .ok_or_else(|| crate::workflow::WorkflowError::NotFound(entry_id.to_string()))?;

        // Track old size for memory_usage delta
        let old_size = serde_json::to_vec(&*entry)
            .map(|v| v.len())
            .unwrap_or_else(|e| {
                tracing::debug!("Failed to serialize entry for size tracking: {e}");
                0
            });

        // Update entry
        entry.data = data;
        entry.modified_at = Utc::now();
        entry.version = entry.version.saturating_add(1);

        // Track new size
        let new_size = serde_json::to_vec(&*entry)
            .map(|v| v.len())
            .unwrap_or_else(|e| {
                tracing::debug!("Failed to serialize entry for size tracking: {e}");
                0
            });

        // Update stats — adjust memory_usage by delta
        self.stats.total_writes = self.stats.total_writes.saturating_add(1);
        self.stats.memory_usage = self
            .stats
            .memory_usage
            .saturating_sub(old_size)
            .saturating_add(new_size);
        self.stats.last_update = Utc::now();

        // Log access
        self.log_access(agent_id, entry_id, AccessOperation::Write, true);

        Ok(())
    }

    /// Query entries by type and agent
    pub fn query(
        &mut self,
        agent_id: &str,
        entry_type: Option<MemoryType>,
        agent_filter: Option<&str>,
    ) -> Result<Vec<MemoryEntry>> {
        let mut results = Vec::new();

        for entry in self.storage.values() {
            // Check access
            if !self.check_access(agent_id, &entry.id, AccessOperation::Read) {
                continue;
            }

            // Filter by type
            if let Some(ref et) = entry_type {
                if &entry.entry_type != et {
                    continue;
                }
            }

            // Filter by agent
            if let Some(agent) = agent_filter {
                if entry.agent_id != agent {
                    continue;
                }
            }

            results.push(entry.clone());
        }

        // Update stats
        self.stats.last_update = Utc::now();
        self.log_access(agent_id, "query", AccessOperation::Query, true);

        Ok(results)
    }

    /// Delete entry
    pub fn delete(&mut self, agent_id: &str, entry_id: &str) -> Result<()> {
        // Check access permissions
        if !self.check_access(agent_id, entry_id, AccessOperation::Delete) {
            self.log_access(agent_id, entry_id, AccessOperation::Delete, false);
            return Err(crate::workflow::WorkflowError::Validation(
                "Access denied".to_string(),
            ));
        }

        let removed = self
            .storage
            .remove(entry_id)
            .ok_or_else(|| crate::workflow::WorkflowError::NotFound(entry_id.to_string()))?;

        // Update stats
        let removed_size = serde_json::to_vec(&removed)
            .map(|v| v.len())
            .unwrap_or_else(|e| {
                tracing::debug!("Failed to serialize removed entry for size tracking: {e}");
                0
            });
        self.stats.memory_usage = self.stats.memory_usage.saturating_sub(removed_size);
        self.stats.current_entries = self.storage.len();
        self.stats.last_update = Utc::now();

        // Log access
        self.log_access(agent_id, entry_id, AccessOperation::Delete, true);

        Ok(())
    }

    /// Check access permissions
    fn check_access(&self, agent_id: &str, entry_id: &str, operation: AccessOperation) -> bool {
        let entry = match self.storage.get(entry_id) {
            Some(e) => e,
            None => return false,
        };

        match &entry.access {
            AccessLevel::Public => true,
            AccessLevel::ProtectedRead { readers } => match operation {
                AccessOperation::Read => readers.contains(&agent_id.to_string()),
                AccessOperation::Write => true,
                _ => false,
            },
            AccessLevel::ProtectedWrite { writers } => match operation {
                AccessOperation::Read => true,
                AccessOperation::Write => writers.contains(&agent_id.to_string()),
                _ => false,
            },
            AccessLevel::Private { readers, writers } => match operation {
                AccessOperation::Read => readers.contains(&agent_id.to_string()),
                AccessOperation::Write => writers.contains(&agent_id.to_string()),
                _ => false,
            },
            AccessLevel::Secret => entry.agent_id == agent_id,
        }
    }

    /// Detect conflicts by checking version delta
    fn detect_conflict(
        &self,
        entry: &MemoryEntry,
        _new_data: &MemoryData,
    ) -> Option<MemoryConflict> {
        if entry.version > 0 {
            let age = Utc::now()
                .signed_duration_since(entry.modified_at)
                .num_seconds();
            if age < 5 {
                return Some(MemoryConflict {
                    entry_id: entry.id.clone(),
                    versions: vec![entry.clone()],
                    conflict_type: ConflictType::ConcurrentWrite,
                    detected_at: Utc::now(),
                });
            }
        }
        None
    }

    /// Evict oldest entry
    fn evict_oldest(&mut self) -> Result<()> {
        let oldest_id = self
            .storage
            .values()
            .min_by_key(|e| e.created_at)
            .map(|e| e.id.clone());

        if let Some(id) = oldest_id {
            if let Some(removed) = self.storage.remove(&id) {
                let removed_size = serde_json::to_vec(&removed)
                    .map(|v| v.len())
                    .unwrap_or_else(|e| {
                        tracing::debug!("Failed to serialize evicted entry for size tracking: {e}");
                        0
                    });
                self.stats.memory_usage = self.stats.memory_usage.saturating_sub(removed_size);
            }
            self.stats.current_entries = self.storage.len();
            Ok(())
        } else {
            Err(crate::workflow::WorkflowError::Validation(
                "No entries to evict".to_string(),
            ))
        }
    }

    /// Log access
    fn log_access(
        &mut self,
        agent_id: &str,
        entry_id: &str,
        operation: AccessOperation,
        success: bool,
    ) {
        let log_entry = AccessLogEntry {
            timestamp: Utc::now(),
            agent_id: agent_id.to_string(),
            entry_id: entry_id.to_string(),
            operation,
            success,
        };

        self.access_log.push(log_entry);

        // Keep log size manageable
        if self.access_log.len() > 1000 {
            self.access_log.drain(0..100);
        }
    }

    /// Get memory statistics
    pub fn stats(&self) -> &MemoryStats {
        &self.stats
    }

    /// Get pending conflicts
    pub fn conflicts(&self) -> &[MemoryConflict] {
        &self.pending_conflicts
    }

    /// Resolve conflict
    pub fn resolve_conflict(&mut self, conflict_id: usize, resolution: &MemoryEntry) -> Result<()> {
        if conflict_id >= self.pending_conflicts.len() {
            return Err(crate::workflow::WorkflowError::NotFound(
                conflict_id.to_string(),
            ));
        }

        let conflict = &self.pending_conflicts[conflict_id];
        self.storage
            .insert(conflict.entry_id.clone(), resolution.clone());
        self.pending_conflicts.remove(conflict_id);

        Ok(())
    }

    /// Clear all entries
    pub fn clear(&mut self) {
        self.storage.clear();
        self.stats.current_entries = 0;
        self.stats.memory_usage = 0;
        self.stats.last_update = Utc::now();
    }

    /// Get entry count
    pub fn entry_count(&self) -> usize {
        self.storage.len()
    }

    /// Export memory state
    pub fn export(&self) -> Result<Vec<MemoryEntry>> {
        Ok(self.storage.values().cloned().collect())
    }

    /// Import memory entries
    pub fn import(&mut self, entries: Vec<MemoryEntry>) -> Result<()> {
        for entry in entries {
            self.storage.insert(entry.id.clone(), entry);
        }
        self.stats.current_entries = self.storage.len();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_shared_memory_creation() {
        let memory = SharedWorkingMemory::new();
        assert_eq!(memory.entry_count(), 0);
    }

    #[test]
    fn test_write_and_read() {
        let mut memory = SharedWorkingMemory::new();

        let data = MemoryData::Text("Hello, World!".to_string());
        let id = memory
            .write("agent1", MemoryType::Analysis, data, AccessLevel::Public)
            .unwrap();

        let entry = memory.read("agent2", &id).unwrap();
        assert_eq!(entry.agent_id, "agent1");
        assert!(matches!(entry.data, MemoryData::Text(_)));
    }

    #[test]
    fn test_access_control() {
        let mut memory = SharedWorkingMemory::new();

        let data = MemoryData::Text("Secret data".to_string());
        let id = memory
            .write("agent1", MemoryType::Analysis, data, AccessLevel::Secret)
            .unwrap();

        // Agent2 should not be able to read agent1's secret
        let result = memory.read("agent2", &id);
        assert!(result.is_err());
    }

    #[test]
    fn test_query_by_type() {
        let mut memory = SharedWorkingMemory::new();

        memory
            .write(
                "agent1",
                MemoryType::Analysis,
                MemoryData::Text("Analysis 1".to_string()),
                AccessLevel::Public,
            )
            .unwrap();

        memory
            .write(
                "agent1",
                MemoryType::CodeFinding,
                MemoryData::Text("Finding 1".to_string()),
                AccessLevel::Public,
            )
            .unwrap();

        let results = memory
            .query("agent1", Some(MemoryType::Analysis), None)
            .unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].entry_type, MemoryType::Analysis);
    }

    #[test]
    fn test_update_entry() {
        let mut memory = SharedWorkingMemory::new();

        let data = MemoryData::Text("Original".to_string());
        let id = memory
            .write("agent1", MemoryType::Analysis, data, AccessLevel::Public)
            .unwrap();

        memory
            .update("agent1", &id, MemoryData::Text("Updated".to_string()))
            .unwrap();

        let entry = memory.read("agent1", &id).unwrap();
        assert_eq!(entry.version, 2);
    }

    #[test]
    fn test_delete_entry() {
        let mut memory = SharedWorkingMemory::new();

        let data = MemoryData::Text("To be deleted".to_string());
        let id = memory
            .write("agent1", MemoryType::Analysis, data, AccessLevel::Public)
            .unwrap();

        assert_eq!(memory.entry_count(), 1);

        memory.delete("agent1", &id).unwrap();
        assert_eq!(memory.entry_count(), 0);
    }

    #[test]
    fn test_memory_stats() {
        let mut memory = SharedWorkingMemory::new();

        memory
            .write(
                "agent1",
                MemoryType::Analysis,
                MemoryData::Text("Test".to_string()),
                AccessLevel::Public,
            )
            .unwrap();

        memory.read("agent1", "dummy").unwrap_err(); // This will fail but should count as a read attempt

        let stats = memory.stats();
        assert_eq!(stats.current_entries, 1);
        assert!(stats.total_writes > 0);
    }

    // =========================================================================
    // Terminal-bench: 15 additional tests for shared_memory
    // =========================================================================

    // 1. MemoryType serde roundtrip for all variants
    #[test]
    fn memory_type_serde_roundtrip() {
        let variants: Vec<MemoryType> = vec![
            MemoryType::Analysis,
            MemoryType::CodeFinding,
            MemoryType::BugReport,
            MemoryType::PerformanceMetrics,
            MemoryType::TestResults,
            MemoryType::Configuration,
            MemoryType::IntermediateResult,
            MemoryType::Conclusion,
            MemoryType::Custom("my_type".to_string()),
        ];
        for v in &variants {
            let json = serde_json::to_string(v).unwrap();
            let decoded: MemoryType = serde_json::from_str(&json).unwrap();
            let json2 = serde_json::to_string(&decoded).unwrap();
            assert_eq!(json, json2);
        }
    }

    // 2. MemoryData serde roundtrip for all variants
    #[test]
    fn memory_data_serde_roundtrip() {
        let variants: Vec<MemoryData> = vec![
            MemoryData::Text("hello".to_string()),
            MemoryData::Json(serde_json::json!({"key": "value"})),
            MemoryData::Code {
                language: "rust".to_string(),
                code: "fn main() {}".to_string(),
            },
            MemoryData::Binary {
                mime_type: "image/png".to_string(),
                data: "base64data".to_string(),
            },
            MemoryData::Reference {
                url: "https://example.com".to_string(),
                description: "example".to_string(),
            },
        ];
        for v in &variants {
            let json = serde_json::to_string(v).unwrap();
            let decoded: MemoryData = serde_json::from_str(&json).unwrap();
            let json2 = serde_json::to_string(&decoded).unwrap();
            assert_eq!(json, json2);
        }

        // Structured variant with HashMap (non-deterministic key order)
        let structured = MemoryData::Structured {
            data_type: "metrics".to_string(),
            content: {
                let mut m = HashMap::new();
                m.insert("cpu".to_string(), serde_json::json!(85));
                m.insert("mem".to_string(), serde_json::json!(4096));
                m
            },
        };
        let json = serde_json::to_string(&structured).unwrap();
        let decoded: MemoryData = serde_json::from_str(&json).unwrap();
        if let MemoryData::Structured { data_type, content } = decoded {
            assert_eq!(data_type, "metrics");
            assert_eq!(content.len(), 2);
        } else {
            panic!("Expected Structured variant");
        }
    }

    // 3. AccessLevel serde roundtrip for all variants
    #[test]
    fn access_level_serde_roundtrip() {
        let variants: Vec<AccessLevel> = vec![
            AccessLevel::Public,
            AccessLevel::ProtectedRead {
                readers: vec!["agent1".to_string()],
            },
            AccessLevel::ProtectedWrite {
                writers: vec!["agent1".to_string()],
            },
            AccessLevel::Private {
                readers: vec!["agent1".to_string()],
                writers: vec!["agent2".to_string()],
            },
            AccessLevel::Secret,
        ];
        for v in &variants {
            let json = serde_json::to_string(v).unwrap();
            let decoded: AccessLevel = serde_json::from_str(&json).unwrap();
            let json2 = serde_json::to_string(&decoded).unwrap();
            assert_eq!(json, json2);
        }
    }

    // 4. ConflictType serde roundtrip
    #[test]
    fn conflict_type_serde_roundtrip() {
        let variants = [
            ConflictType::ConcurrentWrite,
            ConflictType::DependencyCycle,
            ConflictType::AccessViolation,
            ConflictType::DataInconsistency,
        ];
        for v in &variants {
            let json = serde_json::to_string(v).unwrap();
            let decoded: ConflictType = serde_json::from_str(&json).unwrap();
            let json2 = serde_json::to_string(&decoded).unwrap();
            assert_eq!(json, json2);
        }
    }

    // 5. AccessOperation serde roundtrip
    #[test]
    fn access_operation_serde_roundtrip() {
        let ops = [
            AccessOperation::Read,
            AccessOperation::Write,
            AccessOperation::Delete,
            AccessOperation::Query,
        ];
        for op in &ops {
            let json = serde_json::to_string(op).unwrap();
            let decoded: AccessOperation = serde_json::from_str(&json).unwrap();
            let json2 = serde_json::to_string(&decoded).unwrap();
            assert_eq!(json, json2);
        }
    }

    // 6. MemoryStats serde roundtrip
    #[test]
    fn memory_stats_serde_roundtrip() {
        let stats = MemoryStats {
            total_reads: 42,
            total_writes: 17,
            total_conflicts: 3,
            current_entries: 10,
            memory_usage: 4096,
            last_update: Utc::now(),
        };
        let json = serde_json::to_string(&stats).unwrap();
        let decoded: MemoryStats = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.total_reads, 42);
        assert_eq!(decoded.total_writes, 17);
        assert_eq!(decoded.total_conflicts, 3);
        assert_eq!(decoded.current_entries, 10);
        assert_eq!(decoded.memory_usage, 4096);
    }

    // 7. MemoryConflict serde roundtrip
    #[test]
    fn memory_conflict_serde_roundtrip() {
        let conflict = MemoryConflict {
            entry_id: "entry_123".to_string(),
            versions: vec![],
            conflict_type: ConflictType::ConcurrentWrite,
            detected_at: Utc::now(),
        };
        let json = serde_json::to_string(&conflict).unwrap();
        let decoded: MemoryConflict = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.entry_id, "entry_123");
        assert_eq!(decoded.conflict_type, ConflictType::ConcurrentWrite);
    }

    // 8. AccessLogEntry serde roundtrip
    #[test]
    fn access_log_entry_serde_roundtrip() {
        let entry = AccessLogEntry {
            timestamp: Utc::now(),
            agent_id: "agent1".to_string(),
            entry_id: "entry_42".to_string(),
            operation: AccessOperation::Write,
            success: true,
        };
        let json = serde_json::to_string(&entry).unwrap();
        let decoded: AccessLogEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.agent_id, "agent1");
        assert_eq!(decoded.entry_id, "entry_42");
        assert_eq!(decoded.operation, AccessOperation::Write);
        assert!(decoded.success);
    }

    // 9. Write with all MemoryData variants stores and reads back correctly
    #[test]
    fn write_and_read_all_data_types() {
        let mut memory = SharedWorkingMemory::new();

        let test_data: Vec<MemoryData> = vec![
            MemoryData::Text("hello".to_string()),
            MemoryData::Json(serde_json::json!({"x": 1})),
            MemoryData::Code {
                language: "go".to_string(),
                code: "func main() {}".to_string(),
            },
        ];

        for data in &test_data {
            let id = memory
                .write(
                    "agent1",
                    MemoryType::Analysis,
                    data.clone(),
                    AccessLevel::Public,
                )
                .unwrap();
            let entry = memory.read("agent1", &id).unwrap();
            let json1 = serde_json::to_string(&entry.data).unwrap();
            let json2 = serde_json::to_string(data).unwrap();
            assert_eq!(json1, json2);
        }
    }

    // 10. ProtectedWrite access level: only writers can write, everyone can read
    #[test]
    fn protected_write_access_control() {
        let mut memory = SharedWorkingMemory::new();

        let id = memory
            .write(
                "agent1",
                MemoryType::Analysis,
                MemoryData::Text("data".to_string()),
                AccessLevel::ProtectedWrite {
                    writers: vec!["agent1".to_string()],
                },
            )
            .unwrap();

        // agent2 can read (anyone can read ProtectedWrite)
        let entry = memory.read("agent2", &id).unwrap();
        assert_eq!(entry.agent_id, "agent1");

        // agent2 cannot update (not a writer)
        let result = memory.update("agent2", &id, MemoryData::Text("hacked".to_string()));
        assert!(result.is_err());

        // agent1 can update (is a writer)
        memory
            .update("agent1", &id, MemoryData::Text("updated".to_string()))
            .unwrap();
    }

    // 11. Query with agent filter returns only matching entries
    #[test]
    fn query_by_agent_filter() {
        let mut memory = SharedWorkingMemory::new();

        memory
            .write(
                "alice",
                MemoryType::Analysis,
                MemoryData::Text("a1".into()),
                AccessLevel::Public,
            )
            .unwrap();
        memory
            .write(
                "bob",
                MemoryType::Analysis,
                MemoryData::Text("b1".into()),
                AccessLevel::Public,
            )
            .unwrap();
        memory
            .write(
                "alice",
                MemoryType::CodeFinding,
                MemoryData::Text("a2".into()),
                AccessLevel::Public,
            )
            .unwrap();

        // Filter by agent "alice"
        let results = memory.query("alice", None, Some("alice")).unwrap();
        assert_eq!(results.len(), 2);
        assert!(results.iter().all(|r| r.agent_id == "alice"));

        // Filter by agent "bob"
        let results = memory.query("bob", None, Some("bob")).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].agent_id, "bob");
    }

    // 12. Clear removes all entries and resets counters
    #[test]
    fn clear_resets_storage() {
        let mut memory = SharedWorkingMemory::new();
        memory
            .write(
                "a1",
                MemoryType::Analysis,
                MemoryData::Text("x".into()),
                AccessLevel::Public,
            )
            .unwrap();
        memory
            .write(
                "a1",
                MemoryType::Analysis,
                MemoryData::Text("y".into()),
                AccessLevel::Public,
            )
            .unwrap();
        assert_eq!(memory.entry_count(), 2);

        memory.clear();
        assert_eq!(memory.entry_count(), 0);

        let stats = memory.stats();
        assert_eq!(stats.current_entries, 0);
        assert_eq!(stats.memory_usage, 0);
    }

    // 13. Export and import round-trip preserves entries
    #[test]
    fn export_import_roundtrip() {
        let mut memory = SharedWorkingMemory::new();
        memory
            .write(
                "a1",
                MemoryType::BugReport,
                MemoryData::Text("bug1".into()),
                AccessLevel::Public,
            )
            .unwrap();
        memory
            .write(
                "a2",
                MemoryType::TestResults,
                MemoryData::Json(serde_json::json!(42)),
                AccessLevel::Public,
            )
            .unwrap();

        let exported = memory.export().unwrap();
        assert_eq!(exported.len(), 2);

        let mut memory2 = SharedWorkingMemory::new();
        memory2.import(exported).unwrap();
        assert_eq!(memory2.entry_count(), 2);
    }

    // 14. Default trait creates empty memory
    #[test]
    fn default_is_empty() {
        let memory = SharedWorkingMemory::default();
        assert_eq!(memory.entry_count(), 0);
        assert!(memory.conflicts().is_empty());
    }

    // 15. Reading a nonexistent entry returns NotFound error
    #[test]
    fn read_nonexistent_returns_not_found() {
        let mut memory = SharedWorkingMemory::new();
        let result = memory.read("agent1", "does-not-exist");
        assert!(result.is_err());
    }

    // 16. Conflict detection fires for rapid successive updates
    #[test]
    fn conflict_detected_for_rapid_update() {
        let mut memory = SharedWorkingMemory::new();
        let data = MemoryData::Text("initial".to_string());
        let id = memory
            .write("agent1", MemoryType::Analysis, data, AccessLevel::Public)
            .unwrap();

        // First update
        memory
            .update("agent1", &id, MemoryData::Text("update1".to_string()))
            .unwrap();

        // Rapid second update (within 5s) should trigger conflict
        memory
            .update("agent1", &id, MemoryData::Text("update2".to_string()))
            .unwrap();

        let conflicts = memory.conflicts();
        assert!(
            !conflicts.is_empty(),
            "Expected conflict detection for rapid successive updates"
        );
        assert_eq!(conflicts[0].entry_id, id);
        assert!(matches!(
            conflicts[0].conflict_type,
            ConflictType::ConcurrentWrite
        ));
    }

    // 17. No conflict when updates are well-spaced (version > 0 but old)
    #[test]
    fn no_conflict_for_old_entry() {
        let mut memory = SharedWorkingMemory::new();
        let data = MemoryData::Text("initial".to_string());
        let id = memory
            .write("agent1", MemoryType::Analysis, data, AccessLevel::Public)
            .unwrap();

        // Manually set modified_at to well in the past
        if let Some(entry) = memory.storage.get_mut(&id) {
            entry.modified_at = Utc::now() - chrono::Duration::seconds(60);
        }

        // Update should NOT trigger conflict (entry is old)
        memory
            .update("agent1", &id, MemoryData::Text("update1".to_string()))
            .unwrap();

        let conflicts = memory.conflicts();
        assert!(
            conflicts.is_empty(),
            "No conflict expected for old entry, got {}",
            conflicts.len()
        );
    }
}
