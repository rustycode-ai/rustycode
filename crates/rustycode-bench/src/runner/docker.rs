//! Docker benchmark runner — executes tasks in containers via the Job pipeline.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result};

use crate::agent::BenchAgent;
use crate::job::{BenchmarkResults, Job, JobConfig};
use crate::runner::native::AgentFactory;
use crate::task::ResolvedTask;
use crate::trial::RetryConfig;
use crate::verifier::{ScriptVerifier, Verifier};

/// Configuration for the Docker benchmark runner.
#[derive(Debug, Clone)]
pub struct DockerRunnerConfig {
    pub agent_name: String,
    pub model: String,
    pub n_concurrent: usize,
    pub job_name: String,
    pub force_build: bool,
    pub cleanup: bool,
    pub retry_config: RetryConfig,
}

/// Runs benchmark tasks in Docker containers.
pub struct DockerRunner {
    config: DockerRunnerConfig,
}

impl DockerRunner {
    pub fn new(config: DockerRunnerConfig) -> Self {
        Self { config }
    }

    /// Run all tasks in Docker and return aggregated results.
    pub async fn run(
        &self,
        tasks: &[ResolvedTask],
        dataset_path: &Path,
        create_agent: AgentFactory,
    ) -> Result<BenchmarkResults> {
        let jobs_dir = dataset_path.join("_jobs");
        std::fs::create_dir_all(&jobs_dir)?;

        let job_config = JobConfig {
            job_name: self.config.job_name.clone(),
            jobs_dir,
            n_concurrent: self.config.n_concurrent,
            force_build: self.config.force_build,
            cleanup: self.config.cleanup,
        };

        let agent_name = self.config.agent_name.clone();
        let model = self.config.model.clone();
        let create_agent = Arc::new(create_agent);

        let agent_factory = move |solution_dir: PathBuf| -> Box<dyn BenchAgent> {
            create_agent(&agent_name, &model, solution_dir).unwrap_or_else(|e| {
                tracing::error!("Failed to create agent: {e}");
                Box::new(crate::agent::NopAgent) as Box<dyn BenchAgent>
            })
        };

        let verifier_factory = move |tests_dir: PathBuf, timeout_secs: u64| -> Box<dyn Verifier> {
            Box::new(ScriptVerifier::new(tests_dir, timeout_secs)) as Box<dyn Verifier>
        };

        let _ = &self.config.retry_config; // TODO: wire into Job once retry support lands

        tracing::info!("Running {} tasks in Docker mode...", tasks.len());
        let results = Job::new(job_config)
            .run(tasks, &agent_factory, &verifier_factory)
            .await
            .with_context(|| "Benchmark run failed")?;

        Ok(results)
    }
}
