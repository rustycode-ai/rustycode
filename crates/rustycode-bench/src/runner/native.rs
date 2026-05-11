//! Native benchmark runner — executes tasks on the host without containers.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use anyhow::{Context, Result};
use tracing;

use crate::agent::BenchAgent;
use crate::environment::native::NativeEnvironment;
use crate::environment::BenchEnvironment;
use crate::hooks::{HookContext, Hooks, TrialEvent};
use crate::job::BenchmarkResults;
use crate::task::ResolvedTask;
use crate::trial::{RetryConfig, TrialResult};
use crate::verifier::native::NativeVerifier;
use crate::verifier::Verifier;

/// Thread-safe agent factory closure.
pub type AgentFactory =
    Box<dyn Fn(&str, &str, PathBuf) -> Result<Box<dyn BenchAgent>> + Send + Sync>;

/// Configuration for the native benchmark runner.
#[derive(Debug, Clone)]
pub struct NativeRunnerConfig {
    pub agent_name: String,
    pub model: String,
    pub n_concurrent: usize,
    pub job_name: String,
    pub retry_config: RetryConfig,
    pub per_trial_timeout: u64,
}

/// Runs benchmark tasks natively on the host (no Docker/QEMU).
pub struct NativeRunner {
    config: NativeRunnerConfig,
}

impl NativeRunner {
    pub fn new(config: NativeRunnerConfig) -> Self {
        Self { config }
    }

    /// Run all tasks and return aggregated results.
    pub async fn run(
        &self,
        tasks: &[ResolvedTask],
        dataset_path: &Path,
        create_agent: AgentFactory,
    ) -> Result<BenchmarkResults> {
        let pb = indicatif::ProgressBar::new(tasks.len() as u64);
        pb.set_style(
            indicatif::ProgressStyle::with_template("{msg} {pos}/{len} [{bar:40.cyan/blue}] {eta}")
                .context("Invalid progress bar style")?
                .progress_chars("=>-"),
        );
        pb.set_message("Trials");

        let semaphore = Arc::new(tokio::sync::Semaphore::new(self.config.n_concurrent));
        let pb_clone = pb.clone();
        let mut handles = Vec::new();

        let create_agent = Arc::new(create_agent);

        for task in tasks {
            let permit = semaphore.clone().acquire_owned().await?;
            let task = task.clone();
            let agent_name = self.config.agent_name.clone();
            let model = self.config.model.clone();
            let retry_config = self.config.retry_config.clone();
            let job_name = self.config.job_name.clone();
            let task_hooks = default_hooks();
            let pb_ref = pb_clone.clone();
            let per_trial_timeout = self.config.per_trial_timeout;
            let create_agent = create_agent.clone();

            let handle = tokio::spawn(async move {
                let _permit = permit;
                let result = run_native_trial(
                    &task,
                    &agent_name,
                    &model,
                    &job_name,
                    &retry_config,
                    &task_hooks,
                    per_trial_timeout,
                    &create_agent,
                )
                .await;
                let status = if result.success { "PASS" } else { "FAIL" };
                pb_ref.inc(1);
                pb_ref.set_message(format!("{status} {}", task.name));
                result
            });
            handles.push(handle);
        }

        let mut trial_results = Vec::new();
        for handle in handles {
            match handle.await {
                Ok(result) => trial_results.push(result),
                Err(e) => {
                    pb.inc(1);
                    tracing::error!("Trial task panicked: {e}");
                }
            }
        }

        pb.finish_with_message(format!(
            "Done: {}/{} passed",
            trial_results.iter().filter(|r| r.success).count(),
            trial_results.len()
        ));

        let results = BenchmarkResults::from_trials(&trial_results);

        // Save results to dataset's _jobs directory
        let result_path = dataset_path
            .join("_jobs")
            .join(&self.config.job_name)
            .join("result.json");
        if let Some(parent) = result_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let result_json = serde_json::to_string_pretty(&results)?;
        std::fs::write(&result_path, result_json)?;

        // Preserve conversation traces before cleaning up temp workspaces.
        let job_dir = dataset_path.join("_jobs").join(&self.config.job_name);
        let temp_base = std::env::temp_dir()
            .join("rtk-bench")
            .join(&self.config.job_name);
        if temp_base.exists() {
            if let Ok(entries) = std::fs::read_dir(&temp_base) {
                for entry in entries.flatten() {
                    if entry.path().is_dir() {
                        // Copy conversation_trace.md to job dir before removing
                        let trace_src =
                            entry.path().join("workspace").join("conversation_trace.md");
                        if trace_src.exists() {
                            let task_name = entry.file_name().to_string_lossy().to_string();
                            // Extract task name from trial dir (format: {task}-{session})
                            let task = task_name.split("-native-").next().unwrap_or(&task_name);
                            let trace_dst = job_dir.join(format!("{task}-conversation-trace.md"));
                            if let Some(parent) = trace_dst.parent() {
                                let _ = std::fs::create_dir_all(parent);
                            }
                            let _ = std::fs::copy(&trace_src, &trace_dst);
                        }
                        // TEMP: skip cleanup to debug file-organizer
                        eprintln!("DEBUG: keeping trial dir {}", entry.path().display());
                        //let _ = std::fs::remove_dir_all(entry.path());
                    }
                }
            }
            // Try removing the now-empty parent (fails silently if not empty)
            let _ = std::fs::remove_dir(&temp_base);
        }

        Ok(results)
    }
}

