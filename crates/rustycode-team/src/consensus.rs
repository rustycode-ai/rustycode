//! Consensus mechanisms for ensemble decision making.
//!
//! Provides three strategies for resolving multi-team outcomes into a single
//! agreed-upon result:
//! - **Simple majority**: whichever answer has the most teams behind it wins.
//! - **Weighted confidence**: teams with higher convergence confidence count more.
//! - **Unanimous**: a single dissenting team blocks consensus.

use crate::convergence::{ConvergenceView, DissentingOpinion};
use crate::team_context::TeamContext;
use std::collections::HashMap;

/// Determines the outcome of an ensemble run based on team contributions.
#[derive(Debug, Clone)]
pub enum ConsensusResult {
    /// Consensus reached, teams agree.
    Agreed(ConvergenceView),
    /// No consensus, surfaced dissenting opinions.
    Dissent(Vec<DissentingOpinion>),
}

/// Evaluates team convergence and produces a consensus result.
pub struct ConsensusEngine;

impl ConsensusEngine {
    /// Evaluate the ensemble convergence based on strategy.
    pub fn evaluate(view: &ConvergenceView) -> ConsensusResult {
        if view.convergence_achieved {
            ConsensusResult::Agreed(view.clone())
        } else {
            ConsensusResult::Dissent(view.dissenting_opinions.clone())
        }
    }
}

/// Resolve by simple majority: the answer held by the most teams wins.
///
/// Teams are grouped by their top insight content. Ties are broken by
/// whichever group appeared first.
pub fn resolve_simple_majority(outcomes: &[TeamContext]) -> ConsensusResult {
    if outcomes.is_empty() {
        return ConsensusResult::Agreed(ConvergenceView::empty());
    }

    let mut groups: HashMap<String, Vec<&TeamContext>> = HashMap::new();
    for ctx in outcomes {
        let key = answer_key(&ctx.convergence);
        groups.entry(key).or_default().push(ctx);
    }

    let majority_count = (outcomes.len() / 2) + 1;
    let winning = groups
        .iter()
        .max_by_key(|(_, v)| v.len())
        .map(|(k, v)| (k.clone(), v.clone()));

    let Some((winning_key, winning_teams)) = winning else {
        return ConsensusResult::Dissent(collect_all_dissent(outcomes));
    };

    if winning_teams.len() >= majority_count {
        ConsensusResult::Agreed(aggregate_convergence(
            &winning_key,
            &winning_teams,
            outcomes,
        ))
    } else {
        ConsensusResult::Dissent(collect_all_dissent(outcomes))
    }
}

/// Resolve by weighted confidence: each team's vote is weighted by its
/// `convergence.max_confidence`. The answer with the highest total weight wins.
pub fn resolve_weighted_confidence(outcomes: &[TeamContext]) -> ConsensusResult {
    if outcomes.is_empty() {
        return ConsensusResult::Agreed(ConvergenceView::empty());
    }

    let mut weights: HashMap<String, f64> = HashMap::new();
    let mut members: HashMap<String, Vec<&TeamContext>> = HashMap::new();

    for ctx in outcomes {
        let key = answer_key(&ctx.convergence);
        let weight = ctx.convergence.max_confidence;
        *weights.entry(key.clone()).or_default() += weight;
        members.entry(key).or_default().push(ctx);
    }

    let winning = weights
        .iter()
        .max_by(|a, b| a.1.partial_cmp(b.1).unwrap_or(std::cmp::Ordering::Equal))
        .map(|(k, _)| k.clone());

    let Some(winning_key) = winning else {
        return ConsensusResult::Dissent(collect_all_dissent(outcomes));
    };

    let winning_teams: Vec<&TeamContext> = members.get(&winning_key).cloned().unwrap_or_default();

    let total_weight: f64 = weights.values().sum();
    if total_weight == 0.0 {
        return ConsensusResult::Dissent(collect_all_dissent(outcomes));
    }

    ConsensusResult::Agreed(aggregate_convergence(
        &winning_key,
        &winning_teams,
        outcomes,
    ))
}

/// Resolve by unanimous consensus: every team must agree. A single dissenting
/// team blocks consensus and its opinion is surfaced.
pub fn resolve_unanimous(outcomes: &[TeamContext]) -> ConsensusResult {
    if outcomes.is_empty() {
        return ConsensusResult::Agreed(ConvergenceView::empty());
    }

    let first_key = answer_key(&outcomes[0].convergence);
    let all_agree = outcomes
        .iter()
        .all(|ctx| answer_key(&ctx.convergence) == first_key);

    if all_agree {
        let all_refs: Vec<&TeamContext> = outcomes.iter().collect();
        ConsensusResult::Agreed(aggregate_convergence(&first_key, &all_refs, outcomes))
    } else {
        ConsensusResult::Dissent(collect_all_dissent(outcomes))
    }
}

