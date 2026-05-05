//! Iteration-level checkpointing for LLM stream resilience.
//!
//! This module provides checkpointing of tool execution outputs within a single
//! agent iteration. When an LLM stream fails after tools have been executed,
//! the checkpoint allows recovery without re-running those tools.
//!
//! # Problem
//! Agent runs tools (e.g., `python setup.py build_ext`), gets output, then
//! the LLM API fails when trying to send results to Claude. The tool outputs
//! are lost and the agent restarts from scratch.
//!
//! # Solution
//! Save tool outputs to disk BEFORE calling the LLM. If the LLM call fails,
//! retry using the saved outputs instead of re-executing tools.

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use uuid::Uuid;

/// A single tool call with its inputs and output.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckpointToolCall {
    /// Unique ID for this tool call
    pub id: String,
    /// Tool name (e.g., "bash", "read_file", "write_file")
    pub name: String,
    /// Input arguments as JSON
    pub input: serde_json::Value,
    /// Output from the tool (if successful)
    pub output: Option<String>,
    /// Whether the tool call succeeded
    pub success: bool,
    /// Size of the output in bytes (for metrics)
    pub output_size_bytes: usize,
    /// Timestamp when tool was executed
    pub executed_at: DateTime<Utc>,
}

/// A checkpoint representing the state after tool execution in an iteration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IterationCheckpoint {
    /// Unique ID for this checkpoint
    pub id: String,
    /// Iteration sequence number (0-based)
    pub sequence_id: u32,
    /// All tool calls executed in this iteration
    pub tool_calls: Vec<CheckpointToolCall>,
    /// Formatted prompt that was about to be sent to the LLM
    /// (used to verify we're resuming the same context)
    pub prompt_cache: String,
    /// Hash of prompt for validation (detect context changes)
    pub prompt_hash: u64,
    /// Total bytes of tool output in this iteration
    pub total_output_bytes: usize,
    /// Timestamp when checkpoint was created
    pub created_at: DateTime<Utc>,
    /// Whether this checkpoint was successfully sent to the LLM
    /// (helps track completion)
    pub sent_to_llm: bool,
}

impl IterationCheckpoint {
    pub fn new(sequence_id: u32, tool_calls: Vec<CheckpointToolCall>, prompt: String) -> Self {
        let prompt_hash = Self::hash_prompt(&prompt);
        let total_output_bytes = tool_calls.iter().map(|tc| tc.output_size_bytes).sum();

        Self {
            id: Uuid::new_v4().to_string(),
            sequence_id,
            tool_calls,
            prompt_cache: prompt,
            prompt_hash,
            total_output_bytes,
            created_at: Utc::now(),
            sent_to_llm: false,
        }
    }

    /// Hash the prompt for integrity checking.
    fn hash_prompt(prompt: &str) -> u64 {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let mut hasher = DefaultHasher::new();
        prompt.hash(&mut hasher);
        hasher.finish()
    }

    /// Verify that the prompt hasn't changed since checkpoint creation.
    pub fn verify_prompt(&self, prompt: &str) -> bool {
        Self::hash_prompt(prompt) == self.prompt_hash
    }

    /// Get a human-readable summary of this checkpoint.
    pub fn summary(&self) -> String {
        format!(
            "Checkpoint #{}: {} tool calls, {} bytes, {} at {}",
            self.sequence_id,
            self.tool_calls.len(),
            self.total_output_bytes,
            &self.id[..8],
            self.created_at.format("%H:%M:%S"),
        )
    }
}

/// Persistence layer for iteration checkpoints.
pub struct CheckpointStorage {
    /// Base directory for all checkpoints
    checkpoints_dir: PathBuf,
}

impl CheckpointStorage {
    /// Create a new checkpoint storage at the given directory.
    /// Creates the directory if it doesn't exist.
    pub fn new(checkpoints_dir: impl AsRef<Path>) -> Result<Self> {
        let dir = checkpoints_dir.as_ref().to_path_buf();
        fs::create_dir_all(&dir).with_context(|| {
            format!("Failed to create checkpoints directory: {}", dir.display())
        })?;
        Ok(Self {
            checkpoints_dir: dir,
        })
    }

    /// Get the default checkpoints directory for a session.
    pub fn default_session_dir(session_id: &str) -> Result<PathBuf> {
        let home = dirs::home_dir().context("Could not determine home directory")?;
        let checkpoints_dir = home
            .join(".rustycode")
            .join("sessions")
            .join(session_id)
            .join("checkpoints");
        Ok(checkpoints_dir)
    }

    /// Create a storage instance using the default session directory.
    pub fn for_session(session_id: &str) -> Result<Self> {
        let dir = Self::default_session_dir(session_id)?;
        Self::new(dir)
    }

