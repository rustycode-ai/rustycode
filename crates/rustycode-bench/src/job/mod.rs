//! Job orchestration — manages N concurrent benchmark trials.

mod result;

pub use result::{BenchmarkResults, TaskResult};

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tokio::sync::Semaphore;

use crate::agent::BenchAgent;
use crate::task::ResolvedTask;
use crate::trial::{Trial, TrialResult};
use crate::verifier::Verifier;

/// Job configuration for a benchmark run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobConfig {
    /// Human-readable job name.
    pub job_name: String,
    /// Directory for job output.
    pub jobs_dir: PathBuf,
    /// Maximum concurrent trials.
    pub n_concurrent: usize,
    /// Whether to force-build images (needed for aarch64).
    pub force_build: bool,
    /// Whether to delete containers after each trial.
    pub cleanup: bool,
}

/// Orchestrates multiple benchmark trials concurrently.
///
/// A Job discovers tasks from a dataset directory, runs each one through
/// the Trial pipeline (environment → agent → verifier), and aggregates
/// results into a [`BenchmarkResults`].
pub struct Job {
    config: JobConfig,
}

impl Job {
    #[must_use]
    #[allow(clippy::missing_const_for_fn)]
    pub fn new(config: JobConfig) -> Self {
        Self { config }
    }

    /// Run pre-discovered tasks with the given agent and verifier.
    ///
    /// If resuming, skips tasks that already have completed trial results
    /// in the job directory.
    pub async fn run(
        &self,
        tasks: &[ResolvedTask],
        agent_factory: &(dyn Fn(PathBuf) -> Box<dyn BenchAgent> + Send + Sync),
        verifier_factory: &(dyn Fn(PathBuf, u64) -> Box<dyn Verifier> + Send + Sync),
    ) -> anyhow::Result<BenchmarkResults> {
        if tasks.is_empty() {
            anyhow::bail!("No tasks provided");
        }

        tracing::info!("Running {} tasks", tasks.len());

        // Find already-completed tasks (for resume)
        let completed = self.find_completed_tasks()?;
        let total_tasks = tasks.len();
        let remaining: Vec<ResolvedTask> = tasks
            .iter()
            .filter(|t| !completed.contains(&t.name))
            .cloned()
            .collect();

        tracing::info!(
            "Tasks: {} total, {} completed (resume), {} remaining",
            total_tasks,
            completed.len(),
            remaining.len(),
        );

        // Create job directory
        let job_dir = self.job_dir();
        std::fs::create_dir_all(&job_dir)?;

        // Save job config
        let config_path = job_dir.join("config.json");
        let config_json = serde_json::to_string_pretty(&self.config)?;
        std::fs::write(&config_path, config_json)?;

        // Run trials concurrently with a semaphore
        let semaphore = Arc::new(Semaphore::new(self.config.n_concurrent));
        let mut handles = Vec::new();

        for task in &remaining {
            let permit = semaphore.clone().acquire_owned().await?;
            let task = task.clone();
            let job_dir = job_dir.clone();
            let force_build = self.config.force_build;
            let cleanup = self.config.cleanup;
            let session_base = format!("bench-{}", self.config.job_name);

            let mut agent = agent_factory(task.solution_dir.clone());
            let verifier = verifier_factory(
                task.tests_dir.clone(),
                task.config.verifier.timeout_sec as u64,
            );

            let handle = tokio::spawn(async move {
                let _permit = permit;
                let session_id = format!("{}-{}", session_base, uuid::Uuid::new_v4());
                let trial = Trial::new(session_id, job_dir)
                    .with_force_build(force_build)
                    .with_cleanup(cleanup);
                trial.run(&task, &mut *agent, verifier.as_ref()).await
            });

            handles.push(handle);
        }

        // Collect results
        let mut new_results = Vec::new();
        for handle in handles {
            match handle.await {
                Ok(result) => new_results.push(result),
                Err(e) => {
                    tracing::error!("Trial task panicked: {}", e);
                }
            }
        }

        // Load existing results from previous run
        let existing_results = self.load_existing_results();

        // Combine and save
        let all_results: Vec<TrialResult> =
            existing_results.into_iter().chain(new_results).collect();

        let results = BenchmarkResults::from_trials(&all_results);

        // Save results
        let result_path = job_dir.join("result.json");
        let result_json = serde_json::to_string_pretty(&results)?;
        std::fs::write(&result_path, result_json)?;

        tracing::info!("{}", results.summary());

        Ok(results)
    }

    /// Job output directory.
    fn job_dir(&self) -> PathBuf {
        self.config.jobs_dir.join(&self.config.job_name)
    }

    /// Find tasks that already have completed results in the job directory.
    fn find_completed_tasks(&self) -> anyhow::Result<HashSet<String>> {
        let job_dir = self.job_dir();
        if !job_dir.exists() {
            return Ok(HashSet::new());
        }

        let mut completed = HashSet::new();
        for entry in std::fs::read_dir(&job_dir)? {
            let entry = entry?;
            if !entry.file_type()?.is_dir() {
                continue;
            }

            // Check if trial has a reward.txt file (completed)
            let reward_file = entry.path().join("verifier").join("reward.txt");
            if reward_file.exists() {
                if let Some(name) = entry.file_name().to_str() {
                    // Trial dir format: "taskname-sessionid-uuid"
                    // Extract task name by taking everything before the first "-bench-"
                    if let Some(idx) = name.find("-bench-") {
                        completed.insert(name[..idx].to_string());
                    }
                }
            }
        }

        Ok(completed)
    }

    /// Load existing trial results from the job directory.
    fn load_existing_results(&self) -> Vec<TrialResult> {
        let job_dir = self.job_dir();
        let result_path = job_dir.join("result.json");

        if !result_path.exists() {
            return Vec::new();
        }

        let content = std::fs::read_to_string(&result_path).unwrap_or_default();
        let results: BenchmarkResults = match serde_json::from_str(&content) {
            Ok(r) => r,
            Err(_) => return Vec::new(),
        };

        results.trials
    }
}
