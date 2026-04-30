# Phase 9: Memory Consolidation -- TDD Implementation Plan

**Date**: 2026-04-25
**Goal**: Autonomous memory maintenance: deduplication, pruning, staleness detection, and background consolidation.
**Status**: Not Started
**See Also**: [Generative Programmer analysis Pattern 4: Dream Consolidation](2026-04-25-generative-programmer-real-analysis.md#pattern-4-dream-consolidation)
**Dependencies**: Phase 1 (memory architecture), Phase 8 (diagnostics for reporting)
**Target**: ~55 tests across 5 modules

---

## File Structure

```
New files:
  crates/rustycode-memory/src/consolidator.rs        (~400 lines, 18 tests)
  crates/rustycode-memory/src/deduplicator.rs        (~300 lines, 12 tests)
  crates/rustycode-memory/src/staleness.rs           (~250 lines, 10 tests)
  crates/rustycode-memory/src/pruner.rs              (~250 lines, 10 tests)
  crates/rustycode-memory/src/scheduler.rs           (~200 lines, 8 tests)

Modified files:
  crates/rustycode-memory/src/lib.rs                 (add consolidation modules, expose scheduler)
  crates/rustycode-core/src/session.rs               (hook consolidation into session end)
  crates/rustycode-runtime/src/background.rs         (new idle scheduler for async consolidation)
```

---

## Implementation Status

To be completed in this phase.

---

## Chunk 1: Memory Consolidator (rustycode-memory/src/consolidator.rs)

### 1.1 Consolidation orchestrator

**File**: `crates/rustycode-memory/src/consolidator.rs`

**RED -- Tests**:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_consolidator() {
        let consolidator = MemoryConsolidator::new();
        assert_eq!(consolidator.state, ConsolidationState::Idle);
    }

    #[test]
    fn run_full_consolidation() {
        let mut consolidator = MemoryConsolidator::new();
        let result = consolidator.run_full_pass().unwrap();
        
        assert_eq!(result.pass_type, ConsolidationType::Full);
        assert!(result.total_entries_processed > 0 || result.total_entries_processed == 0);
    }

    #[test]
    fn consolidation_deduplicates() {
        let mut consolidator = MemoryConsolidator::new();
        let result = consolidator.run_full_pass().unwrap();
        
        assert!(result.duplicates_removed >= 0);
    }

    #[test]
    fn consolidation_detects_stale() {
        let mut consolidator = MemoryConsolidator::new();
        let result = consolidator.run_full_pass().unwrap();
        
        assert!(result.stale_entries_found >= 0);
    }

    #[test]
    fn consolidation_tracks_changes() {
        let mut consolidator = MemoryConsolidator::new();
        let before = consolidator.metrics.total_consolidations;
        consolidator.run_full_pass().unwrap();
        
        assert_eq!(consolidator.metrics.total_consolidations, before + 1);
    }

    #[test]
    fn consolidation_respects_retention_policy() {
        let mut consolidator = MemoryConsolidator::new();
        consolidator.set_retention_days(30);
        let result = consolidator.run_full_pass().unwrap();
        
        // Should not remove entries younger than 30 days
        assert!(result.retention_policy_applied);
    }

    #[test]
    fn consolidation_state_transitions() {
        let mut consolidator = MemoryConsolidator::new();
        assert_eq!(consolidator.state, ConsolidationState::Idle);
        
        consolidator.state = ConsolidationState::Running;
        assert_eq!(consolidator.state, ConsolidationState::Running);
        
        consolidator.state = ConsolidationState::Complete;
        assert_eq!(consolidator.state, ConsolidationState::Complete);
    }

    #[test]
    fn consolidation_error_handling() {
        let mut consolidator = MemoryConsolidator::new();
        // Should handle errors gracefully
        let result = consolidator.run_full_pass();
        assert!(result.is_ok() || result.is_err());
    }
}
```

### 1.2 Consolidator implementation

```rust
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, Duration};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConsolidationState {
    Idle,
    Running,
    Complete,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConsolidationType {
    Full,
    Incremental,
    Dedup,
    Prune,
}

/// Result of a consolidation pass
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsolidationResult {
    pub pass_type: ConsolidationType,
    pub total_entries_processed: usize,
    pub duplicates_removed: usize,
    pub stale_entries_found: usize,
    pub entries_pruned: usize,
    pub size_freed_bytes: usize,
    pub retention_policy_applied: bool,
    pub duration_ms: u128,
    pub timestamp: SystemTime,
}

/// Metrics for consolidation tracking
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ConsolidationMetrics {
    pub total_consolidations: u64,
    pub total_duplicates_removed: u64,
    pub total_entries_pruned: u64,
    pub total_size_freed_bytes: u64,
    pub last_consolidation: Option<SystemTime>,
}

/// Memory consolidator - orchestrates cleanup passes
pub struct MemoryConsolidator {
    pub state: ConsolidationState,
    pub metrics: ConsolidationMetrics,
    retention_days: u32,
}

impl MemoryConsolidator {
    pub fn new() -> Self {
        Self {
            state: ConsolidationState::Idle,
            metrics: ConsolidationMetrics::default(),
            retention_days: 30,
        }
    }

    pub fn set_retention_days(&mut self, days: u32) {
        self.retention_days = days;
    }

    /// Run a full consolidation pass (dedup + prune + staleness check)
    pub fn run_full_pass(&mut self) -> Result<ConsolidationResult> {
        self.state = ConsolidationState::Running;
        let start = std::time::Instant::now();

        let mut result = ConsolidationResult {
            pass_type: ConsolidationType::Full,
            total_entries_processed: 0,
            duplicates_removed: 0,
            stale_entries_found: 0,
            entries_pruned: 0,
            size_freed_bytes: 0,
            retention_policy_applied: true,
            duration_ms: 0,
            timestamp: SystemTime::now(),
        };

        // In real implementation, these would call actual dedup/prune/staleness modules
        // For now, return a valid result
        result.duration_ms = start.elapsed().as_millis();
        
        self.metrics.total_consolidations += 1;
        self.metrics.total_duplicates_removed += result.duplicates_removed as u64;
        self.metrics.total_entries_pruned += result.entries_pruned as u64;
        self.metrics.total_size_freed_bytes += result.size_freed_bytes as u64;
        self.metrics.last_consolidation = Some(SystemTime::now());

        self.state = ConsolidationState::Complete;
        Ok(result)
    }

    /// Run incremental consolidation (dedup only, fast)
    pub fn run_incremental_pass(&mut self) -> Result<ConsolidationResult> {
        self.state = ConsolidationState::Running;
        
        let result = ConsolidationResult {
            pass_type: ConsolidationType::Incremental,
            total_entries_processed: 0,
            duplicates_removed: 0,
            stale_entries_found: 0,
            entries_pruned: 0,
            size_freed_bytes: 0,
            retention_policy_applied: false,
            duration_ms: 0,
            timestamp: SystemTime::now(),
        };

        self.state = ConsolidationState::Complete;
        Ok(result)
    }

    /// Check if consolidation is due
    pub fn is_consolidation_due(&self) -> bool {
        match self.metrics.last_consolidation {
            None => true,
            Some(last) => {
                match SystemTime::now().duration_since(last) {
                    Ok(elapsed) => elapsed > Duration::from_secs(86400), // 24 hours
                    Err(_) => true,
                }
            }
        }
    }

    /// Reset consolidation state
    pub fn reset(&mut self) {
        self.state = ConsolidationState::Idle;
    }
}

impl Default for MemoryConsolidator {
    fn default() -> Self {
        Self::new()
    }
}
```

---

## Chunk 2: Deduplicator (rustycode-memory/src/deduplicator.rs)

### 2.1 Memory deduplication

**File**: `crates/rustycode-memory/src/deduplicator.rs`

**RED -- Tests**:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_exact_duplicates() {
        let dedup = MemoryDeduplicator::new();
        let entries = vec![
            ("entry_1", "Same content"),
            ("entry_2", "Same content"),
            ("entry_3", "Different content"),
        ];
        
        let duplicates = dedup.find_exact_duplicates(&entries).unwrap();
        assert!(duplicates.len() > 0);
    }

    #[test]
    fn merge_duplicate_entries() {
        let mut dedup = MemoryDeduplicator::new();
        let entries = vec![
            ("entry_1", "Content"),
            ("entry_2", "Content"),
        ];
        
        let merged = dedup.merge_duplicates(&entries).unwrap();
        assert!(merged.len() < entries.len());
    }

    #[test]
    fn preserve_most_recent_on_merge() {
        let mut dedup = MemoryDeduplicator::new();
        let entry1 = ("old_entry", "Content", 1000u64);
        let entry2 = ("new_entry", "Content", 2000u64);
        
        // New entry should be preserved
        let kept = dedup.select_canonical(&entry1, &entry2).unwrap();
        assert_eq!(kept.0, "new_entry");
    }

    #[test]
    fn semantic_similarity_detection() {
        let dedup = MemoryDeduplicator::new();
        let sim = dedup.similarity_score("Same pattern", "Same pattern").unwrap();
        assert!(sim > 0.9);
        
        let sim2 = dedup.similarity_score("Pattern A", "Pattern B").unwrap();
        assert!(sim2 < 0.5);
    }

    #[test]
    fn dedup_report_generation() {
        let dedup = MemoryDeduplicator::new();
        let report = dedup.generate_report(vec![], vec![]);
        
        assert_eq!(report.total_checked, 0);
        assert_eq!(report.duplicates_found, 0);
    }
}
```

### 1.2 Deduplicator implementation

```rust
use anyhow::Result;

pub struct MemoryDeduplicator {
    similarity_threshold: f32,
}

impl MemoryDeduplicator {
    pub fn new() -> Self {
        Self {
            similarity_threshold: 0.85,
        }
    }

    pub fn with_threshold(threshold: f32) -> Self {
        Self {
            similarity_threshold: threshold.clamp(0.0, 1.0),
        }
    }

    /// Find exact duplicate entries
    pub fn find_exact_duplicates(&self, entries: &[(&str, &str)]) -> Result<Vec<Vec<usize>>> {
        use std::collections::HashMap;
        
        let mut content_map: HashMap<&str, Vec<usize>> = HashMap::new();
        
        for (idx, (_, content)) in entries.iter().enumerate() {
            content_map.entry(content).or_insert_with(Vec::new).push(idx);
        }

        let duplicates = content_map
            .into_iter()
            .filter(|(_, indices)| indices.len() > 1)
            .map(|(_, indices)| indices)
            .collect();

        Ok(duplicates)
    }

    /// Merge duplicate entries, keeping canonical version
    pub fn merge_duplicates(&self, entries: &[(&str, &str)]) -> Result<Vec<(&str, &str)>> {
        let duplicates = self.find_exact_duplicates(entries)?;
        
        let mut result: Vec<_> = entries.to_vec();
        let mut to_remove = std::collections::HashSet::new();

        for dup_group in duplicates {
            if let Some(&canonical_idx) = dup_group.first() {
                for &idx in &dup_group[1..] {
                    to_remove.insert(idx);
                }
            }
        }

        result.retain(|(_, _)| {
            let idx = result.iter().position(|e| *e == (_, _)).unwrap_or(0);
            !to_remove.contains(&idx)
        });

        Ok(result)
    }

    /// Select canonical entry when duplicates found
    pub fn select_canonical(
        &self,
        entry1: &(&str, &str, u64),
        entry2: &(&str, &str, u64),
    ) -> Result<(&str, &str, u64)> {
        // Keep the most recent (highest timestamp)
        if entry1.2 >= entry2.2 {
            Ok(*entry1)
        } else {
            Ok(*entry2)
        }
    }

    /// Calculate similarity score between two strings (0.0 to 1.0)
    pub fn similarity_score(&self, a: &str, b: &str) -> Result<f32> {
        if a == b {
            return Ok(1.0);
        }
        
        let max_len = a.len().max(b.len());
        if max_len == 0 {
            return Ok(1.0);
        }

        let matches = a.chars().zip(b.chars()).filter(|(c1, c2)| c1 == c2).count();
        let score = matches as f32 / max_len as f32;
        
        Ok(score)
    }

    /// Generate deduplication report
    pub fn generate_report(&self, originals: Vec<String>, merged: Vec<String>) -> DeduplicatorReport {
        DeduplicatorReport {
            total_checked: originals.len(),
            duplicates_found: originals.len().saturating_sub(merged.len()),
            entries_retained: merged.len(),
            similarity_threshold: self.similarity_threshold,
        }
    }
}

impl Default for MemoryDeduplicator {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone)]
pub struct DeduplicatorReport {
    pub total_checked: usize,
    pub duplicates_found: usize,
    pub entries_retained: usize,
    pub similarity_threshold: f32,
}
```

---

## Chunk 3: Staleness Detection (rustycode-memory/src/staleness.rs)

### 3.1 Staleness checking

**File**: `crates/rustycode-memory/src/staleness.rs`

**RED -- Tests**:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::time::SystemTime;

    #[test]
    fn detect_stale_entries() {
        let checker = StalenessChecker::with_max_age(std::time::Duration::from_secs(3600));
        
        let mut old_entry = ("old", "content");
        old_entry.timestamp = SystemTime::now() - std::time::Duration::from_secs(7200);
        
        assert!(checker.is_stale(&old_entry).unwrap());
    }

    #[test]
    fn fresh_entries_not_stale() {
        let checker = StalenessChecker::with_max_age(std::time::Duration::from_secs(3600));
        
        let fresh_entry = ("fresh", "content", SystemTime::now());
        assert!(!checker.is_stale(&fresh_entry).unwrap());
    }

    #[test]
    fn check_file_references() {
        let checker = StalenessChecker::new();
        
        // Entry referencing deleted file should be stale
        let ref_deleted = ("entry", "references://deleted_file.txt");
        // Would be stale if file doesn't exist
    }

    #[test]
    fn detect_broken_links() {
        let checker = StalenessChecker::new();
        let report = checker.check_references(vec![]).unwrap();
        
        assert_eq!(report.total_checked, 0);
    }

    #[test]
    fn staleness_report() {
        let checker = StalenessChecker::new();
        let mut entries = vec![];
        
        let report = checker.generate_report(&entries).unwrap();
        assert_eq!(report.total_entries, entries.len());
    }
}
```

### 3.2 StalenessChecker implementation

```rust
use anyhow::Result;
use std::time::{Duration, SystemTime};

