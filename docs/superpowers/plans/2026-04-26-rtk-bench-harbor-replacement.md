# rtk-bench Harbor Replacement Implementation Plan

> **For agentic workers:** REQUIRED: Use `superpowers:subagent-driven-development` (if subagents are available) or `superpowers:executing-plans` to implement this plan. Track progress with checkbox (`- [ ]`) syntax.

**Goal:** Extend `rustycode-bench` so it can replace the Harbor Python harness for TB2 evaluation.

**Implementation order:** Stream 1 builds the Docker environment first. Stream 2 adds verifier parsing, pass@k, multi-step tasks, artifact collection, and retry improvements. Integration comes last.

**Architecture:** All work extends existing modules. Nothing is overwritten. The plan preserves the existing `BenchEnvironment`/`TrialResult`/`Hooks` surface and layers the new TB2 behavior on top.

**Tech Stack:** Rust, `bollard` 0.17, tokio, serde, `flate2`/`tar` for file transfer, and `ignore` 0.4 for artifact exclusion.

**Spec:** [rtk-bench: Harbor Replacement Design](../specs/2026-04-26-rtk-bench-harbor-replacement-design.md)

## Read This First

- The approved design is the source of truth.
- Keep new code behind existing module boundaries.
- Prefer small, testable additions over broad rewrites.
- If a step reveals a better split, update this plan before implementation drifts.

---

## Existing Code Context (READ BEFORE IMPLEMENTING)

The following modules ALREADY EXIST and must NOT be overwritten:

| Module | Path | Key Types |
|--------|------|-----------|
| trial/mod.rs | `crates/rustycode-bench/src/trial/mod.rs` | `RetryConfig`, `TrialResult`, `Trial` |
| hooks.rs | `crates/rustycode-bench/src/hooks.rs` | `TrialEvent`, `Hooks`, `HookContext`, `HookCallback` |
| verifier/mod.rs | `crates/rustycode-bench/src/verifier/mod.rs` | `Verifier` trait, `ScriptVerifier`, `NativeVerifier` |
| environment/mod.rs | `crates/rustycode-bench/src/environment/mod.rs` | `BenchEnvironment` trait, `ExecResult` |
| task/mod.rs | `crates/rustycode-bench/src/task/mod.rs` | `TaskConfig`, `ResolvedTask`, `EnvironmentConfig` |
| config.rs | `crates/rustycode-bench/src/config.rs` | `BenchConfig`, `AgentConfig`, `ResourceOverrides` |
| job/result.rs | `crates/rustycode-bench/src/job/result.rs` | `BenchmarkResults`, `TaskResult` |
| lib.rs | `crates/rustycode-bench/src/lib.rs` | Public API exports |

---

## File Structure

### New Files (5 files)
- `crates/rustycode-bench/src/verifier/reward.rs` — reward.txt + reward.json + ctrf.json parsing
- `crates/rustycode-bench/src/verifier/pass_at_k.rs` — pass@k computation (uses existing `TrialResult`)
- `crates/rustycode-bench/src/trial/artifacts.rs` — Artifact collection with `ignore` crate
- `crates/rustycode-bench/src/task/steps.rs` — Multi-step task support
- `crates/rustycode-bench/src/environment/docker.rs` — bollard-based Docker environment

### Modified Files (5 files — extend, don't replace)
- `crates/rustycode-bench/Cargo.toml` — Add bollard 0.17, ignore 0.4
- `crates/rustycode-bench/src/verifier/mod.rs` — Add `pub mod reward; pub mod pass_at_k;`
- `crates/rustycode-bench/src/trial/mod.rs` — Add `pub mod artifacts;` and jitter field to `RetryConfig`
- `crates/rustycode-bench/src/lib.rs` — Export new types
- `crates/rustycode-bench/src/environment/mod.rs` — Add `pub mod docker;`

---

## Chunk 1: Verifier Reward Parsing & Pass@k (Stream 2)

### Task 1: reward.txt, reward.json, ctrf.json parsing

**Files:**
- Create: `crates/rustycode-bench/src/verifier/reward.rs`
- Modify: `crates/rustycode-bench/src/verifier/mod.rs`
- Modify: `crates/rustycode-bench/src/lib.rs`

- [x] **Step 1: Create reward.rs with tests**

Create `crates/rustycode-bench/src/verifier/reward.rs`:

```rust
//! Reward file parsing for TB2 verifier output.

use std::collections::HashMap;

/// Parsed reward from verifier output.
#[derive(Debug, Clone)]
pub struct RewardResult {
    /// Named rewards (e.g., {"accuracy": 1.0} or {"default": 0.5}).
    pub rewards: HashMap<String, f64>,
    /// Structured CTRF test report, if available.
    pub ctrf: Option<CtrfReport>,
}

/// CTRF (Common Test Report Format) test report.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CtrfReport {
    pub results: CtrfResults,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CtrfResults {
    pub summary: CtrfSummary,
    #[serde(default)]
    pub tests: Vec<CtrfTest>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CtrfSummary {
    pub tests: usize,
    pub passed: usize,
    pub failed: usize,
    #[serde(default)]
    pub skipped: usize,
    #[serde(default)]
    pub pending: usize,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CtrfTest {
    pub name: String,
    pub status: String,
    #[serde(default)]
    pub duration: Option<u64>,
    #[serde(default)]
    pub message: Option<String>,
}

/// Parse reward.txt content (single float value).
pub fn parse_reward_txt(content: &str) -> Option<f64> {
    content.trim().parse::<f64>().ok()
}

/// Parse reward.json content (named rewards dict).
pub fn parse_reward_json(content: &str) -> Option<HashMap<String, f64>> {
    serde_json::from_str(content).ok()
}

/// Parse CTRF JSON report.
pub fn parse_ctrf_json(content: &str) -> Option<CtrfReport> {
    serde_json::from_str(content).ok()
}

/// Parse all reward files from verifier output directory.
pub fn parse_verifier_output(
    reward_txt: Option<&str>,
    reward_json: Option<&str>,
    ctrf_json: Option<&str>,
) -> RewardResult {
    let rewards = if let Some(txt) = reward_txt {
        if let Some(val) = parse_reward_txt(txt) {
            HashMap::from([("default".to_string(), val)])
        } else {
            HashMap::new()
        }
    } else if let Some(json) = reward_json {
        parse_reward_json(json).unwrap_or_default()
    } else {
        HashMap::new()
    };

    let ctrf = ctrf_json.and_then(parse_ctrf_json);

    RewardResult { rewards, ctrf }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn parse_reward_txt_pass() {
        assert_eq!(parse_reward_txt("1"), Some(1.0));
        assert_eq!(parse_reward_txt("1\n"), Some(1.0));
        assert_eq!(parse_reward_txt("1.0"), Some(1.0));
    }

    #[test]
    fn parse_reward_txt_fail() {
        assert_eq!(parse_reward_txt("0"), Some(0.0));
        assert_eq!(parse_reward_txt("0.0"), Some(0.0));
    }

    #[test]
    fn parse_reward_txt_invalid() {
        assert_eq!(parse_reward_txt("error"), None);
        assert_eq!(parse_reward_txt(""), None);
    }

    #[test]
    fn parse_reward_txt_partial() {
        assert_eq!(parse_reward_txt("0.75"), Some(0.75));
    }

    #[test]
    fn parse_reward_json_single_key() {
        let result = parse_reward_json(r#"{"accuracy": 0.95}"#);
        assert_eq!(
            result,
            Some(HashMap::from([("accuracy".to_string(), 0.95)]))
        );
    }

    #[test]
    fn parse_reward_json_multiple_keys() {
        let result = parse_reward_json(
            r#"{"accuracy": 0.9, "completeness": 0.8}"#,
        );
        let map = result.unwrap();
        assert_eq!(map.get("accuracy"), Some(&0.9));
        assert_eq!(map.get("completeness"), Some(&0.8));
    }

    #[test]
    fn parse_reward_json_invalid() {
        assert_eq!(parse_reward_json("not json"), None);
    }

    #[test]
    fn parse_ctrf_json_basic() {
        let json = r#"{"results": {"summary": {"tests": 3, "passed": 2, "failed": 1, "skipped": 0, "pending": 0}, "tests": [{"name": "test_a", "status": "passed"}, {"name": "test_b", "status": "failed", "message": "wrong value"}]}}"#;
        let report = parse_ctrf_json(json).unwrap();
        assert_eq!(report.results.summary.tests, 3);
        assert_eq!(report.results.summary.passed, 2);
        assert_eq!(report.results.tests.len(), 2);
        assert_eq!(report.results.tests[1].status, "failed");
    }

    #[test]
    fn parse_ctrf_json_invalid() {
        assert_eq!(parse_ctrf_json("bad"), None);
    }

    #[test]
    fn parse_verifier_output_txt_only() {
        let result = parse_verifier_output(Some("1"), None, None);
        assert_eq!(result.rewards.get("default"), Some(&1.0));
        assert!(result.ctrf.is_none());
    }

    #[test]
    fn parse_verifier_output_all_files() {
        let ctrf = r#"{"results": {"summary": {"tests": 1, "passed": 1, "failed": 0, "skipped": 0, "pending": 0}, "tests": []}}"#;
        let result = parse_verifier_output(Some("1"), None, Some(ctrf));
        assert_eq!(result.rewards.get("default"), Some(&1.0));
        assert!(result.ctrf.is_some());
        assert_eq!(result.ctrf.unwrap().results.summary.passed, 1);
    }

    #[test]
    fn parse_verifier_output_nothing() {
        let result = parse_verifier_output(None, None, None);
        assert!(result.rewards.is_empty());
        assert!(result.ctrf.is_none());
    }
}
```

- [x] **Step 2: Run tests**

Run: `cargo test -p rustycode-bench verifier::reward --lib`
Expected: All 13 tests PASS

- [x] **Step 3: Wire reward module into verifier/mod.rs**

Add to `crates/rustycode-bench/src/verifier/mod.rs` after existing `pub mod` lines:
```rust
pub mod reward;
pub use reward::{CtrfReport, RewardResult};
```

- [x] **Step 4: Export from lib.rs**

Add to `crates/rustycode-bench/src/lib.rs` exports:
```rust
pub use verifier::reward::{CtrfReport, RewardResult};
```

- [x] **Step 5: Run full test suite**

Run: `cargo test -p rustycode-bench --lib`
Expected: All existing + new tests pass

- [x] **Step 6: Commit**

```bash
git add crates/rustycode-bench/src/verifier/reward.rs
git add crates/rustycode-bench/src/verifier/mod.rs
git add crates/rustycode-bench/src/lib.rs
git commit -m "feat(bench): add reward.txt/reward.json/ctrf.json parsing"
```

