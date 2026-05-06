//! Automatic memory management for RustyCode TUI
//!
//! This module provides intelligent auto-save and auto-retrieval of important
//! information without requiring manual user intervention.

use anyhow::Result;
use chrono::{DateTime, Utc};
use rustycode_memory::{MemoryDomain, MemoryEntry, MemoryEntryConfig, MemoryScope, MemorySource};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use tracing::{debug, info, warn};

/// Maximum number of auto-memories to store before cleanup
const MAX_AUTO_MEMORIES: usize = 1000;

/// Auto-memory storage file
const AUTO_MEMORY_FILE: &str = ".rustycode/auto-memory.json";

/// Type of auto-memory
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum MemoryType {
    /// User preferences (theme, model, input mode)
    Preference,
    /// Important decisions (architecture choices, file selections)
    Decision,
    /// Errors and their solutions (for learning)
    Error,
    /// Working context (recent files, common operations)
    Context,
    /// Repeated patterns (frequently used combinations)
    Pattern,
}

impl MemoryType {
    /// Get default importance score for this memory type
    fn default_importance(&self) -> f64 {
        match self {
            MemoryType::Preference => 0.9, // High importance
            MemoryType::Decision => 0.8,   // High importance
            MemoryType::Error => 0.7,      // Medium-high importance
            MemoryType::Context => 0.5,    // Medium importance
            MemoryType::Pattern => 0.6,    // Medium importance
        }
    }

    /// Convert to MemoryDomain for integration with memory system
    fn to_domain(&self) -> MemoryDomain {
        match self {
            MemoryType::Preference => MemoryDomain::Workflow,
            MemoryType::Decision => MemoryDomain::Architecture,
            MemoryType::Error => MemoryDomain::Debugging,
            MemoryType::Context => MemoryDomain::ProjectSpecific,
            MemoryType::Pattern => MemoryDomain::CodeStyle,
        }
    }
}

/// Auto-memory entry with importance scoring
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutoMemory {
    /// Unique identifier
    pub id: String,
    /// Memory key (for lookups)
    pub key: String,
    /// Memory value
    pub value: String,
    pub memory_type: MemoryType,
    /// Importance score (0.0 to 1.0)
    pub importance: f64,
    /// When this memory was created
    pub created_at: DateTime<Utc>,
    /// When this memory was last accessed
    pub accessed_at: DateTime<Utc>,
    /// Number of times this memory has been accessed
    pub access_count: usize,
    /// Optional metadata
    pub metadata: HashMap<String, String>,
}

impl AutoMemory {
    pub fn new(key: impl Into<String>, value: impl Into<String>, memory_type: MemoryType) -> Self {
        let key = key.into();
        let value = value.into();
        let importance = memory_type.default_importance();
        let now = Utc::now();

        Self {
            id: format!("auto-{}", uuid::Uuid::new_v4()),
            key,
            value,
            memory_type,
            importance,
            created_at: now,
            accessed_at: now,
            access_count: 0,
            metadata: HashMap::new(),
        }
    }

    /// Update importance based on access pattern
    pub fn update_importance(&mut self) {
        // Decay importance over time, but boost with access
        let age_days = (Utc::now() - self.created_at).num_days();
        let decay = 1.0 / (1.0 + age_days as f64 * 0.1);

        // Boost based on access frequency
        let access_boost = 1.0 + (self.access_count as f64 * 0.05);

        self.importance = self.memory_type.default_importance() * decay * access_boost;
        self.importance = self.importance.min(1.0); // Cap at 1.0
    }

    /// Record an access to this memory
    pub fn record_access(&mut self) {
        self.accessed_at = Utc::now();
        self.access_count += 1;
        self.update_importance();
    }

    pub fn should_cleanup(&self) -> bool {
        // Keep important memories
        if self.importance > 0.7 {
            return false;
        }

        // Cleanup old, low-importance memories
        let age_days = (Utc::now() - self.created_at).num_days();
        age_days > 30 && self.importance < 0.3
    }

    /// Add metadata to this memory
    pub fn with_metadata(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.metadata.insert(key.into(), value.into());
        self
    }
}