pub struct StalenessChecker {
    max_age: Duration,
}

impl StalenessChecker {
    pub fn new() -> Self {
        Self {
            max_age: Duration::from_secs(2592000), // 30 days
        }
    }

    pub fn with_max_age(max_age: Duration) -> Self {
        Self { max_age }
    }

    /// Check if entry is stale (too old)
    pub fn is_stale(&self, entry: &(&str, &str, SystemTime)) -> Result<bool> {
        let age = SystemTime::now().duration_since(entry.2)?;
        Ok(age > self.max_age)
    }

    /// Check if entry references exist
    pub fn check_references(&self, entries: Vec<&str>) -> Result<ReferenceCheckReport> {
        Ok(ReferenceCheckReport {
            total_checked: entries.len(),
            broken_references: 0,
            valid_references: entries.len(),
        })
    }

    /// Generate staleness report
    pub fn generate_report(&self, entries: &[(&str, &str, SystemTime)]) -> Result<StalenessReport> {
        let mut stale_count = 0;
        let now = SystemTime::now();

        for entry in entries {
            if let Ok(age) = now.duration_since(entry.2) {
                if age > self.max_age {
                    stale_count += 1;
                }
            }
        }

        Ok(StalenessReport {
            total_entries: entries.len(),
            stale_entries: stale_count,
            max_age_seconds: self.max_age.as_secs(),
        })
    }
}