fn answer_key(view: &ConvergenceView) -> String {
    view.top_insights
        .first()
        .map(|i| i.content.clone())
        .unwrap_or_else(|| {
            if view.convergence_achieved {
                "converged".to_string()
            } else {
                "diverged".to_string()
            }
        })
}

fn collect_all_dissent(outcomes: &[TeamContext]) -> Vec<DissentingOpinion> {
    let mut opinions = Vec::new();
    for ctx in outcomes {
        opinions.extend(ctx.convergence.dissenting_opinions.clone());
    }
    // If no pre-existing dissenting opinions, synthesize from non-converging teams
    // or teams whose answer differs from the most common answer.
    if opinions.is_empty() {
        let mut counts: HashMap<String, usize> = HashMap::new();
        for ctx in outcomes {
            *counts.entry(answer_key(&ctx.convergence)).or_default() += 1;
        }
        let majority_key = counts
            .iter()
            .max_by_key(|(_, v)| *v)
            .map(|(k, _)| k.clone());

        for ctx in outcomes {
            let key = answer_key(&ctx.convergence);
            let is_minority = majority_key.as_ref().is_some_and(|mk| key != *mk);
            if !ctx.convergence.convergence_achieved || is_minority {
                opinions.push(DissentingOpinion {
                    agent_id: format!("{}_agent", ctx.team_id),
                    team_id: ctx.team_id.clone(),
                    opinion: format!(
                        "Team {} dissents (confidence: {:.2}, answer: {})",
                        ctx.team_id, ctx.convergence.max_confidence, key
                    ),
                    confidence: ctx.convergence.max_confidence,
                    evidence: ctx
                        .convergence
                        .top_insights
                        .iter()
                        .map(|i| i.content.clone())
                        .collect(),
                });
            }
        }
    }
    opinions
}

