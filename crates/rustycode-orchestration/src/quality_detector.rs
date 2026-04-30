//! Quality Detector
//!
//! Evaluates the quality of LLM responses using heuristic scoring.

use crate::types::QualityScore;

pub struct QualityDetector;

impl Default for QualityDetector {
    fn default() -> Self {
        Self::new()
    }
}

impl QualityDetector {
    pub const fn new() -> Self {
        Self
    }

    /// Evaluates response quality, returning sub-scores and total (0.0 - 7.0).
    pub fn evaluate(&self, response: &str) -> QualityScore {
        let specificity = Self::score_specificity(response);
        let depth = Self::score_depth(response);
        let completeness = Self::score_completeness(response);
        let uncertainty = Self::score_uncertainty(response);
        let total = (specificity + depth + completeness + uncertainty).min(7.0);

        QualityScore {
            specificity,
            depth,
            completeness,
            uncertainty,
            total,
        }
    }

    fn score_specificity(response: &str) -> f64 {
        let mut score = 0.0_f64;
        if response.contains("algorithm") || response.contains("rationale") {
            score += 1.5;
        }
        if response.contains("struct ")
            || response.contains("fn ")
            || response.contains("function ")
        {
            score += 1.5;
        }
        if response.contains("impl ")
            || response.contains("interface ")
            || response.contains("class ")
        {
            score += 1.0;
        }
        score.min(5.0)
    }

    fn score_depth(response: &str) -> f64 {
        let mut score = 0.0_f64;
        let lower = response.to_ascii_lowercase();
        if lower.contains("because") || lower.contains("therefore") || lower.contains("since ") {
            score += 1.5;
        }
        if lower.contains("for example") || lower.contains("such as") {
            score += 1.0;
        }
        if response.len() > 500 {
            score += 1.0;
        }
        if response.len() > 1500 {
            score += 1.0;
        }
        score.min(5.0)
    }

    fn score_completeness(response: &str) -> f64 {
        let mut score = 0.0_f64;
        if response.contains("edge case") || response.contains("validation") {
            score += 1.5;
        }
        if response.contains("error") || response.contains("handle") {
            score += 1.0;
        }
        if response.contains("test") || response.contains("assert") {
            score += 1.0;
        }
        score.min(5.0)
    }

    fn score_uncertainty(response: &str) -> f64 {
        let mut score = 0.0_f64;
        if response.contains("assumption") {
            score += 0.5;
        }
        if response.contains("limitation") || response.contains("caveat") {
            score += 0.5;
        }
        if response.contains("trade-off") || response.contains("tradeoff") {
            score += 0.5;
        }
        score.min(2.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_quality_detector_scores_good_response() {
        let detector = QualityDetector::new();
        let response = "I will use a BFS algorithm to ensure correctness. This covers the edge case of cycles. My assumption is a connected graph.";
        let score = detector.evaluate(response);
        assert!(
            score.total >= 3.0,
            "expected total >= 3.0, got {}",
            score.total
        );
    }

    #[test]
    fn test_empty_string_scores_zero() {
        let detector = QualityDetector::new();
        let score = detector.evaluate("");
        assert!(
            score.total < 1.0,
            "empty string should score near zero, got {}",
            score.total
        );
    }

    #[test]
    fn test_score_never_exceeds_seven() {
        let detector = QualityDetector::new();
        let response = "algorithm struct fn impl interface class \
            because therefore since for example such as \
            edge case validation error handle test assert \
            assumption limitation caveat trade-off tradeoff";
        let score = detector.evaluate(response);
        assert!(
            score.total <= 7.0,
            "total should be capped at 7.0, got {}",
            score.total
        );
    }

    #[test]
    fn test_code_only_response() {
        let detector = QualityDetector::new();
        let code = "fn main() { let x = 42; struct Foo { bar: i32 } impl Foo { fn new() -> Self { Self { bar: 0 } } } }";
        let score = detector.evaluate(code);
        // Code has struct/fn/impl keywords but no reasoning depth
        assert!(
            score.specificity > 0.0,
            "code should have some specificity, got {}",
            score.specificity
        );
    }

    #[test]
    fn test_non_english_response() {
        let detector = QualityDetector::new();
        let response = "これはテストです。アルゴリズムを実装します。エラー処理を追加します。";
        let score = detector.evaluate(response);
        // Non-English should score low on keyword matching
        assert!(
            score.total < 3.0,
            "non-English should score low, got {}",
            score.total
        );
    }

    #[test]
    fn test_very_long_response_gets_depth_bonus() {
        let detector = QualityDetector::new();
        let short = "Use a BFS algorithm.";
        let long = "Use a BFS algorithm. ".repeat(50);
        let short_score = detector.evaluate(short);
        let long_score = detector.evaluate(&long);
        assert!(
            long_score.depth > short_score.depth,
            "long response should have higher depth: {} vs {}",
            long_score.depth,
            short_score.depth
        );
    }

    #[test]
    fn test_each_subscore_capped() {
        let detector = QualityDetector::new();
        let response = "algorithm rationale struct fn impl interface class \
            because therefore since for example such as edge case validation";
        let score = detector.evaluate(response);
        assert!(
            score.specificity <= 5.0,
            "specificity capped at 5.0, got {}",
            score.specificity
        );
        assert!(
            score.depth <= 5.0,
            "depth capped at 5.0, got {}",
            score.depth
        );
        assert!(
            score.completeness <= 5.0,
            "completeness capped at 5.0, got {}",
            score.completeness
        );
        assert!(
            score.uncertainty <= 2.0,
            "uncertainty capped at 2.0, got {}",
            score.uncertainty
        );
    }

    #[test]
    fn test_detailed_outscores_minimal() {
        let detector = QualityDetector::new();
        let detailed = "The MIPS interpreter uses a fetch-decode-execute cycle. \
          R-type format: opcode(6) rs(5) rt(5) rd(5) shamt(5) funct(6). \
          The ADD instruction performs rs + rt and stores in rd. \
          Edge cases: signed overflow triggers an exception when OF=1. \
          This approach was chosen because it separates concerns cleanly.";
        let minimal = "Use a switch statement for opcodes.";
        let d = detector.evaluate(detailed);
        let m = detector.evaluate(minimal);
        assert!(
            d.total > m.total,
            "detailed ({}) should outscore minimal ({})",
            d.total,
            m.total
        );
    }
}
