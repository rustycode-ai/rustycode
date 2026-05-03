//! Complexity classifier -- scores task descriptors to determine routing tier.
//!
//! Uses a weighted heuristic combining step count, description/context length,
//! and keyword detection to classify tasks as [`Simple`], [`Moderate`], or
//! [`Complex`].
//!
//! [`Simple`]: TaskComplexity::Simple
//! [`Moderate`]: TaskComplexity::Moderate
//! [`Complex`]: TaskComplexity::Complex

use serde::{Deserialize, Serialize};

// -- Types --------------------------------------------------------------------

/// Complexity level assigned to a task descriptor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TaskComplexity {
    /// Straightforward task suitable for the fastest tier.
    Simple,
    /// Balanced task requiring a mid-tier model.
    Moderate,
    /// Demanding task that warrants the most capable tier.
    Complex,
}

/// Input for complexity classification.
#[derive(Debug, Clone)]
pub struct TaskDescriptor {
    /// Natural-language description of the task.
    pub description: String,
    /// Surrounding context (prior conversation, file contents, etc.).
    pub context: String,
    /// Number of discrete steps the task is expected to require.
    pub step_count: usize,
}

/// Extracted features used by the scoring function.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskFeatures {
    step_count: usize,
    description_length: usize,
    context_length: usize,
    keyword_count: usize,
}

// -- Keywords -----------------------------------------------------------------

/// Complexity-indicating keywords. Case-insensitive matching.
const COMPLEXITY_KEYWORDS: &[&str] = &[
    "design",
    "architecture",
    "algorithm",
    "optimize",
    "refactor",
    "migrate",
    "integrate",
    "debug",
    "analyze",
    "evaluate",
];

// -- Classifier ---------------------------------------------------------------

/// Stateless classifier that maps [`TaskDescriptor`] values to [`TaskComplexity`].
///
/// The scoring formula is:
///
/// ```text
/// score = step_count
///       + (description_length / 100.0)
///       + (context_length / 500.0)
///       + (keyword_count * 2.0)
/// ```
///
/// Thresholds (configurable via [`ComplexityClassifier::new`]):
///
/// | Complexity | Score range            |
/// |------------|------------------------|
/// | Simple     | score < simple_thresh  |
/// | Moderate   | score < moderate_thresh|
/// | Complex    | otherwise              |
#[derive(Debug, Clone)]
pub struct ComplexityClassifier {
    simple_threshold: usize,
    moderate_threshold: usize,
}

impl Default for ComplexityClassifier {
    fn default() -> Self {
        Self {
            simple_threshold: 2,
            moderate_threshold: 10,
        }
    }
}

impl ComplexityClassifier {
    /// Create a classifier with custom thresholds.
    ///
    /// `simple_threshold` -- scores below this are classified [`Simple`].
    /// `moderate_threshold` -- scores below this (but >= simple) are [`Moderate`].
    pub const fn new(simple_threshold: usize, moderate_threshold: usize) -> Self {
        Self {
            simple_threshold,
            moderate_threshold,
        }
    }

    /// Classify a task descriptor into a complexity level.
    pub fn classify(&self, task: &TaskDescriptor) -> TaskComplexity {
        let features = self.extract_features(task);
        self.score(&features)
    }

    /// Extract numeric features from a task descriptor.
    pub fn extract_features(&self, task: &TaskDescriptor) -> TaskFeatures {
        TaskFeatures {
            step_count: task.step_count,
            description_length: task.description.len(),
            context_length: task.context.len(),
            keyword_count: self.count_complexity_keywords(&task.description)
                + self.count_complexity_keywords(&task.context),
        }
    }

    /// Count how many complexity-indicating keywords appear in `text`.
    ///
    /// Matching is case-insensitive and counts each keyword at most once
    /// regardless of how many times it appears.
    pub fn count_complexity_keywords(&self, text: &str) -> usize {
        let lower = text.to_lowercase();
        COMPLEXITY_KEYWORDS
            .iter()
            .filter(|keyword| lower.contains(*keyword))
            .count()
    }

    /// Apply the weighted scoring formula to produce a complexity level.
    #[allow(clippy::suboptimal_flops)]
    pub fn score(&self, features: &TaskFeatures) -> TaskComplexity {
        let score = (features.description_length as f64)
            .mul_add(0.01, features.step_count as f64)
            + (features.context_length as f64 * 0.002)
            + (features.keyword_count as f64 * 2.0);

        if score < self.simple_threshold as f64 {
            TaskComplexity::Simple
        } else if score < self.moderate_threshold as f64 {
            TaskComplexity::Moderate
        } else {
            TaskComplexity::Complex
        }
    }
}