    /// Save a checkpoint to disk.
    pub fn save(&self, checkpoint: &IterationCheckpoint) -> Result<PathBuf> {
        let filename = format!(
            "iteration_{:03}_{}.json",
            checkpoint.sequence_id,
            &checkpoint.id[..8]
        );
        let path = self.checkpoints_dir.join(&filename);

        let json =
            serde_json::to_string_pretty(checkpoint).context("Failed to serialize checkpoint")?;
        let tmp_path = path.with_extension("json.tmp");
        fs::write(&tmp_path, &json)
            .with_context(|| format!("Failed to write checkpoint to {}", tmp_path.display()))?;
        fs::rename(&tmp_path, &path)
            .with_context(|| format!("Failed to rename checkpoint to {}", path.display()))?;

        Ok(path)
    }

    /// Load a specific checkpoint by ID.
    pub fn load_by_id(&self, checkpoint_id: &str) -> Result<IterationCheckpoint> {
        for entry in
            fs::read_dir(&self.checkpoints_dir).context("Failed to read checkpoints directory")?
        {
            let entry = entry?;
            let path = entry.path();
            if path.is_file() && path.extension().is_some_and(|ext| ext == "json") {
                let content = fs::read_to_string(&path)
                    .with_context(|| format!("Failed to read checkpoint {}", path.display()))?;
                let checkpoint: IterationCheckpoint = serde_json::from_str(&content)
                    .with_context(|| format!("Invalid checkpoint data in {}", path.display()))?;
                if checkpoint.id == checkpoint_id {
                    return Ok(checkpoint);
                }
            }
        }
        anyhow::bail!("Checkpoint not found: {}", checkpoint_id);
    }

    /// Load the most recent checkpoint for a given sequence ID.
    pub fn load_by_sequence(&self, sequence_id: u32) -> Result<IterationCheckpoint> {
        for entry in
            fs::read_dir(&self.checkpoints_dir).context("Failed to read checkpoints directory")?
        {
            let entry = entry?;
            let path = entry.path();
            if path.is_file() && path.extension().is_some_and(|ext| ext == "json") {
                let content = fs::read_to_string(&path)
                    .with_context(|| format!("Failed to read checkpoint {}", path.display()))?;
                let checkpoint: IterationCheckpoint = serde_json::from_str(&content)
                    .with_context(|| format!("Invalid checkpoint data in {}", path.display()))?;
                if checkpoint.sequence_id == sequence_id {
                    return Ok(checkpoint);
                }
            }
        }
        anyhow::bail!("Checkpoint not found for sequence: {}", sequence_id);
    }

    /// List all checkpoints in order (oldest first).
    pub fn list_all(&self) -> Result<Vec<IterationCheckpoint>> {
        let mut checkpoints = Vec::new();

        if !self.checkpoints_dir.exists() {
            return Ok(checkpoints);
        }

        for entry in
            fs::read_dir(&self.checkpoints_dir).context("Failed to read checkpoints directory")?
        {
            let entry = entry?;
            let path = entry.path();
            if path.is_file() && path.extension().is_some_and(|ext| ext == "json") {
                let content = fs::read_to_string(&path)?;
                if let Ok(checkpoint) = serde_json::from_str::<IterationCheckpoint>(&content) {
                    checkpoints.push(checkpoint);
                }
            }
        }

        // Sort by sequence ID
        checkpoints.sort_by_key(|c| c.sequence_id);
        Ok(checkpoints)
    }

    /// Get the latest checkpoint.
    pub fn latest(&self) -> Result<Option<IterationCheckpoint>> {
        let checkpoints = self.list_all()?;
        Ok(checkpoints.into_iter().last())
    }

    /// Delete a checkpoint by ID.
    pub fn delete(&self, checkpoint_id: &str) -> Result<()> {
        for entry in
            fs::read_dir(&self.checkpoints_dir).context("Failed to read checkpoints directory")?
        {
            let entry = entry?;
            let path = entry.path();
            if path.is_file() && path.extension().is_some_and(|ext| ext == "json") {
                let content = fs::read_to_string(&path)?;
                if let Ok(checkpoint) = serde_json::from_str::<IterationCheckpoint>(&content) {
                    if checkpoint.id == checkpoint_id {
                        fs::remove_file(&path).with_context(|| {
                            format!("Failed to delete checkpoint: {}", path.display())
                        })?;
                        return Ok(());
                    }
                }
            }
        }
        anyhow::bail!("Checkpoint not found: {}", checkpoint_id);
    }