/// Auto-memory manager
pub struct AutoMemoryManager {
    /// Auto-memory storage path
    storage_path: PathBuf,
    /// In-memory cache of auto-memories
    memories: Vec<AutoMemory>,
    /// Memory directory for persistent storage
    memory_dir: PathBuf,
}

impl AutoMemoryManager {
    pub fn new(cwd: &Path) -> Result<Self> {
        let memory_dir = rustycode_memory::memory_dir(cwd);
        let storage_path = cwd.join(AUTO_MEMORY_FILE);

        // Load existing auto-memories
        let memories = Self::load_from_disk(&storage_path).unwrap_or_default();

        info!(
            "AutoMemoryManager initialized with {} memories",
            memories.len()
        );

        Ok(Self {
            storage_path,
            memories,
            memory_dir,
        })
    }

    /// Load auto-memories from disk
    fn load_from_disk(path: &Path) -> Result<Vec<AutoMemory>> {
        if !path.exists() {
            return Ok(Vec::new());
        }

        let content = fs::read_to_string(path)?;
        let memories: Vec<AutoMemory> = serde_json::from_str(&content).unwrap_or_else(|e| {
            warn!("Failed to parse auto-memory file, creating new: {}", e);
            Vec::new()
        });

        Ok(memories)
    }

    /// Save auto-memories to disk
    fn save_to_disk(&self) -> Result<()> {
        // Ensure parent directory exists
        if let Some(parent) = self.storage_path.parent() {
            fs::create_dir_all(parent)?;
        }

        let content = serde_json::to_string_pretty(&self.memories)?;
        fs::write(&self.storage_path, content)?;

        debug!("Saved {} auto-memories to disk", self.memories.len());
        Ok(())
    }

    /// Add an auto-memory
    pub fn add_memory(&mut self, memory: AutoMemory) -> Result<()> {
        // Check for existing memory with same key and type
        if let Some(existing) = self
            .memories
            .iter()
            .position(|m| m.key == memory.key && m.memory_type == memory.memory_type)
        {
            // Update existing memory
            debug!("Updating existing auto-memory: {}", memory.key);
            self.memories[existing] = memory;
        } else {
            // Add new memory
            debug!("Adding new auto-memory: {}", memory.key);
            self.memories.push(memory);
        }

        // Cleanup if needed
        self.cleanup_if_needed()?;

        // Persist to disk
        self.save_to_disk()?;

        Ok(())
    }

    /// Get a memory by key and type
    pub fn memory(&mut self, key: &str, memory_type: MemoryType) -> Option<AutoMemory> {
        if let Some(idx) = self
            .memories
            .iter()
            .position(|m| m.key == key && m.memory_type == memory_type)
        {
            // Record access
            self.memories[idx].record_access();
            if let Err(e) = self.save_to_disk() {
                tracing::debug!("Auto-memory best-effort save failed: {}", e);
            }
            Some(self.memories[idx].clone())
        } else {
            None
        }
    }

    /// Fetch memories of a specific type, recording access and persisting to disk.
    pub fn fetch_memories_by_type(&mut self, memory_type: MemoryType) -> Vec<AutoMemory> {
        // Record access for all memories of this type
        for memory in &mut self.memories {
            if memory.memory_type == memory_type {
                memory.record_access();
            }
        }

        if let Err(e) = self.save_to_disk() {
            tracing::debug!("Auto-memory best-effort save failed: {}", e);
        }

        self.memories
            .iter()
            .filter(|m| m.memory_type == memory_type)
            .cloned()
            .collect()
    }

    /// Fetch recent memories, recording access and persisting to disk.
    pub fn fetch_recent_memories(&mut self, days: i64) -> Vec<AutoMemory> {
        let cutoff = Utc::now() - chrono::Duration::days(days);

        // Record access for recent memories
        for memory in &mut self.memories {
            if memory.accessed_at > cutoff {
                memory.record_access();
            }
        }

        if let Err(e) = self.save_to_disk() {
            tracing::debug!("Auto-memory best-effort save failed: {}", e);
        }

        self.memories
            .iter()
            .filter(|m| m.accessed_at > cutoff)
            .cloned()
            .collect()
    }

