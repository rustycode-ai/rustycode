use crate::actions::{ActionResult, Change};
use crate::error::{LearningError, Result, StorageError};
use crate::patterns::{Instinct, Pattern};
use crate::triggers::Context;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

/// Storage for patterns and instincts
#[derive(Debug)]
pub struct PatternStorage {
    patterns: HashMap<String, Pattern>,
    instincts: HashMap<String, Instinct>,
    storage_path: PathBuf,
}

/// Serialized storage format
#[derive(Debug, Serialize, Deserialize)]
struct StorageData {
    patterns: HashMap<String, Pattern>,
    instincts: HashMap<String, Instinct>,
    version: String,
}

impl PatternStorage {
    /// Create new pattern storage
    pub fn new(storage_path: &Path) -> Result<Self> {
        let storage_path = storage_path.join("instincts");

        // Create directory if it doesn't exist (atomic - handles race condition)
        // fs::create_dir_all is idempotent - no TOCTOU vulnerability
        fs::create_dir_all(&storage_path).map_err(|e| {
            StorageError::DirectoryNotFound(format!(
                "failed to create directory '{}': {}",
                storage_path.display(),
                e
            ))
        })?;

        let mut storage = Self {
            patterns: HashMap::new(),
            instincts: HashMap::new(),
            storage_path,
        };

        storage.load()?;
        Ok(storage)
    }

    /// Save patterns and instincts to disk
    pub fn save(&self) -> Result<()> {
        let data = StorageData {
            patterns: self.patterns.clone(),
            instincts: self.instincts.clone(),
            version: "1.0".to_string(),
        };

        let json = serde_json::to_string_pretty(&data)?;
        let file_path = self.storage_path.join("patterns.json");

        let tmp_path = file_path.with_extension("json.tmp");
        fs::write(&tmp_path, &json).map_err(|e| StorageError::SaveFailed(e.to_string()))?;
        fs::rename(&tmp_path, &file_path).map_err(|e| StorageError::SaveFailed(e.to_string()))?;

        Ok(())
    }

    /// Load patterns and instincts from disk
    pub fn load(&mut self) -> Result<()> {
        let file_path = self.storage_path.join("patterns.json");

        // Load file directly - handle NotFound as "no existing data" (atomic operation)
        // This avoids TOCTOU: file could be created/deleted between check and read
        let json = match fs::read_to_string(&file_path) {
            Ok(content) => content,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Ok(()); // No existing data is fine
            }
            Err(e) => {
                return Err(LearningError::Storage(StorageError::LoadFailed(format!(
                    "failed to read '{}': {}",
                    file_path.display(),
                    e
                ))))
            }
        };

        let data: StorageData =
            serde_json::from_str(&json).map_err(|e| StorageError::LoadFailed(e.to_string()))?;

        self.patterns = data.patterns;
        self.instincts = data.instincts;

