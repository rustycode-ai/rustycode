//! Strategy selection for reasoning orchestration.
//!
//! Picks the appropriate reasoning strategy based on task complexity,
//! response quality, and confidence level. Does NOT require an LLM call.

use crate::types::{QualityScore, ReasoningStrategy};

/// Selects reasoning strategy based on task characteristics.
pub struct StrategySelector;

impl Default for StrategySelector {
    fn default() -> Self {
        Self::new()
    }
}

impl StrategySelector {
    pub const fn new() -> Self {
        Self
    }

    /// Select the appropriate reasoning strategy.
    ///
    /// Decision tree:
    /// - Simple (complexity `< 2.0`) + confidence >= 75 → `DirectExecution`
    /// - Moderate (complexity `< 3.0`) + good quality (`>= 4.0`) + good confidence (`>= 75`) → `QuickSelfEval`
    /// - Moderate-high (complexity `< 4.0`) → `SequentialThinking`
    /// - Complex (complexity `>= 4.0`) → `PhasedOrchestration`
    pub fn select(
        &self,
        complexity: f64,
        quality: &QualityScore,
        confidence: u32,
    ) -> ReasoningStrategy {
        if complexity < 2.0 && confidence >= 75 {
            ReasoningStrategy::DirectExecution
        } else if complexity < 3.0 && quality.total >= 4.0 && confidence >= 75 {
            ReasoningStrategy::QuickSelfEval
        } else if complexity < 4.0 {
            ReasoningStrategy::SequentialThinking
        } else {
            ReasoningStrategy::PhasedOrchestration
        }
    }

    /// Detect task complexity from text using keyword heuristics.
    ///
    /// Returns 0.0-5.0 where higher = more complex.
    /// For production use, prefer `rustycode_classification::UnifiedTaskClassifier`.
    pub fn detect_complexity(text: &str) -> f64 {
        let lower = text.to_lowercase();

        let signals: [(&str, f64); 22] = [
            ("explore", 4.5),
            ("investigate", 4.5),
            ("analyze", 4.0),
            ("interpreter", 4.0),
            ("compiler", 4.0),
            ("mips", 4.0),
            ("instruction", 3.8),
            ("architecture", 3.8),
            ("design", 3.5),
            ("architect", 3.5),
            ("refactor", 3.0),
            ("middleware", 3.0),
            ("implement", 2.5),
            ("build", 2.5),
            ("algorithm", 2.5),
            ("create", 2.0),
            ("add", 2.0),
            ("fix", 1.5),
            ("update", 1.5),
            ("change", 1.0),
            ("rename", 1.0),
            ("typo", 0.5),
        ];

        let max_signal = signals
            .iter()
            .filter(|(kw, _)| lower.contains(*kw))
            .map(|(_, score)| *score)
            .fold(0.0_f64, f64::max);

        let match_count = signals.iter().filter(|(kw, _)| lower.contains(*kw)).count();
        let multi_boost = if match_count >= 3 { 0.5 } else { 0.0 };

        let base = if max_signal > 0.0 { max_signal } else { 1.5 };

        (base + multi_boost).min(5.0)
    }
}

/// Returns a human-readable prompt hint for the given strategy.
///
/// Used by the pipeline to inject the active strategy into the system prompt
/// so the LLM is aware of the execution mode it should follow.
pub const fn strategy_hint(strategy: &ReasoningStrategy) -> &'static str {
    match strategy {
        ReasoningStrategy::DirectExecution =>
            "Strategy: DirectExecution — act immediately, no extended planning.",
        ReasoningStrategy::QuickSelfEval =>
            "Strategy: QuickSelfEval — one self-check before committing.",
        ReasoningStrategy::SequentialThinking =>
            "Strategy: SequentialThinking — decompose into ordered steps.",
        ReasoningStrategy::PhasedOrchestration =>
            "Strategy: PhasedOrchestration — follow AST phases strictly.",
    }
}

