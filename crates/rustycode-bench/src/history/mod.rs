//! Result history — persist, list, and diff benchmark runs.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use chrono::Utc;
use serde::{Deserialize, Serialize};

use crate::config::BenchConfig;
use crate::job::BenchmarkResults;

/// A saved benchmark result with metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoricalResult {
    /// Unique run ID.
    pub run_id: String,
    /// Human-readable job name.
    pub job_name: String,
    /// ISO 8601 timestamp when the run started.
    pub timestamp: String,
    /// Configuration used for this run.
    pub config: BenchConfig,
    /// The benchmark results.
    pub results: BenchmarkResults,
    /// Git commit of the dataset (if available).
    pub dataset_commit: Option<String>,
    /// Git commit of rtk-bench itself.
    pub bench_commit: Option<String>,
}

/// Diff between two benchmark runs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunDiff {
    /// Baseline run ID.
    pub baseline_run: String,
    /// Comparison run ID.
    pub comparison_run: String,
    /// Change in overall accuracy (-1.0 to 1.0).
    pub accuracy_delta: f64,
    /// Change in mean reward.
    pub mean_reward_delta: f64,
    /// Tasks that improved (reward increased).
    pub improved: Vec<TaskDiff>,
    /// Tasks that regressed (reward decreased).
    pub regressed: Vec<TaskDiff>,
    /// Tasks unchanged.
    pub unchanged: Vec<String>,
    /// Tasks only in the comparison run.
    pub new_tasks: Vec<String>,
    /// Tasks only in the baseline run.
    pub removed_tasks: Vec<String>,
}

/// Per-task diff between two runs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskDiff {
    pub task_name: String,
    pub baseline_reward: f64,
    pub comparison_reward: f64,
    pub delta: f64,
}

/// Manages result persistence in a `_jobs/` directory.
pub struct HistoryStore {
    base_dir: PathBuf,
}

impl HistoryStore {
    /// Create a store rooted at the given directory.
    pub fn new(base_dir: PathBuf) -> Self {
        Self { base_dir }
    }

    /// Save a historical result.
    pub fn save(&self, result: &HistoricalResult) -> Result<()> {
        let dir = self.base_dir.join("_history").join(&result.run_id);
        std::fs::create_dir_all(&dir)?;
        let path = dir.join("result.json");
        let json = serde_json::to_string_pretty(result)?;
        std::fs::write(&path, json)?;
        Ok(())
    }

    /// List all saved results, ordered by timestamp (newest first).
    pub fn list(&self) -> Result<Vec<HistoricalResult>> {
        let history_dir = self.base_dir.join("_history");
        if !history_dir.exists() {
            return Ok(Vec::new());
        }

        let mut results = Vec::new();
        for entry in std::fs::read_dir(&history_dir)? {
            let entry = entry?;
            if entry.file_type()?.is_dir() {
                let path = entry.path().join("result.json");
                if path.exists() {
                    if let Ok(r) = Self::load_from_path(&path) {
                        results.push(r);
                    }
                }
            }
        }

        results.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
        Ok(results)
    }

    /// Load a specific result by run ID.
    pub fn load(&self, run_id: &str) -> Result<HistoricalResult> {
        let path = self
            .base_dir
            .join("_history")
            .join(run_id)
            .join("result.json");
        Self::load_from_path(&path).with_context(|| format!("Run '{run_id}' not found"))
    }

    /// Get the most recent result.
    pub fn latest(&self) -> Result<HistoricalResult> {
        let results = self.list()?;
        results
            .into_iter()
            .next()
            .context("No historical results found")
    }

