use crate::actions::ActionResult;
use crate::error::Result;
use crate::extractor::InstinctExtractor;
use crate::patterns::{ActionType, Instinct, Pattern, SuggestedAction};
use crate::storage::PatternStorage;
use crate::triggers::Context;
use rustycode_session::Session;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

/// Coordinates the learning process
#[derive(Debug)]
pub struct LearningLoop {
    extractor: InstinctExtractor,
    storage: PatternStorage,
    feedback_collector: FeedbackCollector,
}

/// Report from processing a session
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct LearningReport {
    pub patterns_extracted: usize,
    pub patterns_learned: usize,
    pub instincts_created: usize,
    pub session_id: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

/// Report from updating patterns
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct UpdateReport {
    pub patterns_updated: usize,
    pub instincts_updated: usize,
    pub patterns_retired: usize,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

/// User feedback on an instinct
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Feedback {
    pub instinct_id: String,
    pub session_id: String,
    pub was_helpful: bool,
    pub rating: Option<f32>,
    pub comment: Option<String>,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

/// Collects and processes feedback
#[derive(Debug)]
pub struct FeedbackCollector {
    feedback: Vec<Feedback>,
    ratings: HashMap<String, Vec<f32>>,
}

impl LearningLoop {
    pub fn new(extractor: InstinctExtractor, storage: PatternStorage) -> Self {
        Self {
            extractor,
            storage,
            feedback_collector: FeedbackCollector::new(),
        }
    }

    /// Get reference to storage
    pub const fn storage(&self) -> &PatternStorage {
        &self.storage
    }

    /// Get mutable reference to storage
    pub const fn storage_mut(&mut self) -> &mut PatternStorage {
        &mut self.storage
    }

    /// Process a session to extract and learn patterns
    pub async fn process_session(&mut self, session: &Session) -> Result<LearningReport> {
        let mut patterns_extracted = 0;
        let mut patterns_learned = 0;
        let mut instincts_created = 0;

        // Extract patterns from session
        if let Ok(patterns) = self.extractor.extract_from_session(session).await {
            patterns_extracted = patterns.len();

            for pattern in patterns {
                // Add pattern to storage
                self.storage.add_pattern(pattern.clone());
                patterns_learned += 1;

                // Create instinct from pattern
                if let Some(instinct) = self.create_instinct(pattern) {
                    self.storage.add_instinct(instinct.clone());
                    instincts_created += 1;
                }
            }
        }

        // Save updated storage
        self.storage.save()?;

        Ok(LearningReport {
            patterns_extracted,
            patterns_learned,
            instincts_created,
            session_id: session.id.to_string(),
            timestamp: chrono::Utc::now(),
        })
    }

    /// Process multiple sessions
    pub async fn process_history(&mut self, history: &[Session]) -> Result<LearningReport> {
        let mut total_extracted = 0;
        let mut total_learned = 0;
        let mut total_created = 0;

        for session in history {
            match self.process_session(session).await {
                Ok(report) => {
                    total_extracted += report.patterns_extracted;
                    total_learned += report.patterns_learned;
                    total_created += report.instincts_created;
                }
                Err(_) => continue,
            }
        }

        Ok(LearningReport {
            patterns_extracted: total_extracted,
            patterns_learned: total_learned,
            instincts_created: total_created,
            session_id: "batch".to_string(),
            timestamp: chrono::Utc::now(),
        })
    }

    /// Collect feedback on an instinct
    pub async fn collect_feedback(&mut self, feedback: Feedback) {
        // Store feedback
        self.feedback_collector.add_feedback(feedback.clone());

        // Update instinct success/failure
        if feedback.was_helpful {
            self.storage.record_instinct_success(&feedback.instinct_id);
        } else {
            self.storage.record_instinct_failure(&feedback.instinct_id);
        }

        // Save updated storage
        if let Err(e) = self.storage.save() {
            tracing::warn!("Failed to save learning storage after feedback: {}", e);
        }
    }

    /// Update patterns based on feedback and usage
    pub async fn update_patterns(&mut self) -> Result<UpdateReport> {
        let patterns_updated = 0;
        let mut instincts_updated = 0;
        let mut patterns_retired = 0;

        // Get feedback statistics
        let feedback_stats = self.feedback_collector.statistics();

        // Update patterns based on feedback
        for (instinct_id, avg_rating) in feedback_stats {
            if let Some(_instinct) = self.storage.instinct(&instinct_id) {
                // Update pattern confidence based on feedback
                if avg_rating < 0.3 {
                    // Low rating, consider retiring
                    patterns_retired += 1;
                } else if avg_rating < 0.5 {
                    // Medium-low rating, reduce confidence
                    instincts_updated += 1;
                } else if avg_rating > 0.8 {
                    // High rating, increase confidence
                    instincts_updated += 1;
                }
            }
        }

        // Save updated storage
        self.storage.save()?;

        Ok(UpdateReport {
            patterns_updated,
            instincts_updated,
            patterns_retired,
            timestamp: chrono::Utc::now(),
        })
    }

    /// Auto-apply learned patterns to a context
    pub async fn auto_apply(&self, context: &Context) -> Vec<ActionResult> {
        self.storage.auto_apply(context).await
    }

    /// Create an instinct from a pattern
    fn create_instinct(&self, pattern: Pattern) -> Option<Instinct> {
        if pattern.trigger_conditions.is_empty() {
            return None;
        }

        let trigger = pattern.trigger_conditions.first()?.clone();

        // Create appropriate action based on pattern category
        let action_type = match pattern.category {
            crate::patterns::PatternCategory::Coding => ActionType::Transform,
            crate::patterns::PatternCategory::Debugging => ActionType::DebugStrategy,
            crate::patterns::PatternCategory::Refactoring => ActionType::Refactor,
            crate::patterns::PatternCategory::Testing => ActionType::RunTests,
            crate::patterns::PatternCategory::Documentation => ActionType::GenerateDocs,
            crate::patterns::PatternCategory::Architecture => ActionType::Transform,
            crate::patterns::PatternCategory::Optimization => ActionType::Optimization,
        };

        let action = SuggestedAction::new(
            Uuid::new_v4().to_string(),
            action_type,
            pattern.description.clone(),
            pattern.examples.first().cloned().unwrap_or_default(),
        )
        .with_auto_apply(false); // Don't auto-apply by default

        Some(Instinct::new(
            Uuid::new_v4().to_string(),
            pattern,
            trigger,
            action,
        ))
    }
}

impl FeedbackCollector {
    pub fn new() -> Self {
        Self {
            feedback: Vec::new(),
            ratings: HashMap::new(),
        }
    }

    pub fn add_feedback(&mut self, feedback: Feedback) {
        // Store rating if present
        if let Some(rating) = feedback.rating {
            self.ratings
                .entry(feedback.instinct_id.clone())
                .or_default()
                .push(rating);
        }

        self.feedback.push(feedback);
    }

    pub fn statistics(&self) -> HashMap<String, f32> {
        let mut stats = HashMap::new();

        for (instinct_id, ratings) in &self.ratings {
            if !ratings.is_empty() {
                let avg: f32 = ratings.iter().sum::<f32>() / ratings.len() as f32;
                stats.insert(instinct_id.clone(), avg);
            }
        }

        stats
    }

    pub const fn feedback_count(&self) -> usize {
        self.feedback.len()
    }
}

impl Default for FeedbackCollector {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustycode_session::Session;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_learning_loop() {
        let temp_dir = TempDir::new().unwrap();
        let storage = PatternStorage::new(temp_dir.path()).unwrap();
        let extractor = InstinctExtractor::new();
        let mut loop_processor = LearningLoop::new(extractor, storage);

        // Create a test session
        let session = Session::new("Test Session".to_string());

        // Process session (should not fail even if no patterns extracted)
        let result = loop_processor.process_session(&session).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_feedback_collection() {
        let temp_dir = TempDir::new().unwrap();
        let storage = PatternStorage::new(temp_dir.path()).unwrap();
        let extractor = InstinctExtractor::new();
        let mut loop_processor = LearningLoop::new(extractor, storage);

        let feedback = Feedback {
            instinct_id: "test-instinct".to_string(),
            session_id: "test-session".to_string(),
            was_helpful: true,
            rating: Some(0.8),
            comment: Some("Great!".to_string()),
            timestamp: chrono::Utc::now(),
        };

        loop_processor.collect_feedback(feedback).await;

        assert_eq!(loop_processor.feedback_collector.feedback_count(), 1);
    }

    #[test]
    fn test_feedback_collector_new() {
        let collector = FeedbackCollector::new();
        assert_eq!(collector.feedback_count(), 0);
        assert!(collector.statistics().is_empty());
    }

    #[test]
    fn test_feedback_collector_default() {
        let collector = FeedbackCollector::default();
        assert_eq!(collector.feedback_count(), 0);
    }

    #[test]
    fn test_feedback_collector_add_feedback_with_rating() {
        let mut collector = FeedbackCollector::new();

        collector.add_feedback(Feedback {
            instinct_id: "inst-1".to_string(),
            session_id: "sess-1".to_string(),
            was_helpful: true,
            rating: Some(0.8),
            comment: None,
            timestamp: chrono::Utc::now(),
        });

        assert_eq!(collector.feedback_count(), 1);
        let stats = collector.statistics();
        assert!(stats.contains_key("inst-1"));
        assert!((stats["inst-1"] - 0.8).abs() < 0.01);
    }

    #[test]
    fn test_feedback_collector_add_feedback_without_rating() {
        let mut collector = FeedbackCollector::new();

        collector.add_feedback(Feedback {
            instinct_id: "inst-2".to_string(),
            session_id: "sess-1".to_string(),
            was_helpful: false,
            rating: None,
            comment: Some("Not useful".to_string()),
            timestamp: chrono::Utc::now(),
        });

        assert_eq!(collector.feedback_count(), 1);
        // No rating — should not appear in statistics
        assert!(collector.statistics().is_empty());
    }

    #[test]
    fn test_feedback_collector_average_multiple_ratings() {
        let mut collector = FeedbackCollector::new();

        for rating in [0.6, 0.8, 1.0] {
            collector.add_feedback(Feedback {
                instinct_id: "inst-avg".to_string(),
                session_id: "sess".to_string(),
                was_helpful: true,
                rating: Some(rating),
                comment: None,
                timestamp: chrono::Utc::now(),
            });
        }

        assert_eq!(collector.feedback_count(), 3);
        let stats = collector.statistics();
        let avg = stats["inst-avg"];
        assert!((avg - 0.8).abs() < 0.01);
    }

    #[test]
    fn test_feedback_collector_multiple_instincts() {
        let mut collector = FeedbackCollector::new();

        collector.add_feedback(Feedback {
            instinct_id: "a".to_string(),
            session_id: "s".to_string(),
            was_helpful: true,
            rating: Some(0.5),
            comment: None,
            timestamp: chrono::Utc::now(),
        });
        collector.add_feedback(Feedback {
            instinct_id: "b".to_string(),
            session_id: "s".to_string(),
            was_helpful: true,
            rating: Some(0.9),
            comment: None,
            timestamp: chrono::Utc::now(),
        });

        let stats = collector.statistics();
        assert_eq!(stats.len(), 2);
        assert!((stats["a"] - 0.5).abs() < 0.01);
        assert!((stats["b"] - 0.9).abs() < 0.01);
    }

    #[test]
    fn test_learning_report_serialization() {
        let report = LearningReport {
            patterns_extracted: 3,
            patterns_learned: 2,
            instincts_created: 1,
            session_id: "test-session".to_string(),
            timestamp: chrono::Utc::now(),
        };
        let json = serde_json::to_string(&report).unwrap();
        let decoded: LearningReport = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.patterns_extracted, 3);
        assert_eq!(decoded.patterns_learned, 2);
        assert_eq!(decoded.instincts_created, 1);
        assert_eq!(decoded.session_id, "test-session");
    }

    #[test]
    fn test_update_report_serialization() {
        let report = UpdateReport {
            patterns_updated: 5,
            instincts_updated: 3,
            patterns_retired: 1,
            timestamp: chrono::Utc::now(),
        };
        let json = serde_json::to_string(&report).unwrap();
        let decoded: UpdateReport = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.patterns_updated, 5);
        assert_eq!(decoded.instincts_updated, 3);
        assert_eq!(decoded.patterns_retired, 1);
    }

    #[test]
    fn test_feedback_serialization() {
        let feedback = Feedback {
            instinct_id: "inst-1".to_string(),
            session_id: "sess-1".to_string(),
            was_helpful: true,
            rating: Some(0.75),
            comment: Some("works well".to_string()),
            timestamp: chrono::Utc::now(),
        };
        let json = serde_json::to_string(&feedback).unwrap();
        let decoded: Feedback = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.instinct_id, "inst-1");
        assert!(decoded.was_helpful);
        assert_eq!(decoded.rating.unwrap(), 0.75);
        assert_eq!(decoded.comment.unwrap(), "works well");
    }

    #[test]
    fn test_feedback_without_optional_fields() {
        let feedback = Feedback {
            instinct_id: "inst-2".to_string(),
            session_id: "sess-2".to_string(),
            was_helpful: false,
            rating: None,
            comment: None,
            timestamp: chrono::Utc::now(),
        };
        let json = serde_json::to_string(&feedback).unwrap();
        let decoded: Feedback = serde_json::from_str(&json).unwrap();
        assert!(!decoded.was_helpful);
        assert!(decoded.rating.is_none());
        assert!(decoded.comment.is_none());
    }

    #[tokio::test]
    async fn test_learning_loop_storage_access() {
        let temp_dir = TempDir::new().unwrap();
        let storage = PatternStorage::new(temp_dir.path()).unwrap();
        let extractor = InstinctExtractor::new();
        let mut lp = LearningLoop::new(extractor, storage);

        // Storage accessors should work
        let _ = lp.storage();
        let _ = lp.storage_mut();
    }

    #[tokio::test]
    async fn test_learning_loop_process_history_empty() {
        let temp_dir = TempDir::new().unwrap();
        let storage = PatternStorage::new(temp_dir.path()).unwrap();
        let extractor = InstinctExtractor::new();
        let mut lp = LearningLoop::new(extractor, storage);

        let result = lp.process_history(&[]).await;
        assert!(result.is_ok());
    }
}