// -- Tests --------------------------------------------------------------------

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn make_task(desc: &str, context: &str, steps: usize) -> TaskDescriptor {
        TaskDescriptor {
            description: desc.to_string(),
            context: context.to_string(),
            step_count: steps,
        }
    }

    #[test]
    fn test_classifies_simple_exploration() {
        let classifier = ComplexityClassifier::default();
        let task = make_task("List files in /src", "", 1);
        let complexity = classifier.classify(&task);
        assert_eq!(complexity, TaskComplexity::Simple);
    }

    #[test]
    fn test_classifies_moderate_tasks() {
        let classifier = ComplexityClassifier::default();
        // "refactor" is a keyword (+2.0), 5 steps (+5.0), description ~35 chars (+0.35)
        // Total ~7.35 which is >= 2 (simple) and < 10 (moderate) -> Moderate
        let task = make_task("Refactor authentication module", "", 5);
        let complexity = classifier.classify(&task);
        assert_eq!(complexity, TaskComplexity::Moderate);
    }

    #[test]
    fn test_classifies_complex_tasks() {
        let classifier = ComplexityClassifier::default();
        // 20 steps (+20.0), keywords "design"(+2), "algorithm"(+2), "distributed"(no),
        // "architecture"(no, not in keywords), "consensus"(no)
        // Actually "design" and "algorithm" are keywords: +4.0
        // description ~50 chars: +0.5
        // Total ~24.5 which is >= 10 -> Complex
        let task = make_task(
            "Design distributed consensus algorithm",
            "",
            20,
        );
        let complexity = classifier.classify(&task);
        assert_eq!(complexity, TaskComplexity::Complex);
    }

    #[test]
    fn test_keyword_detection() {
        let classifier = ComplexityClassifier::default();
        // No keywords
        assert_eq!(classifier.count_complexity_keywords("list the files"), 0);

        // Single keyword
        assert_eq!(classifier.count_complexity_keywords("debug the issue"), 1);

        // Multiple keywords
        let count = classifier.count_complexity_keywords(
            "design the architecture and optimize the algorithm",
        );
        assert_eq!(count, 4); // design, architecture, optimize, algorithm

        // Case insensitive
        assert_eq!(classifier.count_complexity_keywords("DESIGN and REFACTOR"), 2);
    }

    #[test]
    fn test_default_thresholds() {
        let classifier = ComplexityClassifier::default();

        // Verify that a task with score < 2 is Simple.
        // step_count=0, desc="", context="", keywords=0 => score=0.0 < 2 => Simple
        let task = make_task("", "", 0);
        assert_eq!(classifier.classify(&task), TaskComplexity::Simple);

        // Verify threshold boundaries explicitly via score()
        let features_at_simple = TaskFeatures {
            step_count: 1,
            description_length: 0,
            context_length: 0,
            keyword_count: 0,
        };
        // score = 1.0 < 2.0 => Simple
        assert_eq!(classifier.score(&features_at_simple), TaskComplexity::Simple);

        let features_at_moderate = TaskFeatures {
            step_count: 3,
            description_length: 0,
            context_length: 0,
            keyword_count: 0,
        };
        // score = 3.0 >= 2.0 and < 10.0 => Moderate
        assert_eq!(classifier.score(&features_at_moderate), TaskComplexity::Moderate);

        let features_at_complex = TaskFeatures {
            step_count: 12,
            description_length: 0,
            context_length: 0,
            keyword_count: 0,
        };
        // score = 12.0 >= 10.0 => Complex
        assert_eq!(classifier.score(&features_at_complex), TaskComplexity::Complex);
    }

    #[test]
    fn test_extract_features() {
        let classifier = ComplexityClassifier::default();
        let task = make_task("Design the algorithm", "some context here", 5);
        let features = classifier.extract_features(&task);

        assert_eq!(features.step_count, 5);
        assert_eq!(features.description_length, 20);
        assert_eq!(features.context_length, 17);
        assert_eq!(features.keyword_count, 2); // "design" + "algorithm"
    }

    #[test]
    fn test_keywords_counted_across_description_and_context() {
        let classifier = ComplexityClassifier::default();
        // "design" in description, "optimize" in context
        let task = make_task("Design the system", "optimize for performance", 1);
        let features = classifier.extract_features(&task);
        assert_eq!(features.keyword_count, 2);
    }

    #[test]
    fn test_custom_thresholds() {
        let classifier = ComplexityClassifier::new(5, 20);

        let features = TaskFeatures {
            step_count: 3,
            description_length: 0,
            context_length: 0,
            keyword_count: 0,
        };
        // score = 3.0 < 5.0 => Simple (would be Moderate with defaults)
        assert_eq!(classifier.score(&features), TaskComplexity::Simple);
    }

    #[test]
    fn test_scoring_formula() {
        let classifier = ComplexityClassifier::new(0, 100);

        let features = TaskFeatures {
            step_count: 5,
            description_length: 200,
            context_length: 500,
            keyword_count: 3,
        };
        // score = 5 + (200/100) + (500/500) + (3*2) = 5 + 2 + 1 + 6 = 14.0

        let result = classifier.score(&features);
        assert_eq!(result, TaskComplexity::Moderate);
    }
}
