//! Pass@k computation for benchmark evaluation.
//!
//! Standard formula: pass@k(n, c, k) = 1 - C(n-c, k) / C(n, k)
//! Simplified as: 1 - prod((n-c-i)/(n-i) for i in 0..k)

use std::collections::HashMap;

use crate::trial::TrialResult;

/// Fixed k values per spec.
const K_VALUES: [usize; 5] = [2, 5, 10, 20, 50];

/// Compute pass@k for a group of trial results.
///
/// Groups by `agent_name`, computes for each k where `k <= min_trials_per_task`.
/// Returns map of `agent_name -> {k -> pass@k value}`.
#[allow(clippy::cast_precision_loss)]
pub fn compute_pass_at_k(trials: &[TrialResult]) -> HashMap<String, HashMap<usize, f64>> {
    let mut groups: HashMap<&str, Vec<&TrialResult>> = HashMap::new();
    for trial in trials {
        groups.entry(&trial.agent_name).or_default().push(trial);
    }

    groups
        .into_iter()
        .filter_map(|(agent, agent_trials)| {
            let pass_at_k = compute_agent_pass_at_k(&agent_trials);
            if pass_at_k.is_empty() {
                None
            } else {
                Some((agent.to_string(), pass_at_k))
            }
        })
        .collect()
}

/// Compute pass@k for a single agent's trials.
fn compute_agent_pass_at_k(trials: &[&TrialResult]) -> HashMap<usize, f64> {
    let mut task_outcomes: HashMap<&str, Vec<bool>> = HashMap::new();
    for trial in trials {
        task_outcomes
            .entry(&trial.task_name)
            .or_default()
            .push(trial.passed());
    }

    if task_outcomes.is_empty() {
        return HashMap::new();
    }

    let min_trials = task_outcomes.values().map(|v| v.len()).min().unwrap_or(0);
    if min_trials < 2 {
        return HashMap::new();
    }

    let mut result = HashMap::new();
    for &k in &K_VALUES {
        if k > min_trials {
            continue;
        }
        let total: f64 = task_outcomes
            .values()
            .map(|outcomes| {
                let n = outcomes.len();
                let c = outcomes.iter().filter(|&&p| p).count();
                pass_at_k_for_task(n, c, k)
            })
            .sum();
        let avg = total / task_outcomes.len() as f64;
        result.insert(k, avg);
    }

    result
}

/// Compute pass@k for a single task.
fn pass_at_k_for_task(n: usize, c: usize, k: usize) -> f64 {
    if k > n {
        return 0.0;
    }
    if n - c < k {
        return 1.0;
    }
    let product: f64 = (0..k)
        .map(|i| (n - c - i) as f64 / (n - i) as f64)
        .product();
    1.0 - product
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn make_trial(task: &str, agent: &str, reward: f64, success: bool) -> TrialResult {
        TrialResult {
            task_name: task.to_string(),
            agent_name: agent.to_string(),
            reward,
            success,
            error: None,
            duration_secs: 10.0,
            trial_dir: PathBuf::from("/tmp"),
            input_tokens: 0,
            output_tokens: 0,
            cost_usd: 0.0,
        }
    }

    #[test]
    fn pass_at_k_all_pass() {
        let p = pass_at_k_for_task(5, 5, 2);
        assert!((p - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn pass_at_k_all_fail() {
        let p = pass_at_k_for_task(5, 0, 2);
        assert!(p.abs() < f64::EPSILON);
    }

    #[test]
    fn pass_at_k_half_pass() {
        let p = pass_at_k_for_task(10, 5, 2);
        assert!(p > 0.5);
        assert!(p <= 1.0);
    }

    #[test]
    fn pass_at_k_one_pass_out_of_many() {
        let p = pass_at_k_for_task(10, 1, 2);
        assert!((p - 0.2).abs() < 1e-10);
    }

    #[test]
    fn pass_at_k_fewer_remaining_than_k() {
        let p = pass_at_k_for_task(3, 2, 2);
        assert!((p - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn compute_pass_at_k_single_task() {
        let trials = vec![
            make_trial("task_a", "agent1", 1.0, true),
            make_trial("task_a", "agent1", 0.0, true),
        ];
        let result = compute_pass_at_k(&trials);
        let group = result.get("agent1").unwrap();
        assert!(group.contains_key(&2));
        assert!((group[&2] - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn compute_pass_at_k_multiple_tasks() {
        let trials = vec![
            make_trial("t1", "a", 1.0, true),
            make_trial("t1", "a", 1.0, true),
            make_trial("t2", "a", 0.0, true),
            make_trial("t2", "a", 0.0, true),
        ];
        let result = compute_pass_at_k(&trials);
        let group = result.get("a").unwrap();
        assert!((group[&2] - 0.5).abs() < 1e-10);
    }

    #[test]
    fn compute_pass_at_k_empty() {
        let result = compute_pass_at_k(&[]);
        assert!(result.is_empty());
    }

    #[test]
    fn compute_pass_at_k_only_k2_eligible() {
        let trials = vec![
            make_trial("t1", "a", 1.0, true),
            make_trial("t1", "a", 0.0, true),
        ];
        let result = compute_pass_at_k(&trials);
        let group = result.get("a").unwrap();
        assert!(group.contains_key(&2));
        assert!(!group.contains_key(&5));
    }

    #[test]
    fn compute_pass_at_k_multiple_agents() {
        let trials = vec![
            make_trial("t1", "agent_a", 1.0, true),
            make_trial("t1", "agent_a", 0.0, true),
            make_trial("t1", "agent_b", 0.0, true),
            make_trial("t1", "agent_b", 0.0, true),
        ];
        let result = compute_pass_at_k(&trials);
        assert_eq!(result.len(), 2);
        assert!(result.contains_key("agent_a"));
        assert!(result.contains_key("agent_b"));
    }
}
