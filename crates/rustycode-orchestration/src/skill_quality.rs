//! Skill quality scoring: multi-factor evaluation with dimension-based scoring.
//!
//! This module provides quality scoring for skill outputs using multiple
//! dimensions (completeness, clarity, safety, testability). It complements
//! the LLM-as-Judge evaluation in the `judge` module with rule-based,
//! dimension-specific quality assessment.

use serde::{Deserialize, Serialize};

/// Quality scoring dimensions for skill output evaluation.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum QualityDimension {
    Completeness,
    Clarity,
    Safety,
    Testability,
}

impl std::fmt::Display for QualityDimension {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Completeness => write!(f, "completeness"),
            Self::Clarity => write!(f, "clarity"),
            Self::Safety => write!(f, "safety"),
            Self::Testability => write!(f, "testability"),
        }
    }
}

/// Per-dimension quality score.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DimensionScore {
    pub dimension: QualityDimension,
    pub score: f64,
    pub notes: String,
}

/// Multi-factor quality report for a skill output.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillQualityReport {
    /// Per-dimension scores.
    pub dimension_scores: Vec<DimensionScore>,
    /// Weighted overall score (0.0-1.0).
    pub overall_score: f64,
    /// Whether the output passes the quality threshold.
    pub passed: bool,
    /// Summary of findings.
    pub summary: String,
}

impl SkillQualityReport {
    /// Create a report with an empty summary.
    #[allow(clippy::missing_const_for_fn)]
    pub fn new(dimension_scores: Vec<DimensionScore>, overall_score: f64, passed: bool) -> Self {
        Self {
            dimension_scores,
            overall_score,
            passed,
            summary: String::new(),
        }
    }
}

/// Threshold configuration for quality pass/fail.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QualityThreshold {
    /// Minimum overall score to pass.
    pub min_overall: f64,
    /// Per-dimension minimums (dimension -> minimum score).
    pub dimension_minimums: Vec<(QualityDimension, f64)>,
}

impl Default for QualityThreshold {
    fn default() -> Self {
        Self {
            min_overall: 0.6,
            dimension_minimums: vec![
                (QualityDimension::Completeness, 0.5),
                (QualityDimension::Clarity, 0.5),
                (QualityDimension::Safety, 0.7),
                (QualityDimension::Testability, 0.4),
            ],
        }
    }
}

impl QualityThreshold {
    /// Create a new threshold with a custom minimum overall score.
    pub fn new(min_overall: f64) -> Self {
        Self {
            min_overall,
            ..Self::default()
        }
    }

    /// Create a threshold with all dimension minimums set to the same value.
    pub fn uniform(min_overall: f64, dim_minimum: f64) -> Self {
        Self {
            min_overall,
            dimension_minimums: vec![
                (QualityDimension::Completeness, dim_minimum),
                (QualityDimension::Clarity, dim_minimum),
                (QualityDimension::Safety, dim_minimum),
                (QualityDimension::Testability, dim_minimum),
            ],
        }
    }

    /// Check whether a report passes this threshold.
    pub fn is_passing(&self, report: &SkillQualityReport) -> bool {
        if report.overall_score < self.min_overall {
            return false;
        }
        for (dim, min_score) in &self.dimension_minimums {
            if let Some(ds) = report
                .dimension_scores
                .iter()
                .find(|ds| ds.dimension == *dim)
            {
                if ds.score < *min_score {
                    return false;
                }
            }
        }
        true
    }
}

/// Multi-factor quality scorer for skill outputs.
pub struct SkillQualityScorer {
    /// Weights for each dimension (should sum to 1.0).
    pub weights: Vec<(QualityDimension, f64)>,
}

impl Default for SkillQualityScorer {
    fn default() -> Self {
        Self {
            weights: vec![
                (QualityDimension::Completeness, 0.35),
                (QualityDimension::Clarity, 0.25),
                (QualityDimension::Safety, 0.25),
                (QualityDimension::Testability, 0.15),
            ],
        }
    }
}

impl SkillQualityScorer {
    /// Create a new scorer with custom weights.
    #[allow(clippy::missing_const_for_fn)]
    pub fn new(weights: Vec<(QualityDimension, f64)>) -> Self {
        Self { weights }
    }

    /// Compute a quality report from dimension scores.
    pub fn score(&self, dimension_scores: Vec<DimensionScore>) -> SkillQualityReport {
        let mut overall = 0.0;
        for (dim, weight) in &self.weights {
            if let Some(ds) = dimension_scores.iter().find(|ds| ds.dimension == *dim) {
                overall += ds.score * weight;
            }
        }

        let threshold = QualityThreshold::default();
        let report =
            SkillQualityReport::new(dimension_scores, overall, threshold.min_overall <= overall);

        SkillQualityReport {
            passed: threshold.is_passing(&report),
            ..report
        }
    }