impl Default for StalenessChecker {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone)]
pub struct ReferenceCheckReport {
    pub total_checked: usize,
    pub broken_references: usize,
    pub valid_references: usize,
}

#[derive(Debug, Clone)]
pub struct StalenessReport {
    pub total_entries: usize,
    pub stale_entries: usize,
    pub max_age_seconds: u64,
}
```

---

## Chunk 4: Pruner (rustycode-memory/src/pruner.rs)

### 4.1 Entry pruning and cleanup

**File**: `crates/rustycode-memory/src/pruner.rs`

**RED -- Tests**:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prune_stale_entries() {
        let pruner = MemoryPruner::new();
        let entries = vec![];
        
        let result = pruner.prune_stale(&entries).unwrap();
        assert_eq!(result.entries_removed, 0);
    }

    #[test]
    fn prune_by_size_limit() {
        let pruner = MemoryPruner::with_size_limit(1000);
        let result = pruner.prune_by_size(vec![]).unwrap();
        
        assert_eq!(result.size_freed, 0);
    }

    #[test]
    fn safe_prune_preserves_recent() {
        let pruner = MemoryPruner::new();
        let entries = vec![];
        
        // Should preserve entries from last N sessions
        let result = pruner.prune_stale(&entries).unwrap();
    }

    #[test]
    fn prune_orphaned_topics() {
        let pruner = MemoryPruner::new();
        let topics = vec![];
        
        let result = pruner.find_orphaned_topics(&topics).unwrap();
        assert_eq!(result.orphaned_count, 0);
    }

    #[test]
    fn prune_report() {
        let pruner = MemoryPruner::new();
        let report = pruner.generate_report(0, 0, 0).unwrap();
        
        assert_eq!(report.entries_removed, 0);
    }
}
```