    /// Fetch important memories, recording access and persisting to disk.
    pub fn fetch_important_memories(&mut self, threshold: f64) -> Vec<AutoMemory> {
        // Record access for important memories
        for memory in &mut self.memories {
            if memory.importance >= threshold {
                memory.record_access();
            }
        }

        if let Err(e) = self.save_to_disk() {
            tracing::debug!("Auto-memory best-effort save failed: {}", e);
        }

        self.memories
            .iter()
            .filter(|m| m.importance >= threshold)
            .cloned()
            .collect()
    }

    /// Get memory count
    pub fn memory_count(&self) -> usize {
        self.memories.len()
    }

    /// Cleanup old/low-importance memories
    pub fn cleanup(&mut self) -> Result<usize> {
        let initial_count = self.memories.len();

        // Remove low-value memories
        self.memories.retain(|m| !m.should_cleanup());

        // If still over limit, remove oldest low-importance memories
        if self.memories.len() > MAX_AUTO_MEMORIES {
            // Sort by importance and recency
            self.memories.sort_by(|a, b| {
                // Primary sort: importance
                let imp_cmp = b
                    .importance
                    .partial_cmp(&a.importance)
                    .unwrap_or(std::cmp::Ordering::Equal);

                // Secondary sort: accessed time
                if imp_cmp == std::cmp::Ordering::Equal {
                    b.accessed_at.cmp(&a.accessed_at)
                } else {
                    imp_cmp
                }
            });

            // Keep only the most important ones
            self.memories.truncate(MAX_AUTO_MEMORIES);
        }

        let removed = initial_count - self.memories.len();

        if removed > 0 {
            info!("Cleaned up {} auto-memories", removed);
            self.save_to_disk()?;
        }

        Ok(removed)
    }

    /// Cleanup only if needed (over limit)
    fn cleanup_if_needed(&mut self) -> Result<()> {
        if self.memories.len() > MAX_AUTO_MEMORIES {
            self.cleanup()?;
        }
        Ok(())
    }

    /// Sync important auto-memories to persistent memory system
    pub fn sync_to_memory(&self) -> Result<()> {
        let important_memories: Vec<_> = self
            .memories
            .iter()
            .filter(|m| m.importance > 0.7)
            .collect();

        for auto_mem in important_memories {
            // Skip if already exists in memory
            let exists = self.memory_entry_exists(auto_mem)?;
            if exists {
                continue;
            }

            // Create memory entry
            let config = MemoryEntryConfig {
                id: auto_mem.id.clone(),
                trigger: auto_mem.key.clone(),
                confidence: auto_mem.importance as f32,
                domain: auto_mem.memory_type.to_domain(),
                source: MemorySource::SessionObservation, // Use existing variant
                scope: MemoryScope::Global,
                project_id: None,
                action: auto_mem.value.clone(),
            };
            let entry = MemoryEntry::new(config);

            rustycode_memory::add_entry(&self.memory_dir, entry)?;
            debug!("Synced auto-memory to persistent storage: {}", auto_mem.key);
        }

        Ok(())
    }

    fn memory_entry_exists(&self, auto_mem: &AutoMemory) -> Result<bool> {
        let entries = rustycode_memory::load(&self.memory_dir)?;
        Ok(entries
            .iter()
            .any(|e| e.id == format!("auto-{}", auto_mem.id)))
    }

    /// Get smart get_suggestions based on context
    pub fn get_suggestions(&self, context: &str) -> Vec<String> {
        let mut get_suggestions = Vec::new();

        // Find relevant memories based on context keywords
        let context_lower = context.to_lowercase();

        for memory in &self.memories {
            // Check if memory key or value relates to context
            let key_lower = memory.key.to_lowercase();
            let value_lower = memory.value.to_lowercase();

            if (key_lower.contains(&context_lower) || value_lower.contains(&context_lower))
                && memory.importance > 0.5
            {
                get_suggestions.push(format!("💡 {}: {}", memory.key, memory.value));
            }
        }

        // Sort by importance
        get_suggestions.sort_by(|a, b| {
            // Extract importance from suggestion text (simplified)
            b.cmp(a) // Reverse alphabetical for now
        });

        get_suggestions.truncate(5); // Max 5 get_suggestions
        get_suggestions
    }
}

/// Thread-safe auto-memory manager wrapper
pub struct ThreadSafeAutoMemory(Arc<Mutex<AutoMemoryManager>>);