/// Convenience alias for use in TUI and external callers.
pub type Strategy = ReasoningStrategy;

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn high_quality() -> QualityScore {
        QualityScore {
            specificity: 4.0,
            depth: 4.0,
            completeness: 3.5,
            uncertainty: 1.5,
            total: 6.5,
        }
    }

    fn medium_quality() -> QualityScore {
        QualityScore {
            specificity: 2.5,
            depth: 2.0,
            completeness: 2.0,
            uncertainty: 0.5,
            total: 3.5,
        }
    }

    fn low_quality() -> QualityScore {
        QualityScore {
            specificity: 0.5,
            depth: 0.5,
            completeness: 0.5,
            uncertainty: 0.0,
            total: 1.0,
        }
    }

    #[test]
    fn test_direct_for_simple_high_quality() {
        let selector = StrategySelector;
        let strategy = selector.select(1.0, &high_quality(), 90);
        assert_eq!(strategy, ReasoningStrategy::DirectExecution);
    }

    #[test]
    fn test_quick_eval_for_moderate() {
        let selector = StrategySelector;
        let strategy = selector.select(2.5, &medium_quality(), 80);
        assert_eq!(strategy, ReasoningStrategy::SequentialThinking);
    }

    #[test]
    fn test_quick_eval_triggers() {
        let selector = StrategySelector;
        let good_quality = QualityScore {
            specificity: 3.0,
            depth: 3.0,
            completeness: 2.5,
            uncertainty: 1.0,
            total: 4.5,
        };
        let strategy = selector.select(2.5, &good_quality, 80);
        assert_eq!(strategy, ReasoningStrategy::QuickSelfEval);
    }

    #[test]
    fn test_sequential_for_moderate_complexity() {
        let selector = StrategySelector;
        let strategy = selector.select(3.5, &medium_quality(), 60);
        assert_eq!(strategy, ReasoningStrategy::SequentialThinking);
    }

    #[test]
    fn test_phased_for_complex() {
        let selector = StrategySelector;
        let strategy = selector.select(4.5, &low_quality(), 30);
        assert_eq!(strategy, ReasoningStrategy::PhasedOrchestration);
    }

    #[test]
    fn test_phased_even_with_high_quality() {
        let selector = StrategySelector;
        let strategy = selector.select(4.5, &high_quality(), 90);
        assert_eq!(strategy, ReasoningStrategy::PhasedOrchestration);
    }

    #[test]
    fn test_detect_explore_high_complexity() {
        assert!(StrategySelector::detect_complexity("explore the maze solver algorithms") > 4.0);
    }

    #[test]
    fn test_detect_fix_low_complexity() {
        assert!(StrategySelector::detect_complexity("fix this typo") < 2.0);
    }

    #[test]
    fn test_detect_implement_moderate() {
        let complexity = StrategySelector::detect_complexity("implement user authentication");
        assert!(
            (2.0..=3.5).contains(&complexity),
            "implement should be 2.0-3.5, got {complexity}"
        );
    }

    #[test]
    fn test_detect_multi_keyword_boost() {
        let single = StrategySelector::detect_complexity("fix the bug");
        let multi =
            StrategySelector::detect_complexity("explore and design and implement the new system");
        assert!(
            multi > single,
            "Multi-keyword should boost: {multi} vs {single}"
        );
    }

    #[test]
    fn test_detect_default_moderate() {
        let complexity = StrategySelector::detect_complexity("hello world");
        assert!(
            (1.0..=2.0).contains(&complexity),
            "Default should be moderate, got {complexity}"
        );
    }

    #[test]
    fn test_strategy_requires_structured_thinking() {
        assert!(!ReasoningStrategy::DirectExecution.requires_structured_thinking());
        assert!(!ReasoningStrategy::QuickSelfEval.requires_structured_thinking());
        assert!(ReasoningStrategy::SequentialThinking.requires_structured_thinking());
        assert!(ReasoningStrategy::PhasedOrchestration.requires_structured_thinking());
    }

    #[test]
    fn test_default_trait() {
        // Default trait is verified by `impl Default for StrategySelector` above
        let selector = StrategySelector::new();
        let low_quality = QualityScore {
            specificity: 0.0,
            depth: 0.0,
            completeness: 0.0,
            uncertainty: 0.0,
            total: 0.0,
        };
        let strategy = selector.select(4.5, &low_quality, 50);
        assert_eq!(strategy, ReasoningStrategy::PhasedOrchestration);
    }

    #[test]
    fn test_detect_complexity_analyze() {
        let complexity = StrategySelector::detect_complexity("analyze the performance bottleneck");
        assert!(
            complexity >= 4.0,
            "analyze should be >= 4.0, got {complexity}"
        );
    }

    #[test]
    fn test_detect_complexity_rename_low() {
        let complexity = StrategySelector::detect_complexity("rename the variable");
        assert!(
            complexity <= 1.5,
            "rename should be <= 1.5, got {complexity}"
        );
    }

    #[test]
    fn test_select_boundary_complexity_2() {
        let selector = StrategySelector::new();
        let medium = QualityScore {
            specificity: 3.0,
            depth: 3.0,
            completeness: 2.5,
            uncertainty: 1.0,
            total: 4.5,
        };
        let strategy = selector.select(2.0, &medium, 80);
        assert_eq!(strategy, ReasoningStrategy::QuickSelfEval);
    }

    #[test]
    fn test_select_confidence_below_threshold() {
        let selector = StrategySelector::new();
        let high_quality = QualityScore {
            specificity: 4.0,
            depth: 4.0,
            completeness: 3.5,
            uncertainty: 1.5,
            total: 6.5,
        };
        let strategy = selector.select(1.0, &high_quality, 74);
        assert_ne!(
            strategy,
            ReasoningStrategy::DirectExecution,
            "Confidence < 85 should not get DirectExecution"
        );
    }

    #[test]
    fn test_strategy_type_alias() {
        fn takes_strategy(_: Strategy) {}
        takes_strategy(ReasoningStrategy::DirectExecution);
    }

    #[test]
    fn test_boundary_complexity_2_exactly() {
        let selector = StrategySelector;
        // complexity < 2.0 → DirectExecution (if confidence >= 75)
        // complexity < 3.0 → QuickSelfEval (if quality >= 4.0 && confidence >= 75)
        let below = selector.select(1.999, &high_quality(), 90);
        assert_eq!(below, ReasoningStrategy::DirectExecution);
        let at = selector.select(2.0, &high_quality(), 90);
        assert_eq!(at, ReasoningStrategy::QuickSelfEval);
    }

    #[test]
    fn test_boundary_complexity_3_exactly() {
        let selector = StrategySelector;
        let below = selector.select(2.999, &medium_quality(), 80);
        assert_eq!(below, ReasoningStrategy::SequentialThinking);
        let at = selector.select(3.0, &medium_quality(), 80);
        assert_eq!(at, ReasoningStrategy::SequentialThinking);
    }

    #[test]
    fn test_boundary_complexity_4_exactly() {
        let selector = StrategySelector;
        let below = selector.select(3.999, &low_quality(), 50);
        assert_eq!(below, ReasoningStrategy::SequentialThinking);
        let at = selector.select(4.0, &low_quality(), 50);
        assert_eq!(at, ReasoningStrategy::PhasedOrchestration);
    }

    #[test]
    fn test_confidence_zero() {
        let selector = StrategySelector;
        let strategy = selector.select(1.0, &high_quality(), 0);
        assert_ne!(
            strategy,
            ReasoningStrategy::DirectExecution,
            "confidence=0 should not get DirectExecution"
        );
    }

    #[test]
    fn test_confidence_100_simple() {
        let selector = StrategySelector;
        let strategy = selector.select(1.0, &high_quality(), 100);
        assert_eq!(strategy, ReasoningStrategy::DirectExecution);
    }

    #[test]
    fn test_all_strategies_reachable() {
        let selector = StrategySelector;
        let high = high_quality();
        let medium = medium_quality();
        let low = low_quality();

        let strategies: std::collections::HashSet<_> = [
            selector.select(1.0, &high, 90),   // DirectExecution
            selector.select(2.5, &high, 90),   // QuickSelfEval
            selector.select(3.5, &medium, 60), // SequentialThinking
            selector.select(4.5, &low, 30),    // PhasedOrchestration
        ]
        .into_iter()
        .collect();

        assert_eq!(strategies.len(), 4, "all 4 strategies should be reachable");
    }

    #[test]
    fn test_strategy_hint_all_variants() {
        let variants = [
            ReasoningStrategy::DirectExecution,
            ReasoningStrategy::QuickSelfEval,
            ReasoningStrategy::SequentialThinking,
            ReasoningStrategy::PhasedOrchestration,
        ];

        let hints: Vec<&str> = variants.iter().map(strategy_hint).collect();

        // All hints are non-empty
        for hint in &hints {
            assert!(!hint.is_empty(), "hint should not be empty");
            assert!(
                hint.starts_with("Strategy:"),
                "hint should start with 'Strategy:': {hint}"
            );
        }

        // All hints are distinct
        let unique: std::collections::HashSet<&str> = hints.iter().copied().collect();
        assert_eq!(
            unique.len(),
            variants.len(),
            "each variant should produce a distinct hint"
        );
    }
}
