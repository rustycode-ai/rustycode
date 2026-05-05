//! BEDD (Brainstorm-Evaluate-Drop-Expand) funnel for AST SKELETON phase.
//!
//! BEDD is a proposal selection funnel that runs inside the SKELETON phase for
//! complex tasks. It selects the best approach before committing to a detailed
//! plan by generating multiple proposals, scoring them, pruning weak ones, and
//! expanding the winner.
//!
//! Pipeline: BRAINSTORM -> EVALUATE -> DROP -> EXPAND
//!             (generate)   (score)    (prune)  (detail)
//!
//! Complexity gating:
//! - TRIVIAL: skip BEDD entirely
//! - MODERATE: optional (only if uncertainty flag)
//! - COMPLEX: mandatory

use std::fmt::Write;

use serde::{Deserialize, Serialize};

use super::types::{ComplexityLevel, ContextBrief, TaskAssessment};

// Types

/// A proposed approach generated during the Brainstorm phase.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Proposal {
    pub id: String,
    pub approach: String,
    pub tradeoffs: Vec<String>,
    pub estimated_effort: u8,
    pub feasibility: Option<u8>,
    pub risk: Option<u8>,
    pub alignment: Option<u8>,
    pub score: Option<f64>,
}

/// Scored result of the Evaluate phase.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvaluatedProposal {
    pub proposal: Proposal,
    pub feasibility: u8,
    /// 0-10 (inverted during scoring: lower risk is better).
    pub risk: u8,
    /// 0-10 alignment with task requirements.
    pub alignment: u8,
    /// Relative effort level.
    pub effort: u8,
    /// Qualitative upside description.
    pub optimistic_upside: String,
    /// Qualitative risk description.
    pub critical_risks: String,
    /// Computed weighted score.
    pub score: f64,
}

// Multi-Round Dialogue (Gap 1)

/// A critique of a single proposal from one perspective.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProposalCritique {
    pub proposal_id: String,
    pub strengths: Vec<String>,
    pub weaknesses: Vec<String>,
    pub suggested_improvements: Vec<String>,
}

/// A single round of critique across all proposals.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CritiqueRound {
    pub round_number: u32,
    pub critiques: Vec<ProposalCritique>,
    /// 0.0-1.0, how much agreement across critiques.
    pub consensus_score: f64,
}

// Voting and Consensus (Gap 2)

/// A single voter's scores for each proposal.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Vote {
    pub voter_id: String,
    /// (`proposal_id`, score) pairs.
    pub rankings: Vec<(String, f64)>,
}

/// Voting method used to determine the winner.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum VotingMethod {
    SimpleMajority,
    WeightedScore,
}

/// The result of voting across multiple voters.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VotingResult {
    pub votes: Vec<Vote>,
    pub winner_id: String,
    /// (`proposal_id`, `weighted_score`) pairs sorted descending.
    pub weighted_scores: Vec<(String, f64)>,
    pub method: VotingMethod,
}

// Diminishing Returns Detection (Gap 5)

/// Reason for stopping iteration.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum StopReason {
    /// Iteration should continue.
    ContinueIterating,
    /// Scores have plateaued within the threshold.
    ScorePlateau { best: f64, last: f64 },
    /// Maximum iteration count reached.
    MaxIterationsReached,
    /// Token budget exhausted.
    TokenBudgetExhausted { used: u32, budget: u32 },
}

/// Detects when further iteration yields diminishing returns.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiminishingReturnsDetector {
    pub score_history: Vec<f64>,
    /// Default 0.1 -- stop if last 3 scores are within this range.
    pub plateau_threshold: f64,
    /// Default 5 -- hard cap on iterations.
    pub max_iterations: u32,
    /// Optional token budget.
    pub token_budget: Option<u32>,
}

impl DiminishingReturnsDetector {
    /// Create a detector with sensible defaults.
    pub const fn new() -> Self {
        Self {
            score_history: Vec::new(),
            plateau_threshold: 0.1,
            max_iterations: 5,
            token_budget: None,
        }
    }

    /// Record a new score from the current iteration.
    pub fn record_score(&mut self, score: f64) {
        self.score_history.push(score);
    }

    /// Whether iteration should continue given current state.
    pub fn should_continue(&self, current_iteration: u32, tokens_used: u32) -> bool {
        matches!(
            self.diagnose(current_iteration, tokens_used),
            StopReason::ContinueIterating
        )
    }

    /// Diagnose why iteration should stop (or continue).
    pub fn diagnose(&self, current_iteration: u32, tokens_used: u32) -> StopReason {
        // Check max iterations first.
        if current_iteration >= self.max_iterations {
            return StopReason::MaxIterationsReached;
        }

        // Check token budget.
        if let Some(budget) = self.token_budget {
            if tokens_used >= budget {
                return StopReason::TokenBudgetExhausted {
                    used: tokens_used,
                    budget,
                };
            }
        }

        // Check plateau: need at least 3 scores to detect.
        if self.score_history.len() >= 3 {
            let len = self.score_history.len();
            let last_three = &self.score_history[len - 3..];
            let max = last_three.iter().copied().fold(f64::NEG_INFINITY, f64::max);
            let min = last_three.iter().copied().fold(f64::INFINITY, f64::min);
            if (max - min) <= self.plateau_threshold {
                return StopReason::ScorePlateau {
                    best: self
                        .score_history
                        .iter()
                        .copied()
                        .fold(f64::NEG_INFINITY, f64::max),
                    last: self.score_history[len - 1],
                };
            }
        }

        StopReason::ContinueIterating
    }
}

impl Default for DiminishingReturnsDetector {
    fn default() -> Self {
        Self::new()
    }
}

/// The BEDD funnel result containing the full decision trail.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BeddResult {
    pub proposals: Vec<Proposal>,
    pub evaluations: Vec<EvaluatedProposal>,
    pub selected: Proposal,
    pub dropped: Vec<Proposal>,
    pub decision_reason: String,
}

impl BeddResult {
    /// Render the BEDD result as markdown for the SKELETON section.
    pub fn render_markdown(&self) -> String {
        let mut md = String::with_capacity(512);

        // Proposals section
        let _ = writeln!(md, "### Proposals");
        for (i, p) in self.proposals.iter().enumerate() {
            let _ = writeln!(md, "{}. id: {}", i + 1, p.id);
            let _ = writeln!(md, "   approach: {}", p.approach);
            let _ = writeln!(md, "   tradeoffs: {}", p.tradeoffs.join(", "));
        }

        md.push('\n');

        // Evaluation section
        let _ = writeln!(md, "### Evaluation");
        for ev in &self.evaluations {
            let _ = writeln!(
                md,
                "- {}: feasibility={} risk={} alignment={} effort={} -> score={:.1}",
                ev.proposal.id, ev.feasibility, ev.risk, ev.alignment, ev.effort, ev.score,
            );
        }

        md.push('\n');

        // Decision section
        let _ = writeln!(md, "### Decision");
        let _ = writeln!(md, "- selected: {}", self.selected.id);
        let dropped_ids: Vec<&str> = self.dropped.iter().map(|p| p.id.as_str()).collect();
        let _ = writeln!(md, "- dropped: {}", dropped_ids.join(", "));
        let _ = writeln!(md, "- reason: {}", self.decision_reason);

        md
    }
}

// Configuration

/// Configuration for the BEDD funnel.
#[derive(Debug, Clone)]
pub struct BeddConfig {
    /// Minimum number of proposals to generate (default: 2).
    pub min_proposals: usize,
    /// Maximum number of proposals to generate (default: 5).
    pub max_proposals: usize,
    /// Keep top K proposals after the Drop phase (default: 1).
    pub keep_top_k: usize,
    /// Score weight for feasibility dimension (default: 0.3).
    pub feasibility_weight: f64,
    /// Score weight for risk dimension (default: 0.3, inverted in scoring).
    pub risk_weight: f64,
    /// Score weight for alignment dimension (default: 0.25).
    pub alignment_weight: f64,
    /// Score weight for effort dimension (default: 0.15, inverted in scoring).
    pub effort_weight: f64,
    /// Maximum critique rounds for multi-round dialogue (default: 1, set > 1 to enable).
    pub max_critique_rounds: u32,
}

impl Default for BeddConfig {
    fn default() -> Self {
        Self {
            min_proposals: 2,
            max_proposals: 5,
            keep_top_k: 1,
            feasibility_weight: 0.3,
            risk_weight: 0.3,
            alignment_weight: 0.25,
            effort_weight: 0.15,
            max_critique_rounds: 1,
        }
    }
}

impl BeddConfig {
    /// Validate config invariants.
    pub fn validate(&self) -> Result<(), String> {
        if self.min_proposals == 0 {
            return Err("min_proposals must be >= 1".into());
        }
        if self.min_proposals > self.max_proposals {
            return Err("min_proposals must be <= max_proposals".into());
        }
        if self.keep_top_k == 0 {
            return Err("keep_top_k must be >= 1".into());
        }
        let weight_sum =
            self.feasibility_weight + self.risk_weight + self.alignment_weight + self.effort_weight;
        if (weight_sum - 1.0).abs() > 0.01 {
            return Err(format!(
                "score weights must sum to 1.0, got {weight_sum:.2}"
            ));
        }
        Ok(())
    }

