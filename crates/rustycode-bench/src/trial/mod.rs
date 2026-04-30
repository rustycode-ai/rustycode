//! Trial lifecycle — orchestrates a single benchmark task run.

pub mod artifacts;

use std::path::{Path, PathBuf};
use std::time::Instant;

use serde::{Deserialize, Serialize};
use tracing;

use crate::agent::BenchAgent;
use crate::environment::docker::{DockerEnvironment, EnvironmentConfig, TrialPaths};
use crate::environment::BenchEnvironment;
use crate::task::ResolvedTask;
use crate::verifier::Verifier;

/// Configuration for retry behavior with exponential backoff.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryConfig {
    /// Maximum number of retries after the initial attempt.
    pub max_retries: usize,
    /// Minimum wait time in seconds before the first retry.
    pub min_wait_sec: f64,
    /// Maximum wait time in seconds (caps exponential growth).
    pub max_wait_sec: f64,
    /// Multiplier applied to the wait time after each retry.
    pub wait_multiplier: f64,
    /// Only retry if the error message matches one of these regex patterns.
    #[serde(default)]
    pub include_patterns: Vec<String>,
    /// Never retry if the error message matches one of these regex patterns.
    #[serde(default)]
    pub exclude_patterns: Vec<String>,
    /// Jitter factor (0.0 to 1.0) added to backoff delay.
    #[serde(default = "default_jitter")]
    pub jitter: f64,
}

fn default_jitter() -> f64 {
    0.1
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_retries: 2,
            min_wait_sec: 1.0,
            max_wait_sec: 60.0,
            wait_multiplier: 2.0,
            include_patterns: Vec::new(),
            exclude_patterns: Vec::new(),
            jitter: default_jitter(),
        }
    }
}

impl RetryConfig {
    /// Calculate the wait duration in seconds for the given attempt (0-indexed).
    ///
    /// Uses exponential backoff: `min_wait_sec * wait_multiplier^attempt`, capped at `max_wait_sec`.
    pub fn wait_duration_secs(&self, attempt: usize) -> f64 {
        let raw = self.min_wait_sec * self.wait_multiplier.powi(attempt as i32);
        raw.min(self.max_wait_sec)
    }

    /// Calculate wait duration with random jitter applied.
    /// Jitter range: `[delay * (1 - jitter), delay]`.
    pub fn wait_duration_secs_with_jitter(&self, attempt: usize) -> f64 {
        let base = self.wait_duration_secs(attempt);
        let jitter_range = base * self.jitter;
        base - jitter_range / 2.0
    }

    /// Determine whether an error should be retried based on include/exclude patterns.
    ///
    /// - Empty `include_patterns` means all errors are candidates for retry.
    /// - If `include_patterns` is non-empty, the error must match at least one.
    /// - If `exclude_patterns` matches, the error is never retried (takes precedence).
    pub fn should_retry(&self, error: &anyhow::Error) -> bool {
        let msg = error.to_string();

        // Exclude patterns take precedence
        for pattern in &self.exclude_patterns {
            if let Ok(re) = regex::Regex::new(pattern) {
                if re.is_match(&msg) {
                    return false;
                }
            }
        }

        // If include patterns are set, at least one must match
        if !self.include_patterns.is_empty() {
            let matches_include = self
                .include_patterns
                .iter()
                .any(|pattern| regex::Regex::new(pattern).is_ok_and(|re| re.is_match(&msg)));
            if !matches_include {
                return false;
            }
        }

        true
    }
}

/// Result of a single benchmark trial.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrialResult {
    /// Task name.
    pub task_name: String,
    /// Agent that was used.
    pub agent_name: String,
    /// Reward score (0.0 to 1.0).
    pub reward: f64,
    /// Whether the trial completed without infrastructure errors.
    pub success: bool,
    /// Error message if the trial failed.
    pub error: Option<String>,
    /// Duration of the trial in seconds.
    pub duration_secs: f64,
    /// Path to the trial output directory.
    pub trial_dir: PathBuf,
}

impl TrialResult {
    /// Whether the task passed (reward >= 0.5).
    pub fn passed(&self) -> bool {
        self.reward >= 0.5
    }
}

