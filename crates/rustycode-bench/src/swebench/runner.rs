//! SWE-bench runner — clones repos, runs the agent, captures patches.
//!
//! This is the honest, no-tricks evaluation: the agent reads the code,
//! makes edits, and we capture the diff. The official SWE-bench evaluation
//! harness (separate tool) applies the patch and runs tests.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use super::instance::SweBenchInstance;
use super::prediction::{save_predictions, SweBenchPrediction};
use crate::agent::{BenchAgent, CodeAgentConfig};

/// Configuration for the SWE-bench runner.
pub struct SweBenchConfig {
    /// Path to instances JSON/JSONL file.
    pub instances_path: PathBuf,
    /// Output path for predictions.
    pub output_path: PathBuf,
    /// Output format: "json" or "jsonl".
    pub format: String,
    /// Model name for predictions metadata.
    pub model_name: String,
    /// Specific instance IDs to run (None = all).
    pub instance_ids: Option<Vec<String>>,
    /// Working directory for cloning repos.
    pub work_dir: PathBuf,
    /// Agent name: "code", "real", "oracle", "nop".
    pub agent_name: String,
    /// Agent configuration (used by CodeAgent).
    pub agent_config: CodeAgentConfig,
    /// Wall-clock timeout per instance in seconds.
    pub timeout_secs: u64,
}

/// Run SWE-bench predictions.
///
/// For each instance:
/// 1. Clone the repo at the base commit
/// 2. Run the CodeAgent with the problem statement
/// 3. Capture `git diff` as the model patch
/// 4. Save predictions incrementally
pub async fn run_swebench(config: SweBenchConfig) -> Result<Vec<SweBenchPrediction>> {
    let all_instances = super::instance::load_instances(&config.instances_path)?;

    let instances: Vec<&SweBenchInstance> = if let Some(ref ids) = config.instance_ids {
        all_instances
            .iter()
            .filter(|i| ids.contains(&i.instance_id))
            .collect()
    } else {
        all_instances.iter().collect()
    };

    tracing::info!(
        "SWE-bench: {} instances to process ({} total in file)",
        instances.len(),
        all_instances.len()
    );
    println!(
        "Instances: {} (of {} total)",
        instances.len(),
        all_instances.len()
    );

    std::fs::create_dir_all(&config.work_dir)?;

    let mut predictions = Vec::with_capacity(instances.len());

    for (i, inst) in instances.iter().enumerate() {
        println!(
            "[{}/{}] {} — {}",
            i + 1,
            instances.len(),
            inst.instance_id,
            truncate(&inst.problem_statement, 80)
        );

        match run_single_instance(inst, &config).await {
            Ok(patch) => {
                let has_patch = !patch.is_empty();
                println!(
                    "  → {} ({} bytes)",
                    if has_patch { "PATCH" } else { "EMPTY" },
                    patch.len()
                );
                predictions.push(SweBenchPrediction {
                    instance_id: inst.instance_id.clone(),
                    model_patch: patch,
                });
            }
            Err(e) => {
                tracing::warn!("[{}] Failed: {e:#}", inst.instance_id);
                println!("  → ERROR: {e:#}");
                predictions.push(SweBenchPrediction {
                    instance_id: inst.instance_id.clone(),
                    model_patch: String::new(),
                });
            }
        }

        // Incremental save after each instance
        save_predictions(
            &predictions,
            &config.output_path,
            &config.format,
            &config.model_name,
        )?;
    }

    // Summary
    let with_patches = predictions
        .iter()
        .filter(|p| !p.model_patch.is_empty())
        .count();
    println!();
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("SWE-bench complete");
    println!("  Total:   {}", predictions.len());
    println!("  Patches: {with_patches}");
    println!("  Empty:   {}", predictions.len() - with_patches);
    println!("  Output:  {}", config.output_path.display());
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");

    Ok(predictions)
}

/// Run a single SWE-bench instance: clone → agent → diff.
async fn run_single_instance(inst: &SweBenchInstance, config: &SweBenchConfig) -> Result<String> {
    let instance_dir = config.work_dir.join(&inst.instance_id);
    std::fs::create_dir_all(&instance_dir)?;

    let clone_dir = instance_dir.join("repo");

    // Clone repo if not already cloned
    if !clone_dir.join(".git").exists() {
        let repo_url = format!("https://github.com/{}.git", inst.repo);
        tracing::info!("[{}] Cloning {}...", inst.instance_id, inst.repo);

        let status = std::process::Command::new("git")
            .args([
                "clone",
                "--quiet",
                &repo_url,
                clone_dir.to_str().unwrap_or(""),
            ])
            .current_dir(&instance_dir)
            .status()
            .context("git clone")?;

        if !status.success() {
            anyhow::bail!("git clone failed for {}", inst.repo);
        }
    }

    // Fetch the specific base commit (shallow clones may not include it)
    let _ = std::process::Command::new("git")
        .args(["fetch", "--quiet", "origin", &inst.base_commit])
        .current_dir(&clone_dir)
        .status();

    // Checkout base commit
    tracing::info!("[{}] Checking out {}", inst.instance_id, inst.base_commit);
    let status = std::process::Command::new("git")
        .args(["checkout", "--quiet", &inst.base_commit])
        .current_dir(&clone_dir)
        .status()
        .context("git checkout")?;

    if !status.success() {
        anyhow::bail!(
            "git checkout {} failed for {}",
            inst.base_commit,
            inst.instance_id
        );
    }

    // Clean any leftover changes from previous runs
    let _ = std::process::Command::new("git")
        .args(["checkout", "--quiet", "."])
        .current_dir(&clone_dir)
        .status();

    // Build file tree for context (saves ~10 turns of exploration)
    let file_tree = build_file_tree(&clone_dir, 2);

    // Build hints section if available
    let hints_section = if inst.hints_text.is_empty() {
        String::new()
    } else {
        format!("\n## Hints\n\n{}\n", inst.hints_text)
    };

    // Build install hint — tells the agent to use the repo's code, not system packages
    let install_hint = if clone_dir.join("setup.py").exists()
        || clone_dir.join("pyproject.toml").exists()
    {
        "\n5. When testing, ALWAYS use `PYTHONPATH=.. python3 test.py` (or `pip install -e .`) to import from THIS repo, not the system package.\n"
    } else {
        ""
    };

    // Build prompt — honest, no tricks, but includes context to reduce exploration turns
    let prompt = format!(
        "Please fix the following issue in this repository.\n\n\
         ## Repository Structure\n\n```\n{file_tree}\n```\n\n\
         ## Issue\n\n{}\n\
         {hints_section}\
         ## Instructions\n\n\
         1. Use MULTIPLE tool calls per turn (e.g. read several files at once, or grep + glob together).\n\
         2. Make minimal, targeted changes to fix the issue.\n\
         3. Do NOT add tests — only fix the source code.\n\
         4. Prefer editing over reading — once you understand the bug, fix it immediately.{install_hint}",
        inst.problem_statement
    );

    // Create agent via factory (supports code, real, oracle, nop)
    let agent = crate::config::create_agent(
        &config.agent_name,
        &config.agent_config.model,
        clone_dir.clone(),
        Some(&config.agent_config.provider),
    )
    .with_context(|| format!("Failed to create '{}' agent", config.agent_name))?;

    run_agent_with_timeout(agent, &prompt, &clone_dir, config.timeout_secs).await?;

    // Capture diff
    capture_diff(&clone_dir)
}

