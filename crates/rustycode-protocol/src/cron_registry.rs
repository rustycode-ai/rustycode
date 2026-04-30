//! Cron Registry — Scheduled autonomous task execution.
//!
//! This module provides scheduling for autonomous operations:
//! - Cron-style scheduled prompts/tasks
//! - Enable/disable scheduling
//! - Run history tracking
//! - Global registry for centralized access
//!
//! Inspired by claw-code's team_cron_registry module.
//!
//! # Architecture
//!
//! ```text
//! CronRegistry → CronEntry { schedule, prompt, enabled, last_run_at, run_count }
//!      │
//!      ├─ create("0 9 * * *", "Run morning tests", None, None)
//!      ├─ list(enabled_only=true)
//!      └─ record_run() → updates last_run_at, run_count
//! ```

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

/// Status of a scheduled cron task
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum CronStatus {
    /// Waiting for its scheduled time
    #[default]
    Idle,
    /// Currently executing
    Running,
    /// Last execution succeeded
    Succeeded,
    /// Last execution failed (and potentially retrying)
    Failed,
    /// Waiting for dependencies to complete
    AwaitingDependency,
}

/// Backoff strategy for retries
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum BackoffStrategy {
    #[default]
    Fixed,
    Exponential,
}

/// Retry configuration for failed cron tasks
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RetryConfig {
    /// Maximum number of retry attempts
    pub max_attempts: u32,
    /// Current attempt count
    pub current_attempt: u32,
    /// Strategy for backoff between attempts
    pub backoff: BackoffStrategy,
    /// Interval in seconds for fixed backoff
    pub interval_secs: u64,
}

/// A scheduled cron entry for autonomous task execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CronEntry {
    /// Unique cron identifier
    pub cron_id: String,
    /// Cron schedule expression (e.g., "0 9 * * *" for daily at 9am)
    pub schedule: String,
    /// Prompt/task to execute on schedule
    pub prompt: String,
    /// Optional description of what this cron does
    pub description: Option<String>,
    /// Whether this cron is active
    pub enabled: bool,
    /// Current status of the task
    pub status: CronStatus,
    /// List of cron IDs this task depends on
    pub depends_on: Vec<String>,
    /// Optional retry configuration
    pub retry_config: Option<RetryConfig>,
    /// Error message from the last failure, if any
    pub last_error: Option<String>,
    /// Creation timestamp (unix epoch seconds)
    pub created_at: u64,
    /// Last update timestamp (unix epoch seconds)
    pub updated_at: u64,
    /// Last run timestamp (unix epoch seconds), if ever run
    pub last_run_at: Option<u64>,
    /// Number of times this cron has executed
    pub run_count: u64,
    /// Flag to trigger immediate execution on next scheduler tick
    pub manual_trigger: bool,
}

/// Internal registry state
#[derive(Debug, Default, Serialize, Deserialize)]
struct CronRegistryInner {
    entries: HashMap<String, CronEntry>,
    #[serde(default)]
    counter: u64,
}

/// Registry for scheduled autonomous tasks
///
/// Provides cron-style scheduling for automated operations like:
/// - Running tests on a schedule
/// - Checking for merge conflicts periodically
/// - Sending status reports
/// - Cleaning up temporary resources
#[derive(Debug, Clone, Default)]
pub struct CronRegistry {
    inner: Arc<Mutex<CronRegistryInner>>,
}

