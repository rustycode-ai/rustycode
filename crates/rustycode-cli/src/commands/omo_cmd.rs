//! Handler for the `omo` CLI subcommand.
//!
//! Delegates to `rustycode_runtime::multi_agent::MultiAgentOrchestrator` for
//! multi-agent code analysis.

use crate::commands::cli_args::OmoCommand;
use anyhow::Result;
use rustycode_runtime::multi_agent::{
    AgentRole, AgentRoleExt, MultiAgentConfig, MultiAgentOrchestrator,
};
use std::io::Read;

fn format_role_short_name(role: &AgentRole) -> &'static str {
    match role {
        AgentRole::Reviewer => "reviewer",
        AgentRole::Skeptic => "security",
        AgentRole::Judge => "judge",
        AgentRole::Researcher => "research",
        AgentRole::Worker => "worker",
        AgentRole::Architect => "architect",
        AgentRole::Builder => "builder",
        AgentRole::Planner => "planner",
        AgentRole::Scalpel => "scalpel",
        AgentRole::Coordinator => "coordinator",
        #[allow(unreachable_patterns)]
        _ => "other",
    }
}

pub async fn execute(command: OmoCommand) -> Result<()> {
    match command {
        OmoCommand::Analyze {
            file,
            roles,
            parallelism,
            context,
            instructions,
        } => {
            // Read content from file or stdin
            let content = if let Some(ref file_path) = file {
                std::fs::read_to_string(file_path)
                    .map_err(|e| anyhow::anyhow!("Failed to read file {}: {}", file_path, e))?
            } else {
                println!("Reading code from stdin (press Ctrl+D when done)...");
                let mut input = String::new();
                std::io::stdin().read_to_string(&mut input)?;
                input
            };

            // Parse roles if specified
            let agent_roles = if let Some(role_names) = roles {
                let mut parsed_roles = Vec::new();
                for role_name in role_names {
                    let role = match role_name.to_lowercase().as_str() {
                        "factual" | "factual-reviewer" | "senior" | "senior-engineer" | "test"
                        | "test-coverage" => AgentRole::Reviewer,
                        "security" | "security-expert" => AgentRole::Skeptic,
                        "consistency"
                        | "consistency-reviewer"
                        | "redundancy"
                        | "redundancy-checker" => AgentRole::Judge,
                        "performance" | "performance-analyst" => AgentRole::Researcher,
                        "docs" | "documentation" => AgentRole::Worker,
                        "architect" => AgentRole::Architect,
                        "builder" => AgentRole::Builder,
                        "planner" => AgentRole::Planner,
                        "scalpel" => AgentRole::Scalpel,
                        "coordinator" => AgentRole::Coordinator,
                        "reviewer" => AgentRole::Reviewer,
                        "research" | "researcher" => AgentRole::Researcher,
                        "worker" => AgentRole::Worker,
                        "judge" => AgentRole::Judge,
                        _ => return Err(anyhow::anyhow!("Unknown agent role: {}", role_name)),
                    };
                    parsed_roles.push(role);
                }
                if parsed_roles.is_empty() {
                    AgentRole::all()
                } else {
                    parsed_roles
                }
            } else {
                AgentRole::all()
            };

            // Build configuration
            let config = MultiAgentConfig {
                roles: agent_roles,
                max_parallelism: parallelism,
                context: context.unwrap_or_default(),
                content,
                file_path: file,
                instructions,
            };

            println!(
                "Starting multi-agent analysis with {} agents...",
                config.roles.len()
            );
            println!(
                "Running up to {} agents in parallel...\n",
                config.max_parallelism
            );

            let orchestrator = MultiAgentOrchestrator::from_config(config)?;
            let analysis = orchestrator.analyze().await?;

            println!("{}", MultiAgentOrchestrator::format_analysis(&analysis));
        }
        OmoCommand::ListRoles => {
            println!("Available Agent Roles:\n");
            for role in AgentRole::all() {
                println!("  • {} ({})", role.name(), format_role_short_name(&role));
            }
            println!("\nUse these role names (or abbreviations) with --roles flag.");
        }
    }
    Ok(())
}
