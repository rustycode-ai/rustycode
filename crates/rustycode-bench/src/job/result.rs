//! Benchmark results aggregation and statistics.

use std::fmt::Write;

use serde::{Deserialize, Serialize};

use crate::trial::TrialResult;

/// Aggregated results from a benchmark job.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkResults {
    /// Total number of trials.
    pub total: usize,
    /// Number of trials that passed (reward >= 0.5).
    pub passed: usize,
    /// Number of trials that failed (reward < 0.5).
    pub failed: usize,
    /// Number of trials that had infrastructure errors.
    pub errors: usize,
    /// Overall accuracy (passed / total).
    pub accuracy: f64,
    /// Mean reward across all trials.
    pub mean_reward: f64,
    /// Individual trial results.
    pub trials: Vec<TrialResult>,
    /// Per-task breakdown.
    pub task_results: Vec<TaskResult>,
    /// Pass@k metrics by agent name.
    #[serde(default)]
    pub pass_at_k: std::collections::HashMap<String, std::collections::HashMap<usize, f64>>,
    /// Total input tokens across all trials.
    #[serde(default)]
    pub total_input_tokens: u64,
    /// Total output tokens across all trials.
    #[serde(default)]
    pub total_output_tokens: u64,
    /// Total estimated cost in USD.
    #[serde(default)]
    pub total_cost_usd: f64,
}

/// Aggregated result for a single task.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskResult {
    pub task_name: String,
    /// Agent used.
    pub agent_name: String,
    /// Reward score.
    pub reward: f64,
    /// Whether the task passed.
    pub passed: bool,
    /// Error message if any.
    pub error: Option<String>,
    /// Duration in seconds.
    pub duration_secs: f64,
    /// Input tokens consumed.
    #[serde(default)]
    pub input_tokens: u64,
    /// Output tokens generated.
    #[serde(default)]
    pub output_tokens: u64,
    /// Estimated cost in USD.
    #[serde(default)]
    pub cost_usd: f64,
}

impl BenchmarkResults {
    /// Create results from a list of trial results.
    #[allow(clippy::cast_precision_loss)]
    pub fn from_trials(trials: &[TrialResult]) -> Self {
        let total = trials.len();
        let passed = trials.iter().filter(|t| t.passed()).count();
        let failed = trials.iter().filter(|t| t.success && !t.passed()).count();
        let errors = trials.iter().filter(|t| !t.success).count();

        let accuracy = if total > 0 {
            passed as f64 / total as f64
        } else {
            0.0
        };

        let mean_reward = if total > 0 {
            trials.iter().map(|t| t.reward).sum::<f64>() / total as f64
        } else {
            0.0
        };

        let task_results = trials
            .iter()
            .map(|t| TaskResult {
                task_name: t.task_name.clone(),
                agent_name: t.agent_name.clone(),
                reward: t.reward,
                passed: t.passed(),
                error: t.error.clone(),
                duration_secs: t.duration_secs,
                input_tokens: t.input_tokens,
                output_tokens: t.output_tokens,
                cost_usd: t.cost_usd,
            })
            .collect();

        let pass_at_k = crate::verifier::pass_at_k::compute_pass_at_k(trials);

        let total_input_tokens = trials.iter().map(|t| t.input_tokens).sum();
        let total_output_tokens = trials.iter().map(|t| t.output_tokens).sum();
        let total_cost_usd = trials.iter().map(|t| t.cost_usd).sum();

        Self {
            total,
            passed,
            failed,
            errors,
            accuracy,
            mean_reward,
            trials: trials.to_vec(),
            task_results,
            pass_at_k,
            total_input_tokens,
            total_output_tokens,
            total_cost_usd,
        }
    }