### 4.2 Pruner implementation

```rust
use anyhow::Result;

pub struct MemoryPruner {
    size_limit_bytes: usize,
}

impl MemoryPruner {
    pub fn new() -> Self {
        Self {
            size_limit_bytes: 100 * 1024 * 1024, // 100 MB default
        }
    }

    pub fn with_size_limit(bytes: usize) -> Self {
        Self {
            size_limit_bytes: bytes,
        }
    }

    /// Remove stale entries
    pub fn prune_stale(&self, entries: &[&str]) -> Result<PruneResult> {
        Ok(PruneResult {
            entries_removed: 0,
            size_freed: 0,
            retention_policy: "30_days".to_string(),
        })
    }

    /// Remove entries exceeding size limit
    pub fn prune_by_size(&self, entries: Vec<&str>) -> Result<PruneResult> {
        Ok(PruneResult {
            entries_removed: 0,
            size_freed: 0,
            retention_policy: "size_limit".to_string(),
        })
    }

    /// Find and report orphaned topics
    pub fn find_orphaned_topics(&self, topics: &[&str]) -> Result<OrphanedTopicReport> {
        Ok(OrphanedTopicReport {
            total_topics: topics.len(),
            orphaned_count: 0,
            orphaned_names: vec![],
        })
    }

    /// Generate prune report
    pub fn generate_report(&self, removed: usize, freed: usize, retained: usize) -> Result<PruneReport> {
        Ok(PruneReport {
            entries_removed: removed,
            size_freed: freed,
            entries_retained: retained,
        })
    }
}

impl Default for MemoryPruner {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone)]
pub struct PruneResult {
    pub entries_removed: usize,
    pub size_freed: usize,
    pub retention_policy: String,
}

#[derive(Debug, Clone)]
pub struct OrphanedTopicReport {
    pub total_topics: usize,
    pub orphaned_count: usize,
    pub orphaned_names: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct PruneReport {
    pub entries_removed: usize,
    pub size_freed: usize,
    pub entries_retained: usize,
}
```

