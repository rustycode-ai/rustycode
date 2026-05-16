//! Ensemble orchestration for multi-team consensus.
//!
//! The ensemble layer dispatches the same (or decomposed) task to multiple
//! independent teams, collects their `TeamContext` results, and produces a
//! shared `ConvergenceView` through pluggable consensus strategies.

use crate::consensus::{
    resolve_simple_majority, resolve_unanimous, resolve_weighted_confidence, ConsensusResult,
};
use crate::convergence::ConvergenceView;
use crate::team_context::TeamContext;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::Mutex;

/// Configuration for an ensemble execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnsembleConfig {
    /// Number of parallel teams to run.
    pub team_count: usize,
    /// Strategy for team coordination.
    pub strategy: EnsembleStrategy,
    /// Optional total token budget across all teams (0 = unlimited).
    #[serde(default)]
    pub total_token_budget: u64,
}

/// Strategies for reaching consensus.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub enum EnsembleStrategy {
    #[default]
    Majority,
    Unanimous,
    WeightedConfidence,
}

/// Error conditions during ensemble execution.
#[derive(Debug, Clone)]
pub enum EnsembleError {
    /// Token budget exceeded before all teams finished.
    BudgetExceeded {
        teams_completed: usize,
        total_teams: usize,
        budget_used: u64,
        budget_limit: u64,
    },
    /// A team failed during execution.
    TeamFailed { team_id: String, reason: String },
}

/// Result of an ensemble run.
#[derive(Debug, Clone)]
pub struct EnsembleResult {
    /// The consensus outcome.
    pub consensus: ConsensusResult,
    /// Aggregated convergence view across all teams.
    pub convergence: ConvergenceView,
    /// Per-team results.
    pub team_results: Vec<TeamContext>,
    /// Total tokens consumed across all teams.
    pub total_tokens_used: u64,
}

/// Orchestrates multiple teams and aggregates their convergence views.
///
/// The orchestrator is agnostic to how teams produce their results — it
/// accepts a closure (`TeamExecutor`) for each team and collects the
/// `TeamContext` outputs, then resolves consensus.
pub struct EnsembleOrchestrator {
    pub config: EnsembleConfig,
    pub convergence: ConvergenceView,
}

impl EnsembleOrchestrator {
    /// Create a new ensemble orchestrator.
    pub fn new(config: EnsembleConfig) -> Self {
        Self {
            config,
            convergence: ConvergenceView::empty(),
        }
    }

    /// Execute the ensemble: run all teams, collect results, resolve consensus.
    ///
    /// The `executor` closure is called once per team with the team index.
    /// It returns a `TeamContext` representing that team's output.
    pub async fn run<F, Fut>(&mut self, executor: F) -> Result<EnsembleResult, EnsembleError>
    where
        F: Fn(usize) -> Fut + Send + Sync + 'static,
        Fut: std::future::Future<Output = TeamContext> + Send + 'static,
    {
        let team_count = self.config.team_count;
        let per_team_budget = if self.config.total_token_budget > 0 && team_count > 0 {
            Some(self.config.total_token_budget / team_count as u64)
        } else {
            None
        };

        let results: Arc<Mutex<Vec<TeamContext>>> = Arc::new(Mutex::new(Vec::new()));
        let mut handles = Vec::new();

        for i in 0..team_count {
            let results = Arc::clone(&results);
            let budget = per_team_budget;
            let fut = executor(i);
            handles.push(tokio::spawn(async move {
                let ctx = fut.await;
                let mut results = results.lock().await;

                if let Some(budget) = budget {
                    let used: u64 = results.iter().map(|r| r.total_usage.total()).sum::<u64>()
                        + ctx.total_usage.total();
                    if used > budget * (results.len() as u64 + 1) {
                        return Err(EnsembleError::BudgetExceeded {
                            teams_completed: results.len(),
                            total_teams: 0,
                            budget_used: used,
                            budget_limit: budget * (results.len() as u64 + 1),
                        });
                    }
                }

                results.push(ctx);
                Ok(())
            }));
        }

        let mut errors = Vec::new();
        for handle in handles {
            match handle.await {
                Ok(Ok(())) => {}
                Ok(Err(e)) => errors.push(e),
                Err(_) => {}
            }
        }

        if let Some(e) = errors.into_iter().next() {
            return Err(e);
        }

        let results = match Arc::try_unwrap(results) {
            Ok(mutex) => mutex.into_inner(),
            Err(arc) => {
                // Fallback: lock and clone if Arc still has multiple references
                let guard = arc.lock().await;
                guard.clone()
            }
        };

        let total_tokens: u64 = results.iter().map(|r| r.total_usage.total()).sum();

        let consensus = self.resolve_consensus(&results);
        let convergence = self.build_convergence(&results);

        self.convergence = convergence.clone();

        Ok(EnsembleResult {
            convergence,
            consensus,
            team_results: results,
            total_tokens_used: total_tokens,
        })
    }