    /// Returns true when BEDD should run for the given complexity level.
    pub const fn should_run(&self, complexity: ComplexityLevel) -> bool {
        matches!(complexity, ComplexityLevel::Complex)
    }
}

// BEDD Funnel

/// The BEDD funnel: Brainstorm -> Evaluate -> Drop -> Expand.
///
/// Generates multiple approach proposals, scores them, prunes the weakest,
/// and selects the best candidate for expansion into a skeleton.
pub struct BeddFunnel {
    config: BeddConfig,
}

impl BeddFunnel {
    pub fn new() -> Self {
        Self {
            config: BeddConfig::default(),
        }
    }

    /// Create a new funnel with custom configuration.
    pub const fn with_config(config: BeddConfig) -> Self {
        Self { config }
    }

    /// Run the full BRAINSTORM -> EVALUATE -> DROP -> EXPAND pipeline.
    ///
    /// Returns a `BeddResult` containing the full decision trail.
    pub fn run(&self, assessment: &TaskAssessment, brief: &ContextBrief) -> BeddResult {
        let proposals = self.brainstorm(assessment, brief);
        let evaluations = self.evaluate(&proposals, assessment);
        self.drop(&evaluations)
    }

    // -- Phase 1: Brainstorm ------------------------------------------------

    /// Generate 2-5 proposals for the task approach.
    ///
    /// Uses rule-based keyword matching against the task summary to produce a
    /// set of candidate approaches, each with rough tradeoffs and an effort
    /// estimate.
    pub fn brainstorm(&self, assessment: &TaskAssessment, brief: &ContextBrief) -> Vec<Proposal> {
        let candidates = Self::match_templates(&assessment.task_summary, brief);
        let count = candidates
            .len()
            .clamp(self.config.min_proposals, self.config.max_proposals);
        candidates
            .into_iter()
            .take(count)
            .enumerate()
            .map(|(i, tmpl)| Proposal {
                id: format!("P{}", i + 1),
                approach: tmpl.approach.into(),
                tradeoffs: tmpl.tradeoffs.into_iter().map(String::from).collect(),
                estimated_effort: tmpl.effort,
                feasibility: Some(tmpl.feasibility),
                risk: Some(tmpl.risk),
                alignment: Some(tmpl.alignment),
                score: None,
            })
            .collect()
    }

    // -- Phase 2: Evaluate --------------------------------------------------

    /// Score each proposal across four dimensions and attach a two-sided review
    /// (optimistic upside + critical risks).
    pub fn evaluate(
        &self,
        proposals: &[Proposal],
        assessment: &TaskAssessment,
    ) -> Vec<EvaluatedProposal> {
        let summary_lower = assessment.task_summary.to_lowercase();
        proposals
            .iter()
            .map(|p| self.score_proposal(p, &summary_lower))
            .collect()
    }

    // -- Phase 3: Drop ------------------------------------------------------

    /// Prune bottom 50% of proposals and keep top K.
    ///
    /// Returns a `BeddResult` with the selected winner and dropped proposals.
    pub fn drop(&self, evaluations: &[EvaluatedProposal]) -> BeddResult {
        let mut sorted: Vec<EvaluatedProposal> = evaluations.to_vec();
        sorted.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        let keep_count = self.config.keep_top_k.min(sorted.len());
        let kept: Vec<EvaluatedProposal> = sorted.iter().take(keep_count).cloned().collect();
        let dropped_proposals: Vec<Proposal> = sorted
            .iter()
            .skip(keep_count)
            .map(|ev| ev.proposal.clone())
            .collect();

        let selected = match kept.first() {
            Some(ev) => ev.proposal.clone(),
            None => {
                // SAFETY: drop() is only called with evaluations derived from
                // brainstorm which always produces >= min_proposals >= 1.
                // If we reach this branch, the funnel is misconfigured.
                return BeddResult {
                    proposals: evaluations.iter().map(|ev| ev.proposal.clone()).collect(),
                    evaluations: kept,
                    selected: Proposal {
                        id: "P0".into(),
                        approach: "no proposals".into(),
                        tradeoffs: vec![],
                        estimated_effort: 0,
                        feasibility: None,
                        risk: None,
                        alignment: None,
                        score: None,
                    },
                    dropped: dropped_proposals,
                    decision_reason: "No proposals were evaluated.".into(),
                };
            }
        };

        let decision_reason = Self::compose_decision_reason(&kept, &dropped_proposals);

        BeddResult {
            proposals: evaluations.iter().map(|ev| ev.proposal.clone()).collect(),
            evaluations: kept,
            selected,
            dropped: dropped_proposals,
            decision_reason,
        }
    }

    // -- Multi-Round Critique (Gap 1) ----------------------------------------

    /// Run a single critique round over evaluated proposals.
    ///
    /// Generates a critique for each proposal based on its evaluation dimensions,
    /// then computes a consensus score reflecting agreement among the evaluations.
    pub fn run_critique_round(&self, proposals: &[EvaluatedProposal], round: u32) -> CritiqueRound {
        let critiques: Vec<ProposalCritique> = proposals
            .iter()
            .map(|ev| {
                let strengths = Self::strengths_for(ev);
                let weaknesses = Self::weaknesses_for(ev);
                let suggested_improvements = Self::improvements_for(ev);
                ProposalCritique {
                    proposal_id: ev.proposal.id.clone(),
                    strengths,
                    weaknesses,
                    suggested_improvements,
                }
            })
            .collect();

        // Consensus: 1.0 - (stddev / mean), clamped to [0.0, 1.0].
        // When all scores are identical, consensus = 1.0.
        let scores: Vec<f64> = proposals.iter().map(|ev| ev.score).collect();
        let consensus_score = Self::compute_consensus(&scores);

        CritiqueRound {
            round_number: round,
            critiques,
            consensus_score,
        }
    }

    /// Compute consensus from a list of scores.
    ///
    /// Returns `1.0 - (stddev / mean)`, clamped to `[0.0, 1.0]`.
    /// Returns 1.0 for empty or single-element lists.
    fn compute_consensus(scores: &[f64]) -> f64 {
        if scores.len() <= 1 {
            return 1.0;
        }
        let n = f64::from(scores.len() as u32);
        let mean = scores.iter().sum::<f64>() / n;
        if mean.abs() < f64::EPSILON {
            return 1.0;
        }
        let variance = scores.iter().map(|s| (s - mean).powi(2)).sum::<f64>() / n;
        let stddev = variance.sqrt();
        let consensus = 1.0 - (stddev / mean);
        consensus.clamp(0.0, 1.0)
    }

    /// Generate strengths for a proposal based on its evaluation.
    fn strengths_for(ev: &EvaluatedProposal) -> Vec<String> {
        let mut strengths = Vec::new();
        if ev.feasibility >= 7 {
            strengths.push(format!("High feasibility ({}/10)", ev.feasibility));
        }
        if ev.risk <= 3 {
            strengths.push(format!("Low risk ({}/10)", ev.risk));
        }
        if ev.alignment >= 7 {
            strengths.push(format!("Strong alignment ({}/10)", ev.alignment));
        }
        if ev.effort <= 4 {
            strengths.push(format!("Low effort ({}/10)", ev.effort));
        }
        if strengths.is_empty() {
            strengths.push("Acceptable overall profile".into());
        }
        strengths
    }

    /// Generate weaknesses for a proposal based on its evaluation.
    fn weaknesses_for(ev: &EvaluatedProposal) -> Vec<String> {
        let mut weaknesses = Vec::new();
        if ev.feasibility <= 4 {
            weaknesses.push(format!("Low feasibility ({}/10)", ev.feasibility));
        }
        if ev.risk >= 7 {
            weaknesses.push(format!("High risk ({}/10)", ev.risk));
        }
        if ev.alignment <= 4 {
            weaknesses.push(format!("Weak alignment ({}/10)", ev.alignment));
        }
        if ev.effort >= 7 {
            weaknesses.push(format!("High effort ({}/10)", ev.effort));
        }
        if weaknesses.is_empty() {
            weaknesses.push("No significant weaknesses".into());
        }
        weaknesses
    }

    /// Generate improvement suggestions for a proposal.
    fn improvements_for(ev: &EvaluatedProposal) -> Vec<String> {
        let mut suggestions = Vec::new();
        if ev.feasibility <= 6 {
            suggestions.push("Consider simplifying the approach to improve feasibility".into());
        }
        if ev.risk >= 5 {
            suggestions.push("Add mitigation strategies to reduce risk".into());
        }
        if ev.alignment <= 6 {
            suggestions.push("Refine scope to better match task requirements".into());
        }
        if ev.effort >= 6 {
            suggestions
                .push("Look for opportunities to reduce scope or reuse existing code".into());
        }
        if suggestions.is_empty() {
            suggestions.push("Proposal is well-balanced; minor refinements only".into());
        }
        suggestions
    }