fn aggregate_convergence(
    _winning_key: &str,
    winning: &[&TeamContext],
    all: &[TeamContext],
) -> ConvergenceView {
    let winning_keys: std::collections::HashSet<&str> =
        winning.iter().map(|ctx| ctx.team_id.as_str()).collect();

    let max_confidence = winning
        .iter()
        .map(|ctx| ctx.convergence.max_confidence)
        .fold(0.0_f64, f64::max);

    let mean_confidence = if winning.is_empty() {
        0.0
    } else {
        winning
            .iter()
            .map(|ctx| ctx.convergence.max_confidence)
            .sum::<f64>()
            / winning.len() as f64
    };

    let mut seen = std::collections::HashSet::new();
    let mut top_insights = Vec::new();
    for ctx in winning {
        for insight in &ctx.convergence.top_insights {
            if seen.insert(insight.content.clone()) {
                top_insights.push(insight.clone());
            }
        }
    }
    top_insights.sort_by(|a, b| {
        b.confidence
            .partial_cmp(&a.confidence)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let mut dissenting_opinions = Vec::new();
    for ctx in all {
        if !winning_keys.contains(ctx.team_id.as_str()) {
            dissenting_opinions.extend(ctx.convergence.dissenting_opinions.clone());
            if ctx.convergence.dissenting_opinions.is_empty()
                && !ctx.convergence.convergence_achieved
            {
                dissenting_opinions.push(DissentingOpinion {
                    agent_id: format!("{}_agent", ctx.team_id),
                    team_id: ctx.team_id.clone(),
                    opinion: format!(
                        "Team {} dissents (confidence: {:.2})",
                        ctx.team_id, ctx.convergence.max_confidence
                    ),
                    confidence: ctx.convergence.max_confidence,
                    evidence: ctx
                        .convergence
                        .top_insights
                        .iter()
                        .map(|i| i.content.clone())
                        .collect(),
                });
            }
        }
    }

    ConvergenceView {
        team_count: all.len(),
        max_confidence,
        mean_confidence,
        top_insights,
        dissenting_opinions,
        convergence_achieved: true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustycode_orchestration::agent_outcome::AgentOutcome;
    use rustycode_protocol::reasoning_summary::Insight;
    use rustycode_protocol::token_usage::TokenUsage;

    fn make_team_ctx(team_id: &str, answer: &str, confidence: f64, converged: bool) -> TeamContext {
        let insights = if answer.is_empty() {
            vec![]
        } else {
            vec![Insight::new(answer, confidence, "ensemble", 0)]
        };
        TeamContext {
            team_id: team_id.to_string(),
            task_id: "ensemble_task".to_string(),
            agent_outcomes: vec![AgentOutcome::failed(team_id, "ensemble_task", "mock")],
            convergence: ConvergenceView {
                team_count: 1,
                max_confidence: confidence,
                mean_confidence: confidence,
                top_insights: insights,
                dissenting_opinions: vec![],
                convergence_achieved: converged,
            },
            combined_changes: vec![],
            total_usage: TokenUsage::zero(),
        }
    }

    #[test]
    fn simple_majority_two_of_three_agree() {
        let t1 = make_team_ctx("t1", "use hashmap", 0.9, true);
        let t2 = make_team_ctx("t2", "use hashmap", 0.85, true);
        let t3 = make_team_ctx("t3", "use btree", 0.7, true);

        let result = resolve_simple_majority(&[t1, t2, t3]);
        match result {
            ConsensusResult::Agreed(view) => {
                assert!(view.convergence_achieved);
                assert_eq!(view.team_count, 3);
            }
            ConsensusResult::Dissent(_) => panic!("expected consensus"),
        }
    }

    #[test]
    fn simple_majority_no_majority() {
        let t1 = make_team_ctx("t1", "a", 0.9, true);
        let t2 = make_team_ctx("t2", "b", 0.9, true);
        let t3 = make_team_ctx("t3", "c", 0.9, true);

        let result = resolve_simple_majority(&[t1, t2, t3]);
        assert!(matches!(result, ConsensusResult::Dissent(_)));
    }

    #[test]
    fn weighted_confidence_high_weight_wins() {
        let t1 = make_team_ctx("t1", "answer_a", 0.3, true);
        let t2 = make_team_ctx("t2", "answer_b", 0.95, true);
        let t3 = make_team_ctx("t3", "answer_a", 0.4, true);

        let result = resolve_weighted_confidence(&[t1, t2, t3]);
        match result {
            ConsensusResult::Agreed(view) => {
                assert!(view.convergence_achieved);
                assert!(view
                    .top_insights
                    .first()
                    .is_some_and(|i| i.content == "answer_b"));
            }
            ConsensusResult::Dissent(_) => panic!("expected consensus"),
        }
    }

    #[test]
    fn weighted_confidence_zero_weights() {
        let t1 = make_team_ctx("t1", "a", 0.0, false);
        let t2 = make_team_ctx("t2", "b", 0.0, false);

        let result = resolve_weighted_confidence(&[t1, t2]);
        assert!(matches!(result, ConsensusResult::Dissent(_)));
    }

    #[test]
    fn unanimous_all_agree() {
        let t1 = make_team_ctx("t1", "use arc", 0.9, true);
        let t2 = make_team_ctx("t2", "use arc", 0.85, true);
        let t3 = make_team_ctx("t3", "use arc", 0.88, true);

        let result = resolve_unanimous(&[t1, t2, t3]);
        match result {
            ConsensusResult::Agreed(view) => {
                assert!(view.convergence_achieved);
                assert_eq!(view.team_count, 3);
            }
            ConsensusResult::Dissent(_) => panic!("expected consensus"),
        }
    }

    #[test]
    fn unanimous_single_dissent_blocks() {
        let t1 = make_team_ctx("t1", "use arc", 0.9, true);
        let t2 = make_team_ctx("t2", "use arc", 0.85, true);
        let t3 = make_team_ctx("t3", "use rc", 0.7, true);

        let result = resolve_unanimous(&[t1, t2, t3]);
        match result {
            ConsensusResult::Dissent(opinions) => {
                assert!(!opinions.is_empty());
            }
            ConsensusResult::Agreed(_) => panic!("expected dissent"),
        }
    }

    #[test]
    fn unanimous_empty_passes() {
        let result = resolve_unanimous(&[]);
        assert!(matches!(result, ConsensusResult::Agreed(_)));
    }

    #[test]
    fn consensus_engine_agreed() {
        let view = ConvergenceView {
            team_count: 2,
            max_confidence: 0.9,
            mean_confidence: 0.85,
            top_insights: vec![],
            dissenting_opinions: vec![],
            convergence_achieved: true,
        };
        let result = ConsensusEngine::evaluate(&view);
        assert!(matches!(result, ConsensusResult::Agreed(_)));
    }

    #[test]
    fn consensus_engine_dissent() {
        let view = ConvergenceView {
            team_count: 2,
            max_confidence: 0.5,
            mean_confidence: 0.4,
            top_insights: vec![],
            dissenting_opinions: vec![DissentingOpinion {
                agent_id: "a1".into(),
                team_id: "t1".into(),
                opinion: "disagree".into(),
                confidence: 0.5,
                evidence: vec![],
            }],
            convergence_achieved: false,
        };
        let result = ConsensusEngine::evaluate(&view);
        match result {
            ConsensusResult::Dissent(opinions) => {
                assert_eq!(opinions.len(), 1);
            }
            ConsensusResult::Agreed(_) => panic!("expected dissent"),
        }
    }
}