impl ThreadSafeAutoMemory {
    pub fn new(cwd: &Path) -> Result<Self> {
        Ok(Self(Arc::new(Mutex::new(AutoMemoryManager::new(cwd)?))))
    }

    /// Add a preference memory
    pub fn save_preference(&self, key: impl Into<String>, value: impl Into<String>) -> Result<()> {
        let memory = AutoMemory::new(key, value, MemoryType::Preference);
        self.0
            .lock()
            .map_err(|_| anyhow::anyhow!("Lock poisoned"))?
            .add_memory(memory)
    }

    /// Save an error with solution
    pub fn save_error(&self, error: impl Into<String>, solution: impl Into<String>) -> Result<()> {
        let error_str = error.into();
        let key = format!("error:{}", error_str);
        let value = format!("Error: {}. Solution: {}", error_str, solution.into());
        let memory = AutoMemory::new(key, value, MemoryType::Error);
        self.0
            .lock()
            .map_err(|_| anyhow::anyhow!("Lock poisoned"))?
            .add_memory(memory)
    }

    /// Save a decision
    pub fn save_decision(
        &self,
        decision: impl Into<String>,
        reasoning: impl Into<String>,
    ) -> Result<()> {
        let decision_str = decision.into();
        let key = format!("decision:{}", decision_str);
        let value = format!(
            "Decision: {}. Reasoning: {}",
            decision_str,
            reasoning.into()
        );
        let memory = AutoMemory::new(key, value, MemoryType::Decision);
        self.0
            .lock()
            .map_err(|_| anyhow::anyhow!("Lock poisoned"))?
            .add_memory(memory)
    }

    /// Save context
    pub fn save_context(&self, key: impl Into<String>, value: impl Into<String>) -> Result<()> {
        let memory = AutoMemory::new(key, value, MemoryType::Context);
        self.0
            .lock()
            .map_err(|_| anyhow::anyhow!("Lock poisoned"))?
            .add_memory(memory)
    }

    /// Get memory count
    pub fn count(&self) -> usize {
        self.0.lock().map(|m| m.memory_count()).unwrap_or(0)
    }

    /// Get a specific memory by key and type
    pub fn memory(&self, key: &str, memory_type: MemoryType) -> Option<AutoMemory> {
        self.0
            .lock()
            .ok()
            .and_then(|mut m| m.memory(key, memory_type))
    }

    /// Get all preferences
    pub fn preferences(&self) -> Vec<AutoMemory> {
        self.0
            .lock()
            .map(|mut m| m.fetch_memories_by_type(MemoryType::Preference))
            .unwrap_or_default()
    }

    /// Get all decisions
    pub fn decisions(&self) -> Vec<AutoMemory> {
        self.0
            .lock()
            .map(|mut m| m.fetch_memories_by_type(MemoryType::Decision))
            .unwrap_or_default()
    }

    /// Get all errors
    pub fn errors(&self) -> Vec<AutoMemory> {
        self.0
            .lock()
            .map(|mut m| m.fetch_memories_by_type(MemoryType::Error))
            .unwrap_or_default()
    }

    /// Get recent memories
    pub fn recent(&self, days: i64) -> Vec<AutoMemory> {
        self.0
            .lock()
            .map(|mut m| m.fetch_recent_memories(days))
            .unwrap_or_default()
    }

    /// Get important memories
    pub fn important(&self, threshold: f64) -> Vec<AutoMemory> {
        self.0
            .lock()
            .map(|mut m| m.fetch_important_memories(threshold))
            .unwrap_or_default()
    }

    /// Get get_suggestions
    pub fn get_suggestions(&self, context: &str) -> Vec<String> {
        self.0
            .lock()
            .map(|m| m.get_suggestions(context))
            .unwrap_or_default()
    }

    /// Cleanup old memories
    pub fn cleanup(&self) -> Result<usize> {
        self.0
            .lock()
            .map_err(|_| anyhow::anyhow!("Lock poisoned"))?
            .cleanup()
    }