    // -- Voting and Consensus (Gap 2) ----------------------------------------

    /// Collect votes from multiple voter perspectives on proposals.
    ///
    /// Each voter scores proposals from their perspective:
    /// - "scout": risk-focused (high weight on low risk)
    /// - "architect": feasibility-focused (high weight on feasibility)
    /// - "builder": effort-focused (high weight on low effort)
    ///
    /// Returns a `VotingResult` with the weighted winner.
    pub fn vote_on_proposals(
        &self,
        proposals: &[EvaluatedProposal],
        voters: &[&str],
    ) -> VotingResult {
        let cast_votes: Vec<Vote> = voters
            .iter()
            .map(|voter_id| {
                let rankings: Vec<(String, f64)> = proposals
                    .iter()
                    .map(|ev| {
                        let score = Self::perspective_score(voter_id, ev);
                        (ev.proposal.id.clone(), score)
                    })
                    .collect();
                Vote {
                    voter_id: voter_id.to_string(),
                    rankings,
                }
            })
            .collect();

        // Compute weighted scores: sum of all voter scores for each proposal.
        let mut score_map: std::collections::HashMap<String, f64> =
            std::collections::HashMap::new();
        for vote in &cast_votes {
            for (pid, score) in &vote.rankings {
                *score_map.entry(pid.clone()).or_default() += score;
            }
        }

        let mut weighted_scores: Vec<(String, f64)> = score_map.into_iter().collect();
        weighted_scores.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        let winner_id = weighted_scores
            .first()
            .map(|(id, _)| id.clone())
            .unwrap_or_default();

        VotingResult {
            votes: cast_votes,
            winner_id,
            weighted_scores,
            method: VotingMethod::WeightedScore,
        }
    }

    /// Score a proposal from a given voter perspective.
    fn perspective_score(voter_id: &str, ev: &EvaluatedProposal) -> f64 {
        let f = f64::from(ev.feasibility) / 10.0;
        let r = f64::from(10_u8.saturating_sub(ev.risk)) / 10.0;
        let a = f64::from(ev.alignment) / 10.0;
        let e = f64::from(10_u8.saturating_sub(ev.effort)) / 10.0;

        match voter_id {
            "scout" => r * 0.5 + f * 0.2 + a * 0.2 + e * 0.1,
            "architect" => f * 0.5 + a * 0.25 + r * 0.15 + e * 0.1,
            "builder" => e * 0.4 + f * 0.3 + r * 0.2 + a * 0.1,
            _ => (f + r + a + e) / 4.0,
        }
    }

    // -- Internal helpers ----------------------------------------------------

    /// Match keyword templates from the task summary to generate proposals.
    ///
    /// This function is intentionally long because it encodes a fixed table
    /// of keyword-to-proposal mappings. Each branch is a self-contained template.
    #[allow(clippy::too_many_lines)]
    fn match_templates(summary: &str, _brief: &ContextBrief) -> Vec<ProposalTemplate> {
        let lower = summary.to_lowercase();

        // Check keyword families in priority order; first match wins and
        // determines the proposal set.
        if lower.contains("implement") || lower.contains("create") || lower.contains("build") {
            return vec![
                ProposalTemplate {
                    approach: "greenfield",
                    tradeoffs: vec![
                        "clean architecture",
                        "no legacy constraints",
                        "higher initial effort",
                    ],
                    effort: 7,
                    feasibility: 7,
                    risk: 5,
                    alignment: 8,
                },
                ProposalTemplate {
                    approach: "fork existing",
                    tradeoffs: vec![
                        "proven foundation",
                        "must understand base code",
                        "divergence risk",
                    ],
                    effort: 5,
                    feasibility: 6,
                    risk: 6,
                    alignment: 7,
                },
                ProposalTemplate {
                    approach: "wrap library",
                    tradeoffs: vec![
                        "fastest to deliver",
                        "dependency on external code",
                        "limited customization",
                    ],
                    effort: 3,
                    feasibility: 8,
                    risk: 4,
                    alignment: 6,
                },
            ];
        }

        if lower.contains("refactor")
            || lower.contains("restructure")
            || lower.contains("reorganize")
        {
            return vec![
                ProposalTemplate {
                    approach: "incremental",
                    tradeoffs: vec![
                        "lower risk",
                        "gradual improvement",
                        "slower overall progress",
                    ],
                    effort: 4,
                    feasibility: 8,
                    risk: 3,
                    alignment: 8,
                },
                ProposalTemplate {
                    approach: "big bang",
                    tradeoffs: vec![
                        "immediate clean result",
                        "high risk of breakage",
                        "hard to roll back",
                    ],
                    effort: 6,
                    feasibility: 5,
                    risk: 8,
                    alignment: 7,
                },
                ProposalTemplate {
                    approach: "strangler fig",
                    tradeoffs: vec![
                        "safe incremental migration",
                        "maintains running system",
                        "temporary duplication",
                    ],
                    effort: 5,
                    feasibility: 7,
                    risk: 4,
                    alignment: 9,
                },
            ];
        }

        if lower.contains("fix") || lower.contains("bug") || lower.contains("issue") {
            return vec![
                ProposalTemplate {
                    approach: "targeted fix",
                    tradeoffs: vec![
                        "minimal change surface",
                        "fast to implement",
                        "may miss root cause",
                    ],
                    effort: 2,
                    feasibility: 9,
                    risk: 2,
                    alignment: 7,
                },
                ProposalTemplate {
                    approach: "root cause analysis",
                    tradeoffs: vec![
                        "addresses underlying issue",
                        "may expand scope",
                        "more thorough",
                    ],
                    effort: 5,
                    feasibility: 7,
                    risk: 4,
                    alignment: 9,
                },
                ProposalTemplate {
                    approach: "workaround",
                    tradeoffs: vec![
                        "immediate mitigation",
                        "does not fix root cause",
                        "technical debt",
                    ],
                    effort: 1,
                    feasibility: 8,
                    risk: 6,
                    alignment: 4,
                },
            ];
        }

        if lower.contains("test") || lower.contains("coverage") {
            return vec![
                ProposalTemplate {
                    approach: "unit tests first",
                    tradeoffs: vec![
                        "fast feedback loop",
                        "isolated verification",
                        "may miss integration gaps",
                    ],
                    effort: 3,
                    feasibility: 9,
                    risk: 2,
                    alignment: 8,
                },
                ProposalTemplate {
                    approach: "integration tests first",
                    tradeoffs: vec![
                        "tests real behavior",
                        "slower to run",
                        "covers cross-cutting concerns",
                    ],
                    effort: 5,
                    feasibility: 7,
                    risk: 4,
                    alignment: 7,
                },
                ProposalTemplate {
                    approach: "property-based",
                    tradeoffs: vec![
                        "finds edge cases automatically",
                        "higher setup cost",
                        "strong correctness guarantees",
                    ],
                    effort: 6,
                    feasibility: 6,
                    risk: 5,
                    alignment: 6,
                },
            ];
        }

        // Generic fallback
        vec![
            ProposalTemplate {
                approach: "straightforward approach",
                tradeoffs: vec![
                    "direct implementation",
                    "simple to understand",
                    "may not be optimal",
                ],
                effort: 4,
                feasibility: 7,
                risk: 4,
                alignment: 7,
            },
            ProposalTemplate {
                approach: "conservative approach",
                tradeoffs: vec![
                    "minimal risk",
                    "proven patterns",
                    "may leave value on table",
                ],
                effort: 3,
                feasibility: 8,
                risk: 2,
                alignment: 6,
            },
            ProposalTemplate {
                approach: "iterative approach",
                tradeoffs: vec![
                    "incremental delivery",
                    "adapts to feedback",
                    "requires more planning cycles",
                ],
                effort: 5,
                feasibility: 8,
                risk: 3,
                alignment: 8,
            },
        ]
    }

    /// Score a single proposal using the weighted formula.
    fn score_proposal(&self, proposal: &Proposal, summary_lower: &str) -> EvaluatedProposal {
        let feasibility = Self::estimate_feasibility(proposal, summary_lower);
        let risk = Self::estimate_risk(proposal, summary_lower);
        let alignment = Self::estimate_alignment(proposal, summary_lower);
        let effort = proposal.estimated_effort;

        let score = Self::compute_score(
            feasibility,
            risk,
            alignment,
            effort,
            self.config.feasibility_weight,
            self.config.risk_weight,
            self.config.alignment_weight,
            self.config.effort_weight,
        );

        let optimistic_upside = Self::optimistic_upside_for(proposal);
        let critical_risks = Self::critical_risks_for(proposal);

        EvaluatedProposal {
            proposal: proposal.clone(),
            feasibility,
            risk,
            alignment,
            effort,
            optimistic_upside,
            critical_risks,
            score,
        }
    }