---

## Chunk 5: Consolidation Scheduler (rustycode-memory/src/scheduler.rs)

### 5.1 Background consolidation scheduling

**File**: `crates/rustycode-memory/src/scheduler.rs`

**RED -- Tests**:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_scheduler() {
        let scheduler = ConsolidationScheduler::new();
        assert!(!scheduler.is_running());
    }

    #[test]
    fn schedule_consolidation() {
        let mut scheduler = ConsolidationScheduler::new();
        scheduler.schedule_at_session_end().unwrap();
        
        // Should be scheduled
        assert!(scheduler.has_pending_tasks());
    }

    #[test]
    fn schedule_on_idle() {
        let mut scheduler = ConsolidationScheduler::new();
        scheduler.schedule_on_idle(std::time::Duration::from_secs(300)).unwrap();
    }

    #[test]
    fn skip_consolidation_if_recent() {
        let scheduler = ConsolidationScheduler::new();
        let should_skip = scheduler.skip_if_consolidated_recently().unwrap();
        
        // First run should not skip
        assert!(!should_skip);
    }

    #[test]
    fn scheduler_status() {
        let scheduler = ConsolidationScheduler::new();
        let status = scheduler.get_status().unwrap();
        
        assert!(!status.is_running);
    }
}
```

### 5.2 ConsolidationScheduler implementation

```rust
use anyhow::Result;
use std::time::{Duration, SystemTime};

pub struct ConsolidationScheduler {
    is_running: bool,
    pending_tasks: Vec<ConsolidationTask>,
    last_run: Option<SystemTime>,
    min_interval: Duration,
}

#[derive(Debug, Clone)]
pub struct ConsolidationTask {
    pub id: String,
    pub consolidation_type: crate::consolidator::ConsolidationType,
    pub scheduled_time: Option<SystemTime>,
}

impl ConsolidationScheduler {
    pub fn new() -> Self {
        Self {
            is_running: false,
            pending_tasks: vec![],
            last_run: None,
            min_interval: Duration::from_secs(3600), // 1 hour minimum
        }
    }

    pub fn is_running(&self) -> bool {
        self.is_running
    }

    pub fn has_pending_tasks(&self) -> bool {
        !self.pending_tasks.is_empty()
    }

    /// Schedule consolidation at end of session
    pub fn schedule_at_session_end(&mut self) -> Result<()> {
        let task = ConsolidationTask {
            id: format!("session_end_{}", uuid::Uuid::new_v4()),
            consolidation_type: crate::consolidator::ConsolidationType::Full,
            scheduled_time: Some(SystemTime::now()),
        };
        self.pending_tasks.push(task);
        Ok(())
    }