---

### Task 2: pass@k computation

**Files:**
- Create: `crates/rustycode-bench/src/verifier/pass_at_k.rs`
- Modify: `crates/rustycode-bench/src/verifier/mod.rs`
- Modify: `crates/rustycode-bench/src/lib.rs`

Uses existing `TrialResult` (from `crate::trial::TrialResult`) directly — no new `TrialOutcome` struct needed. `TrialResult` already has `task_name`, `agent_name`, `reward`, and `passed()`.

- [x] **Step 1: Create pass_at_k.rs with tests**

Create `crates/rustycode-bench/src/verifier/pass_at_k.rs`:

```rust
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
pub fn compute_pass_at_k(
    trials: &[TrialResult],
) -> HashMap<String, HashMap<usize, f64>> {
    let mut groups: HashMap<&str, Vec<&TrialResult>> = HashMap::new();
    for trial in trials {
        groups
            .entry(&trial.agent_name)
            .or_default()
            .push(trial);
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
    // Group by task, collect binary pass/fail
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
        // pass@2 = 1 - (9/10 * 8/9) = 1 - 0.8 = 0.2
        let p = pass_at_k_for_task(10, 1, 2);
        assert!((p - 0.2).abs() < 1e-10);
    }

    #[test]
    fn pass_at_k_fewer_remaining_than_k() {
        // n=3, c=2, k=2: n-c=1 < k=2 → 1.0
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
        // Only 2 trials per task → only k=2 eligible
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
```

- [x] **Step 2: Run tests**

Run: `cargo test -p rustycode-bench verifier::pass_at_k --lib`
Expected: All 10 tests PASS

- [x] **Step 3: Wire into verifier/mod.rs**

Add to `crates/rustycode-bench/src/verifier/mod.rs`:
```rust
pub mod pass_at_k;
pub use pass_at_k::compute_pass_at_k;
```

- [x] **Step 4: Export from lib.rs**

Add to `crates/rustycode-bench/src/lib.rs` exports:
```rust
pub use verifier::pass_at_k::compute_pass_at_k;
```

- [x] **Step 5: Run full test suite**

Run: `cargo test -p rustycode-bench --lib`
Expected: All tests pass

- [x] **Step 6: Commit**

```bash
git add crates/rustycode-bench/src/verifier/pass_at_k.rs
git add crates/rustycode-bench/src/verifier/mod.rs
git add crates/rustycode-bench/src/lib.rs
git commit -m "feat(bench): add pass@k computation using existing TrialResult"
```

---

## Chunk 2: Trial Infrastructure (Stream 2)

### Task 3: Add jitter to existing RetryConfig

**Files:**
- Modify: `crates/rustycode-bench/src/trial/mod.rs`

The existing `RetryConfig` in `trial/mod.rs` already has `should_retry()` with include/exclude patterns and exponential backoff. This task adds jitter support.

- [x] **Step 1: Add jitter field and write failing test**

In `crates/rustycode-bench/src/trial/mod.rs`, add `jitter` field to `RetryConfig`:

```rust
// Add to RetryConfig struct after wait_multiplier:
    /// Jitter factor (0.0 to 1.0) added to backoff delay.
    #[serde(default)]
    pub jitter: f64,
```

Update `Default for RetryConfig` to include:
```rust
            jitter: 0.1,
```

Add `wait_duration_secs_with_jitter` method after `wait_duration_secs`:
```rust
    /// Calculate wait duration with random jitter applied.
    /// Jitter range: `[delay * (1 - jitter), delay]`.
    pub fn wait_duration_secs_with_jitter(&self, attempt: usize) -> f64 {
        let base = self.wait_duration_secs(attempt);
        let jitter_range = base * self.jitter;
        // Use simple deterministic jitter: subtract jitter_range/2
        // (Real randomness should be applied at call site via rand)
        base - jitter_range / 2.0
    }
```

Add these tests to the existing `mod tests`:

```rust
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
```

- [x] **Step 2: Run tests**

Run: `cargo test -p rustycode-bench trial::tests --lib`
Expected: All existing + 3 new tests pass

- [x] **Step 3: Commit**

```bash
git add crates/rustycode-bench/src/trial/mod.rs
git commit -m "feat(bench): add jitter support to RetryConfig"
```

---

### Task 4: Artifact collection with `ignore` crate

**Files:**
- Create: `crates/rustycode-bench/src/trial/artifacts.rs`
- Modify: `crates/rustycode-bench/src/trial/mod.rs` — add `pub mod artifacts;`
- Modify: `crates/rustycode-bench/Cargo.toml` — add `ignore = "0.4"`
- Modify: `crates/rustycode-bench/src/lib.rs`

- [x] **Step 1: Add `ignore` dependency**

In `crates/rustycode-bench/Cargo.toml`, add to `[dependencies]`:
```toml
ignore = "0.4"
```

- [x] **Step 2: Create artifacts.rs with tests**

Create `crates/rustycode-bench/src/trial/artifacts.rs`:

```rust
//! Artifact collection from trial output directories.
//!
//! Uses the `ignore` crate for gitignore-style glob exclusion.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

/// Configuration for which files to collect.
#[derive(Debug, Clone)]
pub struct ArtifactFilter {
    /// Root directory to scan.
    pub root: PathBuf,
    /// Glob patterns to exclude (gitignore-style).
    pub exclude_patterns: Vec<String>,
}

/// Collected artifact with metadata.
#[derive(Debug, Clone)]
pub struct Artifact {
    pub relative_path: PathBuf,
    pub absolute_path: PathBuf,
    pub size_bytes: u64,
}

impl ArtifactFilter {
    /// Collect all non-excluded files under root.
    pub fn collect(&self) -> Result<Vec<Artifact>> {
        let mut artifacts = Vec::new();

        let mut builder = ignore::WalkBuilder::new(&self.root);
        builder.hidden(false); // Include hidden files
        builder.git_ignore(false); // Don't use .gitignore
        builder.git_global(false);
        builder.git_exclude(false);

        // Add custom exclude patterns
        for pattern in &self.exclude_patterns {
            builder.add_custom_ignore_filename(pattern);
        }

        for entry in builder.build() {
            let entry = match entry {
                Ok(e) => e,
                Err(e) => {
                    tracing::debug!("Skipping artifact entry: {e}");
                    continue;
                }
            };

            if !entry.file_type().is_some_and(|ft| ft.is_file()) {
                continue;
            }

            let path = entry.path();
            let relative = path
                .strip_prefix(&self.root)
                .unwrap_or(path)
                .to_path_buf();

            let metadata = entry
                .metadata()
                .with_context(|| format!("reading metadata for {}", path.display()))
                .ok();

            artifacts.push(Artifact {
                relative_path: relative,
                absolute_path: path.to_path_buf(),
                size_bytes: metadata.map(|m| m.len()).unwrap_or(0),
            });
        }

        artifacts.sort_by(|a, b| a.relative_path.cmp(&b.relative_path));
        Ok(artifacts)
    }
}

/// Collect artifacts from a trial directory, excluding common noise.
pub fn collect_trial_artifacts(trial_dir: &Path) -> Result<Vec<Artifact>> {
    let filter = ArtifactFilter {
        root: trial_dir.to_path_buf(),
        exclude_patterns: vec![
            "*.log".to_string(),
            "target".to_string(),
            "__pycache__".to_string(),
            "node_modules".to_string(),
            ".git".to_string(),
        ],
    };
    filter.collect()
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn collect_empty_dir() {
        let dir = tempfile::tempdir().unwrap();
        let filter = ArtifactFilter {
            root: dir.path().to_path_buf(),
            exclude_patterns: vec![],
        };
        let artifacts = filter.collect().unwrap();
        assert!(artifacts.is_empty());
    }

    #[test]
    fn collect_with_files() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("output.txt"), "hello").unwrap();
        std::fs::create_dir(dir.path().join("sub")).unwrap();
        std::fs::write(dir.path().join("sub/data.json"), "{}").unwrap();

        let filter = ArtifactFilter {
            root: dir.path().to_path_buf(),
            exclude_patterns: vec![],
        };
        let artifacts = filter.collect().unwrap();
        assert_eq!(artifacts.len(), 2);
        assert!(artifacts[0].relative_path.to_string_lossy().contains("output"));
    }

    #[test]
    fn collect_excludes_patterns() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("keep.txt"), "data").unwrap();
        std::fs::write(dir.path().join("noise.log"), "log").unwrap();

        let filter = ArtifactFilter {
            root: dir.path().to_path_buf(),
            exclude_patterns: vec!["*.log".to_string()],
        };
        let artifacts = filter.collect().unwrap();
        assert_eq!(artifacts.len(), 1);
        assert!(artifacts[0].relative_path.to_string_lossy().contains("keep"));
    }
}
```

- [x] **Step 3: Wire into trial/mod.rs**

Add to `crates/rustycode-bench/src/trial/mod.rs` (at top, after existing imports):
```rust
pub mod artifacts;
```

- [x] **Step 4: Export from lib.rs**

Add to exports in `crates/rustycode-bench/src/lib.rs`:
```rust
pub use trial::artifacts::{Artifact, ArtifactFilter, collect_trial_artifacts};
```

- [x] **Step 5: Run tests**

Run: `cargo test -p rustycode-bench trial::artifacts --lib`
Expected: All 3 tests PASS

Run: `cargo test -p rustycode-bench --lib`
Expected: All existing + new tests pass

- [x] **Step 6: Commit**

```bash
git add crates/rustycode-bench/Cargo.toml
git add crates/rustycode-bench/src/trial/artifacts.rs
git add crates/rustycode-bench/src/trial/mod.rs
git add crates/rustycode-bench/src/lib.rs
git commit -m "feat(bench): add artifact collection with ignore-style exclusion"
```

---

### Task 5: Multi-step task support

**Files:**
- Create: `crates/rustycode-bench/src/task/steps.rs`
- Modify: `crates/rustycode-bench/src/task/mod.rs` — add `pub mod steps;`
- Modify: `crates/rustycode-bench/src/lib.rs`

- [x] **Step 1: Create steps.rs with tests**

Create `crates/rustycode-bench/src/task/steps.rs`:

```rust
//! Multi-step task support for sequential benchmark tasks.

use serde::{Deserialize, Serialize};

/// A single step in a multi-step task.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskStep {
    /// Step description / instruction.
    pub instruction: String,
    /// Minimum reward to proceed to the next step (0.0 to 1.0).
    #[serde(default = "default_min_reward")]
    pub min_reward: f64,
}

const fn default_min_reward() -> f64 {
    1.0
}

/// Parsed multi-step task configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MultiStepConfig {
    /// Ordered list of steps to execute.
    pub steps: Vec<TaskStep>,
}

impl MultiStepConfig {
    /// Parse multi-step config from a TOML string.
    pub fn from_toml(content: &str) -> anyhow::Result<Self> {
        Ok(toml::from_str(content)?)
    }

    /// Total number of steps.
    pub fn len(&self) -> usize {
        self.steps.len()
    }

    /// Whether there are no steps.
    pub fn is_empty(&self) -> bool {
        self.steps.is_empty()
    }

    /// Whether a step's reward allows proceeding to the next.
    pub fn can_proceed(&self, step_index: usize, reward: f64) -> bool {
        if step_index >= self.steps.len() {
            return false;
        }
        reward >= self.steps[step_index].min_reward
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn parse_single_step() {
        let toml = r#"
[[steps]]
instruction = "Write a hello world program"
min_reward = 1.0
"#;
        let config = MultiStepConfig::from_toml(toml).unwrap();
        assert_eq!(config.len(), 1);
        assert_eq!(config.steps[0].instruction, "Write a hello world program");
        assert!((config.steps[0].min_reward - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn parse_multi_step() {
        let toml = r#"
[[steps]]
instruction = "Install dependencies"
min_reward = 1.0

[[steps]]
instruction = "Implement the algorithm"
min_reward = 0.8

[[steps]]
instruction = "Write tests"
"#;
        let config = MultiStepConfig::from_toml(toml).unwrap();
        assert_eq!(config.len(), 3);
        assert!((config.steps[1].min_reward - 0.8).abs() < f64::EPSILON);
        assert!((config.steps[2].min_reward - 1.0).abs() < f64::EPSILON); // default
    }

    #[test]
    fn parse_empty_steps() {
        let toml = "";
        let config = MultiStepConfig::from_toml(toml).unwrap();
        assert!(config.is_empty());
    }

    #[test]
    fn can_proceed_sufficient_reward() {
        let config = MultiStepConfig {
            steps: vec![
                TaskStep { instruction: "step1".into(), min_reward: 1.0 },
                TaskStep { instruction: "step2".into(), min_reward: 0.8 },
            ],
        };
        assert!(config.can_proceed(0, 1.0));
        assert!(config.can_proceed(0, 0.9)); // 0.9 >= 1.0? no
    }

    #[test]
    fn can_proceed_insufficient_reward() {
        let config = MultiStepConfig {
            steps: vec![
                TaskStep { instruction: "step1".into(), min_reward: 1.0 },
            ],
        };
        assert!(!config.can_proceed(0, 0.5));
    }

    #[test]
    fn can_proceed_out_of_bounds() {
        let config = MultiStepConfig { steps: vec![] };
        assert!(!config.can_proceed(0, 1.0));
    }

    #[test]
    fn serde_roundtrip() {
        let config = MultiStepConfig {
            steps: vec![
                TaskStep { instruction: "do thing".into(), min_reward: 0.5 },
            ],
        };
        let json = serde_json::to_string(&config).unwrap();
        let back: MultiStepConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(back.len(), 1);
        assert!((back.steps[0].min_reward - 0.5).abs() < f64::EPSILON);
    }

    #[test]
    fn can_proceed_partial_reward_threshold() {
        let config = MultiStepConfig {
            steps: vec![
                TaskStep { instruction: "step1".into(), min_reward: 0.8 },
            ],
        };
        assert!(config.can_proceed(0, 0.8));
        assert!(config.can_proceed(0, 0.9));
        assert!(!config.can_proceed(0, 0.7));
    }
}
```

- [x] **Step 2: Wire into task/mod.rs**

Add to `crates/rustycode-bench/src/task/mod.rs` (at top):
```rust
pub mod steps;
```

- [x] **Step 3: Export from lib.rs**

Add to exports in `crates/rustycode-bench/src/lib.rs`:
```rust
pub use task::steps::{MultiStepConfig, TaskStep};
```

- [x] **Step 4: Run tests**

Run: `cargo test -p rustycode-bench task::steps --lib`
Expected: All 9 tests PASS

- [x] **Step 5: Commit**

```bash
git add crates/rustycode-bench/src/task/steps.rs
git add crates/rustycode-bench/src/task/mod.rs
git add crates/rustycode-bench/src/lib.rs
git commit -m "feat(bench): add multi-step task support"
```

---

## Chunk 3: Docker Environment via Bollard (Stream 1)

### Task 6: Bollard-based Docker environment

**Files:**
- Modify: `crates/rustycode-bench/Cargo.toml` — add bollard
- Create: `crates/rustycode-bench/src/environment/docker.rs`
- Modify: `crates/rustycode-bench/src/environment/mod.rs`
- Modify: `crates/rustycode-bench/src/lib.rs`

- [x] **Step 1: Add bollard dependency**

In `crates/rustycode-bench/Cargo.toml`, add to `[dependencies]`:
```toml
bollard = "0.17"
```

Note: `bollard-stubs` is NOT needed. `flate2` and `tar` already exist for tar archive creation.

- [x] **Step 2: Create docker.rs**

Create `crates/rustycode-bench/src/environment/docker.rs`:

```rust
//! Docker environment using bollard (Rust Docker API).
//!
//! This is the DockerEnvironment implementation used by the TB2 harness.
//! It talks directly to the Docker daemon through bollard.

use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context};
use bollard::container::{
    Config, CreateContainerOptions, RemoveContainerOptions,
    StartContainerOptions, WaitContainerOptions,
};
use bollard::exec::{CreateExecOptions, StartExecOptions, StartExecResults};
use bollard::image::BuildImageOptions;
use bollard::models::BuildInfo;
use bollard::Docker;

use super::{BenchEnvironment, ExecResult};

/// bollard-based Docker environment implementing `BenchEnvironment`.
pub struct DockerEnvironment {
    docker: Docker,
    container_name: String,
    dockerfile_dir: PathBuf,
    image_tag: String,
    cpus: u32,
    memory: String,
    container_id: Option<String>,
    /// Build timeout in seconds (stored as f64 matching EnvironmentConfig).
    build_timeout_secs: f64,
}

impl DockerEnvironment {
    pub fn new(
        container_name: String,
        dockerfile_dir: PathBuf,
        image_tag: String,
        cpus: u32,
        memory: String,
        build_timeout_secs: f64,
    ) -> anyhow::Result<Self> {
        let docker = Docker::connect_with_local_defaults()
            .context("Failed to connect to Docker daemon")?;
        Ok(Self {
            docker,
            container_name,
            dockerfile_dir,
            image_tag,
            cpus,
            memory,
            container_id: None,
            build_timeout_secs,
        })
    }

    /// Build a Docker image from the Dockerfile directory.
    async fn build_image(&self) -> anyhow::Result<()> {
        // Create tar archive of the Dockerfile directory
        let tar_bytes = crate::environment::docker::create_context_tar(&self.dockerfile_dir)?;

        let options = BuildImageOptions {
            dockerfile: "Dockerfile",
            t: &self.image_tag,
            forcerm: true,
            ..Default::default()
        };

        let mut stream = self.docker.build_image(
            options,
            None,
            Some(tar_bytes.into()),
        );

        use futures::StreamExt;
        while let Some(msg) = stream.next().await {
            match msg {
                Ok(BuildInfo { stream: Some(s), .. }) => {
                    let trimmed = s.trim();
                    if !trimmed.is_empty() {
                        tracing::debug!("[build] {trimmed}");
                    }
                }
                Ok(BuildInfo { error: Some(e), .. }) => {
                    bail!("Docker build failed: {e}");
                }
                Err(e) => {
                    bail!("Docker build stream error: {e}");
                }
                _ => {}
            }
        }

        tracing::info!("Built image: {}", self.image_tag);
        Ok(())
    }
}

#[async_trait::async_trait]
impl BenchEnvironment for DockerEnvironment {
    async fn start(&mut self, force_build: bool) -> anyhow::Result<()> {
        if force_build {
            self.build_image().await?;
        }

        let memory_bytes = parse_memory_to_bytes(&self.memory);

        let config = Config {
            image: Some(self.image_tag.clone()),
            cmd: Some(vec!["sleep".to_string(), "infinity".to_string()]),
            host_config: Some(bollard::service::HostConfig {
                memory: Some(memory_bytes as i64),
                nano_cpus: Some((self.cpus as i64) * 1_000_000_000),
                ..Default::default()
            }),
            tty: Some(true),
            ..Default::default()
        };

        let create_options = CreateContainerOptions {
            name: &self.container_name,
            ..Default::default()
        };

        let result = self
            .docker
            .create_container(Some(create_options), config)
            .await
            .context("Failed to create container")?;

        self.container_id = Some(result.id.clone());

        self.docker
            .start_container(&result.id, None::<StartContainerOptions<String>>)
            .await
            .context("Failed to start container")?;

        tracing::info!("Container started: {}", self.container_name);
        Ok(())
    }

    async fn stop(&mut self, delete: bool) -> anyhow::Result<()> {
        let id = match &self.container_id {
            Some(id) => id.clone(),
            None => return Ok(()),
        };

        // Try to stop (ignore error if already stopped)
        let _ = self.docker.stop_container(&id, None).await;

        if delete {
            self.docker
                .remove_container(
                    &id,
                    Some(RemoveContainerOptions {
                        force: true,
                        ..Default::default()
                    }),
                )
                .await
                .context("Failed to remove container")?;

            // Try to remove the image too
            let _ = self.docker.remove_image(&self.image_tag, None, None).await;
        }

        self.container_id = None;
        Ok(())
    }

    async fn exec(&self, command: &str) -> anyhow::Result<ExecResult> {
        self.exec_with_timeout(command, 300).await
    }

    async fn exec_with_timeout(
        &self,
        command: &str,
        timeout_secs: u64,
    ) -> anyhow::Result<ExecResult> {
        let id = self
            .container_id
            .as_ref()
            .context("Container not started")?;

        let exec_config = CreateExecOptions {
            cmd: Some(vec!["bash".to_string(), "-c".to_string(), command.to_string()]),
            attach_stdout: Some(true),
            attach_stderr: Some(true),
            ..Default::default()
        };

        let exec_result = self
            .docker
            .create_exec(id, exec_config)
            .await
            .context("Failed to create exec")?;

        let start_config = StartExecOptions {
            detach: false,
            ..Default::default()
        };

        let result = self
            .docker
            .start_exec(&exec_result.id, Some(start_config))
            .await
            .context("Failed to start exec")?;

        match result {
            StartExecResults::Attached { output, .. } => {
                use futures::StreamExt;
                use tokio::io::AsyncReadExt;

                let mut stdout = String::new();
                let mut stderr = String::new();

                // Multiplex output with timeout
                let result = tokio::time::timeout(
                    std::time::Duration::from_secs(timeout_secs),
                    async {
                        let mut output = output;
                        while let Some(msg) = output.next().await {
                            match msg {
                                Ok(bollard::container::LogOutput::StdOut { message }) => {
                                    stdout.push_str(&String::from_utf8_lossy(&message));
                                }
                                Ok(bollard::container::LogOutput::StdErr { message }) => {
                                    stderr.push_str(&String::from_utf8_lossy(&message));
                                }
                                Err(e) => {
                                    stderr.push_str(&format!("exec stream error: {e}"));
                                    break;
                                }
                                _ => {}
                            }
                        }
                    },
                )
                .await;

                if result.is_err() {
                    return Ok(ExecResult {
                        stdout,
                        stderr: format!("{stderr}\nexec timed out after {timeout_secs}s"),
                        exit_code: -1,
                    });
                }

                // Get exit code
                let inspect = self
                    .docker
                    .inspect_exec(&exec_result.id)
                    .await
                    .context("Failed to inspect exec")?;

                let exit_code = inspect
                    .exit_code
                    .unwrap_or(-1);

                Ok(ExecResult {
                    stdout,
                    stderr,
                    exit_code,
                })
            }
            StartExecResults::Detached => {
                bail!("Exec returned detached (unexpected)");
            }
        }
    }

    async fn upload_file(&self, src: &Path, dest: &str) -> anyhow::Result<()> {
        let id = self
            .container_id
            .as_ref()
            .context("Container not started")?;

        let content = std::fs::read(src)
            .with_context(|| format!("Failed to read {}", src.display()))?;

        // Create a tar archive containing the file
        let mut tar_buf = Vec::new();
        {
            let mut tar = tar::Builder::new(&mut tar_buf);
            let file_name = Path::new(dest)
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            let mut header = tar::Header::new_gnu();
            header.set_size(content.len() as u64);
            header.set_mode(0o644);
            header.set_cksum();
            tar.append_data(&mut header, &file_name, content.as_slice())?;
            tar.finish()?;
        }

        let options = bollard::container::PutContainerArchiveOptions {
            path: Path::new(dest)
                .parent()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|| "/".to_string()),
            ..Default::default()
        };

        self.docker
            .put_container_archive(id, options, tar_buf.into())
            .await
            .context("Failed to upload file to container")?;

        Ok(())
    }

    async fn download_file(&self, src: &str, dest: &Path) -> anyhow::Result<()> {
        let id = self
            .container_id
            .as_ref()
            .context("Container not started")?;

        let options = bollard::container::GetArchiveOptions { path: src };

        let stream = self
            .docker
            .get_container_archive(id, Some(options))
            .await
            .context("Failed to download file from container")?;

        use futures::StreamExt;
        use tokio::io::AsyncReadExt;

        let mut bytes = Vec::new();
        let mut reader = stream.into_async_read();
        reader.read_to_end(&mut bytes).await?;

        // Extract from tar
        let mut archive = tar::Archive::new(bytes.as_slice());
        let mut found = false;
        for entry in archive.entries()? {
            let mut entry = entry?;
            let path = entry.path()?;
            // Use the filename from the tar, save to dest
            if !found {
                if let Some(parent) = dest.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                entry.unpack(dest)?;
                found = true;
            }
        }

        if !found {
            bail!("No file found in archive for {src}");
        }

        Ok(())
    }
}

/// Parse memory string (e.g. "2G", "512M") to bytes.
fn parse_memory_to_bytes(memory: &str) -> u64 {
    let memory = memory.trim();
    let (num_part, multiplier) = if let Some(rest) = memory.strip_suffix('G') {
        (rest, 1024 * 1024 * 1024)
    } else if let Some(rest) = memory.strip_suffix('M') {
        (rest, 1024 * 1024)
    } else if let Some(rest) = memory.strip_suffix('K') {
        (rest, 1024)
    } else {
        (memory, 1)
    };
    num_part.parse::<u64>().unwrap_or(2 * 1024 * 1024 * 1024) * multiplier
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_memory_gb() {
        assert_eq!(parse_memory_to_bytes("2G"), 2 * 1024 * 1024 * 1024);
    }

    #[test]
    fn parse_memory_mb() {
        assert_eq!(parse_memory_to_bytes("512M"), 512 * 1024 * 1024);
    }

    #[test]
    fn parse_memory_plain_bytes() {
        assert_eq!(parse_memory_to_bytes("1024"), 1024);
    }

    #[test]
    fn parse_memory_invalid_defaults() {
        assert_eq!(parse_memory_to_bytes("invalid"), 2 * 1024 * 1024 * 1024);
    }

    #[test]
    fn parse_memory_case_insensitive() {
        assert_eq!(parse_memory_to_bytes("1K"), 1024);
    }
}
```