    /// Compute the diff between two runs.
    pub fn diff(&self, baseline_id: &str, comparison_id: &str) -> Result<RunDiff> {
        let baseline = self.load(baseline_id)?;
        let comparison = self.load(comparison_id)?;

        let accuracy_delta = comparison.results.accuracy - baseline.results.accuracy;
        let mean_reward_delta = comparison.results.mean_reward - baseline.results.mean_reward;

        let baseline_tasks: std::collections::HashMap<&str, f64> = baseline
            .results
            .task_results
            .iter()
            .map(|t| (t.task_name.as_str(), t.reward))
            .collect();

        let comparison_tasks: std::collections::HashMap<&str, f64> = comparison
            .results
            .task_results
            .iter()
            .map(|t| (t.task_name.as_str(), t.reward))
            .collect();

        let baseline_names: HashSet<&str> = baseline_tasks.keys().copied().collect();
        let comparison_names: HashSet<&str> = comparison_tasks.keys().copied().collect();

        let new_tasks: Vec<String> = comparison_names
            .difference(&baseline_names)
            .map(|s| s.to_string())
            .collect();

        let removed_tasks: Vec<String> = baseline_names
            .difference(&comparison_names)
            .map(|s| s.to_string())
            .collect();

        let common_names: Vec<&str> = baseline_names
            .intersection(&comparison_names)
            .copied()
            .collect();

        let mut improved = Vec::new();
        let mut regressed = Vec::new();
        let mut unchanged = Vec::new();

        for name in common_names {
            let b = baseline_tasks[name];
            let c = comparison_tasks[name];
            let delta = c - b;
            let task_diff = TaskDiff {
                task_name: name.to_string(),
                baseline_reward: b,
                comparison_reward: c,
                delta,
            };
            if delta > 0.001 {
                improved.push(task_diff);
            } else if delta < -0.001 {
                regressed.push(task_diff);
            } else {
                unchanged.push((*name).to_string());
            }
        }

        improved.sort_by(|a, b| {
            b.delta
                .partial_cmp(&a.delta)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        regressed.sort_by(|a, b| {
            a.delta
                .partial_cmp(&b.delta)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        Ok(RunDiff {
            baseline_run: baseline_id.to_string(),
            comparison_run: comparison_id.to_string(),
            accuracy_delta,
            mean_reward_delta,
            improved,
            regressed,
            unchanged,
            new_tasks,
            removed_tasks,
        })
    }

    fn load_from_path(path: &Path) -> Result<HistoricalResult> {
        let content = std::fs::read_to_string(path)?;
        Ok(serde_json::from_str(&content)?)
    }
}

/// Generate a new run ID from the current timestamp.
pub fn generate_run_id() -> String {
    format!("run-{}", Utc::now().format("%Y%m%d-%H%M%S"))
}

/// Format a run diff for terminal output.
pub fn format_diff(diff: &RunDiff) -> String {
    let mut out = String::new();

    out.push_str(&format!(
        "=== Diff: {} vs {} ===\n\n",
        diff.baseline_run, diff.comparison_run
    ));

    let acc_arrow = if diff.accuracy_delta > 0.0 { "+" } else { "" };
    let reward_arrow = if diff.mean_reward_delta > 0.0 {
        "+"
    } else {
        ""
    };

    out.push_str(&format!(
        "Accuracy:  {acc_arrow}{:.1}%  ({:.3})\n",
        diff.accuracy_delta * 100.0,
        diff.accuracy_delta
    ));
    out.push_str(&format!(
        "Mean reward: {reward_arrow}{:.3}\n\n",
        diff.mean_reward_delta
    ));

    if !diff.improved.is_empty() {
        out.push_str(&format!("Improved ({}):\n", diff.improved.len()));
        for t in &diff.improved {
            out.push_str(&format!(
                "  + {} ({:.2} -> {:.2}, +{:.2})\n",
                t.task_name, t.baseline_reward, t.comparison_reward, t.delta
            ));
        }
        out.push('\n');
    }

    if !diff.regressed.is_empty() {
        out.push_str(&format!("Regressed ({}):\n", diff.regressed.len()));
        for t in &diff.regressed {
            out.push_str(&format!(
                "  - {} ({:.2} -> {:.2}, {:.2})\n",
                t.task_name, t.baseline_reward, t.comparison_reward, t.delta
            ));
        }
        out.push('\n');
    }

    if !diff.new_tasks.is_empty() {
        out.push_str(&format!("New tasks: {}\n", diff.new_tasks.join(", ")));
    }

    if !diff.removed_tasks.is_empty() {
        out.push_str(&format!(
            "Removed tasks: {}\n",
            diff.removed_tasks.join(", ")
        ));
    }

    out.push_str(&format!(
        "\nUnchanged: {}/{} tasks\n",
        diff.unchanged.len(),
        diff.improved.len() + diff.regressed.len() + diff.unchanged.len()
    ));

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::job::TaskResult;

    fn make_result(total: usize, passed: usize, tasks: Vec<(&str, f64)>) -> BenchmarkResults {
        let accuracy = if total > 0 {
            passed as f64 / total as f64
        } else {
            0.0
        };
        let mean_reward = if tasks.is_empty() {
            0.0
        } else {
            tasks.iter().map(|(_, r)| r).sum::<f64>() / tasks.len() as f64
        };
        let task_results: Vec<TaskResult> = tasks
            .into_iter()
            .map(|(name, reward)| TaskResult {
                task_name: name.to_string(),
                agent_name: "oracle".to_string(),
                reward,
                passed: reward >= 0.5,
                error: None,
                duration_secs: 1.0,
            })
            .collect();
        BenchmarkResults {
            total,
            passed,
            failed: total.saturating_sub(passed),
            errors: 0,
            accuracy,
            mean_reward,
            trials: Vec::new(),
            task_results,
            pass_at_k: std::collections::HashMap::new(),
        }
    }

    fn make_historical(run_id: &str, results: BenchmarkResults) -> HistoricalResult {
        HistoricalResult {
            run_id: run_id.to_string(),
            job_name: "test".to_string(),
            timestamp: format!("2026-04-25T12:00:00Z-{}", run_id),
            config: BenchConfig::default(),
            results,
            dataset_commit: None,
            bench_commit: None,
        }
    }

    #[test]
    fn history_store_save_list_load() {
        let dir = tempfile::tempdir().unwrap();
        let store = HistoryStore::new(dir.path().to_path_buf());

        let r1 = make_historical(
            "run-1",
            make_result(3, 2, vec![("a", 1.0), ("b", 0.0), ("c", 1.0)]),
        );
        let r2 = make_historical(
            "run-2",
            make_result(3, 3, vec![("a", 1.0), ("b", 1.0), ("c", 1.0)]),
        );

        store.save(&r1).unwrap();
        store.save(&r2).unwrap();

        let list = store.list().unwrap();
        assert_eq!(list.len(), 2);

        let loaded = store.load("run-1").unwrap();
        assert_eq!(loaded.run_id, "run-1");
        assert_eq!(loaded.results.total, 3);
    }

    #[test]
    fn history_store_diff_improved_and_regressed() {
        let dir = tempfile::tempdir().unwrap();
        let store = HistoryStore::new(dir.path().to_path_buf());

        let baseline = make_historical(
            "base",
            make_result(3, 1, vec![("a", 1.0), ("b", 0.0), ("c", 0.0)]),
        );
        let comparison = make_historical(
            "comp",
            make_result(4, 3, vec![("a", 1.0), ("b", 1.0), ("c", 0.5), ("d", 1.0)]),
        );

        store.save(&baseline).unwrap();
        store.save(&comparison).unwrap();

        let diff = store.diff("base", "comp").unwrap();

        assert!(diff.accuracy_delta > 0.0);
        assert!(diff.mean_reward_delta > 0.0);
        assert_eq!(diff.improved.len(), 2); // b and c improved
        assert!(diff.regressed.is_empty());
        assert_eq!(diff.unchanged.len(), 1); // a unchanged
        assert_eq!(diff.new_tasks.len(), 1); // d is new
        assert!(diff.removed_tasks.is_empty());
    }

    #[test]
    fn history_store_diff_regressed() {
        let dir = tempfile::tempdir().unwrap();
        let store = HistoryStore::new(dir.path().to_path_buf());

        let baseline = make_historical("base", make_result(2, 2, vec![("a", 1.0), ("b", 1.0)]));
        let comparison = make_historical("comp", make_result(2, 0, vec![("a", 0.0), ("b", 0.0)]));

        store.save(&baseline).unwrap();
        store.save(&comparison).unwrap();

        let diff = store.diff("base", "comp").unwrap();

        assert!(diff.accuracy_delta < 0.0);
        assert_eq!(diff.regressed.len(), 2);
        assert!(diff.improved.is_empty());
    }

    #[test]
    fn format_diff_output() {
        let diff = RunDiff {
            baseline_run: "run-1".to_string(),
            comparison_run: "run-2".to_string(),
            accuracy_delta: 0.1,
            mean_reward_delta: 0.05,
            improved: vec![TaskDiff {
                task_name: "task-a".to_string(),
                baseline_reward: 0.0,
                comparison_reward: 1.0,
                delta: 1.0,
            }],
            regressed: vec![],
            unchanged: vec!["task-b".to_string()],
            new_tasks: vec!["task-c".to_string()],
            removed_tasks: vec![],
        };

        let output = format_diff(&diff);
        assert!(output.contains("Improved (1)"));
        assert!(output.contains("task-a"));
        assert!(output.contains("task-c"));
        assert!(output.contains("Unchanged: 1/2 tasks"));
    }

    #[test]
    fn generate_run_id_format() {
        let id = generate_run_id();
        assert!(id.starts_with("run-"));
        assert!(id.len() > "run-".len());
    }
}