    /// Execute teams in parallel using pre-built TeamContext results.
    ///
    /// This is the synchronous counterpart that doesn't need an async executor.
    /// Useful when team results are already available (e.g., from prior runs).
    pub fn resolve(&mut self, results: Vec<TeamContext>) -> EnsembleResult {
        let total_tokens: u64 = results.iter().map(|r| r.total_usage.total()).sum();

        if let Some(budget) = self.budget_per_team() {
            if total_tokens > budget * results.len() as u64 {
                return EnsembleResult {
                    consensus: ConsensusResult::Dissent(vec![]),
                    convergence: ConvergenceView::empty(),
                    team_results: results,
                    total_tokens_used: total_tokens,
                };
            }
        }

        let consensus = self.resolve_consensus(&results);
        let convergence = self.build_convergence(&results);

        self.convergence = convergence.clone();

        EnsembleResult {
            convergence,
            consensus,
            team_results: results,
            total_tokens_used: total_tokens,
        }
    }

    fn resolve_consensus(&self, results: &[TeamContext]) -> ConsensusResult {
        if results.is_empty() {
            return ConsensusResult::Agreed(ConvergenceView::empty());
        }
        match self.config.strategy {
            EnsembleStrategy::Majority => resolve_simple_majority(results),
            EnsembleStrategy::Unanimous => resolve_unanimous(results),
            EnsembleStrategy::WeightedConfidence => resolve_weighted_confidence(results),
        }
    }

