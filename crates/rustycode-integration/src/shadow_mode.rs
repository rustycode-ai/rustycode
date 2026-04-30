//! Shadow mode types for integration metrics.
//!
//! Shadow mode runs both orchestration and legacy paths in parallel to compare
//! performance. The actual shadow execution logic lives in `rustycode-orchestration`.

use serde::{Deserialize, Serialize};

/// Recommendation from comparing shadow execution results.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ShadowRecommendation {
    /// Orchestration performed better.
    PreferOrchestration,
    /// Legacy performed better.
    PreferLegacy,
    /// Both paths performed equivalently.
    BothViable,
    /// Both paths had poor results.
    BothPoor,
    /// Comparison not yet available.
    Pending,
}

/// Result of a shadow mode comparison.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShadowComparison {
    /// The recommendation based on comparison.
    pub recommendation: ShadowRecommendation,
    /// Orchestration execution time in seconds.
    pub orchestration_time_secs: Option<f64>,
    /// Legacy execution time in seconds.
    pub legacy_time_secs: Option<f64>,
    /// Orchestration success flag.
    pub orchestration_success: bool,
    /// Legacy success flag.
    pub legacy_success: bool,
}

/// Full result of a shadow execution run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShadowExecutionResult {
    /// The comparison result.
    pub comparison: ShadowComparison,
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn shadow_recommendation_equality() {
        assert_eq!(
            ShadowRecommendation::PreferOrchestration,
            ShadowRecommendation::PreferOrchestration
        );
        assert_ne!(
            ShadowRecommendation::PreferOrchestration,
            ShadowRecommendation::PreferLegacy
        );
    }

    #[test]
    fn shadow_recommendation_serde_roundtrip() {
        for rec in [
            ShadowRecommendation::PreferOrchestration,
            ShadowRecommendation::PreferLegacy,
            ShadowRecommendation::BothViable,
            ShadowRecommendation::BothPoor,
            ShadowRecommendation::Pending,
        ] {
            let json = serde_json::to_string(&rec).unwrap();
            let back: ShadowRecommendation = serde_json::from_str(&json).unwrap();
            assert_eq!(back, rec);
        }
    }

    #[test]
    fn shadow_comparison_serde() {
        let comp = ShadowComparison {
            recommendation: ShadowRecommendation::PreferOrchestration,
            orchestration_time_secs: Some(5.0),
            legacy_time_secs: Some(10.0),
            orchestration_success: true,
            legacy_success: true,
        };
        let json = serde_json::to_string(&comp).unwrap();
        let back: ShadowComparison = serde_json::from_str(&json).unwrap();
        assert_eq!(
            back.recommendation,
            ShadowRecommendation::PreferOrchestration
        );
        assert_eq!(back.orchestration_time_secs, Some(5.0));
        assert!(back.orchestration_success);
    }

    #[test]
    fn shadow_comparison_minimal() {
        let comp = ShadowComparison {
            recommendation: ShadowRecommendation::Pending,
            orchestration_time_secs: None,
            legacy_time_secs: None,
            orchestration_success: false,
            legacy_success: false,
        };
        let json = serde_json::to_string(&comp).unwrap();
        let back: ShadowComparison = serde_json::from_str(&json).unwrap();
        assert_eq!(back.recommendation, ShadowRecommendation::Pending);
        assert_eq!(back.orchestration_time_secs, None);
    }

    #[test]
    fn shadow_execution_result_serde() {
        let result = ShadowExecutionResult {
            comparison: ShadowComparison {
                recommendation: ShadowRecommendation::BothPoor,
                orchestration_time_secs: None,
                legacy_time_secs: None,
                orchestration_success: false,
                legacy_success: false,
            },
        };
        let json = serde_json::to_string(&result).unwrap();
        let back: ShadowExecutionResult = serde_json::from_str(&json).unwrap();
        assert_eq!(
            back.comparison.recommendation,
            ShadowRecommendation::BothPoor
        );
    }

    // ── Additional shadow_mode tests ─────────────

    #[test]
    fn shadow_recommendation_all_variants_distinct() {
        let variants = [
            ShadowRecommendation::PreferOrchestration,
            ShadowRecommendation::PreferLegacy,
            ShadowRecommendation::BothViable,
            ShadowRecommendation::BothPoor,
            ShadowRecommendation::Pending,
        ];
        // Check all pairwise distinct
        for (i, a) in variants.iter().enumerate() {
            for (j, b) in variants.iter().enumerate() {
                assert_eq!(i == j, a == b);
            }
        }
    }

    #[test]
    fn shadow_recommendation_debug_format() {
        assert!(format!("{:?}", ShadowRecommendation::PreferOrchestration)
            .contains("PreferOrchestration"));
        assert!(format!("{:?}", ShadowRecommendation::PreferLegacy).contains("PreferLegacy"));
        assert!(format!("{:?}", ShadowRecommendation::BothViable).contains("BothViable"));
        assert!(format!("{:?}", ShadowRecommendation::BothPoor).contains("BothPoor"));
        assert!(format!("{:?}", ShadowRecommendation::Pending).contains("Pending"));
    }

    #[test]
    #[allow(clippy::float_cmp)]
    fn shadow_comparison_with_times() {
        let comp = ShadowComparison {
            recommendation: ShadowRecommendation::PreferOrchestration,
            orchestration_time_secs: Some(3.5),
            legacy_time_secs: Some(7.0),
            orchestration_success: true,
            legacy_success: true,
        };
        let json = serde_json::to_string(&comp).unwrap();
        let back: ShadowComparison = serde_json::from_str(&json).unwrap();
        assert!((back.orchestration_time_secs.unwrap() - 3.5).abs() < f64::EPSILON);
        assert!((back.legacy_time_secs.unwrap() - 7.0).abs() < f64::EPSILON);
        assert!(back.orchestration_success);
        assert!(back.legacy_success);
    }

    #[test]
    fn shadow_comparison_mixed_success() {
        let comp = ShadowComparison {
            recommendation: ShadowRecommendation::PreferOrchestration,
            orchestration_time_secs: Some(5.0),
            legacy_time_secs: None,
            orchestration_success: true,
            legacy_success: false,
        };
        let json = serde_json::to_string(&comp).unwrap();
        let back: ShadowComparison = serde_json::from_str(&json).unwrap();
        assert!(back.orchestration_success);
        assert!(!back.legacy_success);
        assert!(back.legacy_time_secs.is_none());
    }

    #[test]
    fn shadow_comparison_pending_serde() {
        let comp = ShadowComparison {
            recommendation: ShadowRecommendation::Pending,
            orchestration_time_secs: None,
            legacy_time_secs: None,
            orchestration_success: false,
            legacy_success: false,
        };
        let json = serde_json::to_string(&comp).unwrap();
        let back: ShadowComparison = serde_json::from_str(&json).unwrap();
        assert_eq!(back.recommendation, ShadowRecommendation::Pending);
    }

    #[test]
    fn shadow_execution_result_debug() {
        let result = ShadowExecutionResult {
            comparison: ShadowComparison {
                recommendation: ShadowRecommendation::BothViable,
                orchestration_time_secs: Some(1.0),
                legacy_time_secs: Some(1.0),
                orchestration_success: true,
                legacy_success: true,
            },
        };
        let debug = format!("{result:?}");
        assert!(debug.contains("ShadowExecutionResult"));
        assert!(debug.contains("BothViable"));
    }

    #[test]
    fn shadow_comparison_clone() {
        let comp = ShadowComparison {
            recommendation: ShadowRecommendation::PreferLegacy,
            orchestration_time_secs: Some(2.0),
            legacy_time_secs: Some(1.0),
            orchestration_success: true,
            legacy_success: true,
        };
        let cloned = comp.clone();
        assert_eq!(comp.recommendation, cloned.recommendation);
        assert_eq!(comp.orchestration_time_secs, cloned.orchestration_time_secs);
    }

    #[test]
    fn shadow_execution_result_clone() {
        let result = ShadowExecutionResult {
            comparison: ShadowComparison {
                recommendation: ShadowRecommendation::BothPoor,
                orchestration_time_secs: None,
                legacy_time_secs: None,
                orchestration_success: false,
                legacy_success: false,
            },
        };
        let cloned = result;
        assert_eq!(
            cloned.comparison.recommendation,
            ShadowRecommendation::BothPoor
        );
    }
}
