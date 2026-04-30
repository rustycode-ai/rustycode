#![allow(clippy::unwrap_used)]

use std::sync::Arc;

use regex::Regex;
use rustycode_protocol::agent_protocol::AgentRole;
use rustycode_protocol::task_routing::TaskWorkflow;
use std::sync::LazyLock;

use crate::types::{
    ClassificationReason, ClassificationResult, ComplexitySignals, ComplexityTier, PatternQuery,
    TaskClassification, TaskComplexity,
};

pub struct UnifiedTaskClassifier {
    mundane_keywords: Vec<Regex>,
    complex_keywords: Vec<Regex>,
    failure_store: Option<Arc<dyn PatternQuery>>,
}

pub type LocalTaskClassifier = UnifiedTaskClassifier;

static MUNDANE_KEYWORDS: LazyLock<Vec<Regex>> = LazyLock::new(|| {
    vec![
        Regex::new(r"(?i)\b(list|show|read|cat|count|grep|find|check|verify)\b").unwrap(),
        Regex::new(r"(?i)\b(typo|spelling|comment|readme)\b").unwrap(),
    ]
});

static COMPLEX_KEYWORDS: LazyLock<Vec<Regex>> = LazyLock::new(|| {
    vec![
        Regex::new(r"(?i)\b(refactor|debug|optimize|design|architect|architecture)\b").unwrap(),
        Regex::new(r"(?i)\b(implement|analyze|rewrite|migrate|migration|security|auth)\b").unwrap(),
        Regex::new(r"(?i)\b(performance|database|api|microservices|breaking change)\b").unwrap(),
    ]
});

impl UnifiedTaskClassifier {
    pub fn new() -> Self {
        Self {
            mundane_keywords: MUNDANE_KEYWORDS.clone(),
            complex_keywords: COMPLEX_KEYWORDS.clone(),
            failure_store: None,
        }
    }

    pub fn with_failure_store(mut self, store: Arc<dyn PatternQuery>) -> Self {
        self.failure_store = Some(store);
        self
    }

    pub fn classify(&self, task: &str) -> TaskClassification {
        self.classify_with_budget(task, None)
    }

    pub fn classify_with_budget(
        &self,
        task: &str,
        budget_pressure: Option<f64>,
    ) -> TaskClassification {
        let mut score = base_score(task);
        let mut keywords = Vec::new();

        for pattern in &self.mundane_keywords {
            if pattern.is_match(task) {
                score -= 10;
                keywords.push(pattern.as_str().to_string());
                break;
            }
        }

        for pattern in &self.complex_keywords {
            if pattern.is_match(task) {
                score += 25;
                keywords.push(pattern.as_str().to_string());
                break;
            }
        }

        let mut signals = extract_signals(task);
        score += signal_score(&signals);

        if let Some(ref store) = self.failure_store {
            if let Ok(patterns) = store.query_patterns("*") {
                let historical_weight: u32 =
                    patterns.iter().map(|p| p.occurrence_count.max(1)).sum();
                if patterns.len() > 5 || historical_weight > 12 {
                    score += 15;
                    signals.requires_context = true;
                    keywords.push("historical_pattern".into());
                }
            }
        }

        let mut score = score.clamp(5, 95) as u8;
        let mut tier = tier_from_score(score);
        let mut reasoning = reasoning_for(score, &signals, &keywords);

        if let Some(pressure) = budget_pressure {
            let adjusted = apply_budget_pressure(tier, pressure);
            if adjusted != tier {
                tier = adjusted;
                score = score_for_budget_tier(score, tier);
                reasoning = format!("{reasoning} Budget pressure lowered execution tier.");
            }
        }

        let workflow = infer_workflow(task, &signals);
        let agent_role = RoleRouter::select_for_score(&signals, workflow, score);

        TaskClassification {
            complexity_score: score,
            tier,
            signals,
            agent_role,
            reasoning,
        }
    }

    pub fn classify_legacy(&self, task: &str) -> ClassificationResult {
        let classification = self.classify(task);
        let complexity = if classification.complexity_score >= 51 {
            TaskComplexity::Complex
        } else {
            TaskComplexity::Mundane
        };
        let confidence =
            (0.5 + f64::from(classification.complexity_score.abs_diff(50)) / 100.0).clamp(0.0, 1.0);
        let mut reasons = Vec::new();
        if classification.reasoning.contains("Historical") {
            reasons.push(ClassificationReason::HistoricalPattern);
        }
        if reasons.is_empty() {
            reasons.push(ClassificationReason::Unknown);
        }

        ClassificationResult {
            complexity,
            confidence,
            reasons,
        }
    }
}