/// Build a Hooks instance with the standard logging hook.
pub(crate) fn default_hooks() -> Hooks {
    let mut h = Hooks::new();
    h.on_start(Arc::new(crate::hooks::logging_hook));
    h.on_end(Arc::new(crate::hooks::logging_hook));
    h.on_cancel(Arc::new(crate::hooks::logging_hook));
    h
}

/// Run a single native-mode trial with retries.
async fn run_native_trial(
    task: &ResolvedTask,
    agent_name: &str,
    model: &str,
    job_name: &str,
    retry_config: &RetryConfig,
    hooks: &Hooks,
    per_trial_timeout: u64,
    create_agent: &Arc<AgentFactory>,
) -> TrialResult {
    let session_id = format!("native-{}", uuid::Uuid::new_v4());
    let trial_name = format!("{}-{}", task.name, session_id);
    let temp_base = std::env::temp_dir().join("rtk-bench").join(job_name);
    let trial_dir = temp_base.join(&trial_name);
    let start = Instant::now();

    let hook_ctx = HookContext {
        task_name: task.name.clone(),
        agent_name: agent_name.to_string(),
        attempt: 1,
        event: TrialEvent::Start,
    };
    hooks.fire(TrialEvent::Start, &hook_ctx);

    let max_attempts = 1 + retry_config.max_retries;
    let mut last_result: Option<anyhow::Result<f64>> = None;

    for attempt in 0..max_attempts {
        let attempt_ctx = HookContext {
            task_name: task.name.clone(),
            agent_name: agent_name.to_string(),
            attempt: attempt + 1,
            event: TrialEvent::EnvironmentStart,
        };
        hooks.fire(TrialEvent::EnvironmentStart, &attempt_ctx);

        let result = run_native_trial_attempt(
            task,
            agent_name,
            model,
            &trial_dir,
            hooks,
            per_trial_timeout,
            create_agent,
        )
        .await;

        match result {
            Ok(reward) => {
                last_result = Some(Ok(reward));
                break;
            }
            Err(e) => {
                tracing::warn!("[{}] Attempt {} failed: {e}", task.name, attempt + 1);
                last_result = Some(Err(e));

                if attempt + 1 < max_attempts {
                    let wait = retry_config.wait_duration_secs(attempt);
                    tracing::info!("[{}] Retrying in {wait:.1}s...", task.name);
                    tokio::time::sleep(std::time::Duration::from_secs_f64(wait)).await;
                }
            }
        }
    }

    let duration = start.elapsed().as_secs_f64();

    hooks.fire(
        TrialEvent::End,
        &HookContext {
            task_name: task.name.clone(),
            agent_name: agent_name.to_string(),
            attempt: 1,
            event: TrialEvent::End,
        },
    );

    match last_result {
        Some(Ok(reward)) => TrialResult {
            task_name: task.name.clone(),
            agent_name: agent_name.to_string(),
            reward,
            success: true,
            error: None,
            duration_secs: duration,
            trial_dir,
            input_tokens: 0,
            output_tokens: 0,
            cost_usd: 0.0,
        },
        Some(Err(e)) => TrialResult {
            task_name: task.name.clone(),
            agent_name: agent_name.to_string(),
            reward: 0.0,
            success: false,
            error: Some(e.to_string()),
            duration_secs: duration,
            trial_dir,
            input_tokens: 0,
            output_tokens: 0,
            cost_usd: 0.0,
        },
        None => TrialResult {
            task_name: task.name.clone(),
            agent_name: agent_name.to_string(),
            reward: 0.0,
            success: false,
            error: Some("no attempts made".to_string()),
            duration_secs: duration,
            trial_dir,
            input_tokens: 0,
            output_tokens: 0,
            cost_usd: 0.0,
        },
    }
}