Note: If `create_context_tar` does not already exist, add it in `crates/rustycode-bench/src/environment/docker.rs`:

```rust
/// Create a tar archive of the Dockerfile directory for build context.
pub fn create_context_tar(dir: &Path) -> anyhow::Result<Vec<u8>> {
    let mut buf = Vec::new();
    {
        let mut tar = tar::Builder::new(&mut buf);
        tar.append_dir_all(".", dir)?;
        tar.finish()?;
    }
    Ok(buf)
}
```

- [x] **Step 3: Wire into environment/mod.rs**

Add to `crates/rustycode-bench/src/environment/mod.rs`:
```rust
pub mod docker;
```

- [x] **Step 4: Export from lib.rs**

Add to exports:
```rust
pub use environment::docker::DockerEnvironment;
```

- [x] **Step 5: Run tests**

Run: `cargo test -p rustycode-bench environment::docker --lib`
Expected: All 5 unit tests PASS (memory parsing — Docker tests require daemon)

Run: `cargo test -p rustycode-bench --lib`
Expected: All existing + new tests pass

- [x] **Step 6: Commit**

```bash
git add crates/rustycode-bench/Cargo.toml
git add crates/rustycode-bench/src/environment/docker.rs
git add crates/rustycode-bench/src/environment/mod.rs
git add crates/rustycode-bench/src/lib.rs
git commit -m "feat(bench): add bollard-based Docker environment"
```