    /// Sync to persistent memory
    pub fn sync(&self) -> Result<()> {
        self.0
            .lock()
            .map_err(|_| anyhow::anyhow!("Lock poisoned"))?
            .sync_to_memory()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_auto_memory_creation() {
        let memory = AutoMemory::new("test_key", "test_value", MemoryType::Preference);

        assert_eq!(memory.key, "test_key");
        assert_eq!(memory.value, "test_value");
        assert_eq!(memory.memory_type, MemoryType::Preference);
        assert_eq!(memory.importance, 0.9);
        assert_eq!(memory.access_count, 0);
    }

    #[test]
    fn test_importance_scoring() {
        let preference = AutoMemory::new("theme", "dark", MemoryType::Preference);
        assert_eq!(preference.importance, 0.9);

        let error = AutoMemory::new("error", "fix", MemoryType::Error);
        assert_eq!(error.importance, 0.7);

        let context = AutoMemory::new("context", "info", MemoryType::Context);
        assert_eq!(context.importance, 0.5);
    }

    #[test]
    fn test_access_recording() {
        let mut memory = AutoMemory::new("key", "value", MemoryType::Preference);
        assert_eq!(memory.access_count, 0);

        memory.record_access();
        assert_eq!(memory.access_count, 1);
        assert!(memory.accessed_at > memory.created_at);
    }

    #[test]
    fn test_metadata() {
        let memory = AutoMemory::new("key", "value", MemoryType::Preference)
            .with_metadata("source", "user")
            .with_metadata("category", "ui");

        assert_eq!(memory.metadata.len(), 2);
        assert_eq!(memory.metadata.get("source"), Some(&"user".to_string()));
    }

    #[test]
    fn test_cleanup_criteria() {
        let mut memory = AutoMemory::new("key", "value", MemoryType::Context);

        // Low importance memories should not be cleaned up immediately
        assert!(!memory.should_cleanup());

        // Simulate old memory
        memory.created_at = Utc::now() - chrono::Duration::days(31);
        memory.importance = 0.2;
        assert!(memory.should_cleanup());

        // Important memories should never be cleaned up
        memory.importance = 0.8;
        assert!(!memory.should_cleanup());
    }

    #[test]
    fn test_auto_memory_manager() {
        let temp_dir = TempDir::new().unwrap();
        let cwd = temp_dir.path();

        let mut manager = AutoMemoryManager::new(cwd).unwrap();

        // Add some memories
        let memory1 = AutoMemory::new("theme", "dark", MemoryType::Preference);
        let memory2 = AutoMemory::new("error1", "fix1", MemoryType::Error);

        manager.add_memory(memory1).unwrap();
        manager.add_memory(memory2).unwrap();

        assert_eq!(manager.memory_count(), 2);

        // Get memory by key
        let found = manager.memory("theme", MemoryType::Preference);
        assert!(found.is_some());
        assert_eq!(found.unwrap().value, "dark");
    }

    #[test]
    fn test_memory_cleanup() {
        let temp_dir = TempDir::new().unwrap();
        let cwd = temp_dir.path();

        let mut manager = AutoMemoryManager::new(cwd).unwrap();

        // Add many low-importance memories
        for i in 0..1100 {
            let memory = AutoMemory::new(
                format!("key_{}", i),
                format!("value_{}", i),
                MemoryType::Context,
            );
            manager.add_memory(memory).unwrap();
        }

        // Should be cleaned up automatically
        assert!(manager.memory_count() <= MAX_AUTO_MEMORIES);
    }

    #[test]
    fn test_thread_safe_manager() {
        let temp_dir = TempDir::new().unwrap();
        let cwd = temp_dir.path();

        let manager = ThreadSafeAutoMemory::new(cwd).unwrap();

        // Save preference
        manager.save_preference("theme", "dark").unwrap();
        manager.save_preference("model", "claude").unwrap();

        // Get count
        assert_eq!(manager.count(), 2);

        // Get preferences
        let prefs = manager.preferences();
        assert_eq!(prefs.len(), 2);
    }

    #[test]
    fn test_get_suggestions() {
        let temp_dir = TempDir::new().unwrap();
        let cwd = temp_dir.path();

        let manager = ThreadSafeAutoMemory::new(cwd).unwrap();

        // Save some memories
        manager.save_preference("theme", "dark").unwrap();
        manager.save_context("recent_file", "main.rs").unwrap();

        // Get get_suggestions
        let get_suggestions = manager.get_suggestions("theme");
        assert!(!get_suggestions.is_empty());
    }
}
