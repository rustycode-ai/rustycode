#![allow(clippy::unwrap_used)]

use std::sync::Arc;

use rustycode_classification::{
    AgentRole, ComplexityTier, PatternQuery, StoredPattern, UnifiedTaskClassifier,
};

struct MockFailureStore {
    patterns: Vec<StoredPattern>,
}

impl PatternQuery for MockFailureStore {
    fn query_patterns(&self, _task_type: &str) -> anyhow::Result<Vec<StoredPattern>> {
        Ok(self.patterns.clone())
    }
}

#[test]
fn test_simple_list_is_mundane() {
    let classifier = UnifiedTaskClassifier::new();
    let result = classifier.classify("list files in /tmp");
    assert_eq!(result.tier, ComplexityTier::Light);
    assert_eq!(result.agent_role, AgentRole::Worker);
}

#[test]
fn test_refactoring_is_complex() {
    let classifier = UnifiedTaskClassifier::new();
    let result = classifier.classify("refactor this Rust module to use async/await patterns");
    assert!(result.complexity_score >= 51);
}

#[test]
fn test_show_command_is_mundane() {
    let classifier = UnifiedTaskClassifier::new();
    let result = classifier.classify("show the contents of config.yaml");
    assert_eq!(result.tier, ComplexityTier::Light);
}

#[test]
fn test_debug_is_complex() {
    let classifier = UnifiedTaskClassifier::new();
    let result = classifier.classify("debug why the database migration is failing");
    assert!(result.complexity_score >= 51);
    assert_eq!(result.agent_role, AgentRole::Architect);
}

#[test]
fn test_with_failure_store_elevates_to_complex() {
    let many_patterns: Vec<StoredPattern> = (0..10)
        .map(|i| StoredPattern {
            task_type: format!("task_{i}"),
            occurrence_count: i + 1,
        })
        .collect();

    let store = Arc::new(MockFailureStore {
        patterns: many_patterns,
    });
    let classifier = UnifiedTaskClassifier::new().with_failure_store(store);

    let result = classifier.classify("list files in /tmp");
    assert!(result.signals.requires_context);
    assert!(result.reasoning.contains("Matched"));
}

#[test]
fn test_without_failure_store_stays_mundane() {
    let classifier = UnifiedTaskClassifier::new();
    let result = classifier.classify("read the README file");
    assert_eq!(result.tier, ComplexityTier::Light);
}

#[test]
fn test_default_trait() {
    let classifier = UnifiedTaskClassifier::default();
    let result = classifier.classify("find all TODO comments");
    assert_eq!(result.tier, ComplexityTier::Light);
}

#[test]
fn test_confidence_bounded() {
    let classifier = UnifiedTaskClassifier::new();
    let result = classifier.classify("refactor the entire codebase and optimize performance");
    assert!(result.complexity_score <= 100);
    assert!(result.complexity_score > 0);
}