/// Run the agent against a workspace directory with a timeout.
async fn run_agent_with_timeout(
    agent: Box<dyn BenchAgent>,
    prompt: &str,
    workspace: &Path,
    timeout_secs: u64,
) -> Result<()> {
    let prompt_owned = prompt.to_string();
    let workspace_owned = workspace.to_path_buf();

    let result = tokio::time::timeout(
        std::time::Duration::from_secs(timeout_secs),
        run_agent_on_workspace(agent, &prompt_owned, &workspace_owned),
    )
    .await;

    match result {
        Ok(Ok(())) => Ok(()),
        Ok(Err(e)) => Err(e),
        Err(_) => {
            tracing::warn!("Agent timed out after {timeout_secs}s");
            anyhow::bail!("Agent timed out after {timeout_secs}s")
        }
    }
}

/// Run a BenchAgent against a bare workspace (no BenchEnvironment).
///
/// Creates a minimal environment wrapper that provides `workspace_path()`.
async fn run_agent_on_workspace(
    mut agent: Box<dyn BenchAgent>,
    prompt: &str,
    workspace: &Path,
) -> Result<()> {
    use crate::environment::native::NativeEnvironment;

    let mut env = NativeEnvironment::new(workspace.to_path_buf(), workspace.to_path_buf());

    agent.setup(&mut env).await?;
    agent.run(prompt, &mut env).await?;

    Ok(())
}

/// Capture `git diff` as the model patch.
fn capture_diff(repo_dir: &Path) -> Result<String> {
    let output = std::process::Command::new("git")
        .args(["diff"])
        .current_dir(repo_dir)
        .output()
        .context("git diff")?;

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

fn truncate(s: &str, max: usize) -> String {
    rustycode_protocol::text::truncate_with_ellipsis(s, max)
}

/// Build a compact file tree (top-level + one level of subdirs) for prompt context.
fn build_file_tree(repo_dir: &Path, max_depth: usize) -> String {
    let mut lines = Vec::new();
    build_tree_inner(repo_dir, repo_dir, 0, max_depth, &mut lines);
    if lines.len() > 80 {
        lines.truncate(80);
        lines.push("... (truncated)".to_string());
    }
    lines.join("\n")
}

#[allow(clippy::only_used_in_recursion)]
fn build_tree_inner(
    base: &Path,
    dir: &Path,
    depth: usize,
    max_depth: usize,
    lines: &mut Vec<String>,
) {
    if depth > max_depth {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<_> = entries.filter_map(|e| e.ok()).collect();
    entries.sort_by_key(|e| e.file_name());

    for entry in entries {
        let name = entry.file_name().to_string_lossy().to_string();
        // Skip hidden, __pycache__, node_modules, .git
        if name.starts_with('.') || name == "__pycache__" || name == "node_modules" {
            continue;
        }
        let indent = "  ".repeat(depth);
        let path = entry.path();
        if path.is_dir() {
            lines.push(format!("{indent}{name}/"));
            build_tree_inner(base, &path, depth + 1, max_depth, lines);
        } else {
            lines.push(format!("{indent}{name}"));
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn truncate_short_string() {
        assert_eq!(truncate("hello", 10), "hello");
    }

    #[test]
    fn truncate_long_string() {
        let result = truncate("hello world this is long", 5);
        assert_eq!(result, "hello...");
    }

    #[test]
    fn config_fields() {
        let config = SweBenchConfig {
            instances_path: PathBuf::from("instances.json"),
            output_path: PathBuf::from("pred.json"),
            format: "json".to_string(),
            model_name: "test".to_string(),
            instance_ids: None,
            work_dir: PathBuf::from("/tmp/swe"),
            agent_name: "code".to_string(),
            agent_config: CodeAgentConfig::default(),
            timeout_secs: 600,
        };
        assert_eq!(config.format, "json");
        assert_eq!(config.timeout_secs, 600);
    }
}