    /// Core scoring formula:
    /// ```text
    /// score = (feasibility * w_f + (10 - risk) * w_r + alignment * w_a + (10 - effort) * w_e) / 10.0
    /// ```
    #[allow(clippy::cast_precision_loss)]
    fn compute_score(
        feasibility: u8,
        risk: u8,
        alignment: u8,
        effort: u8,
        w_f: f64,
        w_r: f64,
        w_a: f64,
        w_e: f64,
    ) -> f64 {
        let f = f64::from(feasibility);
        let r = f64::from(10_u8.saturating_sub(risk));
        let a = f64::from(alignment);
        let e = f64::from(10_u8.saturating_sub(effort));

        // Clippy wants mul_add here, but the weighted sum is clearer as-is.
        #[allow(clippy::suboptimal_flops)]
        let weighted_sum = f * w_f + r * w_r + a * w_a + e * w_e;
        weighted_sum / 10.0
    }

    /// Adjust feasibility estimate from the proposal template baseline.
    fn estimate_feasibility(proposal: &Proposal, _summary_lower: &str) -> u8 {
        // Use the template value if set; otherwise default to 5.
        proposal.feasibility.unwrap_or(5).min(10)
    }

    /// Adjust risk estimate from the proposal template baseline.
    fn estimate_risk(proposal: &Proposal, _summary_lower: &str) -> u8 {
        proposal.risk.unwrap_or(5).min(10)
    }

    /// Adjust alignment estimate from the proposal template baseline.
    fn estimate_alignment(proposal: &Proposal, _summary_lower: &str) -> u8 {
        proposal.alignment.unwrap_or(5).min(10)
    }

    /// Generate an optimistic upside description for a proposal.
    fn optimistic_upside_for(proposal: &Proposal) -> String {
        match proposal.approach.as_str() {
            "greenfield" => {
                "Clean architecture with no legacy debt; easy to test and extend.".into()
            }
            "fork existing" => {
                "Proven codebase foundation; faster delivery than building from scratch.".into()
            }
            "wrap library" => "Minimal effort; leverage well-tested external solution.".into(),
            "incremental" => {
                "Low disruption; system stays functional throughout the change.".into()
            }
            "big bang" => {
                "Immediate clean result; no temporary duplication or migration overhead.".into()
            }
            "strangler fig" => "Safe migration path with rollback capability at every step.".into(),
            "targeted fix" => "Minimal change surface; fast to implement and verify.".into(),
            "root cause analysis" => {
                "Addresses underlying issue permanently; prevents recurrence.".into()
            }
            "workaround" => "Immediate mitigation; unblocks downstream work quickly.".into(),
            "unit tests first" => "Fast feedback loop; catches regressions early.".into(),
            "integration tests first" => {
                "Validates real system behavior; catches cross-module issues.".into()
            }
            "property-based" => {
                "Automatically discovers edge cases; strong correctness guarantees.".into()
            }
            "straightforward approach" => "Direct path to the goal; easy to reason about.".into(),
            "conservative approach" => "Proven patterns; minimal risk of unexpected issues.".into(),
            "iterative approach" => "Adapts to feedback; delivers value incrementally.".into(),
            _ => "Provides a structured path to the task goal.".into(),
        }
    }

    /// Generate a critical risks description for a proposal.
    fn critical_risks_for(proposal: &Proposal) -> String {
        match proposal.approach.as_str() {
            "greenfield" => "Higher initial effort; may duplicate existing functionality.".into(),
            "fork existing" => {
                "Must understand base code thoroughly; divergence risk over time.".into()
            }
            "wrap library" => {
                "Dependency on external code; limited customization; license risk.".into()
            }
            "incremental" => {
                "Slower overall progress; temporary code duplication during transition.".into()
            }
            "big bang" => {
                "High risk of breakage; hard to roll back; requires extensive testing.".into()
            }
            "strangler fig" => "Temporary duplication; requires careful routing logic.".into(),
            "targeted fix" => {
                "May miss root cause; fix may not generalize; recurrence risk.".into()
            }
            "root cause analysis" => {
                "May expand scope significantly; takes longer to deliver.".into()
            }
            "workaround" => {
                "Does not fix root cause; adds technical debt; may mask other issues.".into()
            }
            "unit tests first" => {
                "May miss integration gaps; does not test real interactions.".into()
            }
            "integration tests first" => "Slower test suite; harder to isolate failures.".into(),
            "property-based" => {
                "Higher setup cost; may produce flaky tests if properties are weak.".into()
            }
            "straightforward approach" => "May not handle edge cases; might need iteration.".into(),
            "conservative approach" => "May leave performance or elegance on the table.".into(),
            "iterative approach" => "Requires more planning cycles; coordination overhead.".into(),
            _ => "Unknown risk profile; requires manual review.".into(),
        }
    }

    /// Compose a human-readable decision reason from the kept and dropped sets.
    fn compose_decision_reason(kept: &[EvaluatedProposal], dropped: &[Proposal]) -> String {
        if kept.is_empty() {
            return "No proposals evaluated.".into();
        }

        let winner = &kept[0];

        if dropped.is_empty() {
            return format!(
                "Selected {} (score {:.1}) as the only proposal.",
                winner.proposal.id, winner.score,
            );
        }

        // Build a comparative reason.
        let reason = if winner.feasibility >= 7 && winner.risk <= 4 {
            "higher feasibility and lower risk"
        } else if winner.alignment >= 8 {
            "strongest alignment with task requirements"
        } else if winner.effort <= 4 {
            "lower effort with acceptable risk"
        } else {
            "best overall weighted score"
        };

        let dropped_ids: Vec<&str> = dropped.iter().map(|p| p.id.as_str()).collect();
        format!(
            "Selected {} (score {:.1}) over {} due to {} despite {}",
            winner.proposal.id,
            winner.score,
            dropped_ids.join(", "),
            reason,
            if winner.effort > 4 {
                "higher effort"
            } else {
                "lower effort"
            },
        )
    }
}

impl Default for BeddFunnel {
    fn default() -> Self {
        Self::new()
    }
}

// Internal template type

/// A proposal template used internally during brainstorming.
struct ProposalTemplate {
    approach: &'static str,
    tradeoffs: Vec<&'static str>,
    effort: u8,
    feasibility: u8,
    risk: u8,
    alignment: u8,
}

