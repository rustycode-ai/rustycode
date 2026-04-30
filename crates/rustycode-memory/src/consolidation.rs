//! Dream consolidation engine: deterministic dedup, prune, and merge.
//!
//! Runs on session end (or when entry count exceeds a threshold) to keep
//! the memory store from growing unbounded. Uses purely deterministic logic —
//! no LLM tokens are consumed during consolidation.

use crate::{MemoryDomain, MemoryEntry};
use std::collections::HashMap;

/// Minimum confidence threshold for pruning consideration.
const PRUNE_CONFIDENCE_THRESHOLD: f32 = 0.3;

/// Minimum age in days (since last use) before an entry can be pruned.
const PRUNE_AGE_DAYS: u64 = 30;

/// Minimum confidence to consider two entries "overlapping" for merge.
const MERGE_OVERLAP_THRESHOLD: f32 = 0.5;

/// Result of a consolidation pass.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConsolidationResult {
    /// Number of entries before consolidation.
    pub entries_before: usize,
    /// Number of entries after consolidation.
    pub entries_after: usize,
    /// Number of exact duplicates removed.
    pub deduped_count: usize,
    /// Number of entries pruned due to age and low confidence.
    pub pruned_count: usize,
    /// Number of entries merged into others.
    pub merged_count: usize,
}

impl ConsolidationResult {
    /// Total number of entries removed.
    #[must_use]
    pub const fn total_removed(&self) -> usize {
        self.deduped_count + self.pruned_count + self.merged_count
    }
}

/// The consolidation engine.
///
/// Runs a deterministic pipeline: dedup -> prune -> merge.
#[derive(Debug)]
pub struct ConsolidationEngine;

impl ConsolidationEngine {
    /// Create a new consolidation engine.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }

    /// Run the full consolidation pipeline.
    ///
    /// Order: dedup -> prune -> merge.
    pub fn run(&self, entries: Vec<MemoryEntry>) -> (Vec<MemoryEntry>, ConsolidationResult) {
        let entries_before = entries.len();

        let (after_dedup, deduped_count) = Self::dedup(entries);
        let (after_prune, pruned_count) = Self::prune(after_dedup);
        let (after_merge, merged_count) = Self::merge(after_prune);

        let result = ConsolidationResult {
            entries_before,
            entries_after: after_merge.len(),
            deduped_count,
            pruned_count,
            merged_count,
        };

        (after_merge, result)
    }

    /// Remove exact duplicates (same trigger + domain).
    ///
    /// When two entries have the same trigger text (case-insensitive) AND the
    /// same domain, only the one with higher confidence is kept.
    pub fn dedup(entries: Vec<MemoryEntry>) -> (Vec<MemoryEntry>, usize) {
        let mut seen: HashMap<(String, MemoryDomain), MemoryEntry> = HashMap::new();
        let mut deduped_count = 0;

        for entry in entries {
            let key = (entry.trigger.to_lowercase(), entry.domain.clone());
            match seen.get(&key) {
                Some(existing) if existing.confidence >= entry.confidence => {
                    deduped_count += 1;
                }
                Some(_) => {
                    seen.insert(key, entry);
                    deduped_count += 1;
                }
                None => {
                    seen.insert(key, entry);
                }
            }
        }

        let result: Vec<MemoryEntry> = seen.into_values().collect();
        (result, deduped_count)
    }

    /// Prune entries that are old and have low confidence.
    ///
    /// An entry is pruned if:
    /// - confidence < `PRUNE_CONFIDENCE_THRESHOLD` (0.3)
    /// - AND has been used at least once (`last_used` is Some)
    /// - AND `last_used` is more than `PRUNE_AGE_DAYS` (30) days ago
    ///
    /// Entries that have never been used are NOT pruned — they may be new.
    pub fn prune(entries: Vec<MemoryEntry>) -> (Vec<MemoryEntry>, usize) {
        let mut kept = Vec::with_capacity(entries.len());
        let mut pruned_count = 0;

        for entry in entries {
            if entry.confidence <= PRUNE_CONFIDENCE_THRESHOLD {
                if let Some(last_used) = entry.last_used {
                    let days_since = last_used.elapsed().unwrap_or_default().as_secs() / 86400;
                    if days_since > PRUNE_AGE_DAYS {
                        pruned_count += 1;
                        continue;
                    }
                }
            }
            kept.push(entry);
        }

        (kept, pruned_count)
    }