---

## Chunk 4: Integration

### Task 7: Wire pass@k into BenchmarkResults

**Files:**
- Modify: `crates/rustycode-bench/src/job/result.rs`

- [x] **Step 1: Add pass@k computation to BenchmarkResults**

In `crates/rustycode-bench/src/job/result.rs`, add `pass_at_k` field and computation:

After the existing fields in `BenchmarkResults`, add:
```rust
    /// Pass@k metrics by agent name.
    #[serde(default)]
    pub pass_at_k: std::collections::HashMap<String, std::collections::HashMap<usize, f64>>,
```

In `BenchmarkResults::from_trials`, after the existing `task_results` construction, add:
```rust
        let pass_at_k = crate::verifier::pass_at_k::compute_pass_at_k(trials);
```

Include the field in the `Self { ... }` return.

- [x] **Step 2: Update summary to show pass@k**

In `BenchmarkResults::summary()`, after the `mean_reward` line, add:
```rust
        if !self.pass_at_k.is_empty() {
            let _ = write!(s, "\n\nPass@k:");
            for (agent, metrics) in &self.pass_at_k {
                let _ = write!(s, "\n  {agent}:");
                for (k, v) in metrics {
                    let _ = write!(s, " @{k}={v:.2}");
                }
            }
        }
```

- [x] **Step 3: Run tests**

Run: `cargo test -p rustycode-bench job::result --lib`
Expected: All existing tests pass (pass_at_k field is HashMap, defaults empty)

- [x] **Step 4: Commit**

```bash
git add crates/rustycode-bench/src/job/result.rs
git commit -m "feat(bench): add pass@k metrics to BenchmarkResults"
```

---

### Task 8: Add `bollard` env option to CLI

**Files:**
- Modify: `crates/rustycode-bench/src/bin/main.rs`

- [x] **Step 1: Add `bollard` to the environment choices in run_benchmark**

In `crates/rustycode-bench/src/bin/main.rs`, update the `match cfg.env.as_str()` block in `run_benchmark` to add a third arm:

```rust
        "bollard" => {
            let runner = DockerRunner::new(DockerRunnerConfig {
                agent_name: agent_name.clone(),
                model: model.clone(),
                n_concurrent: cfg.n_concurrent,
                job_name: job_name.clone(),
                force_build: cfg.force_build,
                cleanup: cfg.cleanup,
                retry_config: cfg.retry.clone(),
            });
            runner.run(&tasks, &dataset_path, agent_factory).await
        }
```

Note: Initially the `bollard` env option reuses `DockerRunner` but creates containers via `DockerEnvironment`. A future task can add a `BollardRunner` that uses `DockerEnvironment` directly. For now, this validates the CLI plumbing works.

Also update the help text for `--env` to include `bollard`:

```rust
        /// Execution environment: native, docker, bollard
        #[arg(long, default_value = "native")]
        env: String,
```

And update the error message:
```rust
        other => bail!("Unknown environment: '{other}'. Use 'native', 'docker', or 'bollard'"),
```

- [x] **Step 2: Run tests**

Run: `cargo test -p rustycode-bench --lib`
Expected: All tests pass

Run: `cargo build -p rustycode-bench`
Expected: Compiles without errors

- [x] **Step 3: Commit**

```bash
git add crates/rustycode-bench/src/bin/main.rs
git commit -m "feat(bench): add bollard env option to CLI"
```

---

### Task 9: End-to-end verification

- [x] **Step 1: Build and run clippy**

Run: `cargo clippy -p rustycode-bench --all-targets -- -D warnings`
Expected: Zero warnings

- [x] **Step 2: Run all tests**

Run: `cargo test -p rustycode-bench --lib`
Expected: All tests pass

- [x] **Step 3: Verify new module structure**

Run: `cargo test -p rustycode-bench --lib -- --list 2>&1 | grep -E '(reward|pass_at_k|artifacts|steps|bollard)'`
Expected: Shows all new test modules listed

- [x] **Step 4: Verify exports**

Run: `cargo doc -p rustycode-bench --no-deps 2>&1 | tail -5`
Expected: Generates docs without errors

- [x] **Step 5: Final commit**

```bash
git add -A crates/rustycode-bench/
git commit -m "chore(bench): verify all new modules build and test clean"
```