// Tests

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    // -- Test helpers -------------------------------------------------------

    #[allow(dead_code)]
    fn trivial_assessment() -> TaskAssessment {
        TaskAssessment {
            task_summary: "Fix typo in README".into(),
            complexity: ComplexityLevel::Trivial,
            success_criteria: vec![],
            route: super::super::types::PhaseRoute::DirectExecute,
            clarity: None,
        }
    }

    fn moderate_assessment() -> TaskAssessment {
        TaskAssessment {
            task_summary: "Add unit tests for the auth module".into(),
            complexity: ComplexityLevel::Moderate,
            success_criteria: vec![],
            route: super::super::types::PhaseRoute::StandardSequence,
            clarity: None,
        }
    }

    fn complex_assessment() -> TaskAssessment {
        TaskAssessment {
            task_summary: "Implement JWT auth with refresh tokens".into(),
            complexity: ComplexityLevel::Complex,
            success_criteria: vec![],
            route: super::super::types::PhaseRoute::RollingWave,
            clarity: None,
        }
    }

    fn complex_refactor_assessment() -> TaskAssessment {
        TaskAssessment {
            task_summary: "Refactor the entire LLM provider layer".into(),
            complexity: ComplexityLevel::Complex,
            success_criteria: vec![],
            route: super::super::types::PhaseRoute::RollingWave,
            clarity: None,
        }
    }

    fn complex_fix_assessment() -> TaskAssessment {
        TaskAssessment {
            task_summary: "Fix race condition in session handler".into(),
            complexity: ComplexityLevel::Complex,
            success_criteria: vec![],
            route: super::super::types::PhaseRoute::RollingWave,
            clarity: None,
        }
    }

    fn empty_brief() -> ContextBrief {
        ContextBrief {
            relevant_files: vec![],
            patterns_found: vec![],
            dependencies: vec![],
            risks: vec![],
            constraints: vec![],
        }
    }

    fn brief_with_files(n: usize) -> ContextBrief {
        ContextBrief {
            relevant_files: (0..n)
                .map(|i| PathBuf::from(format!("src/module_{i}.rs")))
                .collect(),
            patterns_found: vec!["existing_tests".into()],
            dependencies: vec!["serde".into()],
            risks: vec!["high_file_count".into()],
            constraints: vec!["language: rust".into()],
        }
    }

    // -- Brainstorm tests ---------------------------------------------------

    #[test]
    fn brainstorm_generates_min_two_proposals() {
        let funnel = BeddFunnel::new();
        let proposals = funnel.brainstorm(&complex_assessment(), &empty_brief());
        assert!(
            proposals.len() >= 2,
            "Should generate at least 2 proposals, got {}",
            proposals.len(),
        );
    }

    #[test]
    fn brainstorm_generates_max_five_proposals() {
        let funnel = BeddFunnel::new();
        let proposals = funnel.brainstorm(&complex_assessment(), &empty_brief());
        assert!(
            proposals.len() <= 5,
            "Should generate at most 5 proposals, got {}",
            proposals.len(),
        );
    }

    #[test]
    fn brainstorm_implement_keywords() {
        let funnel = BeddFunnel::new();
        let proposals = funnel.brainstorm(&complex_assessment(), &empty_brief());
        let approaches: Vec<&str> = proposals.iter().map(|p| p.approach.as_str()).collect();
        assert!(
            approaches.contains(&"greenfield"),
            "Implement task should propose greenfield: {approaches:?}"
        );
        assert!(
            approaches.contains(&"wrap library"),
            "Implement task should propose wrap library: {approaches:?}"
        );
    }

    #[test]
    fn brainstorm_refactor_keywords() {
        let funnel = BeddFunnel::new();
        let proposals = funnel.brainstorm(&complex_refactor_assessment(), &empty_brief());
        let approaches: Vec<&str> = proposals.iter().map(|p| p.approach.as_str()).collect();
        assert!(
            approaches.contains(&"incremental"),
            "Refactor task should propose incremental: {approaches:?}"
        );
        assert!(
            approaches.contains(&"strangler fig"),
            "Refactor task should propose strangler fig: {approaches:?}"
        );
    }

    #[test]
    fn brainstorm_fix_keywords() {
        let funnel = BeddFunnel::new();
        let proposals = funnel.brainstorm(&complex_fix_assessment(), &empty_brief());
        let approaches: Vec<&str> = proposals.iter().map(|p| p.approach.as_str()).collect();
        assert!(
            approaches.contains(&"targeted fix"),
            "Fix task should propose targeted fix: {approaches:?}"
        );
        assert!(
            approaches.contains(&"root cause analysis"),
            "Fix task should propose root cause analysis: {approaches:?}"
        );
    }

    #[test]
    fn brainstorm_test_keywords() {
        let funnel = BeddFunnel::new();
        let proposals = funnel.brainstorm(&moderate_assessment(), &empty_brief());
        let approaches: Vec<&str> = proposals.iter().map(|p| p.approach.as_str()).collect();
        assert!(
            approaches.contains(&"unit tests first"),
            "Test task should propose unit tests first: {approaches:?}"
        );
    }

    #[test]
    fn brainstorm_generic_fallback() {
        let funnel = BeddFunnel::new();
        let assessment = TaskAssessment {
            task_summary: "Review the dashboard analytics".into(),
            complexity: ComplexityLevel::Complex,
            success_criteria: vec![],
            route: super::super::types::PhaseRoute::RollingWave,
            clarity: None,
        };
        let proposals = funnel.brainstorm(&assessment, &empty_brief());
        assert!(
            !proposals.is_empty(),
            "Generic fallback should still produce proposals"
        );
        assert_eq!(proposals[0].approach, "straightforward approach");
    }

    #[test]
    fn brainstorm_proposals_have_ids() {
        let funnel = BeddFunnel::new();
        let proposals = funnel.brainstorm(&complex_assessment(), &empty_brief());
        for (i, p) in proposals.iter().enumerate() {
            assert_eq!(
                p.id,
                format!("P{}", i + 1),
                "Proposal IDs should be sequential"
            );
        }
    }

    #[test]
    fn brainstorm_proposals_have_tradeoffs() {
        let funnel = BeddFunnel::new();
        let proposals = funnel.brainstorm(&complex_assessment(), &empty_brief());
        for p in &proposals {
            assert!(
                !p.tradeoffs.is_empty(),
                "Proposal {} should have tradeoffs",
                p.id
            );
        }
    }

    #[test]
    fn brainstorm_proposal_effort_is_bounded() {
        let funnel = BeddFunnel::new();
        let proposals = funnel.brainstorm(&complex_assessment(), &empty_brief());
        for p in &proposals {
            assert!(
                p.estimated_effort > 0 && p.estimated_effort <= 10,
                "Effort should be 1-10, got {} for {}",
                p.estimated_effort,
                p.id,
            );
        }
    }

    // -- Evaluate tests -----------------------------------------------------

    #[test]
    fn evaluate_scores_all_proposals() {
        let funnel = BeddFunnel::new();
        let proposals = funnel.brainstorm(&complex_assessment(), &empty_brief());
        let evaluations = funnel.evaluate(&proposals, &complex_assessment());
        assert_eq!(evaluations.len(), proposals.len());
        for ev in &evaluations {
            assert!(ev.score > 0.0, "Score should be positive");
        }
    }

    #[test]
    fn evaluate_dimensions_are_bounded() {
        let funnel = BeddFunnel::new();
        let proposals = funnel.brainstorm(&complex_assessment(), &empty_brief());
        let evaluations = funnel.evaluate(&proposals, &complex_assessment());
        for ev in &evaluations {
            assert!(ev.feasibility <= 10, "feasibility should be <= 10");
            assert!(ev.risk <= 10, "risk should be <= 10");
            assert!(ev.alignment <= 10, "alignment should be <= 10");
            assert!(ev.effort <= 10, "effort should be <= 10");
        }
    }

    #[test]
    fn evaluate_has_two_sided_review() {
        let funnel = BeddFunnel::new();
        let proposals = funnel.brainstorm(&complex_assessment(), &empty_brief());
        let evaluations = funnel.evaluate(&proposals, &complex_assessment());
        for ev in &evaluations {
            assert!(
                !ev.optimistic_upside.is_empty(),
                "Should have optimistic upside"
            );
            assert!(!ev.critical_risks.is_empty(), "Should have critical risks");
        }
    }

    #[test]
    fn evaluate_empty_proposals_returns_empty() {
        let funnel = BeddFunnel::new();
        let evaluations = funnel.evaluate(&[], &complex_assessment());
        assert!(evaluations.is_empty());
    }

    // -- Scoring formula tests ----------------------------------------------

    #[test]
    fn scoring_formula_perfect_score() {
        // feasibility=10, risk=0, alignment=10, effort=0 should give the max score.
        let score = BeddFunnel::compute_score(10, 0, 10, 0, 0.3, 0.3, 0.25, 0.15);
        // (10*0.3 + 10*0.3 + 10*0.25 + 10*0.15) / 10 = 10/10 = 1.0
        assert!(
            (score - 1.0).abs() < 0.001,
            "Perfect score should be 1.0, got {score}"
        );
    }

    #[test]
    fn scoring_formula_zero_score() {
        // feasibility=0, risk=10, alignment=0, effort=10 should give 0.
        let score = BeddFunnel::compute_score(0, 10, 0, 10, 0.3, 0.3, 0.25, 0.15);
        // (0*0.3 + 0*0.3 + 0*0.25 + 0*0.15) / 10 = 0.0
        assert!(score.abs() < 0.001, "Zero score should be 0.0, got {score}");
    }

    #[test]
    fn scoring_formula_risk_inverted() {
        // Lower risk should produce higher score, all else equal.
        let low_risk = BeddFunnel::compute_score(5, 2, 5, 5, 0.3, 0.3, 0.25, 0.15);
        let high_risk = BeddFunnel::compute_score(5, 8, 5, 5, 0.3, 0.3, 0.25, 0.15);
        assert!(
            low_risk > high_risk,
            "Lower risk should score higher: {low_risk} vs {high_risk}"
        );
    }

    #[test]
    fn scoring_formula_effort_inverted() {
        // Lower effort should produce higher score, all else equal.
        let low_effort = BeddFunnel::compute_score(5, 5, 5, 2, 0.3, 0.3, 0.25, 0.15);
        let high_effort = BeddFunnel::compute_score(5, 5, 5, 8, 0.3, 0.3, 0.25, 0.15);
        assert!(
            low_effort > high_effort,
            "Lower effort should score higher: {low_effort} vs {high_effort}"
        );
    }

    #[test]
    fn scoring_formula_midrange() {
        let score = BeddFunnel::compute_score(5, 5, 5, 5, 0.3, 0.3, 0.25, 0.15);
        // (5*0.3 + 5*0.3 + 5*0.25 + 5*0.15) / 10 = 5/10 = 0.5
        assert!(
            (score - 0.5).abs() < 0.001,
            "Midrange score should be 0.5, got {score}"
        );
    }

    // -- Drop tests ---------------------------------------------------------

    #[test]
    fn drop_keeps_top_k() {
        let config = BeddConfig {
            keep_top_k: 1,
            ..Default::default()
        };
        let funnel = BeddFunnel::with_config(config);
        let proposals = funnel.brainstorm(&complex_assessment(), &empty_brief());
        let evaluations = funnel.evaluate(&proposals, &complex_assessment());
        let result = funnel.drop(&evaluations);

        assert_eq!(
            result.evaluations.len(),
            1,
            "Should keep exactly 1 evaluation"
        );
    }

    #[test]
    fn drop_keeps_top_two() {
        let config = BeddConfig {
            keep_top_k: 2,
            ..Default::default()
        };
        let funnel = BeddFunnel::with_config(config);
        let proposals = funnel.brainstorm(&complex_assessment(), &empty_brief());
        let evaluations = funnel.evaluate(&proposals, &complex_assessment());
        let result = funnel.drop(&evaluations);

        assert!(
            result.evaluations.len() <= 2,
            "Should keep at most 2 evaluations"
        );
    }

    #[test]
    fn drop_selected_is_highest_scored() {
        let funnel = BeddFunnel::new();
        let proposals = funnel.brainstorm(&complex_assessment(), &empty_brief());
        let evaluations = funnel.evaluate(&proposals, &complex_assessment());
        let max_score = evaluations
            .iter()
            .map(|ev| ev.score)
            .fold(f64::NEG_INFINITY, f64::max);
        let result = funnel.drop(&evaluations);

        // Find the winning evaluation to check score
        let winner_score = evaluations
            .iter()
            .find(|ev| ev.proposal.id == result.selected.id)
            .map_or(f64::NEG_INFINITY, |ev| ev.score);
        assert!(
            (winner_score - max_score).abs() < 0.001,
            "Selected proposal should have the highest score: {winner_score} vs {max_score}"
        );
    }

    #[test]
    fn drop_drops_remaining() {
        let funnel = BeddFunnel::new();
        let proposals = funnel.brainstorm(&complex_assessment(), &empty_brief());
        let evaluations = funnel.evaluate(&proposals, &complex_assessment());
        let result = funnel.drop(&evaluations);

        assert_eq!(
            result.dropped.len() + result.evaluations.len(),
            proposals.len(),
            "Kept + dropped should equal total"
        );
    }

    #[test]
    fn drop_single_proposal() {
        let funnel = BeddFunnel::new();
        let single = vec![Proposal {
            id: "P1".into(),
            approach: "test approach".into(),
            tradeoffs: vec!["tradeoff".into()],
            estimated_effort: 5,
            feasibility: Some(8),
            risk: Some(3),
            alignment: Some(7),
            score: None,
        }];
        let evaluations = funnel.evaluate(&single, &complex_assessment());
        let result = funnel.drop(&evaluations);

        assert_eq!(result.selected.id, "P1");
        assert!(result.dropped.is_empty());
    }

    // -- Full run tests -----------------------------------------------------

    #[test]
    fn full_run_produces_result() {
        let funnel = BeddFunnel::new();
        let result = funnel.run(&complex_assessment(), &empty_brief());

        assert!(!result.proposals.is_empty());
        assert!(!result.evaluations.is_empty());
        assert!(!result.selected.id.is_empty());
        assert!(!result.decision_reason.is_empty());
    }

    #[test]
    fn full_run_selected_is_from_proposals() {
        let funnel = BeddFunnel::new();
        let result = funnel.run(&complex_assessment(), &empty_brief());

        assert!(
            result.proposals.iter().any(|p| p.id == result.selected.id),
            "Selected ID should be from the proposal set"
        );
    }

    #[test]
    fn full_run_with_brief() {
        let funnel = BeddFunnel::new();
        let result = funnel.run(&complex_assessment(), &brief_with_files(10));

        assert!(!result.proposals.is_empty());
        assert!(!result.evaluations.is_empty());
    }

    #[test]
    fn full_run_refactor_task() {
        let funnel = BeddFunnel::new();
        let result = funnel.run(&complex_refactor_assessment(), &empty_brief());

        assert!(
            result.selected.approach.contains("incremental")
                || result.selected.approach.contains("strangler"),
            "Refactor should select incremental or strangler approach, got: {}",
            result.selected.approach,
        );
    }

    #[test]
    fn full_run_fix_task() {
        let funnel = BeddFunnel::new();
        let result = funnel.run(&complex_fix_assessment(), &empty_brief());

        // Any valid fix approach is acceptable
        assert!(
            result.selected.approach.contains("targeted")
                || result.selected.approach.contains("root cause")
                || result.selected.approach.contains("workaround"),
            "Fix should select a valid approach, got: {}",
            result.selected.approach,
        );
    }

    // -- Complexity gating tests --------------------------------------------

    #[test]
    fn config_should_run_complex() {
        let config = BeddConfig::default();
        assert!(config.should_run(ComplexityLevel::Complex));
    }

    #[test]
    fn config_should_not_run_trivial() {
        let config = BeddConfig::default();
        assert!(!config.should_run(ComplexityLevel::Trivial));
    }

    #[test]
    fn config_should_not_run_moderate() {
        let config = BeddConfig::default();
        assert!(!config.should_run(ComplexityLevel::Moderate));
    }

    // -- Config validation tests --------------------------------------------

    #[test]
    fn config_default_is_valid() {
        assert!(BeddConfig::default().validate().is_ok());
    }

    #[test]
    fn config_zero_min_proposals_fails() {
        let config = BeddConfig {
            min_proposals: 0,
            ..Default::default()
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn config_min_greater_than_max_fails() {
        let config = BeddConfig {
            min_proposals: 6,
            max_proposals: 5,
            ..Default::default()
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn config_zero_keep_top_k_fails() {
        let config = BeddConfig {
            keep_top_k: 0,
            ..Default::default()
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn config_weights_must_sum_to_one() {
        let config = BeddConfig {
            feasibility_weight: 0.5,
            risk_weight: 0.5,
            alignment_weight: 0.5,
            effort_weight: 0.5,
            ..Default::default()
        };
        assert!(config.validate().is_err());
    }

    // -- Edge case tests ----------------------------------------------------

    #[test]
    fn edge_case_all_same_scores() {
        // Create proposals that will all have the same score
        let proposals: Vec<Proposal> = (0..3)
            .map(|i| Proposal {
                id: format!("P{}", i + 1),
                approach: "test".into(),
                tradeoffs: vec!["t".into()],
                estimated_effort: 5,
                feasibility: Some(5),
                risk: Some(5),
                alignment: Some(5),
                score: None,
            })
            .collect();

        let funnel = BeddFunnel::new();
        let evaluations = funnel.evaluate(&proposals, &complex_assessment());
        let result = funnel.drop(&evaluations);

        // Should still produce a valid result
        assert!(!result.selected.id.is_empty());
        assert!(
            result.dropped.len() == proposals.len() - 1,
            "Should drop all but one"
        );
    }

    #[test]
    fn edge_case_custom_weights() {
        let config = BeddConfig {
            feasibility_weight: 1.0,
            risk_weight: 0.0,
            alignment_weight: 0.0,
            effort_weight: 0.0,
            ..Default::default()
        };
        assert!(config.validate().is_ok());

        let funnel = BeddFunnel::with_config(config);
        let proposals = funnel.brainstorm(&complex_assessment(), &empty_brief());
        let evaluations = funnel.evaluate(&proposals, &complex_assessment());

        // With only feasibility weighted, the highest-feasibility proposal should win
        let max_feasibility = evaluations
            .iter()
            .map(|ev| ev.feasibility)
            .max()
            .unwrap_or(0);
        let winner = evaluations
            .iter()
            .max_by(|a, b| {
                a.score
                    .partial_cmp(&b.score)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .expect("at least one evaluation");
        assert!(
            winner.feasibility == max_feasibility,
            "Winner should have max feasibility, got {} (max={})",
            winner.feasibility,
            max_feasibility,
        );
    }

    #[test]
    fn edge_case_keep_more_than_available() {
        let config = BeddConfig {
            keep_top_k: 10,
            ..Default::default()
        };
        let funnel = BeddFunnel::with_config(config);
        let proposals = funnel.brainstorm(&complex_assessment(), &empty_brief());
        let evaluations = funnel.evaluate(&proposals, &complex_assessment());
        let result = funnel.drop(&evaluations);

        // Should keep all proposals, not crash
        assert_eq!(result.evaluations.len(), proposals.len());
        assert!(result.dropped.is_empty());
    }

    // -- Markdown rendering tests -------------------------------------------

    #[test]
    fn render_markdown_contains_proposals() {
        let funnel = BeddFunnel::new();
        let result = funnel.run(&complex_assessment(), &empty_brief());
        let md = result.render_markdown();

        assert!(
            md.contains("### Proposals"),
            "Should contain Proposals header"
        );
        assert!(md.contains("id: P1"), "Should contain proposal IDs");
        assert!(md.contains("approach:"), "Should contain approach field");
        assert!(md.contains("tradeoffs:"), "Should contain tradeoffs field");
    }

    #[test]
    fn render_markdown_contains_evaluation() {
        let funnel = BeddFunnel::new();
        let result = funnel.run(&complex_assessment(), &empty_brief());
        let md = result.render_markdown();

        assert!(
            md.contains("### Evaluation"),
            "Should contain Evaluation header"
        );
        assert!(md.contains("feasibility="), "Should contain feasibility");
        assert!(md.contains("risk="), "Should contain risk");
        assert!(md.contains("alignment="), "Should contain alignment");
        assert!(md.contains("score="), "Should contain score");
    }

    #[test]
    fn render_markdown_contains_decision() {
        let funnel = BeddFunnel::new();
        let result = funnel.run(&complex_assessment(), &empty_brief());
        let md = result.render_markdown();

        assert!(
            md.contains("### Decision"),
            "Should contain Decision header"
        );
        assert!(md.contains("selected:"), "Should contain selected");
        assert!(md.contains("dropped:"), "Should contain dropped");
        assert!(md.contains("reason:"), "Should contain reason");
    }

    #[test]
    fn render_markdown_lists_all_proposals() {
        let funnel = BeddFunnel::new();
        let result = funnel.run(&complex_assessment(), &empty_brief());
        let md = result.render_markdown();

        for p in &result.proposals {
            assert!(
                md.contains(&p.id),
                "Markdown should mention proposal {}",
                p.id
            );
        }
    }

    // -- Serialization roundtrip tests --------------------------------------

    #[test]
    fn proposal_serialization_roundtrip() {
        let proposal = Proposal {
            id: "P1".into(),
            approach: "test approach".into(),
            tradeoffs: vec!["t1".into(), "t2".into()],
            estimated_effort: 5,
            feasibility: Some(8),
            risk: Some(3),
            alignment: Some(7),
            score: Some(0.75),
        };
        let json = serde_json::to_string(&proposal).unwrap();
        let back: Proposal = serde_json::from_str(&json).unwrap();
        assert_eq!(back.id, "P1");
        assert_eq!(back.tradeoffs.len(), 2);
        assert_eq!(back.score, Some(0.75));
    }

    #[test]
    fn evaluated_proposal_serialization_roundtrip() {
        let ep = EvaluatedProposal {
            proposal: Proposal {
                id: "P2".into(),
                approach: "incremental".into(),
                tradeoffs: vec!["low risk".into()],
                estimated_effort: 4,
                feasibility: None,
                risk: None,
                alignment: None,
                score: None,
            },
            feasibility: 8,
            risk: 3,
            alignment: 9,
            effort: 4,
            optimistic_upside: "Safe migration".into(),
            critical_risks: "Slower progress".into(),
            score: 0.82,
        };
        let json = serde_json::to_string(&ep).unwrap();
        let back: EvaluatedProposal = serde_json::from_str(&json).unwrap();
        assert_eq!(back.proposal.id, "P2");
        assert!((back.score - 0.82).abs() < 0.001);
    }

    #[test]
    fn bedd_result_serialization_roundtrip() {
        let funnel = BeddFunnel::new();
        let result = funnel.run(&complex_assessment(), &empty_brief());
        let json = serde_json::to_string(&result).unwrap();
        let back: BeddResult = serde_json::from_str(&json).unwrap();
        assert_eq!(back.selected.id, result.selected.id);
        assert_eq!(back.dropped.len(), result.dropped.len());
    }

    // -- Gap 1: Multi-Round Critique tests -----------------------------------

    #[test]
    fn critique_round_produces_valid_output() {
        let funnel = BeddFunnel::new();
        let proposals = funnel.brainstorm(&complex_assessment(), &empty_brief());
        let evaluations = funnel.evaluate(&proposals, &complex_assessment());
        let round = funnel.run_critique_round(&evaluations, 1);

        assert_eq!(round.round_number, 1);
        assert_eq!(round.critiques.len(), evaluations.len());
        for critique in &round.critiques {
            assert!(!critique.proposal_id.is_empty());
            assert!(!critique.strengths.is_empty());
            assert!(!critique.weaknesses.is_empty());
            assert!(!critique.suggested_improvements.is_empty());
        }
    }

    #[test]
    fn critique_round_consensus_is_bounded() {
        let funnel = BeddFunnel::new();
        let proposals = funnel.brainstorm(&complex_assessment(), &empty_brief());
        let evaluations = funnel.evaluate(&proposals, &complex_assessment());
        let round = funnel.run_critique_round(&evaluations, 1);

        assert!(
            round.consensus_score >= 0.0 && round.consensus_score <= 1.0,
            "Consensus should be in [0.0, 1.0], got {}",
            round.consensus_score,
        );
    }

    #[test]
    fn critique_round_consensus_perfect_for_identical_scores() {
        let consensus = BeddFunnel::compute_consensus(&[0.7, 0.7, 0.7]);
        assert!(
            (consensus - 1.0).abs() < 0.001,
            "Identical scores should give consensus 1.0, got {consensus}"
        );
    }

    #[test]
    fn critique_round_consensus_low_for_divergent_scores() {
        let consensus = BeddFunnel::compute_consensus(&[0.1, 0.9, 0.5]);
        assert!(
            consensus < 0.8,
            "Divergent scores should give lower consensus, got {consensus}"
        );
    }

    #[test]
    fn critique_round_consensus_single_score_is_one() {
        let consensus = BeddFunnel::compute_consensus(&[0.5]);
        assert!(
            (consensus - 1.0).abs() < 0.001,
            "Single score should give consensus 1.0, got {consensus}"
        );
    }

    #[test]
    fn critique_round_consensus_empty_is_one() {
        let consensus = BeddFunnel::compute_consensus(&[]);
        assert!(
            (consensus - 1.0).abs() < 0.001,
            "Empty scores should give consensus 1.0, got {consensus}"
        );
    }

    #[test]
    fn critique_serialization_roundtrip() {
        let critique = ProposalCritique {
            proposal_id: "P1".into(),
            strengths: vec!["High feasibility".into()],
            weaknesses: vec!["High effort".into()],
            suggested_improvements: vec!["Simplify approach".into()],
        };
        let json = serde_json::to_string(&critique).unwrap();
        let back: ProposalCritique = serde_json::from_str(&json).unwrap();
        assert_eq!(back.proposal_id, "P1");
        assert_eq!(back.strengths.len(), 1);

        let round = CritiqueRound {
            round_number: 2,
            critiques: vec![critique],
            consensus_score: 0.85,
        };
        let json = serde_json::to_string(&round).unwrap();
        let back: CritiqueRound = serde_json::from_str(&json).unwrap();
        assert_eq!(back.round_number, 2);
        assert!((back.consensus_score - 0.85).abs() < 0.001);
    }

    // -- Gap 2: Voting and Consensus tests -----------------------------------

    #[test]
    fn vote_on_proposals_produces_winner() {
        let funnel = BeddFunnel::new();
        let proposals = funnel.brainstorm(&complex_assessment(), &empty_brief());
        let evaluations = funnel.evaluate(&proposals, &complex_assessment());
        let result = funnel.vote_on_proposals(&evaluations, &["scout", "architect", "builder"]);

        assert!(!result.winner_id.is_empty());
        assert_eq!(result.votes.len(), 3);
        assert!(!result.weighted_scores.is_empty());
        assert_eq!(result.method, VotingMethod::WeightedScore);
    }

    #[test]
    fn vote_on_proposals_weighted_scores_sorted_descending() {
        let funnel = BeddFunnel::new();
        let proposals = funnel.brainstorm(&complex_assessment(), &empty_brief());
        let evaluations = funnel.evaluate(&proposals, &complex_assessment());
        let result = funnel.vote_on_proposals(&evaluations, &["scout", "architect"]);

        for window in result.weighted_scores.windows(2) {
            assert!(
                window[0].1 >= window[1].1,
                "Weighted scores should be sorted descending: {:?}",
                result.weighted_scores,
            );
        }
    }

    #[test]
    fn vote_on_proposals_winner_is_highest_weighted() {
        let funnel = BeddFunnel::new();
        let proposals = funnel.brainstorm(&complex_assessment(), &empty_brief());
        let evaluations = funnel.evaluate(&proposals, &complex_assessment());
        let result = funnel.vote_on_proposals(&evaluations, &["scout", "architect", "builder"]);

        let max_score = result.weighted_scores.first().map_or(0.0, |(_, s)| *s);
        let winner_score = result
            .weighted_scores
            .iter()
            .find(|(id, _)| id == &result.winner_id)
            .map_or(0.0, |(_, s)| *s);
        assert!(
            (winner_score - max_score).abs() < 0.001,
            "Winner should have highest weighted score: {winner_score} vs {max_score}"
        );
    }

    #[test]
    fn vote_on_proposals_empty_voters() {
        let funnel = BeddFunnel::new();
        let proposals = funnel.brainstorm(&complex_assessment(), &empty_brief());
        let evaluations = funnel.evaluate(&proposals, &complex_assessment());
        let result = funnel.vote_on_proposals(&evaluations, &[]);

        assert!(result.votes.is_empty());
        assert!(result.weighted_scores.is_empty());
    }

    #[test]
    fn vote_on_proposals_empty_proposals() {
        let funnel = BeddFunnel::new();
        let result = funnel.vote_on_proposals(&[], &["scout", "architect"]);

        assert!(result.winner_id.is_empty());
        assert_eq!(result.votes.len(), 2);
        for vote in &result.votes {
            assert!(vote.rankings.is_empty());
        }
    }

    #[test]
    fn vote_on_proposals_perspective_differs() {
        let funnel = BeddFunnel::new();
        // Create proposals with deliberately different profiles
        let high_risk_low_effort = EvaluatedProposal {
            proposal: Proposal {
                id: "P1".into(),
                approach: "test".into(),
                tradeoffs: vec![],
                estimated_effort: 2,
                feasibility: Some(8),
                risk: Some(9),
                alignment: Some(5),
                score: None,
            },
            feasibility: 8,
            risk: 9,
            alignment: 5,
            effort: 2,
            optimistic_upside: "Fast".into(),
            critical_risks: "Risky".into(),
            score: 0.5,
        };
        let low_risk_high_effort = EvaluatedProposal {
            proposal: Proposal {
                id: "P2".into(),
                approach: "test".into(),
                tradeoffs: vec![],
                estimated_effort: 8,
                feasibility: Some(6),
                risk: Some(1),
                alignment: Some(7),
                score: None,
            },
            feasibility: 6,
            risk: 1,
            alignment: 7,
            effort: 8,
            optimistic_upside: "Safe".into(),
            critical_risks: "Slow".into(),
            score: 0.5,
        };
        let proposals = [high_risk_low_effort, low_risk_high_effort];
        let result = funnel.vote_on_proposals(&proposals, &["scout", "architect", "builder"]);

        // Scout (risk-focused) should favor P2 (low risk)
        let scout_vote = &result.votes[0];
        let p1_scout = scout_vote
            .rankings
            .iter()
            .find(|(id, _)| id == "P1")
            .map_or(0.0, |(_, s)| *s);
        let p2_scout = scout_vote
            .rankings
            .iter()
            .find(|(id, _)| id == "P2")
            .map_or(0.0, |(_, s)| *s);
        assert!(
            p2_scout > p1_scout,
            "Scout should favor low-risk P2: P1={p1_scout}, P2={p2_scout}"
        );

        // Builder (effort-focused) should favor P1 (low effort)
        let builder_vote = &result.votes[2];
        let p1_builder = builder_vote
            .rankings
            .iter()
            .find(|(id, _)| id == "P1")
            .map_or(0.0, |(_, s)| *s);
        let p2_builder = builder_vote
            .rankings
            .iter()
            .find(|(id, _)| id == "P2")
            .map_or(0.0, |(_, s)| *s);
        assert!(
            p1_builder > p2_builder,
            "Builder should favor low-effort P1: P1={p1_builder}, P2={p2_builder}"
        );
    }

    #[test]
    fn vote_serialization_roundtrip() {
        let vote = Vote {
            voter_id: "scout".into(),
            rankings: vec![("P1".into(), 0.8), ("P2".into(), 0.5)],
        };
        let json = serde_json::to_string(&vote).unwrap();
        let back: Vote = serde_json::from_str(&json).unwrap();
        assert_eq!(back.voter_id, "scout");
        assert_eq!(back.rankings.len(), 2);

        let result = VotingResult {
            votes: vec![vote],
            winner_id: "P1".into(),
            weighted_scores: vec![("P1".into(), 0.8), ("P2".into(), 0.5)],
            method: VotingMethod::WeightedScore,
        };
        let json = serde_json::to_string(&result).unwrap();
        let back: VotingResult = serde_json::from_str(&json).unwrap();
        assert_eq!(back.winner_id, "P1");
        assert_eq!(back.method, VotingMethod::WeightedScore);
    }

    #[test]
    fn voting_method_serialization_roundtrip() {
        for method in [VotingMethod::SimpleMajority, VotingMethod::WeightedScore] {
            let json = serde_json::to_string(&method).unwrap();
            let back: VotingMethod = serde_json::from_str(&json).unwrap();
            assert_eq!(back, method);
        }
    }

    // -- Gap 5: Diminishing Returns tests ------------------------------------

    #[test]
    fn diminishing_returns_default_values() {
        let detector = DiminishingReturnsDetector::default();
        assert!(detector.score_history.is_empty());
        assert!((detector.plateau_threshold - 0.1).abs() < f64::EPSILON);
        assert_eq!(detector.max_iterations, 5);
        assert!(detector.token_budget.is_none());
    }

    #[test]
    fn diminishing_returns_continue_when_improving() {
        let mut detector = DiminishingReturnsDetector::new();
        detector.record_score(0.3);
        detector.record_score(0.5);
        assert!(detector.should_continue(2, 0));
        assert_eq!(detector.diagnose(2, 0), StopReason::ContinueIterating);
    }

    #[test]
    fn diminishing_returns_detects_plateau() {
        let mut detector = DiminishingReturnsDetector::new();
        detector.plateau_threshold = 0.1;
        detector.record_score(0.7);
        detector.record_score(0.72);
        detector.record_score(0.71);
        let reason = detector.diagnose(3, 0);
        assert!(
            matches!(reason, StopReason::ScorePlateau { .. }),
            "Should detect plateau, got {reason:?}"
        );
    }

    #[test]
    fn diminishing_returns_no_plateau_with_improvement() {
        let mut detector = DiminishingReturnsDetector::new();
        detector.plateau_threshold = 0.1;
        detector.record_score(0.3);
        detector.record_score(0.6);
        detector.record_score(0.9);
        assert_eq!(detector.diagnose(3, 0), StopReason::ContinueIterating);
    }

    #[test]
    fn diminishing_returns_max_iterations() {
        let detector = DiminishingReturnsDetector {
            max_iterations: 3,
            ..DiminishingReturnsDetector::new()
        };
        assert_eq!(detector.diagnose(3, 0), StopReason::MaxIterationsReached);
        assert_eq!(detector.diagnose(5, 0), StopReason::MaxIterationsReached);
    }

    #[test]
    fn diminishing_returns_under_max_iterations() {
        let detector = DiminishingReturnsDetector {
            max_iterations: 5,
            ..DiminishingReturnsDetector::new()
        };
        assert_eq!(detector.diagnose(4, 0), StopReason::ContinueIterating);
    }

    #[test]
    fn diminishing_returns_token_budget_exhausted() {
        let detector = DiminishingReturnsDetector {
            token_budget: Some(1000),
            ..DiminishingReturnsDetector::new()
        };
        let reason = detector.diagnose(1, 1500);
        assert!(
            matches!(
                reason,
                StopReason::TokenBudgetExhausted {
                    used: 1500,
                    budget: 1000
                }
            ),
            "Should detect token exhaustion, got {reason:?}"
        );
    }

    #[test]
    fn diminishing_returns_token_budget_not_exhausted() {
        let detector = DiminishingReturnsDetector {
            token_budget: Some(5000),
            ..DiminishingReturnsDetector::new()
        };
        assert_eq!(detector.diagnose(1, 2000), StopReason::ContinueIterating);
    }

    #[test]
    fn diminishing_returns_should_continue_wrapper() {
        let mut detector = DiminishingReturnsDetector::new();
        detector.record_score(0.5);
        assert!(detector.should_continue(1, 0));

        detector.max_iterations = 1;
        assert!(!detector.should_continue(1, 0));
    }

    #[test]
    fn diminishing_returns_plateau_with_few_scores() {
        let detector = DiminishingReturnsDetector::new();
        // Fewer than 3 scores should not trigger plateau
        assert_eq!(detector.diagnose(0, 0), StopReason::ContinueIterating);
    }

    #[test]
    fn stop_reason_serialization_roundtrip() {
        let reasons = [
            StopReason::ContinueIterating,
            StopReason::MaxIterationsReached,
            StopReason::ScorePlateau {
                best: 0.9,
                last: 0.85,
            },
            StopReason::TokenBudgetExhausted {
                used: 5000,
                budget: 10000,
            },
        ];
        for reason in reasons {
            let json = serde_json::to_string(&reason).unwrap();
            let back: StopReason = serde_json::from_str(&json).unwrap();
            assert_eq!(
                serde_json::to_string(&reason).unwrap(),
                serde_json::to_string(&back).unwrap(),
                "Roundtrip failed for {reason:?}"
            );
        }
    }

    #[test]
    fn diminishing_returns_detector_serialization_roundtrip() {
        let mut detector = DiminishingReturnsDetector::new();
        detector.record_score(0.5);
        detector.record_score(0.7);
        let json = serde_json::to_string(&detector).unwrap();
        let back: DiminishingReturnsDetector = serde_json::from_str(&json).unwrap();
        assert_eq!(back.score_history.len(), 2);
        assert!((back.score_history[0] - 0.5).abs() < 0.001);
        assert!((back.score_history[1] - 0.7).abs() < 0.001);
    }

    // -- max_critique_rounds config test -------------------------------------

    #[test]
    fn config_default_max_critique_rounds() {
        let config = BeddConfig::default();
        assert_eq!(config.max_critique_rounds, 1);
    }

    #[test]
    fn config_custom_max_critique_rounds() {
        let config = BeddConfig {
            max_critique_rounds: 3,
            ..Default::default()
        };
        assert_eq!(config.max_critique_rounds, 3);
        assert!(config.validate().is_ok());
    }
}