impl CronRegistry {
    /// Create a new empty cron registry
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a new scheduled cron entry
    ///
    /// # Arguments
    ///
    /// * `schedule` - Cron expression (e.g., "0 9 * * *" for daily at 9am)
    /// * `prompt` - The prompt/task to execute on schedule
    /// * `description` - Optional human-readable description
    ///
    /// # Returns
    ///
    /// The newly created CronEntry with `enabled=true`
    ///
    /// # Example
    ///
    /// ```
    /// use rustycode_protocol::cron_registry::CronRegistry;
    ///
    /// let registry = CronRegistry::new();
    /// let entry = registry.create(
    ///     "0 9 * * *",
    ///     "Run morning test suite and report results",
    ///     Some("Daily morning tests"),
    ///     None,
    /// );
    /// assert!(entry.enabled);
    /// ```
    pub fn create(
        &self,
        schedule: &str,
        prompt: &str,
        description: Option<&str>,
        custom_id: Option<&str>,
    ) -> CronEntry {
        let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        inner.counter += 1;
        let ts = now_ms();
        let cron_id = custom_id
            .map(str::to_owned)
            .unwrap_or_else(|| format!("cron_{:08x}_{:04x}", ts, inner.counter));

        let entry = CronEntry {
            cron_id: cron_id.clone(),
            schedule: schedule.to_owned(),
            prompt: prompt.to_owned(),
            description: description.map(str::to_owned),
            enabled: true,
            status: CronStatus::default(),
            depends_on: Vec::new(),
            retry_config: None,
            last_error: None,
            created_at: ts,
            updated_at: ts,
            last_run_at: None,
            run_count: 0,
            manual_trigger: false,
        };

        inner.entries.insert(cron_id, entry.clone());
        entry
    }

    /// Add a dependency to a cron entry
    pub fn add_dependency(&self, cron_id: &str, dependency_id: &str) -> Result<(), String> {
        let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());

        // Check dependency exists first to avoid double borrow
        if !inner.entries.contains_key(dependency_id) {
            return Err(format!("dependency cron not found: {dependency_id}"));
        }

        // Check for circular dependency
        if self.would_cause_cycle_locked(&inner, cron_id, dependency_id) {
            return Err(format!(
                "circular dependency detected: {cron_id} -> {dependency_id}"
            ));
        }

        let entry = inner
            .entries
            .get_mut(cron_id)
            .ok_or_else(|| format!("cron not found: {cron_id}"))?;