    fn build_convergence(&self, results: &[TeamContext]) -> ConvergenceView {
        if results.is_empty() {
            return ConvergenceView::empty();
        }

        let max_confidence = results
            .iter()
            .map(|r| r.convergence.max_confidence)
            .fold(0.0_f64, f64::max);

        let mean_confidence = results
            .iter()
            .map(|r| r.convergence.max_confidence)
            .sum::<f64>()
            / results.len() as f64;

        let mut seen = std::collections::HashSet::new();
        let mut top_insights: Vec<_> = results
            .iter()
            .flat_map(|r| r.convergence.top_insights.clone())
            .filter(|i| seen.insert(i.content.clone()))
            .collect();
        top_insights.sort_by(|a, b| {
            b.confidence
                .partial_cmp(&a.confidence)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        let dissenting_opinions: Vec<_> = results
            .iter()
            .flat_map(|r| r.convergence.dissenting_opinions.clone())
            .collect();

        let convergence_achieved = dissenting_opinions.is_empty();

        ConvergenceView {
            team_count: results.len(),
            max_confidence,
            mean_confidence,
            top_insights,
            dissenting_opinions,
            convergence_achieved,
        }
    }

    fn budget_per_team(&self) -> Option<u64> {
        if self.config.total_token_budget > 0 && self.config.team_count > 0 {
            Some(self.config.total_token_budget / self.config.team_count as u64)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustycode_protocol::reasoning_summary::Insight;
    use rustycode_protocol::token_usage::TokenUsage;
    use rustycode_protocol::AgentOutcome;

    fn make_team(
        team_id: &str,
        answer: &str,
        confidence: f64,
        converged: bool,
        tokens: u64,
    ) -> TeamContext {
        let insights = if answer.is_empty() {
            vec![]
        } else {
            vec![Insight::new(answer, confidence, "ensemble_test", 0)]
        };
        let mut usage = TokenUsage::zero();
        usage.input_tokens = tokens;
        TeamContext {
            team_id: team_id.to_string(),
            task_id: "test_task".to_string(),
            agent_outcomes: vec![AgentOutcome::failed(team_id, "test_task", "mock")],
            convergence: ConvergenceView {
                team_count: 1,
                max_confidence: confidence,
                mean_confidence: confidence,
                top_insights: insights,
                dissenting_opinions: vec![],
                convergence_achieved: converged,
            },
            combined_changes: vec![],
            total_usage: usage,
        }
    }

    #[tokio::test]
    async fn ensemble_simple_majority() {
        let config = EnsembleConfig {
            team_count: 3,
            strategy: EnsembleStrategy::Majority,
            total_token_budget: 0,
        };
        let mut orch = EnsembleOrchestrator::new(config);

        let result = orch
            .run(|i| {
                let answer = if i < 2 { "hashmap" } else { "btree" };
                let confidence = if i < 2 { 0.9 } else { 0.7 };
                async move { make_team(&format!("t{i}"), answer, confidence, true, 100) }
            })
            .await
            .unwrap();

        assert!(matches!(result.consensus, ConsensusResult::Agreed(_)));
        assert_eq!(result.team_results.len(), 3);
    }

    #[tokio::test]
    async fn ensemble_weighted_confidence() {
        let config = EnsembleConfig {
            team_count: 3,
            strategy: EnsembleStrategy::WeightedConfidence,
            total_token_budget: 0,
        };
        let mut orch = EnsembleOrchestrator::new(config);

        let result = orch
            .run(|i| {
                let (answer, conf) = match i {
                    0 => ("a", 0.3),
                    1 => ("b", 0.95),
                    _ => ("a", 0.4),
                };
                async move { make_team(&format!("t{i}"), answer, conf, true, 100) }
            })
            .await
            .unwrap();

        match &result.consensus {
            ConsensusResult::Agreed(view) => {
                assert_eq!(view.top_insights.first().unwrap().content, "b");
            }
            ConsensusResult::Dissent(opinions) => {
                panic!(
                    "expected consensus, got dissent: {} opinions",
                    opinions.len()
                );
            }
        }
    }

    #[tokio::test]
    async fn ensemble_unanimous_veto() {
        let config = EnsembleConfig {
            team_count: 3,
            strategy: EnsembleStrategy::Unanimous,
            total_token_budget: 0,
        };
        let mut orch = EnsembleOrchestrator::new(config);

        let result = orch
            .run(|i| {
                let answer = if i == 2 { "dissent" } else { "agree" };
                async move { make_team(&format!("t{i}"), answer, 0.8, i != 2, 100) }
            })
            .await
            .unwrap();

        assert!(matches!(result.consensus, ConsensusResult::Dissent(_)));
    }

    #[tokio::test]
    async fn ensemble_convergence_view() {
        let config = EnsembleConfig {
            team_count: 3,
            strategy: EnsembleStrategy::Majority,
            total_token_budget: 0,
        };
        let mut orch = EnsembleOrchestrator::new(config);

        let result = orch
            .run(|i| {
                let answer = "shared_answer";
                let conf = 0.8 + i as f64 * 0.05;
                async move { make_team(&format!("t{i}"), answer, conf, true, 100) }
            })
            .await
            .unwrap();

        assert_eq!(result.convergence.team_count, 3);
        assert!(result.convergence.convergence_achieved);
        assert!(!result.convergence.top_insights.is_empty());
    }

    #[tokio::test]
    async fn ensemble_budget_enforcement() {
        let config = EnsembleConfig {
            team_count: 3,
            strategy: EnsembleStrategy::Majority,
            total_token_budget: 150,
        };
        let mut orch = EnsembleOrchestrator::new(config);

        let result = orch
            .run(|i| async move { make_team(&format!("t{i}"), "a", 0.9, true, 100) })
            .await;

        // Each team uses 100 tokens, 3 teams = 300, budget = 150 → should fail
        assert!(result.is_err());
        match result.unwrap_err() {
            EnsembleError::BudgetExceeded {
                teams_completed, ..
            } => {
                assert!(teams_completed < 3);
            }
            EnsembleError::TeamFailed { reason, .. } => {
                panic!("expected BudgetExceeded, got TeamFailed: {reason}");
            }
        }
    }

    #[tokio::test]
    async fn ensemble_dissent_surface() {
        let config = EnsembleConfig {
            team_count: 3,
            strategy: EnsembleStrategy::Unanimous,
            total_token_budget: 0,
        };
        let mut orch = EnsembleOrchestrator::new(config);

        let result = orch
            .run(|i| {
                let answer = if i == 0 { "rc" } else { "arc" };
                async move { make_team(&format!("t{i}"), answer, 0.8, i != 0, 100) }
            })
            .await
            .unwrap();

        match &result.consensus {
            ConsensusResult::Dissent(opinions) => {
                assert!(
                    !opinions.is_empty(),
                    "dissenting opinions must be preserved"
                );
            }
            ConsensusResult::Agreed(_) => panic!("expected dissent"),
        }
    }

    #[test]
    fn ensemble_resolve_sync() {
        let config = EnsembleConfig {
            team_count: 3,
            strategy: EnsembleStrategy::Majority,
            total_token_budget: 0,
        };
        let mut orch = EnsembleOrchestrator::new(config);

        let teams = vec![
            make_team("t1", "hashmap", 0.9, true, 100),
            make_team("t2", "hashmap", 0.85, true, 100),
            make_team("t3", "btree", 0.7, true, 100),
        ];

        let result = orch.resolve(teams);
        assert!(matches!(result.consensus, ConsensusResult::Agreed(_)));
        assert_eq!(result.team_results.len(), 3);
        assert_eq!(result.total_tokens_used, 300);
    }

    #[test]
    fn ensemble_budget_sync_blocks() {
        let config = EnsembleConfig {
            team_count: 2,
            strategy: EnsembleStrategy::Majority,
            total_token_budget: 100,
        };
        let mut orch = EnsembleOrchestrator::new(config);

        let teams = vec![
            make_team("t1", "a", 0.9, true, 100),
            make_team("t2", "a", 0.9, true, 100),
        ];

        let result = orch.resolve(teams);
        assert!(matches!(result.consensus, ConsensusResult::Dissent(_)));
    }

    #[test]
    fn ensemble_empty_teams() {
        let config = EnsembleConfig {
            team_count: 0,
            strategy: EnsembleStrategy::Majority,
            total_token_budget: 0,
        };
        let mut orch = EnsembleOrchestrator::new(config);

        let result = orch.resolve(vec![]);
        assert!(matches!(result.consensus, ConsensusResult::Agreed(_)));
        assert_eq!(result.total_tokens_used, 0);
    }
}
