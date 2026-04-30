//! Handler for the `worktree` CLI subcommand.
//!
//! Delegates to `rustycode_runtime::git_worktree::WorktreeManager` for all
//! git worktree operations (create, list, delete, prune).

use crate::commands::cli_args::WorktreeCommand;
use crate::prompt::{Confirm, Prompt};
use anyhow::Result;
use rustycode_runtime::git_worktree::{WorktreeManager, WorktreeType};
use std::path::Path;

pub async fn execute(cwd: &Path, command: WorktreeCommand, format: &str) -> Result<()> {
    match command {
        WorktreeCommand::Create {
            name,
            branch,
            worktree_type,
        } => {
            // Parse worktree type
            let wt_type = match worktree_type.as_str() {
                "session" => WorktreeType::Session,
                "feature" => WorktreeType::Feature,
                "bugfix" => WorktreeType::Bugfix,
                "experiment" => WorktreeType::Experiment,
                _ => return Err(anyhow::anyhow!("invalid worktree type: {}", worktree_type)),
            };

            // Create branch name
            let branch_name = branch.unwrap_or_else(|| format!("feature/{}", name));

            println!("Creating worktree '{}'...", name);
            println!("  Branch: {}", branch_name);
            println!("  Type: {:?}", wt_type);

            let manager = WorktreeManager::new(cwd.to_path_buf(), Default::default())
                .map_err(|e| anyhow::anyhow!("Failed to create worktree manager: {}", e))?;

            let worktree = manager
                .create_worktree(name.clone(), branch_name.clone(), wt_type)
                .await
                .map_err(|e| anyhow::anyhow!("Failed to create worktree: {}", e))?;

            println!("\n✅ Worktree created successfully!");
            println!("  Path: {}", worktree.path.display());
            println!("  ID: {}", worktree.id);
            println!("\nTo work in this worktree:");
            println!("  cd {}", worktree.path.display());
        }
        WorktreeCommand::List { detailed } => {
            let manager = WorktreeManager::new(cwd.to_path_buf(), Default::default())
                .map_err(|e| anyhow::anyhow!("Failed to create worktree manager: {}", e))?;

            let worktrees = manager.list_worktrees().await;

            if worktrees.is_empty() {
                println!("No worktrees found.");
            } else if format == "json" {
                println!("{}", serde_json::to_string_pretty(&worktrees)?);
            } else {
                println!("Git Worktrees ({} total):\n", worktrees.len());

                for wt in &worktrees {
                    if detailed {
                        println!("  📁 {}", wt.name);
                        println!("     ID: {}", wt.id);
                        println!("     Path: {}", wt.path.display());
                        println!("     Branch: {}", wt.branch);
                        println!("     Status: {:?}", wt.status);
                        println!("     Type: {:?}", wt.worktree_type);
                        println!("     Created: {}", wt.created_at.format("%Y-%m-%d %H:%M"));
                        println!();
                    } else {
                        println!("  {} - {} ({:?})", wt.name, wt.branch, wt.status);
                    }
                }
            }
        }
        WorktreeCommand::Delete {
            name,
            force,
            keep_branch,
        } => {
            // Confirm deletion unless --force flag is set
            if !force {
                let confirmed = Confirm::new(format!("Delete worktree '{}'?", name))
                    .with_default(false)
                    .prompt()?;

                if !confirmed {
                    println!("Deletion cancelled.");
                    return Ok(());
                }
            }

            println!("Deleting worktree '{}'...", name);

            let manager = WorktreeManager::new(cwd.to_path_buf(), Default::default())
                .map_err(|e| anyhow::anyhow!("Failed to create worktree manager: {}", e))?;

            manager
                .remove_worktree(&name)
                .await
                .map_err(|e| anyhow::anyhow!("Failed to remove worktree: {}", e))?;

            println!("✅ Worktree '{}' deleted.", name);

            if keep_branch {
                println!("  Branch kept (you can delete it manually with: git branch -D <branch>)");
            }
        }
        WorktreeCommand::Prune { max_age_days } => {
            println!("Pruning worktrees older than {} days...", max_age_days);

            let manager = WorktreeManager::new(cwd.to_path_buf(), Default::default())
                .map_err(|e| anyhow::anyhow!("Failed to create worktree manager: {}", e))?;

            let pruned_count = manager
                .prune_worktrees()
                .await
                .map_err(|e| anyhow::anyhow!("Failed to prune worktrees: {}", e))?;

            if pruned_count == 0 {
                println!("No stale worktrees found.");
            } else {
                println!("✅ Pruned {} worktree(s).", pruned_count);
            }
        }
    }
    Ok(())
}