    /// Human-readable summary of results.
    pub fn summary(&self) -> String {
        let mut s = format!(
            "Benchmark Results: {}/{} passed ({:.1}%)",
            self.passed,
            self.total,
            self.accuracy * 100.0
        );
        if self.errors > 0 {
            let _ = write!(s, ", {} failed, {} errors", self.failed, self.errors);
        } else if self.failed > 0 {
            let _ = write!(s, ", {} failed", self.failed);
        }
        let _ = write!(s, "\nMean reward: {:.3}", self.mean_reward);

        if !self.pass_at_k.is_empty() {
            let _ = write!(s, "\n\nPass@k:");
            for (agent, metrics) in &self.pass_at_k {
                let _ = write!(s, "\n  {agent}:");
                for (k, v) in metrics {
                    let _ = write!(s, " @{k}={v:.2}");
                }
            }
        }

        // Show failed tasks
        let failed_tasks: Vec<&TaskResult> =
            self.task_results.iter().filter(|t| !t.passed).collect();

        if !failed_tasks.is_empty() {
            s.push_str("\n\nFailed tasks:");
            for task in failed_tasks {
                let reason = task.error.as_deref().unwrap_or("reward < 0.5");
                let _ = write!(s, "\n  - {} ({})", task.task_name, reason);
            }
        }

        s
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn make_trial(task: &str, reward: f64, success: bool) -> TrialResult {
        TrialResult {
            task_name: task.to_string(),
            agent_name: "test-agent".to_string(),
            reward,
            success,
            error: if success {
                None
            } else {
                Some("failed".to_string())
            },
            duration_secs: 10.0,
            trial_dir: PathBuf::from("/tmp/trial"),
            input_tokens: 0,
            output_tokens: 0,
            cost_usd: 0.0,
        }
    }

    #[test]
    fn trial_result_passed_threshold() {
        let t = make_trial("task1", 0.7, true);
        assert!(t.passed());
    }

    #[test]
    fn trial_result_not_passed_below_threshold() {
        let t = make_trial("task1", 0.3, true);
        assert!(!t.passed());
    }

    #[test]
    fn trial_result_passed_exact_threshold() {
        let t = make_trial("task1", 0.5, true);
        assert!(t.passed());
    }

    #[test]
    fn benchmark_results_from_empty_trials() {
        let results = BenchmarkResults::from_trials(&[]);
        assert_eq!(results.total, 0);
        assert_eq!(results.passed, 0);
        assert_eq!(results.failed, 0);
        assert_eq!(results.errors, 0);
        assert_eq!(results.accuracy, 0.0);
        assert_eq!(results.mean_reward, 0.0);
    }

    #[test]
    fn benchmark_results_from_mixed_trials() {
        let trials = vec![
            make_trial("task1", 0.9, true),
            make_trial("task2", 0.3, true),
            make_trial("task3", 0.0, false),
        ];
        let results = BenchmarkResults::from_trials(&trials);
        assert_eq!(results.total, 3);
        assert_eq!(results.passed, 1);
        assert_eq!(results.failed, 1);
        assert_eq!(results.errors, 1);
        assert!((results.accuracy - 1.0 / 3.0).abs() < 0.01);
        assert!((results.mean_reward - 0.4).abs() < 0.01);
    }

    #[test]
    fn benchmark_results_all_passed() {
        let trials = vec![make_trial("a", 0.8, true), make_trial("b", 1.0, true)];
        let results = BenchmarkResults::from_trials(&trials);
        assert_eq!(results.passed, 2);
        assert_eq!(results.failed, 0);
        assert_eq!(results.errors, 0);
        assert!((results.accuracy - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn benchmark_results_summary_format() {
        let trials = vec![
            make_trial("pass-task", 0.9, true),
            make_trial("fail-task", 0.1, true),
        ];
        let results = BenchmarkResults::from_trials(&trials);
        let summary = results.summary();
        assert!(summary.contains("1/2 passed"));
        assert!(summary.contains("50.0%"));
        assert!(summary.contains("Failed tasks"));
        assert!(summary.contains("fail-task"));
    }

    #[test]
    fn benchmark_results_summary_with_errors() {
        let trials = vec![make_trial("ok", 0.8, true), make_trial("err", 0.0, false)];
        let results = BenchmarkResults::from_trials(&trials);
        let summary = results.summary();
        assert!(summary.contains("errors"));
    }

    #[test]
    fn benchmark_results_serde_roundtrip() {
        let trials = vec![
            make_trial("task1", 0.75, true),
            make_trial("task2", 0.25, false),
        ];
        let results = BenchmarkResults::from_trials(&trials);
        let json = serde_json::to_string(&results).unwrap();
        let back: BenchmarkResults = serde_json::from_str(&json).unwrap();
        assert_eq!(back.total, 2);
        assert_eq!(back.passed, 1);
        assert_eq!(back.task_results.len(), 2);
    }

    #[test]
    fn task_result_fields() {
        let tr = TaskResult {
            task_name: "my-task".to_string(),
            agent_name: "oracle".to_string(),
            reward: 0.85,
            passed: true,
            error: None,
            duration_secs: 42.5,
            input_tokens: 0,
            output_tokens: 0,
            cost_usd: 0.0,
        };
        assert_eq!(tr.task_name, "my-task");
        assert!(tr.passed);
        assert!(tr.error.is_none());
    }

    #[test]
    fn job_config_serde_roundtrip() {
        use crate::job::JobConfig;
        let config = JobConfig {
            job_name: "test-job".to_string(),
            jobs_dir: PathBuf::from("/tmp/jobs"),
            n_concurrent: 4,
            force_build: false,
            cleanup: true,
        };
        let json = serde_json::to_string(&config).unwrap();
        let back: JobConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(back.job_name, "test-job");
        assert_eq!(back.n_concurrent, 4);
        assert!(!back.force_build);
        assert!(back.cleanup);
    }

    #[test]
    fn trial_result_serde_roundtrip() {
        let tr = make_trial("serde-test", 0.6, true);
        let json = serde_json::to_string(&tr).unwrap();
        let back: TrialResult = serde_json::from_str(&json).unwrap();
        assert_eq!(back.task_name, "serde-test");
        assert!((back.reward - 0.6).abs() < f64::EPSILON);
        assert!(back.success);
    }
}