/// Orchestrates a single benchmark trial: environment → agent → verifier → cleanup.
pub struct Trial {
    /// Unique session ID for this trial.
    session_id: String,
    /// Root directory for all trial outputs.
    jobs_dir: PathBuf,
    /// Whether to force-build images (needed for aarch64).
    force_build: bool,
    /// Whether to delete containers/images after the trial.
    cleanup: bool,
    /// Retry configuration for failed trials.
    retry_config: RetryConfig,
}

impl Trial {
    #[must_use]
    #[allow(clippy::missing_const_for_fn)]
    pub fn new(session_id: String, jobs_dir: PathBuf) -> Self {
        Self {
            session_id,
            jobs_dir,
            force_build: true,
            cleanup: true,
            retry_config: RetryConfig::default(),
        }
    }

    #[allow(clippy::missing_const_for_fn)]
    pub fn with_force_build(mut self, force: bool) -> Self {
        self.force_build = force;
        self
    }

    #[allow(clippy::missing_const_for_fn)]
    pub fn with_cleanup(mut self, cleanup: bool) -> Self {
        self.cleanup = cleanup;
        self
    }

    #[allow(clippy::missing_const_for_fn)]
    pub fn with_retry_config(mut self, config: RetryConfig) -> Self {
        self.retry_config = config;
        self
    }

    /// Run a single trial for the given task.
    pub async fn run(
        &self,
        task: &ResolvedTask,
        agent: &mut dyn BenchAgent,
        verifier: &dyn Verifier,
    ) -> TrialResult {
        let start = Instant::now();
        let trial_name = format!("{}-{}", task.name, self.session_id);
        let trial_dir = self.jobs_dir.join(&trial_name);

        tracing::info!("Starting trial: {}", trial_name);

        let trial_result = self
            .run_inner(task, agent, verifier, &trial_name, &trial_dir)
            .await;

        let duration = start.elapsed().as_secs_f64();

        match trial_result {
            Ok(reward) => TrialResult {
                task_name: task.name.clone(),
                agent_name: agent.name().to_string(),
                reward,
                success: true,
                error: None,
                duration_secs: duration,
                trial_dir,
            },
            Err(e) => {
                tracing::error!("Trial {} failed: {}", trial_name, e);
                TrialResult {
                    task_name: task.name.clone(),
                    agent_name: agent.name().to_string(),
                    reward: 0.0,
                    success: false,
                    error: Some(e.to_string()),
                    duration_secs: duration,
                    trial_dir,
                }
            }
        }
    }

    /// Inner trial execution with full lifecycle management.
    async fn run_inner(
        &self,
        task: &ResolvedTask,
        agent: &mut dyn BenchAgent,
        verifier: &dyn Verifier,
        trial_name: &str,
        trial_dir: &Path,
    ) -> anyhow::Result<f64> {
        // Set up trial paths
        let trial_paths = TrialPaths::new(trial_dir.to_path_buf());
        trial_paths.create_dirs()?;

        // Save instruction to agent logs
        let instruction_path = trial_paths.agent_dir.join("instruction.md");
        std::fs::write(&instruction_path, &task.instruction)?;

        // Create environment config from task
        let env_config = EnvironmentConfig {
            environment_dir: task.environment_dir.clone(),
            cpus: task.config.environment.cpus,
            memory: task.config.environment.memory.clone(),
            docker_image: task.config.environment.docker_image.clone(),
            build_timeout_secs: task.config.environment.build_timeout_sec as u64,
        };

        let mut env = DockerEnvironment::new(trial_name.to_string(), env_config, trial_paths);

        // Start container
        env.start(self.force_build).await?;

        // Ensure container is stopped when we're done
        let result = self.execute_phases(task, agent, verifier, &mut env).await;

        // Always stop the container
        if let Err(e) = env.stop(self.cleanup).await {
            tracing::warn!("Container cleanup failed: {}", e);
        }

        result
    }