        if !entry.depends_on.contains(&dependency_id.to_string()) {
            entry.depends_on.push(dependency_id.to_string());
            entry.updated_at = now_ms();
        }
        Ok(())
    }

    /// Remove a dependency from a cron entry
    pub fn remove_dependency(&self, cron_id: &str, dependency_id: &str) -> Result<(), String> {
        let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        let entry = inner
            .entries
            .get_mut(cron_id)
            .ok_or_else(|| format!("cron not found: {cron_id}"))?;

        let before = entry.depends_on.len();
        entry.depends_on.retain(|dep| dep != dependency_id);
        if entry.depends_on.len() != before {
            entry.updated_at = now_ms();
        }
        Ok(())
    }

    /// Helper to detect if adding a dependency would cause a cycle (uses existing lock)
    fn would_cause_cycle_locked(
        &self,
        inner: &CronRegistryInner,
        cron_id: &str,
        dependency_id: &str,
    ) -> bool {
        let mut stack = vec![dependency_id.to_string()];
        let mut visited = std::collections::HashSet::new();

        while let Some(current) = stack.pop() {
            if current == cron_id {
                return true;
            }
            if visited.insert(current.clone()) {
                if let Some(entry) = inner.entries.get(&current) {
                    for dep in &entry.depends_on {
                        stack.push(dep.clone());
                    }
                }
            }
        }
        false
    }

    /// Set retry configuration for a cron entry
    pub fn set_retry_config(&self, cron_id: &str, config: RetryConfig) -> Result<(), String> {
        let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        let entry = inner
            .entries
            .get_mut(cron_id)
            .ok_or_else(|| format!("cron not found: {cron_id}"))?;

        entry.retry_config = Some(config);
        entry.updated_at = now_ms();
        Ok(())
    }

    /// Update status and error for a cron entry
    pub fn update_status(
        &self,
        cron_id: &str,
        status: CronStatus,
        error: Option<String>,
    ) -> Result<(), String> {
        let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        let entry = inner
            .entries
            .get_mut(cron_id)
            .ok_or_else(|| format!("cron not found: {cron_id}"))?;

        entry.status = status;
        entry.last_error = error;
        entry.updated_at = now_ms();
        Ok(())
    }

    /// Check if all dependencies for a cron are satisfied
    pub fn check_dependencies_satisfied(&self, cron_id: &str) -> bool {
        let inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        let entry = if let Some(e) = inner.entries.get(cron_id) {
            e
        } else {
            return false;
        };

        if entry.depends_on.is_empty() {
            return true;
        }

        for dep_id in &entry.depends_on {
            let dep = if let Some(d) = inner.entries.get(dep_id) {
                d
            } else {
                return false;
            };

            // Dependency must have succeeded
            if dep.status != CronStatus::Succeeded {
                return false;
            }

            // Dependency must have run AFTER our last run (or if we never ran)
            // Using < (less than) to be more resilient to same-millisecond executions
            // while still maintaining the "ran after" semantic in a high-res timer world.
            let our_last = entry.last_run_at.unwrap_or(0);
            let dep_last = dep.last_run_at.unwrap_or(0);

            if dep_last < our_last && entry.run_count > 0 {
                return false;
            }
        }

        true
    }

    /// Get a cron entry by ID
    ///
    /// # Returns
    ///
    /// `Some(CronEntry)` if found, `None` otherwise
    #[must_use]
    pub fn get(&self, cron_id: &str) -> Option<CronEntry> {
        let inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        inner.entries.get(cron_id).cloned()
    }

    /// List all cron entries
    ///
    /// # Arguments
    ///
    /// * `enabled_only` - If true, only return enabled entries
    ///
    /// # Returns
    ///
    /// Vec of CronEntry matching the filter
    #[must_use]
    pub fn list(&self, enabled_only: bool) -> Vec<CronEntry> {
        let inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        inner
            .entries
            .values()
            .filter(|e| !enabled_only || e.enabled)
            .cloned()
            .collect()
    }

    /// Delete a cron entry
    ///
    /// # Arguments
    ///
    /// * `cron_id` - ID of cron to delete
    ///
    /// # Returns
    ///
    /// `Ok(CronEntry)` if deleted, `Err(String)` if not found
    pub fn delete(&self, cron_id: &str) -> Result<CronEntry, String> {
        let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        inner
            .entries
            .remove(cron_id)
            .ok_or_else(|| format!("cron not found: {cron_id}"))
    }

    /// Disable a cron entry without removing it
    ///
    /// # Arguments
    ///
    /// * `cron_id` - ID of cron to disable
    ///
    /// # Returns
    ///
    /// `Ok(())` if disabled, `Err(String)` if not found
    pub fn disable(&self, cron_id: &str) -> Result<(), String> {
        let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        let entry = inner
            .entries
            .get_mut(cron_id)
            .ok_or_else(|| format!("cron not found: {cron_id}"))?;
        entry.enabled = false;
        entry.updated_at = now_ms();
        Ok(())
    }

    /// Enable a previously disabled cron entry
    ///
    /// # Arguments
    ///
    /// * `cron_id` - ID of cron to enable
    ///
    /// # Returns
    ///
    /// `Ok(())` if enabled, `Err(String)` if not found
    pub fn enable(&self, cron_id: &str) -> Result<(), String> {
        let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        let entry = inner
            .entries
            .get_mut(cron_id)
            .ok_or_else(|| format!("cron not found: {cron_id}"))?;
        entry.enabled = true;
        entry.updated_at = now_ms();
        Ok(())
    }

    /// Record a cron execution
    ///
    /// Updates `last_run_at` and increments `run_count`.
    ///
    /// # Arguments
    ///
    /// * `cron_id` - ID of cron that ran
    ///
    /// # Returns
    ///
    /// `Ok(())` if recorded, `Err(String)` if not found
    pub fn record_run(&self, cron_id: &str) -> Result<(), String> {
        let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        let entry = inner
            .entries
            .get_mut(cron_id)
            .ok_or_else(|| format!("cron not found: {cron_id}"))?;
        entry.last_run_at = Some(now_ms());
        entry.run_count = entry.run_count.saturating_add(1);
        entry.updated_at = now_ms();
        Ok(())
    }

    /// Set the manual trigger flag for an entry
    pub fn trigger(&self, cron_id: &str) -> Result<(), String> {
        let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        let entry = inner
            .entries
            .get_mut(cron_id)
            .ok_or_else(|| format!("cron not found: {cron_id}"))?;
        entry.manual_trigger = true;
        entry.updated_at = now_ms();
        Ok(())
    }

    /// Reset the manual trigger flag
    pub fn reset_trigger(&self, cron_id: &str) -> Result<(), String> {
        let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        let entry = inner
            .entries
            .get_mut(cron_id)
            .ok_or_else(|| format!("cron not found: {cron_id}"))?;
        entry.manual_trigger = false;
        entry.updated_at = now_ms();
        Ok(())
    }

    /// Update the prompt for a cron entry
    ///
    /// # Arguments
    ///
    /// * `cron_id` - ID of cron to update
    /// * `new_prompt` - New prompt to execute on schedule
    ///
    /// # Returns
    ///
    /// `Ok(CronEntry)` with updated prompt, `Err(String)` if not found
    pub fn update_prompt(&self, cron_id: &str, new_prompt: &str) -> Result<CronEntry, String> {
        let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        let entry = inner
            .entries
            .get_mut(cron_id)
            .ok_or_else(|| format!("cron not found: {cron_id}"))?;
        entry.prompt = new_prompt.to_owned();
        entry.updated_at = now_ms();
        Ok(entry.clone())
    }

    /// Update the schedule for a cron entry
    pub fn update_schedule(&self, cron_id: &str, new_schedule: &str) -> Result<CronEntry, String> {
        let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        let entry = inner
            .entries
            .get_mut(cron_id)
            .ok_or_else(|| format!("cron not found: {cron_id}"))?;
        entry.schedule = new_schedule.to_owned();
        entry.updated_at = now_ms();
        Ok(entry.clone())
    }

    /// Get count of cron entries
    #[must_use]
    pub fn len(&self) -> usize {
        let inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        inner.entries.len()
    }

    /// Check if registry is empty
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Get enabled cron entries
    #[must_use]
    pub fn enabled(&self) -> Vec<CronEntry> {
        self.list(true)
    }

    // ── Persistence ──────────────────────────────────────────────────────────────────

    /// Serialize the registry to a JSON string
    pub fn to_json(&self) -> String {
        let inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        serde_json::to_string_pretty(&*inner).unwrap_or_default()
    }

    /// Load the registry from a JSON string
    pub fn from_json(&self, json: &str) -> Result<(), String> {
        let new_inner: CronRegistryInner = serde_json::from_str(json).map_err(|e| e.to_string())?;
        let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        *inner = new_inner;
        Ok(())
    }

    /// Save the registry to a file
    pub fn save_to_file<P: AsRef<std::path::Path>>(&self, path: P) -> std::io::Result<()> {
        let json = self.to_json();
        if let Some(parent) = path.as_ref().parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, json)
    }

    /// Load the registry from a file
    pub fn load_from_file<P: AsRef<std::path::Path>>(&self, path: P) -> std::io::Result<()> {
        let json = std::fs::read_to_string(path)?;
        self.from_json(&json)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
    }
}