    /// Merge entries with overlapping triggers.
    ///
    /// Two entries are considered overlapping if:
    /// - They share the same domain
    /// - Their trigger texts share at least one word (after lowercasing)
    /// - The overlapping entry has confidence < `MERGE_OVERLAP_THRESHOLD`
    ///
    /// The lower-confidence entry is absorbed (not combined).
    pub fn merge(entries: Vec<MemoryEntry>) -> (Vec<MemoryEntry>, usize) {
        if entries.is_empty() {
            return (entries, 0);
        }

        let mut merged_count = 0;

        let mut by_domain: HashMap<MemoryDomain, Vec<usize>> = HashMap::new();
        for (i, entry) in entries.iter().enumerate() {
            by_domain.entry(entry.domain.clone()).or_default().push(i);
        }

        let mut absorbed = vec![false; entries.len()];

        for (_, indices) in by_domain {
            for &i in &indices {
                if absorbed[i] {
                    continue;
                }
                let entry_words = extract_words(&entries[i].trigger);

                for &j in &indices {
                    if i == j || absorbed[j] {
                        continue;
                    }
                    let other_words = extract_words(&entries[j].trigger);
                    let has_overlap = entry_words
                        .iter()
                        .any(|w| other_words.iter().any(|ow| ow == w));

                    if has_overlap && entries[j].confidence < MERGE_OVERLAP_THRESHOLD {
                        absorbed[j] = true;
                        merged_count += 1;
                    }
                }
            }
        }

        let mut result = Vec::with_capacity(entries.len());
        for (i, entry) in entries.into_iter().enumerate() {
            if !absorbed[i] {
                result.push(entry);
            }
        }

        (result, merged_count)
    }
}

impl Default for ConsolidationEngine {
    fn default() -> Self {
        Self::new()
    }
}