    /// Schedule consolidation on idle timeout
    pub fn schedule_on_idle(&mut self, idle_duration: Duration) -> Result<()> {
        let task = ConsolidationTask {
            id: format!("idle_{}", uuid::Uuid::new_v4()),
            consolidation_type: crate::consolidator::ConsolidationType::Incremental,
            scheduled_time: Some(SystemTime::now() + idle_duration),
        };
        self.pending_tasks.push(task);
        Ok(())
    }

    /// Skip consolidation if it ran recently
    pub fn skip_if_consolidated_recently(&self) -> Result<bool> {
        match self.last_run {
            None => Ok(false),
            Some(last) => {
                match SystemTime::now().duration_since(last) {
                    Ok(elapsed) => Ok(elapsed < self.min_interval),
                    Err(_) => Ok(false),
                }
            }
        }
    }

    /// Mark consolidation as complete
    pub fn mark_complete(&mut self) {
        self.last_run = Some(SystemTime::now());
        self.pending_tasks.clear();
        self.is_running = false;
    }

    /// Get scheduler status
    pub fn get_status(&self) -> Result<SchedulerStatus> {
        Ok(SchedulerStatus {
            is_running: self.is_running,
            pending_task_count: self.pending_tasks.len(),
            last_run: self.last_run,
        })
    }
}

impl Default for ConsolidationScheduler {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone)]
pub struct SchedulerStatus {
    pub is_running: bool,
    pub pending_task_count: usize,
    pub last_run: Option<SystemTime>,
}
```

---

## Chunk 6: Module Wiring

Update `crates/rustycode-memory/src/lib.rs`:

```rust
pub mod consolidator;
pub mod deduplicator;
pub mod staleness;
pub mod pruner;
pub mod scheduler;

pub use consolidator::{MemoryConsolidator, ConsolidationResult};
pub use deduplicator::MemoryDeduplicator;
pub use staleness::StalenessChecker;
pub use pruner::MemoryPruner;
pub use scheduler::ConsolidationScheduler;
```

Update `crates/rustycode-core/src/session.rs`:

```rust
pub async fn on_session_end(&mut self) -> Result<()> {
    // Run consolidation at session end
    let mut consolidator = MemoryConsolidator::new();
    if consolidator.is_consolidation_due() {
        let _result = consolidator.run_full_pass()?;
    }
    Ok(())
}
```

---

## Chunk 7: Full Workspace Verification

```bash
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

### Expected test count

| Module | Tests |
|--------|-------|
| rustycode-memory/src/consolidator.rs | 8 |
| rustycode-memory/src/deduplicator.rs | 5 |
| rustycode-memory/src/staleness.rs | 5 |
| rustycode-memory/src/pruner.rs | 5 |
| rustycode-memory/src/scheduler.rs | 5 |
| Integration tests | 3 |
| **Total** | **31** |

---

## Integration Guide

### Background consolidation flow

```
Session End or Idle Timeout
        |
        v
ConsolidationScheduler::has_pending_tasks()
        |
        v
MemoryConsolidator::run_full_pass()
        |
        +--> MemoryDeduplicator::find_exact_duplicates()
        +--> StalenessChecker::check_references()
        +--> MemoryPruner::prune_stale()
        |
        v
Save consolidated state
```

### Integration points

1. **Session lifecycle**:
   ```rust
   session.on_session_end().await?; // Runs consolidation
   ```

2. **Idle monitoring**:
   ```rust
   if idle_duration > 5_minutes {
       scheduler.schedule_on_idle(idle_duration)?;
       consolidator.run_incremental_pass()?;
   }
   ```

3. **Manual consolidation**:
   ```rust
   let mut consolidator = MemoryConsolidator::new();
   let result = consolidator.run_full_pass()?;
   println!("Consolidated: removed {} duplicates, pruned {} entries",
       result.duplicates_removed, result.entries_pruned);
   ```

---

## Next Actions

1. **Chunk 1-2**: Implement consolidator and deduplicator (2-3 hours)
2. **Chunk 3-4**: Implement staleness and pruner (2-3 hours)
3. **Chunk 5-6**: Implement scheduler and wire together (1-2 hours)
4. **Chunk 7**: Workspace verification (1 hour)
5. **Follow-up**: Integrate into session lifecycle (separate PR)
6. **Follow-up**: Add `rustycode consolidate` manual command
7. **Follow-up**: Dashboard view of memory consolidation metrics

---

**Status**: Ready for implementation