impl Default for UnifiedTaskClassifier {
    fn default() -> Self {
        Self::new()
    }
}

pub struct RoleRouter;

impl RoleRouter {
    pub fn select(signals: &ComplexitySignals, workflow: &TaskWorkflow) -> AgentRole {
        Self::select_for_score(signals, *workflow, score_from_signals(signals))
    }

    pub const fn select_for_score(
        signals: &ComplexitySignals,
        workflow: TaskWorkflow,
        score: u8,
    ) -> AgentRole {
        if signals.strategic || score >= 81 {
            return AgentRole::Architect;
        }

        if signals.debugging || signals.requires_context {
            return AgentRole::Researcher;
        }

        if signals.risky && score < 51 {
            return AgentRole::Scalpel;
        }

        match workflow {
            TaskWorkflow::Test => AgentRole::Reviewer,
            TaskWorkflow::Plan => AgentRole::Planner,
            _ if signals.ambiguous => AgentRole::Skeptic,
            TaskWorkflow::Code if signals.multi_file => AgentRole::Builder,
            _ => AgentRole::Worker,
        }
    }
}

const fn base_score(task: &str) -> i32 {
    match task.len() {
        0..=99 => 25,
        100..=499 => 40,
        _ => 65,
    }
}

fn extract_signals(task: &str) -> ComplexitySignals {
    let lower = task.to_lowercase();
    let mut signals = ComplexitySignals::default();

    signals.debugging = contains_any(&lower, &["debug", "bug", "failing", "why is", "root cause"]);
    signals.strategic = contains_any(
        &lower,
        &[
            "architect",
            "architecture",
            "strategy",
            "plan for",
            "design",
        ],
    );
    signals.ambiguous = contains_any(
        &lower,
        &["ambiguous", "unclear", "make this better", "somehow"],
    );
    signals.risky = contains_any(
        &lower,
        &[
            "security",
            "auth",
            "breaking change",
            "migration",
            "database",
        ],
    );
    signals.multi_file = contains_any(
        &lower,
        &["multiple", "across", "module", "codebase", "system"],
    );
    signals.requires_context = signals.debugging
        || signals.strategic
        || signals.multi_file
        || contains_any(&lower, &["context", "existing", "current"]);
    signals.estimated_steps = if signals.strategic {
        8
    } else if signals.multi_file || signals.debugging {
        5
    } else if contains_any(&lower, &["implement", "refactor", "add"]) {
        3
    } else {
        1
    };

    signals
}

fn contains_any(input: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| input.contains(needle))
}

const fn signal_score(signals: &ComplexitySignals) -> i32 {
    let mut score = 0;
    if signals.requires_context {
        score += 10;
    }
    if signals.ambiguous {
        score += 15;
    }
    if signals.multi_file {
        score += 15;
    }
    if signals.debugging {
        score += 15;
    }
    if signals.strategic {
        score += 25;
    }
    if signals.risky {
        score += 15;
    }
    score
}

fn score_from_signals(signals: &ComplexitySignals) -> u8 {
    (25 + signal_score(signals)).clamp(5, 95) as u8
}

const fn tier_from_score(score: u8) -> ComplexityTier {
    match score {
        0..=50 => ComplexityTier::Light,
        51..=80 => ComplexityTier::Standard,
        _ => ComplexityTier::Heavy,
    }
}

fn apply_budget_pressure(tier: ComplexityTier, budget_pct: f64) -> ComplexityTier {
    if budget_pct < 0.5 {
        return tier;
    }

    match (budget_pct, tier) {
        (p, ComplexityTier::Heavy) if p >= 0.9 => ComplexityTier::Standard,
        (p, ComplexityTier::Standard) if p >= 0.5 => ComplexityTier::Light,
        _ => tier,
    }
}

fn score_for_budget_tier(score: u8, tier: ComplexityTier) -> u8 {
    match tier {
        ComplexityTier::Light => score.min(50),
        ComplexityTier::Standard => score.clamp(51, 80),
        ComplexityTier::Heavy => score.max(81),
    }
}