        Ok(())
    }

    /// Add a pattern to storage
    pub fn add_pattern(&mut self, pattern: Pattern) {
        self.patterns.insert(pattern.id.clone(), pattern);
    }

    /// Add an instinct to storage
    pub fn add_instinct(&mut self, instinct: Instinct) {
        self.instincts.insert(instinct.id.clone(), instinct);
    }

    /// Get a pattern by ID
    pub fn pattern(&self, id: &str) -> Option<&Pattern> {
        self.patterns.get(id)
    }

    /// Get an instinct by ID
    pub fn instinct(&self, id: &str) -> Option<&Instinct> {
        self.instincts.get(id)
    }

    /// Get all patterns
    pub const fn patterns(&self) -> &HashMap<String, Pattern> {
        &self.patterns
    }

    /// Get all instincts
    pub const fn instincts(&self) -> &HashMap<String, Instinct> {
        &self.instincts
    }

    /// Find instincts that match the given context
    pub fn find_matching_instincts(&self, context: &Context) -> Vec<&Instinct> {
        self.instincts
            .values()
            .filter(|instinct| self.matches_context(instinct, context))
            .collect()
    }

    /// Apply an instinct to a context
    pub async fn apply_instinct(&self, instinct: &Instinct, context: &Context) -> ActionResult {
        let mut changes = Vec::new();
        let mut success = true;
        let mut feedback = String::new();
        #[allow(unused_assignments)]
        if false {
            // Placeholder to satisfy the linter - feedback is always assigned before use
            let _ = feedback;
        }

        // Check if instinct should be auto-applied
        if !instinct.action.auto_apply {
            return ActionResult {
                instinct_id: instinct.id.clone(),
                success: false,
                changes: vec![],
                feedback: "Instinct requires manual approval".to_string(),
            };
        }

        // Apply the action based on type
        match &instinct.action.action_type {
            crate::patterns::ActionType::SuggestCommand => {
                // In a real implementation, this would suggest a command
                feedback = format!("Suggested command: {}", instinct.action.template);
            }
            crate::patterns::ActionType::Transform => {
                // Apply code transformation
                let change = Change {
                    change_type: crate::actions::ChangeType::Code,
                    description: instinct.action.description.clone(),
                    content: instinct.action.template.clone(),
                    file_path: context.file_path.clone().unwrap_or_default(),
                };
                changes.push(change);
                feedback = format!("Applied transformation: {}", instinct.action.description);
            }
            crate::patterns::ActionType::GenerateDocs => {
                let change = Change {
                    change_type: crate::actions::ChangeType::Documentation,
                    description: instinct.action.description.clone(),
                    content: instinct.action.template.clone(),
                    file_path: context.file_path.clone().unwrap_or_default(),
                };
                changes.push(change);
                feedback = "Generated documentation".to_string();
            }
            _ => {
                success = false;
                feedback = "Action type not implemented".to_string();
            }
        }

        ActionResult {
            instinct_id: instinct.id.clone(),
            success,
            changes,
            feedback,
        }
    }

    /// Auto-apply all matching instincts
    pub async fn auto_apply(&self, context: &Context) -> Vec<ActionResult> {
        let matching = self.find_matching_instincts(context);
        let mut results = Vec::new();

        for instinct in matching {
            let result = self.apply_instinct(instinct, context).await;
            results.push(result);
        }

        results
    }

    /// Update instinct success rate
    pub fn record_instinct_success(&mut self, id: &str) {
        if let Some(instinct) = self.instincts.get_mut(id) {
            instinct.record_success();
        }
    }

    /// Update instinct failure rate
    pub fn record_instinct_failure(&mut self, id: &str) {
        if let Some(instinct) = self.instincts.get_mut(id) {
            instinct.record_failure();
        }
    }

    /// Check if an instinct matches the given context
    fn matches_context(&self, instinct: &Instinct, context: &Context) -> bool {
        let trigger = &instinct.trigger;

        // Check confidence threshold
        if instinct.pattern.confidence < trigger.confidence_threshold {
            return false;
        }

        // Check context requirements
        for (key, required_value) in &trigger.context_requirements {
            if let Some(actual_value) = context.get(key) {
                if actual_value != required_value {
                    return false;
                }
            } else {
                return false;
            }
        }

        // Check trigger type specific conditions
        match &trigger.trigger_type {
            crate::patterns::TriggerType::Keyword => {
                if let Some(text) = &context.text {
                    regex::Regex::new(&trigger.pattern)
                        .map(|re| re.is_match(text))
                        .unwrap_or(false)
                } else {
                    false
                }
            }
            crate::patterns::TriggerType::FileType => {
                if let Some(file_type) = &context.file_type {
                    &trigger.pattern == file_type
                } else {
                    false
                }
            }
            crate::patterns::TriggerType::ErrorPattern => {
                if let Some(error) = &context.error {
                    regex::Regex::new(&trigger.pattern)
                        .map(|re| re.is_match(error))
                        .unwrap_or(false)
                } else {
                    false
                }
            }
            _ => true, // Other trigger types always match for now
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_storage_creation() {
        let temp_dir = TempDir::new().unwrap();
        let storage_path = temp_dir.path();

        let storage = PatternStorage::new(storage_path);
        assert!(storage.is_ok());
    }

    #[test]
    fn test_add_and_retrieve_pattern() {
        let temp_dir = TempDir::new().unwrap();
        let storage_path = temp_dir.path();

        let mut storage = PatternStorage::new(storage_path).unwrap();

        let pattern = Pattern::new(
            "test-1".to_string(),
            "Test Pattern".to_string(),
            crate::patterns::PatternCategory::Coding,
            "A test pattern".to_string(),
        );

        storage.add_pattern(pattern.clone());
        let retrieved = storage.pattern("test-1");

        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().name, "Test Pattern");
    }

    #[test]
    fn test_save_and_load() {
        let temp_dir = TempDir::new().unwrap();
        let storage_path = temp_dir.path();

        let mut storage = PatternStorage::new(storage_path).unwrap();

        let pattern = Pattern::new(
            "test-2".to_string(),
            "Test Pattern 2".to_string(),
            crate::patterns::PatternCategory::Debugging,
            "Another test pattern".to_string(),
        );

        storage.add_pattern(pattern);
        storage.save().unwrap();

        let mut storage2 = PatternStorage::new(storage_path).unwrap();
        storage2.load().unwrap();

        let retrieved = storage2.pattern("test-2");
        assert!(retrieved.is_some());
    }

    #[test]
    fn test_get_pattern_nonexistent() {
        let temp_dir = TempDir::new().unwrap();
        let storage = PatternStorage::new(temp_dir.path()).unwrap();
        assert!(storage.pattern("no-such-id").is_none());
    }

    #[test]
    fn test_get_instinct_nonexistent() {
        let temp_dir = TempDir::new().unwrap();
        let storage = PatternStorage::new(temp_dir.path()).unwrap();
        assert!(storage.instinct("no-such-id").is_none());
    }

    #[test]
    fn test_patterns_and_instincts_empty() {
        let temp_dir = TempDir::new().unwrap();
        let storage = PatternStorage::new(temp_dir.path()).unwrap();
        assert!(storage.patterns().is_empty());
        assert!(storage.instincts().is_empty());
    }

    #[test]
    fn test_add_and_retrieve_instinct() {
        let temp_dir = TempDir::new().unwrap();
        let mut storage = PatternStorage::new(temp_dir.path()).unwrap();

        let pattern = Pattern::new(
            "p1".to_string(),
            "P1".to_string(),
            crate::patterns::PatternCategory::Testing,
            "desc".to_string(),
        );

        let trigger = crate::patterns::TriggerCondition::new(
            "t1".to_string(),
            crate::patterns::TriggerType::Keyword,
            "test".to_string(),
        );

        let action = crate::patterns::SuggestedAction::new(
            "a1".to_string(),
            crate::patterns::ActionType::RunTests,
            "Run tests".to_string(),
            "cargo test".to_string(),
        );

        let instinct = Instinct::new("i1".to_string(), pattern, trigger, action);

        storage.add_instinct(instinct);
        let retrieved = storage.instinct("i1");
        assert!(retrieved.is_some());
    }

    #[test]
    fn test_record_instinct_success_and_failure() {
        let temp_dir = TempDir::new().unwrap();
        let mut storage = PatternStorage::new(temp_dir.path()).unwrap();

        let pattern = Pattern::new(
            "p2".to_string(),
            "P2".to_string(),
            crate::patterns::PatternCategory::Debugging,
            "desc".to_string(),
        );

        let trigger = crate::patterns::TriggerCondition::new(
            "t2".to_string(),
            crate::patterns::TriggerType::ErrorPattern,
            "panic".to_string(),
        );

        let action = crate::patterns::SuggestedAction::new(
            "a2".to_string(),
            crate::patterns::ActionType::DebugStrategy,
            "Debug".to_string(),
            "check logs".to_string(),
        );

        let instinct = Instinct::new("i2".to_string(), pattern, trigger, action);

        storage.add_instinct(instinct);

        // Record successes and failures
        storage.record_instinct_success("i2");
        storage.record_instinct_success("i2");
        storage.record_instinct_failure("i2");

        let inst = storage.instinct("i2").unwrap();
        assert_eq!(inst.usage_count, 3); // 2 successes + 1 failure
        assert!(inst.success_rate > 0.0);
        assert!(inst.last_used.is_some());

        // Nonexistent instinct — should not panic
        storage.record_instinct_success("ghost");
        storage.record_instinct_failure("ghost");
    }

    #[test]
    fn test_save_empty_and_reload() {
        let temp_dir = TempDir::new().unwrap();

        // Save empty storage
        let storage = PatternStorage::new(temp_dir.path()).unwrap();
        storage.save().unwrap();

        // Reload — should still be empty
        let storage2 = PatternStorage::new(temp_dir.path()).unwrap();
        assert!(storage2.patterns().is_empty());
        assert!(storage2.instincts().is_empty());
    }

    #[test]
    fn test_overwrite_pattern_on_duplicate_id() {
        let temp_dir = TempDir::new().unwrap();
        let mut storage = PatternStorage::new(temp_dir.path()).unwrap();

        let p1 = Pattern::new(
            "dup".to_string(),
            "V1".to_string(),
            crate::patterns::PatternCategory::Coding,
            "first".to_string(),
        );
        let p2 = Pattern::new(
            "dup".to_string(),
            "V2".to_string(),
            crate::patterns::PatternCategory::Debugging,
            "second".to_string(),
        );

        storage.add_pattern(p1);
        storage.add_pattern(p2);

        let retrieved = storage.pattern("dup").unwrap();
        assert_eq!(retrieved.name, "V2");
        assert_eq!(storage.patterns().len(), 1);
    }

    #[tokio::test]
    async fn test_auto_apply_no_matching_instincts() {
        let temp_dir = TempDir::new().unwrap();
        let storage = PatternStorage::new(temp_dir.path()).unwrap();

        let context = Context::new();
        let results = storage.auto_apply(&context).await;
        assert!(results.is_empty());
    }

    #[test]
    fn test_find_matching_instincts_empty() {
        let temp_dir = TempDir::new().unwrap();
        let storage = PatternStorage::new(temp_dir.path()).unwrap();

        let context = Context::new();
        let matches = storage.find_matching_instincts(&context);
        assert!(matches.is_empty());
    }
}