    /// Clear all checkpoints.
    pub fn clear_all(&self) -> Result<()> {
        if self.checkpoints_dir.exists() {
            fs::remove_dir_all(&self.checkpoints_dir).with_context(|| {
                format!(
                    "Failed to clear checkpoints: {}",
                    self.checkpoints_dir.display()
                )
            })?;
            fs::create_dir_all(&self.checkpoints_dir)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_test_checkpoint() -> IterationCheckpoint {
        let tool_call = CheckpointToolCall {
            id: "tc_001".to_string(),
            name: "bash".to_string(),
            input: serde_json::json!({"command": "echo test"}),
            output: Some("test output".to_string()),
            success: true,
            output_size_bytes: 11,
            executed_at: Utc::now(),
        };

        IterationCheckpoint::new(0, vec![tool_call], "Test prompt".to_string())
    }

    #[test]
    fn test_checkpoint_creation() {
        let checkpoint = create_test_checkpoint();
        assert_eq!(checkpoint.sequence_id, 0);
        assert_eq!(checkpoint.tool_calls.len(), 1);
        assert!(!checkpoint.sent_to_llm);
    }

    #[test]
    fn test_checkpoint_summary() {
        let checkpoint = create_test_checkpoint();
        let summary = checkpoint.summary();
        assert!(summary.contains("1 tool calls"));
    }

    #[test]
    fn test_checkpoint_storage_save_and_load() {
        let temp_dir = TempDir::new().unwrap();
        let storage = CheckpointStorage::new(temp_dir.path()).unwrap();
        let checkpoint = create_test_checkpoint();
        let id = checkpoint.id.clone();

        storage.save(&checkpoint).unwrap();
        let loaded = storage.load_by_id(&id).unwrap();

        assert_eq!(loaded.sequence_id, checkpoint.sequence_id);
        assert_eq!(loaded.tool_calls.len(), checkpoint.tool_calls.len());
    }

    #[test]
    fn test_checkpoint_list_all() {
        let temp_dir = TempDir::new().unwrap();
        let storage = CheckpointStorage::new(temp_dir.path()).unwrap();

        for i in 0..3 {
            let mut checkpoint = create_test_checkpoint();
            checkpoint.sequence_id = i;
            storage.save(&checkpoint).unwrap();
        }

        let all = storage.list_all().unwrap();
        assert_eq!(all.len(), 3);
        assert_eq!(all[0].sequence_id, 0);
        assert_eq!(all[2].sequence_id, 2);
    }

    #[test]
    fn test_prompt_verification() {
        let prompt = "Test prompt";
        let checkpoint = IterationCheckpoint::new(0, vec![], prompt.to_string());

        assert!(checkpoint.verify_prompt(prompt));
        assert!(!checkpoint.verify_prompt("Different prompt"));
    }

    #[test]
    fn test_checkpoint_integrity() {
        let tool_call = CheckpointToolCall {
            id: "tc_001".to_string(),
            name: "bash".to_string(),
            input: serde_json::json!({"command": "echo test"}),
            output: Some("test output".to_string()),
            success: true,
            output_size_bytes: 11,
            executed_at: Utc::now(),
        };

        let mut checkpoint = IterationCheckpoint::new(0, vec![tool_call], "Test".to_string());
        assert!(!checkpoint.sent_to_llm);

        checkpoint.sent_to_llm = true;
        assert!(checkpoint.sent_to_llm);
    }

    #[test]
    fn test_checkpoint_with_multiple_tools() {
        let mut tool_calls = Vec::new();

        for i in 0..5 {
            tool_calls.push(CheckpointToolCall {
                id: format!("tc_{:03}", i),
                name: "bash".to_string(),
                input: serde_json::json!({"command": format!("echo {}", i)}),
                output: Some(format!("output {}", i)),
                success: true,
                output_size_bytes: 8 + i.to_string().len(),
                executed_at: Utc::now(),
            });
        }

        let checkpoint = IterationCheckpoint::new(0, tool_calls, "Multiple tools".to_string());
        assert_eq!(checkpoint.tool_calls.len(), 5);
        assert!(checkpoint.total_output_bytes > 0);
    }

    #[test]
    fn test_checkpoint_storage_delete() {
        let temp_dir = TempDir::new().unwrap();
        let storage = CheckpointStorage::new(temp_dir.path()).unwrap();
        let checkpoint = create_test_checkpoint();
        let id = checkpoint.id.clone();

        storage.save(&checkpoint).unwrap();
        assert!(storage.load_by_id(&id).is_ok());

        storage.delete(&id).unwrap();
        assert!(storage.load_by_id(&id).is_err());
    }

    #[test]
    fn test_checkpoint_storage_clear_all() {
        let temp_dir = TempDir::new().unwrap();
        let storage = CheckpointStorage::new(temp_dir.path()).unwrap();

        for i in 0..3 {
            let mut checkpoint = create_test_checkpoint();
            checkpoint.sequence_id = i;
            storage.save(&checkpoint).unwrap();
        }

        assert_eq!(storage.list_all().unwrap().len(), 3);
        storage.clear_all().unwrap();
        assert_eq!(storage.list_all().unwrap().len(), 0);
    }
}