fn infer_workflow(task: &str, signals: &ComplexitySignals) -> TaskWorkflow {
    let lower = task.to_lowercase();
    if contains_any(&lower, &["test", "coverage", "spec"]) {
        TaskWorkflow::Test
    } else if contains_any(&lower, &["plan", "roadmap"]) || signals.strategic {
        TaskWorkflow::Plan
    } else if signals.debugging {
        TaskWorkflow::Debug
    } else if contains_any(&lower, &["research", "investigate", "analyze"]) {
        TaskWorkflow::Research
    } else {
        TaskWorkflow::Code
    }
}

fn reasoning_for(score: u8, signals: &ComplexitySignals, keywords: &[String]) -> String {
    let tier = tier_from_score(score);
    let mut parts = vec![format!("{tier:?} task scored {score}/100.")];
    if !keywords.is_empty() {
        parts.push(format!("Matched {} routing signal(s).", keywords.len()));
    }
    if signals.requires_context {
        parts.push("Requires broader context.".into());
    }
    if signals.multi_file {
        parts.push("Likely touches multiple files.".into());
    }
    if signals.risky {
        parts.push("Risk-sensitive change.".into());
    }
    parts.join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::StoredPattern;

    #[test]
    fn test_classify_short_task() {
        let classifier = UnifiedTaskClassifier::new();
        let result = classifier.classify("fix typo");
        assert!(result.complexity_score < 50);
        assert_eq!(result.tier, ComplexityTier::Light);
    }

    #[test]
    fn test_classify_long_task() {
        let classifier = UnifiedTaskClassifier::new();
        let task = "Implement a complete microservices architecture with API gateway, service mesh, and distributed tracing across the entire system";
        let result = classifier.classify(task);
        assert!(result.complexity_score >= 51);
    }

    #[test]
    fn test_classify_mundane_keywords() {
        let classifier = UnifiedTaskClassifier::new();
        let result = classifier.classify("list all files in the directory");
        assert!(result.complexity_score < 40);
    }

    #[test]
    fn test_classify_complex_keywords() {
        let classifier = UnifiedTaskClassifier::new();
        let result = classifier.classify("refactor the authentication module");
        assert!(result.complexity_score >= 51);
    }

    #[test]
    fn test_classify_debugging_task() {
        let classifier = UnifiedTaskClassifier::new();
        let result = classifier.classify("debug the failing test in the auth module");
        assert!(result.signals.debugging);
        assert!(result.signals.requires_context);
    }

    #[test]
    fn test_classify_strategic_task() {
        let classifier = UnifiedTaskClassifier::new();
        let result = classifier.classify("design the architecture for the new payment system");
        assert!(result.signals.strategic);
        assert!(result.complexity_score >= 60);
    }

    #[test]
    fn test_classify_ambiguous_task() {
        let classifier = UnifiedTaskClassifier::new();
        let result = classifier.classify("make this somehow better");
        assert!(result.signals.ambiguous);
    }

    #[test]
    fn test_classify_risky_task() {
        let classifier = UnifiedTaskClassifier::new();
        let result = classifier.classify("migrate the database schema for security compliance");
        assert!(result.signals.risky);
    }

    #[test]
    fn test_classify_multi_file_task() {
        let classifier = UnifiedTaskClassifier::new();
        let result = classifier.classify("update multiple modules across the codebase");
        assert!(result.signals.multi_file);
    }

    #[test]
    fn test_classify_test_workflow() {
        let classifier = UnifiedTaskClassifier::new();
        let result = classifier.classify("run test coverage for the util module");
        // Test workflow routes to Reviewer unless strategic/debugging overrides
        assert!(matches!(
            result.agent_role,
            AgentRole::Reviewer | AgentRole::Researcher
        ));
    }

    #[test]
    fn test_classify_plan_workflow() {
        let classifier = UnifiedTaskClassifier::new();
        let result = classifier.classify("create a plan for the migration roadmap");
        // Should route to Planner or Architect
        assert!(matches!(
            result.agent_role,
            AgentRole::Planner | AgentRole::Architect
        ));
    }

    #[test]
    fn test_classify_debug_workflow() {
        let classifier = UnifiedTaskClassifier::new();
        let result = classifier.classify("debug why the service is failing");
        assert!(result.signals.debugging);
        assert!(matches!(result.agent_role, AgentRole::Researcher));
    }

    #[test]
    fn test_classify_legacy_mundane() {
        let classifier = UnifiedTaskClassifier::new();
        let result = classifier.classify_legacy("show the readme");
        assert_eq!(result.complexity, TaskComplexity::Mundane);
    }

    #[test]
    fn test_classify_legacy_complex() {
        let classifier = UnifiedTaskClassifier::new();
        let result = classifier.classify_legacy(
            "refactor the entire architecture of the system for better performance",
        );
        assert_eq!(result.complexity, TaskComplexity::Complex);
    }

    #[test]
    fn test_role_router_strategic() {
        let signals = ComplexitySignals {
            strategic: true,
            ..Default::default()
        };
        let role = RoleRouter::select_for_score(&signals, TaskWorkflow::Code, 85);
        assert_eq!(role, AgentRole::Architect);
    }

    #[test]
    fn test_role_router_debugging() {
        let signals = ComplexitySignals {
            debugging: true,
            ..Default::default()
        };
        let role = RoleRouter::select_for_score(&signals, TaskWorkflow::Code, 40);
        assert_eq!(role, AgentRole::Researcher);
    }

    #[test]
    fn test_role_router_risky_low_score() {
        let signals = ComplexitySignals {
            risky: true,
            ..Default::default()
        };
        let role = RoleRouter::select_for_score(&signals, TaskWorkflow::Code, 40);
        assert_eq!(role, AgentRole::Scalpel);
    }

    #[test]
    fn test_role_router_builder_multi_file() {
        let signals = ComplexitySignals {
            multi_file: true,
            ..Default::default()
        };
        let role = RoleRouter::select_for_score(&signals, TaskWorkflow::Code, 55);
        assert_eq!(role, AgentRole::Builder);
    }

    #[test]
    fn test_role_router_test_workflow() {
        let signals = ComplexitySignals::default();
        let role = RoleRouter::select_for_score(&signals, TaskWorkflow::Test, 30);
        assert_eq!(role, AgentRole::Reviewer);
    }

    #[test]
    fn test_role_router_plan_workflow() {
        let signals = ComplexitySignals::default();
        let role = RoleRouter::select_for_score(&signals, TaskWorkflow::Plan, 40);
        assert_eq!(role, AgentRole::Planner);
    }

    #[test]
    fn test_role_router_worker_default() {
        let signals = ComplexitySignals::default();
        let role = RoleRouter::select_for_score(&signals, TaskWorkflow::Code, 40);
        assert_eq!(role, AgentRole::Worker);
    }

    #[test]
    fn test_base_score_ranges() {
        assert_eq!(base_score(""), 25);
        let s50: String = "a".repeat(50);
        assert_eq!(base_score(&s50), 25);
        let s100: String = "a".repeat(100);
        assert_eq!(base_score(&s100), 40);
        let s300: String = "a".repeat(300);
        assert_eq!(base_score(&s300), 40);
        let s500: String = "a".repeat(500);
        assert_eq!(base_score(&s500), 65);
    }

    #[test]
    fn test_extract_signals_default() {
        let signals = extract_signals("hello world");
        assert_eq!(signals.estimated_steps, 1);
        assert!(!signals.debugging);
        assert!(!signals.strategic);
    }

    #[test]
    fn test_extract_signals_debugging() {
        let signals = extract_signals("debug the failing test");
        assert!(signals.debugging);
    }

    #[test]
    fn test_extract_signals_strategic() {
        let signals = extract_signals("architect the new system design");
        assert!(signals.strategic);
        assert_eq!(signals.estimated_steps, 8);
    }

    #[test]
    fn test_signal_score_calculation() {
        let signals = ComplexitySignals {
            requires_context: true,
            ambiguous: true,
            multi_file: true,
            debugging: true,
            strategic: true,
            risky: true,
            estimated_steps: 10,
        };
        let score = signal_score(&signals);
        assert_eq!(score, 95); // 10 + 15 + 15 + 15 + 25 + 15
    }

    #[test]
    fn test_tier_from_score() {
        assert_eq!(tier_from_score(10), ComplexityTier::Light);
        assert_eq!(tier_from_score(50), ComplexityTier::Light);
        assert_eq!(tier_from_score(51), ComplexityTier::Standard);
        assert_eq!(tier_from_score(80), ComplexityTier::Standard);
        assert_eq!(tier_from_score(81), ComplexityTier::Heavy);
        assert_eq!(tier_from_score(95), ComplexityTier::Heavy);
    }

    #[test]
    fn test_infer_workflow() {
        let signals = ComplexitySignals::default();
        assert_eq!(
            infer_workflow("write tests for coverage", &signals),
            TaskWorkflow::Test
        );
        assert_eq!(
            infer_workflow("create a plan for migration", &signals),
            TaskWorkflow::Plan
        );
        assert_eq!(
            infer_workflow("research the best approach", &signals),
            TaskWorkflow::Research
        );
        assert_eq!(
            infer_workflow("implement the feature", &signals),
            TaskWorkflow::Code
        );
    }

    #[test]
    fn test_infer_workflow_debug_from_signals() {
        let signals = ComplexitySignals {
            debugging: true,
            ..Default::default()
        };
        assert_eq!(
            infer_workflow("check this issue", &signals),
            TaskWorkflow::Debug
        );
    }

    #[test]
    fn test_apply_budget_pressure() {
        assert_eq!(
            apply_budget_pressure(ComplexityTier::Heavy, 0.95),
            ComplexityTier::Standard
        );
        assert_eq!(
            apply_budget_pressure(ComplexityTier::Standard, 0.6),
            ComplexityTier::Light
        );
        assert_eq!(
            apply_budget_pressure(ComplexityTier::Light, 0.9),
            ComplexityTier::Light
        );
        assert_eq!(
            apply_budget_pressure(ComplexityTier::Heavy, 0.3),
            ComplexityTier::Heavy
        );
    }

    #[test]
    fn test_score_for_budget_tier() {
        assert_eq!(score_for_budget_tier(70, ComplexityTier::Light), 50);
        assert_eq!(score_for_budget_tier(30, ComplexityTier::Standard), 51);
        assert_eq!(score_for_budget_tier(60, ComplexityTier::Heavy), 81);
    }

    #[test]
    fn test_classify_with_budget_pressure() {
        let classifier = UnifiedTaskClassifier::new();
        let result = classifier.classify_with_budget(
            "refactor the entire architecture of the system for better performance",
            Some(0.95),
        );
        // Budget pressure should lower tier
        assert!(matches!(
            result.tier,
            ComplexityTier::Light | ComplexityTier::Standard
        ));
    }

    #[test]
    fn test_reasoning_for() {
        let reasoning = reasoning_for(
            75,
            &ComplexitySignals {
                multi_file: true,
                ..Default::default()
            },
            &["refactor".into()],
        );
        assert!(reasoning.contains("Standard"));
        assert!(reasoning.contains("75/100"));
        assert!(reasoning.contains("multiple files"));
    }

    #[test]
    fn test_default_classifier() {
        let classifier = UnifiedTaskClassifier::default();
        let result = classifier.classify("implement feature");
        assert!(result.complexity_score > 0);
    }

    #[test]
    fn test_classifier_with_failure_store() {
        struct MockStore;
        impl PatternQuery for MockStore {
            fn query_patterns(&self, _task_type: &str) -> anyhow::Result<Vec<StoredPattern>> {
                Ok(vec![
                    StoredPattern {
                        task_type: "refactor".into(),
                        occurrence_count: 10,
                    },
                    StoredPattern {
                        task_type: "bug".into(),
                        occurrence_count: 5,
                    },
                    StoredPattern {
                        task_type: "test".into(),
                        occurrence_count: 3,
                    },
                    StoredPattern {
                        task_type: "feature".into(),
                        occurrence_count: 8,
                    },
                    StoredPattern {
                        task_type: "docs".into(),
                        occurrence_count: 2,
                    },
                    StoredPattern {
                        task_type: "perf".into(),
                        occurrence_count: 4,
                    },
                ])
            }
        }
        let classifier = UnifiedTaskClassifier::new().with_failure_store(Arc::new(MockStore));
        let result = classifier.classify("implement a new feature");
        // 6 patterns with high occurrence should boost score
        assert!(result.signals.requires_context);
    }
}
