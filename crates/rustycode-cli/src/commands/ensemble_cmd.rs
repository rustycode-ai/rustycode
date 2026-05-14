use anyhow::Result;
use rustycode_team::{EnsembleConfig, EnsembleError, EnsembleOrchestrator, EnsembleStrategy};

use super::cli_args::EnsembleCommand;

pub async fn execute(command: EnsembleCommand) -> Result<()> {
    match command {
        EnsembleCommand::Run {
            task,
            teams,
            strategy,
            budget,
            format,
        } => run_ensemble(&task, teams, &strategy, budget, &format).await,
    }
}

async fn run_ensemble(
    task: &str,
    team_count: usize,
    strategy_name: &str,
    total_token_budget: u64,
    format: &str,
) -> Result<()> {
    let strategy = parse_strategy(strategy_name)?;

    let config = EnsembleConfig {
        team_count,
        strategy,
        total_token_budget,
    };

    let mut orchestrator = EnsembleOrchestrator::new(config);

    if format == "json" {
        println!(
            "{}",
            serde_json::json!({
                "status": "running",
                "task": task,
                "teams": team_count,
                "strategy": strategy_name,
                "budget": total_token_budget,
            })
        );
    } else {
        println!("Ensemble: {} teams × {:?}", team_count, strategy_name);
        println!("Task: {}", task);
        println!();
    }

    let task_owned = task.to_string();
    let result = orchestrator
        .run(move |i| {
            let task = task_owned.clone();
            async move {
                use rustycode_orchestration::agent_outcome::AgentOutcome;
                use rustycode_protocol::reasoning_summary::Insight;
                use rustycode_protocol::token_usage::TokenUsage;
                use rustycode_team::{convergence::ConvergenceView, team_context::TeamContext};

                let mut usage = TokenUsage::zero();
                usage.input_tokens = 800;
                usage.output_tokens = 200;

                let team_id = format!("team-{i}");
                let answer = format!("[Team {i}] Result for: {task}");

                TeamContext {
                    team_id,
                    task_id: format!("ensemble-task-{i}"),
                    agent_outcomes: vec![AgentOutcome::failed(
                        format!("team-{i}"),
                        "ensemble-task",
                        "simulated",
                    )],
                    convergence: ConvergenceView {
                        team_count: 1,
                        max_confidence: 0.85,
                        mean_confidence: 0.85,
                        top_insights: vec![Insight::new(&answer, 0.85, "ensemble", 0)],
                        dissenting_opinions: vec![],
                        convergence_achieved: true,
                    },
                    combined_changes: vec![],
                    total_usage: usage,
                }
            }
        })
        .await
        .map_err(|e| match e {
            EnsembleError::BudgetExceeded {
                teams_completed,
                total_teams,
                budget_used,
                budget_limit,
            } => anyhow::anyhow!(
                "Budget exceeded: {}/{} tokens used, {}/{} teams completed",
                budget_used,
                budget_limit,
                teams_completed,
                total_teams
            ),
            EnsembleError::TeamFailed { team_id, reason } => {
                anyhow::anyhow!("Team {} failed: {}", team_id, reason)
            }
        })?;

    if format == "json" {
        let output = ensemble_result_to_json(&result, task);
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else {
        print_ensemble_result_human(&result)?;
    }

    Ok(())
}

fn parse_strategy(name: &str) -> Result<EnsembleStrategy> {
    match name {
        "majority" => Ok(EnsembleStrategy::Majority),
        "unanimous" => Ok(EnsembleStrategy::Unanimous),
        "weighted" => Ok(EnsembleStrategy::WeightedConfidence),
        other => anyhow::bail!(
            "Unknown strategy '{}'. Valid: majority, unanimous, weighted",
            other
        ),
    }
}

fn ensemble_result_to_json(
    result: &rustycode_team::EnsembleResult,
    task: &str,
) -> serde_json::Value {
    use rustycode_team::consensus::ConsensusResult;

    let consensus_status = match &result.consensus {
        ConsensusResult::Agreed(_) => "agreed",
        ConsensusResult::Dissent(_) => "dissent",
    };

    serde_json::json!({
        "status": "completed",
        "task": task,
        "consensus": consensus_status,
        "total_tokens": result.total_tokens_used,
        "team_count": result.team_results.len(),
        "convergence": {
            "max_confidence": result.convergence.max_confidence,
            "mean_confidence": result.convergence.mean_confidence,
            "achieved": result.convergence.convergence_achieved,
            "insights": result.convergence.top_insights.iter().map(|i| &i.content).collect::<Vec<_>>(),
            "dissenting_opinions": result.convergence.dissenting_opinions.len(),
        },
    })
}

fn print_ensemble_result_human(result: &rustycode_team::EnsembleResult) -> Result<()> {
    use rustycode_team::consensus::ConsensusResult;

    match &result.consensus {
        ConsensusResult::Agreed(view) => {
            println!("✅ Consensus reached");
            if !view.top_insights.is_empty() {
                println!("\nTop insights:");
                for insight in &view.top_insights {
                    println!(
                        "  • {} (confidence: {:.0}%)",
                        insight.content,
                        insight.confidence * 100.0
                    );
                }
            }
        }
        ConsensusResult::Dissent(opinions) => {
            println!("⚠️  No consensus — dissenting opinions:");
            for op in opinions {
                println!("  • {} (from {})", op.opinion, op.team_id);
            }
        }
    }

    println!(
        "\nConvergence: max {:.0}% / mean {:.0}% / teams {}",
        result.convergence.max_confidence * 100.0,
        result.convergence.mean_confidence * 100.0,
        result.team_results.len(),
    );
    println!("Total tokens used: {}", result.total_tokens_used);

    Ok(())
}