// ── Global Registry Accessor ────────────────────────────────────────────────────────

use std::sync::OnceLock;

/// Global cron registry accessor for centralized state management.
///
/// This follows the claw-code pattern of using OnceLock for global registries,
/// enabling any part of the codebase to access shared state without threading
/// `Arc<Registry>` through every layer.
///
/// # Example
///
/// ```
/// use rustycode_protocol::cron_registry::global_cron_registry;
/// let registry = global_cron_registry();
/// let entry = registry.create("0 * * * *", "Hourly status check", None, None);
/// ```
pub fn global_cron_registry() -> &'static CronRegistry {
    static REGISTRY: OnceLock<CronRegistry> = OnceLock::new();
    REGISTRY.get_or_init(CronRegistry::new)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cron_create_and_get() {
        let registry = CronRegistry::new();

        let entry = registry.create("0 9 * * *", "Run morning tests", Some("Daily tests"), None);

        assert!(entry.cron_id.starts_with("cron_"));
        assert_eq!(entry.schedule, "0 9 * * *");
        assert_eq!(entry.prompt, "Run morning tests");
        assert_eq!(entry.description, Some("Daily tests".to_string()));
        assert!(entry.enabled);
        assert_eq!(entry.run_count, 0);
        assert!(entry.last_run_at.is_none());
        assert!(!entry.manual_trigger);

        // Retrieve by ID
        let retrieved = registry.get(&entry.cron_id).unwrap();
        assert_eq!(retrieved.cron_id, entry.cron_id);
    }

    #[test]
    fn test_cron_list_with_filter() {
        let registry = CronRegistry::new();

        let entry1 = registry.create("0 9 * * *", "Morning task", None, None);
        let entry2 = registry.create("0 17 * * *", "Evening task", None, None);

        // All entries
        let all = registry.list(false);
        assert_eq!(all.len(), 2);

        // Enabled only (all are enabled)
        let enabled = registry.list(true);
        assert_eq!(enabled.len(), 2);

        // Disable one
        registry.disable(&entry1.cron_id).unwrap();

        // Now only 1 enabled
        let enabled = registry.list(true);
        assert_eq!(enabled.len(), 1);
        assert_eq!(enabled[0].cron_id, entry2.cron_id);

        // All still 2
        let all = registry.list(false);
        assert_eq!(all.len(), 2);
    }

    #[test]
    fn test_cron_enable_disable() {
        let registry = CronRegistry::new();
        let entry = registry.create("*/5 * * * *", "Every 5 min task", None, None);

        assert!(entry.enabled);

        // Disable
        registry.disable(&entry.cron_id).unwrap();
        let retrieved = registry.get(&entry.cron_id).unwrap();
        assert!(!retrieved.enabled);

        // Re-enable
        registry.enable(&entry.cron_id).unwrap();
        let retrieved = registry.get(&entry.cron_id).unwrap();
        assert!(retrieved.enabled);
    }

    #[test]
    fn test_cron_record_run() {
        let registry = CronRegistry::new();
        let entry = registry.create("0 * * * *", "Hourly task", None, None);

        assert_eq!(entry.run_count, 0);
        assert!(entry.last_run_at.is_none());

        // Record first run
        registry.record_run(&entry.cron_id).unwrap();
        let retrieved = registry.get(&entry.cron_id).unwrap();
        assert_eq!(retrieved.run_count, 1);
        assert!(retrieved.last_run_at.is_some());

        // Record second run
        registry.record_run(&entry.cron_id).unwrap();
        let retrieved = registry.get(&entry.cron_id).unwrap();
        assert_eq!(retrieved.run_count, 2);
    }

    #[test]
    fn test_cron_update_prompt() {
        let registry = CronRegistry::new();
        let entry = registry.create("0 * * * *", "Original prompt", None, None);

        registry
            .update_prompt(&entry.cron_id, "Updated prompt")
            .unwrap();

        let retrieved = registry.get(&entry.cron_id).unwrap();
        assert_eq!(retrieved.prompt, "Updated prompt");
    }

    #[test]
    fn test_cron_delete() {
        let registry = CronRegistry::new();
        let entry = registry.create("0 * * * *", "To be deleted", None, None);

        // Delete
        let deleted = registry.delete(&entry.cron_id).unwrap();
        assert_eq!(deleted.cron_id, entry.cron_id);

        // Should be gone
        assert!(registry.get(&entry.cron_id).is_none());

        // Delete non-existent
        let result = registry.delete(&entry.cron_id);
        assert!(result.is_err());
    }

    #[test]
    fn test_cron_len_and_is_empty() {
        let registry = CronRegistry::new();
        assert!(registry.is_empty());
        assert_eq!(registry.len(), 0);

        registry.create("0 * * * *", "Task 1", None, None);
        assert!(!registry.is_empty());
        assert_eq!(registry.len(), 1);

        registry.create("0 9 * * *", "Task 2", None, None);
        assert_eq!(registry.len(), 2);
    }

    #[test]
    fn test_cron_not_found_errors() {
        let registry = CronRegistry::new();

        assert!(registry.get("nonexistent").is_none());
        assert!(registry.disable("nonexistent").is_err());
        assert!(registry.enable("nonexistent").is_err());
        assert!(registry.record_run("nonexistent").is_err());
        assert!(registry.update_prompt("nonexistent", "new").is_err());
        assert!(registry.delete("nonexistent").is_err());
    }

    #[test]
    fn test_global_registry() {
        // First call initializes
        let registry1 = global_cron_registry();
        let entry = registry1.create("0 * * * *", "Test cron", None, None);

        // Second call returns same registry
        let registry2 = global_cron_registry();
        let retrieved = registry2.get(&entry.cron_id);

        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().cron_id, entry.cron_id);
    }

    #[test]
    fn test_cron_id_format() {
        let registry = CronRegistry::new();
        let entry = registry.create("0 * * * *", "Test", None, None);

        // Cron ID should start with "cron_"
        assert!(entry.cron_id.starts_with("cron_"));

        // Should have timestamp and counter parts
        let parts: Vec<&str> = entry.cron_id.split('_').collect();
        assert_eq!(parts.len(), 3); // "cron", timestamp, counter
    }

    #[test]
    fn test_cron_dependencies() {
        let registry = CronRegistry::new();
        let entry1 = registry.create("0 9 * * *", "Task 1", None, None);
        let entry2 = registry.create("0 10 * * *", "Task 2", None, None);

        // Add valid dependency
        registry
            .add_dependency(&entry2.cron_id, &entry1.cron_id)
            .unwrap();
        let retrieved = registry.get(&entry2.cron_id).unwrap();
        assert_eq!(retrieved.depends_on, vec![entry1.cron_id.clone()]);

        // Add invalid dependency
        assert!(registry
            .add_dependency(&entry1.cron_id, "nonexistent")
            .is_err());

        // Add circular dependency
        assert!(registry
            .add_dependency(&entry1.cron_id, &entry2.cron_id)
            .is_err());

        registry
            .remove_dependency(&entry2.cron_id, &entry1.cron_id)
            .unwrap();
        let retrieved = registry.get(&entry2.cron_id).unwrap();
        assert!(retrieved.depends_on.is_empty());
    }

    #[test]
    fn test_dependency_satisfaction() {
        let registry = CronRegistry::new();
        let parent = registry.create("0 9 * * *", "Parent", None, None);
        let child = registry.create("0 10 * * *", "Child", None, None);
        registry
            .add_dependency(&child.cron_id, &parent.cron_id)
            .unwrap();

        // Initially not satisfied (parent never ran)
        assert!(!registry.check_dependencies_satisfied(&child.cron_id));

        // Parent failed -> not satisfied
        registry
            .update_status(&parent.cron_id, CronStatus::Failed, None)
            .unwrap();
        registry.record_run(&parent.cron_id).unwrap();
        assert!(!registry.check_dependencies_satisfied(&child.cron_id));

        // Parent succeeded -> satisfied
        registry
            .update_status(&parent.cron_id, CronStatus::Succeeded, None)
            .unwrap();
        registry.record_run(&parent.cron_id).unwrap();
        assert!(registry.check_dependencies_satisfied(&child.cron_id));

        // Child runs
        std::thread::sleep(std::time::Duration::from_millis(2));
        registry.record_run(&child.cron_id).unwrap();
        // Now not satisfied again because parent hasn't run SINCE child ran
        assert!(!registry.check_dependencies_satisfied(&child.cron_id));

        // Parent runs again -> satisfied
        std::thread::sleep(std::time::Duration::from_millis(2));
        registry.record_run(&parent.cron_id).unwrap();

        let child_entry = registry.get(&child.cron_id).unwrap();
        let parent_entry = registry.get(&parent.cron_id).unwrap();
        eprintln!(
            "Child last: {:?}, run_count: {}",
            child_entry.last_run_at, child_entry.run_count
        );
        eprintln!(
            "Parent last: {:?}, status: {:?}",
            parent_entry.last_run_at, parent_entry.status
        );

        assert!(registry.check_dependencies_satisfied(&child.cron_id));
    }

    #[test]
    fn test_cron_retry_config() {
        let registry = CronRegistry::new();
        let entry = registry.create("0 * * * *", "Task", None, None);

        let retry = RetryConfig {
            max_attempts: 3,
            current_attempt: 0,
            backoff: BackoffStrategy::Exponential,
            interval_secs: 60,
        };

        registry
            .set_retry_config(&entry.cron_id, retry.clone())
            .unwrap();
        let retrieved = registry.get(&entry.cron_id).unwrap();
        assert_eq!(retrieved.retry_config.unwrap().max_attempts, 3);
    }

    #[test]
    fn test_cron_status_updates() {
        let registry = CronRegistry::new();
        let entry = registry.create("0 * * * *", "Task", None, None);

        registry
            .update_status(
                &entry.cron_id,
                CronStatus::Failed,
                Some("timeout".to_string()),
            )
            .unwrap();
        let retrieved = registry.get(&entry.cron_id).unwrap();
        assert_eq!(retrieved.status, CronStatus::Failed);
        assert_eq!(retrieved.last_error, Some("timeout".to_string()));
    }

    #[test]
    fn test_cron_persistence() {
        let registry = CronRegistry::new();
        registry.create("0 9 * * *", "Morning Task", None, None);

        let json = registry.to_json();
        assert!(json.contains("Morning Task"));

        let registry2 = CronRegistry::new();
        registry2.from_json(&json).unwrap();
        assert_eq!(registry2.len(), 1);
        assert_eq!(registry2.list(false)[0].prompt, "Morning Task");
    }

    #[test]
    fn test_enabled_helper() {
        let registry = CronRegistry::new();

        let entry1 = registry.create("0 9 * * *", "Morning", None, None);
        let entry2 = registry.create("0 17 * * *", "Evening", None, None);

        registry.disable(&entry1.cron_id).unwrap();

        // enabled() should only return enabled entries
        let enabled = registry.enabled();
        assert_eq!(enabled.len(), 1);
        assert_eq!(enabled[0].cron_id, entry2.cron_id);
    }

    #[test]
    fn test_manual_trigger() {
        let registry = CronRegistry::new();
        let entry = registry.create("0 9 * * *", "Triggered task", None, None);

        // Initially not triggered
        assert!(!registry.get(&entry.cron_id).unwrap().manual_trigger);

        // Set trigger
        registry.trigger(&entry.cron_id).unwrap();
        assert!(registry.get(&entry.cron_id).unwrap().manual_trigger);

        // Reset trigger
        registry.reset_trigger(&entry.cron_id).unwrap();
        assert!(!registry.get(&entry.cron_id).unwrap().manual_trigger);
    }

    #[test]
    fn test_trigger_nonexistent_errors() {
        let registry = CronRegistry::new();
        assert!(registry.trigger("nonexistent").is_err());
        assert!(registry.reset_trigger("nonexistent").is_err());
    }
}