    /// Execute the three phases: agent setup, agent run, verification.
    async fn execute_phases(
        &self,
        task: &ResolvedTask,
        agent: &mut dyn BenchAgent,
        verifier: &dyn Verifier,
        env: &mut dyn BenchEnvironment,
    ) -> anyhow::Result<f64> {
        // Phase 1: Agent setup
        tracing::info!("[{}] Agent setup ({})...", task.name, agent.name());
        agent.setup(env).await?;

        // Phase 2: Agent execution
        tracing::info!("[{}] Agent run...", task.name);
        agent.run(&task.instruction, env).await?;

        // Phase 3: Verification
        tracing::info!("[{}] Verification...", task.name);
        let reward = verifier.verify(env).await?;

        tracing::info!(
            "[{}] Complete — reward: {} ({})",
            task.name,
            reward,
            if reward >= 0.5 { "PASS" } else { "FAIL" }
        );

        Ok(reward)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn trial_result_passed() {
        let result = TrialResult {
            task_name: "test".to_string(),
            agent_name: "oracle".to_string(),
            reward: 0.75,
            success: true,
            error: None,
            duration_secs: 5.0,
            trial_dir: PathBuf::from("/tmp"),
        };
        assert!(result.passed());
    }

    #[test]
    fn trial_result_failed() {
        let result = TrialResult {
            task_name: "test".to_string(),
            agent_name: "oracle".to_string(),
            reward: 0.25,
            success: true,
            error: None,
            duration_secs: 5.0,
            trial_dir: PathBuf::from("/tmp"),
        };
        assert!(!result.passed());
    }

    #[test]
    fn trial_result_infrastructure_error() {
        let result = TrialResult {
            task_name: "test".to_string(),
            agent_name: "code".to_string(),
            reward: 0.0,
            success: false,
            error: Some("container crashed".to_string()),
            duration_secs: 120.0,
            trial_dir: PathBuf::from("/tmp"),
        };
        assert!(!result.passed());
        assert!(!result.success);
        assert_eq!(result.error.as_deref(), Some("container crashed"));
    }

    #[test]
    fn trial_builder_defaults() {
        let trial = Trial::new("session-1".to_string(), PathBuf::from("/tmp/jobs"));
        // Trial created with defaults: force_build=true, cleanup=true
        assert_eq!(trial.session_id, "session-1");
        assert_eq!(trial.jobs_dir, PathBuf::from("/tmp/jobs"));
        assert!(trial.force_build);
        assert!(trial.cleanup);
    }

    #[test]
    fn trial_builder_with_options() {
        let trial = Trial::new("s2".to_string(), PathBuf::from("/tmp"))
            .with_force_build(false)
            .with_cleanup(false);
        assert!(!trial.force_build);
        assert!(!trial.cleanup);
    }

    #[test]
    fn trial_result_exact_threshold() {
        let result = TrialResult {
            task_name: "boundary".to_string(),
            agent_name: "oracle".to_string(),
            reward: 0.5,
            success: true,
            error: None,
            duration_secs: 1.0,
            trial_dir: PathBuf::from("/tmp"),
        };
        assert!(result.passed()); // >= 0.5
    }

    #[test]
    fn trial_result_serde_roundtrip() {
        let result = TrialResult {
            task_name: "serde".to_string(),
            agent_name: "agent".to_string(),
            reward: 0.88,
            success: true,
            error: None,
            duration_secs: 30.5,
            trial_dir: PathBuf::from("/tmp/serde"),
        };
        let json = serde_json::to_string(&result).unwrap();
        let back: TrialResult = serde_json::from_str(&json).unwrap();
        assert_eq!(back.task_name, "serde");
        assert!((back.reward - 0.88).abs() < f64::EPSILON);
        assert!(back.success);
        assert_eq!(back.duration_secs, 30.5);
    }

    #[test]
    fn retry_config_default_values() {
        let config = RetryConfig::default();
        assert_eq!(config.max_retries, 2);
        assert!((config.min_wait_sec - 1.0).abs() < f64::EPSILON);
        assert!((config.max_wait_sec - 60.0).abs() < f64::EPSILON);
        assert!((config.wait_multiplier - 2.0).abs() < f64::EPSILON);
    }

    #[test]
    fn retry_config_wait_duration_exponential() {
        let config = RetryConfig::default();
        // attempt 0: 1.0 * 2^0 = 1.0
        assert!((config.wait_duration_secs(0) - 1.0).abs() < f64::EPSILON);
        // attempt 1: 1.0 * 2^1 = 2.0
        assert!((config.wait_duration_secs(1) - 2.0).abs() < f64::EPSILON);
        // attempt 2: 1.0 * 2^2 = 4.0
        assert!((config.wait_duration_secs(2) - 4.0).abs() < f64::EPSILON);
    }

    #[test]
    fn retry_config_wait_duration_capped_at_max() {
        let config = RetryConfig {
            max_retries: 5,
            min_wait_sec: 1.0,
            max_wait_sec: 10.0,
            wait_multiplier: 3.0,
            ..Default::default()
        };
        // attempt 3: 1.0 * 3^3 = 27.0 → capped at 10.0
        assert!((config.wait_duration_secs(3) - 10.0).abs() < f64::EPSILON);
    }

    #[test]
    fn retry_config_serde_roundtrip() {
        let config = RetryConfig {
            max_retries: 5,
            min_wait_sec: 0.5,
            max_wait_sec: 120.0,
            wait_multiplier: 1.5,
            ..Default::default()
        };
        let json = serde_json::to_string(&config).unwrap();
        let back: RetryConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(back.max_retries, 5);
        assert!((back.min_wait_sec - 0.5).abs() < f64::EPSILON);
        assert!((back.max_wait_sec - 120.0).abs() < f64::EPSILON);
        assert!((back.wait_multiplier - 1.5).abs() < f64::EPSILON);
    }

    #[test]
    fn trial_builder_with_retry_config() {
        let config = RetryConfig {
            max_retries: 5,
            min_wait_sec: 2.0,
            max_wait_sec: 120.0,
            wait_multiplier: 3.0,
            ..Default::default()
        };
        let trial = Trial::new("s3".to_string(), PathBuf::from("/tmp")).with_retry_config(config);
        assert_eq!(trial.retry_config.max_retries, 5);
        assert!((trial.retry_config.min_wait_sec - 2.0).abs() < f64::EPSILON);
        assert!((trial.retry_config.max_wait_sec - 120.0).abs() < f64::EPSILON);
        assert!((trial.retry_config.wait_multiplier - 3.0).abs() < f64::EPSILON);
    }

    #[test]
    fn trial_builder_defaults_include_retry_config() {
        let trial = Trial::new("s4".to_string(), PathBuf::from("/tmp"));
        assert_eq!(trial.retry_config.max_retries, 2);
    }

    #[test]
    fn should_retry_empty_patterns_always_true() {
        let config = RetryConfig::default();
        let err = anyhow::anyhow!("some error");
        assert!(config.should_retry(&err));
    }

    #[test]
    fn should_retry_include_match() {
        let config = RetryConfig {
            include_patterns: vec!["timeout".to_string()],
            ..Default::default()
        };
        let err = anyhow::anyhow!("connection timeout after 30s");
        assert!(config.should_retry(&err));
    }

    #[test]
    fn should_retry_include_no_match() {
        let config = RetryConfig {
            include_patterns: vec!["timeout".to_string()],
            ..Default::default()
        };
        let err = anyhow::anyhow!("permission denied");
        assert!(!config.should_retry(&err));
    }

    #[test]
    fn should_retry_exclude_match() {
        let config = RetryConfig {
            exclude_patterns: vec!["permission denied".to_string()],
            ..Default::default()
        };
        let err = anyhow::anyhow!("permission denied");
        assert!(!config.should_retry(&err));
    }

    #[test]
    fn should_retry_exclude_overrides_include() {
        let config = RetryConfig {
            include_patterns: vec!["timeout".to_string()],
            exclude_patterns: vec!["network".to_string()],
            ..Default::default()
        };
        let err = anyhow::anyhow!("network timeout");
        // matches include but also matches exclude → exclude wins
        assert!(!config.should_retry(&err));
    }

    #[test]
    fn retry_config_jitter_default() {
        let config = RetryConfig::default();
        assert!((config.jitter - 0.1).abs() < f64::EPSILON);
    }

    #[test]
    fn retry_config_jitter_reduces_delay() {
        let config = RetryConfig {
            jitter: 0.5,
            ..Default::default()
        };
        let base = config.wait_duration_secs(0);
        let jittered = config.wait_duration_secs_with_jitter(0);
        assert!(jittered < base);
        assert!(jittered > 0.0);
    }

    #[test]
    fn retry_config_zero_jitter() {
        let config = RetryConfig {
            jitter: 0.0,
            ..Default::default()
        };
        let base = config.wait_duration_secs(0);
        let jittered = config.wait_duration_secs_with_jitter(0);
        assert!((base - jittered).abs() < f64::EPSILON);
    }
}