/// Execute a single native-mode trial attempt (with wall-clock timeout).
async fn run_native_trial_attempt(
    task: &ResolvedTask,
    agent_name: &str,
    model: &str,
    trial_dir: &Path,
    hooks: &Hooks,
    per_trial_timeout: u64,
    create_agent: &Arc<AgentFactory>,
) -> anyhow::Result<f64> {
    let wall_timeout = if per_trial_timeout > 0 {
        per_trial_timeout
    } else {
        600 + task.config.verifier.timeout_sec as u64
    };

    let result = tokio::time::timeout(
        std::time::Duration::from_secs(wall_timeout),
        run_native_trial_attempt_inner(task, agent_name, model, trial_dir, hooks, create_agent),
    )
    .await
    .map_err(|_| anyhow::anyhow!("Trial timed out after {wall_timeout}s"))??;

    Ok(result)
}

async fn run_native_trial_attempt_inner(
    task: &ResolvedTask,
    agent_name: &str,
    model: &str,
    trial_dir: &Path,
    hooks: &Hooks,
    create_agent: &Arc<AgentFactory>,
) -> anyhow::Result<f64> {
    let workspace = trial_dir.join("workspace");
    std::fs::create_dir_all(&workspace)?;

    let instruction_path = workspace.join("instruction.md");
    std::fs::write(&instruction_path, &task.instruction)?;

    let mut env = NativeEnvironment::new(workspace.clone(), task.task_dir.clone());
    env.start(false).await?;

    let mut agent = create_agent(agent_name, model, task.solution_dir.clone())?;

    hooks.fire(
        TrialEvent::AgentStart,
        &HookContext {
            task_name: task.name.clone(),
            agent_name: agent_name.to_string(),
            attempt: 1,
            event: TrialEvent::AgentStart,
        },
    );

    tracing::info!("[{}] Agent setup ({})...", task.name, agent.name());
    agent.setup(&mut env).await?;

    tracing::info!("[{}] Agent run...", task.name);
    agent.run(&task.instruction, &mut env).await?;

    hooks.fire(
        TrialEvent::VerificationStart,
        &HookContext {
            task_name: task.name.clone(),
            agent_name: agent_name.to_string(),
            attempt: 1,
            event: TrialEvent::VerificationStart,
        },
    );

    tracing::info!("[{}] Verification...", task.name);
    let verifier = NativeVerifier::new(
        task.tests_dir.clone(),
        task.config.verifier.timeout_sec as u64,
    );
    let reward = verifier.verify(&mut env).await?;

    if let Err(e) = env.stop(false).await {
        tracing::warn!("Native environment cleanup failed: {e}");
    }

    tracing::info!(
        "[{}] Complete -- reward: {reward:.3} ({})",
        task.name,
        if reward >= 0.5 { "PASS" } else { "FAIL" }
    );

    Ok(reward)
}