    /// Compute a quality report with a custom threshold.
    pub fn score_with_threshold(
        &self,
        dimension_scores: Vec<DimensionScore>,
        threshold: &QualityThreshold,
    ) -> SkillQualityReport {
        let mut overall = 0.0;
        for (dim, weight) in &self.weights {
            if let Some(ds) = dimension_scores.iter().find(|ds| ds.dimension == *dim) {
                overall += ds.score * weight;
            }
        }

        let report =
            SkillQualityReport::new(dimension_scores, overall, threshold.min_overall <= overall);

        SkillQualityReport {
            passed: threshold.is_passing(&report),
            ..report
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::float_cmp)]
mod tests {
    use super::*;

    #[test]
    fn quality_scorer_default_weights() {
        let scorer = SkillQualityScorer::default();
        let total_weight: f64 = scorer.weights.iter().map(|(_, w)| w).sum();
        assert!((total_weight - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn quality_scorer_computes_report() {
        let scorer = SkillQualityScorer::default();
        let dimension_scores = vec![
            DimensionScore {
                dimension: QualityDimension::Completeness,
                score: 0.9,
                notes: "Covers all requirements".to_string(),
            },
            DimensionScore {
                dimension: QualityDimension::Clarity,
                score: 0.8,
                notes: "Clear naming".to_string(),
            },
            DimensionScore {
                dimension: QualityDimension::Safety,
                score: 0.85,
                notes: "Proper error handling".to_string(),
            },
            DimensionScore {
                dimension: QualityDimension::Testability,
                score: 0.7,
                notes: "Good test coverage".to_string(),
            },
        ];
        let report = scorer.score(dimension_scores);
        assert!(report.passed);
        // 0.9*0.35 + 0.8*0.25 + 0.85*0.25 + 0.7*0.15 = 0.315 + 0.2 + 0.2125 + 0.105 = 0.8325
        assert!((report.overall_score - 0.8325).abs() < 0.001);
    }

    #[test]
    fn quality_threshold_default_passing() {
        let threshold = QualityThreshold::default();
        let scorer = SkillQualityScorer::default();
        let dimension_scores = vec![
            DimensionScore {
                dimension: QualityDimension::Completeness,
                score: 0.9,
                notes: String::new(),
            },
            DimensionScore {
                dimension: QualityDimension::Clarity,
                score: 0.8,
                notes: String::new(),
            },
            DimensionScore {
                dimension: QualityDimension::Safety,
                score: 0.85,
                notes: String::new(),
            },
            DimensionScore {
                dimension: QualityDimension::Testability,
                score: 0.7,
                notes: String::new(),
            },
        ];
        let report = scorer.score(dimension_scores);
        assert!(threshold.is_passing(&report));
    }

    #[test]
    fn quality_threshold_fails_on_low_safety() {
        let threshold = QualityThreshold::default();
        let scorer = SkillQualityScorer::default();
        let dimension_scores = vec![
            DimensionScore {
                dimension: QualityDimension::Completeness,
                score: 0.9,
                notes: String::new(),
            },
            DimensionScore {
                dimension: QualityDimension::Clarity,
                score: 0.8,
                notes: String::new(),
            },
            DimensionScore {
                dimension: QualityDimension::Safety,
                score: 0.3, // below 0.7 minimum
                notes: String::new(),
            },
            DimensionScore {
                dimension: QualityDimension::Testability,
                score: 0.7,
                notes: String::new(),
            },
        ];
        let report = scorer.score(dimension_scores);
        assert!(!threshold.is_passing(&report));
    }

    #[test]
    fn quality_threshold_custom_min_overall() {
        let threshold = QualityThreshold::new(0.9);
        assert!((threshold.min_overall - 0.9).abs() < f64::EPSILON);
    }

    #[test]
    fn quality_threshold_uniform() {
        let threshold = QualityThreshold::uniform(0.7, 0.6);
        assert_eq!(threshold.dimension_minimums.len(), 4);
        for (_, min) in &threshold.dimension_minimums {
            assert!((min - 0.6).abs() < f64::EPSILON);
        }
    }

    #[test]
    fn quality_report_new() {
        let report = SkillQualityReport::new(vec![], 0.75, true);
        assert!(report.passed);
        assert!((report.overall_score - 0.75).abs() < f64::EPSILON);
        assert!(report.summary.is_empty());
    }

    #[test]
    fn quality_dimension_display() {
        assert_eq!(QualityDimension::Completeness.to_string(), "completeness");
        assert_eq!(QualityDimension::Clarity.to_string(), "clarity");
        assert_eq!(QualityDimension::Safety.to_string(), "safety");
        assert_eq!(QualityDimension::Testability.to_string(), "testability");
    }

    #[test]
    fn dimension_score_serialization_roundtrip() {
        let score = DimensionScore {
            dimension: QualityDimension::Safety,
            score: 0.85,
            notes: "Good error handling".to_string(),
        };
        let json = serde_json::to_string(&score).unwrap();
        let back: DimensionScore = serde_json::from_str(&json).unwrap();
        assert_eq!(back.dimension, QualityDimension::Safety);
        assert!((back.score - 0.85).abs() < f64::EPSILON);
        assert_eq!(back.notes, "Good error handling");
    }

    #[test]
    fn quality_report_serialization_roundtrip() {
        let report = SkillQualityReport::new(
            vec![DimensionScore {
                dimension: QualityDimension::Clarity,
                score: 0.8,
                notes: "Clear".to_string(),
            }],
            0.8,
            true,
        );
        let json = serde_json::to_string(&report).unwrap();
        let back: SkillQualityReport = serde_json::from_str(&json).unwrap();
        assert!(back.passed);
        assert_eq!(back.dimension_scores.len(), 1);
    }

    #[test]
    fn scorer_with_threshold() {
        let scorer = SkillQualityScorer::default();
        let threshold = QualityThreshold::new(0.95);
        let dimension_scores = vec![
            DimensionScore {
                dimension: QualityDimension::Completeness,
                score: 0.9,
                notes: String::new(),
            },
            DimensionScore {
                dimension: QualityDimension::Clarity,
                score: 0.8,
                notes: String::new(),
            },
            DimensionScore {
                dimension: QualityDimension::Safety,
                score: 0.85,
                notes: String::new(),
            },
            DimensionScore {
                dimension: QualityDimension::Testability,
                score: 0.7,
                notes: String::new(),
            },
        ];
        let report = scorer.score_with_threshold(dimension_scores, &threshold);
        assert!(!report.passed); // 0.8325 < 0.95
    }

    #[test]
    fn quality_threshold_fails_on_low_overall() {
        let threshold = QualityThreshold::new(0.9);
        let report = SkillQualityReport::new(vec![], 0.5, false);
        assert!(!threshold.is_passing(&report));
    }

    #[test]
    fn quality_threshold_passes_when_all_dimensions_met() {
        let threshold = QualityThreshold::default();
        let scorer = SkillQualityScorer::default();
        let dimension_scores = vec![
            DimensionScore {
                dimension: QualityDimension::Completeness,
                score: 1.0,
                notes: String::new(),
            },
            DimensionScore {
                dimension: QualityDimension::Clarity,
                score: 1.0,
                notes: String::new(),
            },
            DimensionScore {
                dimension: QualityDimension::Safety,
                score: 1.0,
                notes: String::new(),
            },
            DimensionScore {
                dimension: QualityDimension::Testability,
                score: 1.0,
                notes: String::new(),
            },
        ];
        let report = scorer.score(dimension_scores);
        assert!(threshold.is_passing(&report));
        assert!((report.overall_score - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn scorer_custom_weights() {
        let scorer = SkillQualityScorer::new(vec![(QualityDimension::Safety, 1.0)]);
        let dimension_scores = vec![DimensionScore {
            dimension: QualityDimension::Safety,
            score: 0.5,
            notes: String::new(),
        }];
        let report = scorer.score(dimension_scores);
        assert!((report.overall_score - 0.5).abs() < f64::EPSILON);
    }

    #[test]
    fn scorer_missing_dimension_treated_as_zero() {
        let scorer = SkillQualityScorer::default();
        // Only provide 2 of 4 dimensions
        let dimension_scores = vec![
            DimensionScore {
                dimension: QualityDimension::Completeness,
                score: 1.0,
                notes: String::new(),
            },
            DimensionScore {
                dimension: QualityDimension::Safety,
                score: 1.0,
                notes: String::new(),
            },
        ];
        let report = scorer.score(dimension_scores);
        // 1.0*0.35 + 0*0.25 + 1.0*0.25 + 0*0.15 = 0.6
        assert!((report.overall_score - 0.6).abs() < 0.001);
    }
}