/// Extract unique lowercase words from a string.
fn extract_words(text: &str) -> Vec<String> {
    text.to_lowercase()
        .split_whitespace()
        .filter(|w| w.len() > 3)
        .map(String::from)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{MemoryEntryConfig, MemoryScope, MemorySource};
    use std::time::{Duration, SystemTime};

    fn make_entry(
        id: &str,
        trigger: &str,
        confidence: f32,
        domain: MemoryDomain,
        last_used: Option<SystemTime>,
    ) -> MemoryEntry {
        let mut entry = MemoryEntry::new(MemoryEntryConfig {
            id: id.to_string(),
            trigger: trigger.to_string(),
            confidence,
            domain,
            source: MemorySource::ManualEntry,
            scope: MemoryScope::Global,
            project_id: None,
            action: format!("action for {id}"),
        });
        entry.last_used = last_used;
        entry
    }

    fn days_ago(days: u64) -> SystemTime {
        SystemTime::now() - Duration::from_secs(days * 86400)
    }

    #[test]
    fn dedup_removes_exact_duplicates() {
        let entries = vec![
            make_entry("a", "async code", 0.8, MemoryDomain::CodeStyle, None),
            make_entry("b", "Async Code", 0.5, MemoryDomain::CodeStyle, None),
        ];
        let (result, count) = ConsolidationEngine::dedup(entries);
        assert_eq!(count, 1);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].id, "a");
    }

    #[test]
    fn dedup_keeps_different_triggers() {
        let entries = vec![
            make_entry("a", "async code", 0.8, MemoryDomain::CodeStyle, None),
            make_entry("b", "database queries", 0.7, MemoryDomain::CodeStyle, None),
        ];
        let (result, count) = ConsolidationEngine::dedup(entries);
        assert_eq!(count, 0);
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn dedup_keeps_different_domains() {
        let entries = vec![
            make_entry("a", "patterns", 0.8, MemoryDomain::CodeStyle, None),
            make_entry("b", "patterns", 0.6, MemoryDomain::Testing, None),
        ];
        let (result, count) = ConsolidationEngine::dedup(entries);
        assert_eq!(count, 0);
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn dedup_empty_input() {
        let (result, count) = ConsolidationEngine::dedup(Vec::new());
        assert_eq!(count, 0);
        assert!(result.is_empty());
    }

    #[test]
    fn prune_removes_old_low_confidence() {
        let entries = vec![
            make_entry(
                "old",
                "unused",
                0.2,
                MemoryDomain::CodeStyle,
                Some(days_ago(60)),
            ),
            make_entry(
                "new",
                "active",
                0.8,
                MemoryDomain::CodeStyle,
                Some(days_ago(5)),
            ),
        ];
        let (result, count) = ConsolidationEngine::prune(entries);
        assert_eq!(count, 1);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].id, "new");
    }

    #[test]
    fn prune_keeps_high_confidence_even_if_old() {
        let entries = vec![make_entry(
            "old-good",
            "still valid",
            0.7,
            MemoryDomain::CodeStyle,
            Some(days_ago(90)),
        )];
        let (result, count) = ConsolidationEngine::prune(entries);
        assert_eq!(count, 0);
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn prune_keeps_never_used_entries() {
        let entries = vec![make_entry(
            "never-used",
            "brand new",
            0.2,
            MemoryDomain::CodeStyle,
            None,
        )];
        let (result, count) = ConsolidationEngine::prune(entries);
        assert_eq!(count, 0);
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn prune_keeps_recently_used_low_confidence() {
        let entries = vec![make_entry(
            "recent",
            "just used",
            0.2,
            MemoryDomain::CodeStyle,
            Some(days_ago(5)),
        )];
        let (result, count) = ConsolidationEngine::prune(entries);
        assert_eq!(count, 0);
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn prune_empty_input() {
        let (result, count) = ConsolidationEngine::prune(Vec::new());
        assert_eq!(count, 0);
        assert!(result.is_empty());
    }

    #[test]
    fn merge_combines_overlapping_low_confidence() {
        let entries = vec![
            make_entry(
                "keep",
                "async code patterns",
                0.8,
                MemoryDomain::CodeStyle,
                None,
            ),
            make_entry(
                "absorb",
                "async error handling",
                0.4,
                MemoryDomain::CodeStyle,
                None,
            ),
        ];
        let (result, count) = ConsolidationEngine::merge(entries);
        assert_eq!(count, 1);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].id, "keep");
    }

    #[test]
    fn merge_keeps_high_confidence_overlapping() {
        let entries = vec![
            make_entry(
                "a",
                "async code patterns",
                0.8,
                MemoryDomain::CodeStyle,
                None,
            ),
            make_entry(
                "b",
                "async error handling",
                0.7,
                MemoryDomain::CodeStyle,
                None,
            ),
        ];
        let (result, count) = ConsolidationEngine::merge(entries);
        assert_eq!(count, 0);
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn merge_keeps_different_domains() {
        let entries = vec![
            make_entry("a", "patterns", 0.8, MemoryDomain::CodeStyle, None),
            make_entry("b", "patterns usage", 0.4, MemoryDomain::Testing, None),
        ];
        let (result, count) = ConsolidationEngine::merge(entries);
        assert_eq!(count, 0);
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn merge_empty_input() {
        let (result, count) = ConsolidationEngine::merge(Vec::new());
        assert_eq!(count, 0);
        assert!(result.is_empty());
    }

    #[test]
    fn full_pipeline_runs_in_order() {
        let entries = vec![
            make_entry("dup", "async code", 0.5, MemoryDomain::CodeStyle, None),
            make_entry("orig", "async code", 0.8, MemoryDomain::CodeStyle, None),
            make_entry(
                "old",
                "deprecated",
                0.2,
                MemoryDomain::Testing,
                Some(days_ago(60)),
            ),
            make_entry(
                "over",
                "async code review",
                0.3,
                MemoryDomain::CodeStyle,
                None,
            ),
            make_entry(
                "good",
                "database queries",
                0.9,
                MemoryDomain::Architecture,
                None,
            ),
        ];

        let engine = ConsolidationEngine::new();
        let (result, stats) = engine.run(entries);

        assert_eq!(stats.entries_before, 5);
        assert!(stats.deduped_count >= 1, "should dedup at least 1");
        assert!(stats.pruned_count >= 1, "should prune at least 1");
        assert!(stats.merged_count >= 1, "should merge at least 1");
        assert!(result.len() < 5, "should reduce total entries");
        assert!(stats.entries_after < stats.entries_before);
    }

    #[test]
    fn full_pipeline_empty_input() {
        let engine = ConsolidationEngine::new();
        let (result, stats) = engine.run(Vec::new());
        assert_eq!(stats.entries_before, 0);
        assert_eq!(stats.entries_after, 0);
        assert_eq!(stats.total_removed(), 0);
        assert!(result.is_empty());
    }

    #[test]
    fn full_pipeline_no_changes_needed() {
        let entries = vec![
            make_entry(
                "a",
                "async patterns",
                0.8,
                MemoryDomain::CodeStyle,
                Some(days_ago(5)),
            ),
            make_entry(
                "b",
                "database queries",
                0.9,
                MemoryDomain::Architecture,
                Some(days_ago(2)),
            ),
        ];

        let engine = ConsolidationEngine::new();
        let (result, stats) = engine.run(entries);
        assert_eq!(stats.total_removed(), 0);
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn consolidation_result_total_removed() {
        let result = ConsolidationResult {
            entries_before: 100,
            entries_after: 85,
            deduped_count: 5,
            pruned_count: 7,
            merged_count: 3,
        };
        assert_eq!(result.total_removed(), 15);
    }

    #[test]
    fn extract_words_ignores_short_words() {
        let words = extract_words("a an the async code");
        assert_eq!(words, vec!["async", "code"]);
    }
}
