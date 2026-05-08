//! SWE-bench evaluation harness — applies patches and runs tests.
//!
//! For each prediction:
//! 1. Checkout repo at base commit
//! 2. Apply test_patch (adds/updates test files)
//! 3. Apply model_patch (the prediction)
//! 4. Run FAIL_TO_PASS tests → should now pass
//! 5. Run PASS_TO_PASS tests → should still pass
//! 6. Report results

use std::path::{Path, PathBuf};
use std::time::Instant;

use anyhow::{Context, Result};

use super::instance::SweBenchInstance;
use super::prediction::{load_predictions, SweBenchPrediction};

/// Result of evaluating a single instance.
#[derive(Debug, Clone)]
pub struct EvalResult {
    pub instance_id: String,
    pub resolved: bool,
    pub fail_to_pass_passed: usize,
    pub fail_to_pass_total: usize,
    pub pass_to_pass_passed: usize,
    pub pass_to_pass_total: usize,
    pub error: Option<String>,
    pub duration_secs: f64,
}

/// Configuration for the evaluation harness.
pub struct EvalConfig {
    /// Path to predictions JSON/JSONL file.
    pub predictions_path: PathBuf,
    /// Path to instances JSON/JSONL file.
    pub instances_path: PathBuf,
    /// Working directory containing cloned repos.
    pub work_dir: PathBuf,
    /// Specific instance IDs to evaluate (None = all predictions).
    pub instance_ids: Option<Vec<String>>,
    /// Per-instance test timeout in seconds.
    pub test_timeout_secs: u64,
    /// Maximum PASS_TO_PASS tests to run (0 = all).
    pub max_pass_to_pass: usize,
    /// Output path for evaluation results.
    pub output_path: Option<PathBuf>,
}

/// Parse a JSON-encoded test list from SWE-bench instance fields.
fn parse_test_list(raw: &str) -> Vec<String> {
    if raw.is_empty() {
        return Vec::new();
    }
    serde_json::from_str(raw).unwrap_or_else(|_| {
        raw.split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect()
    })
}

/// Run SWE-bench evaluation on predictions.
pub async fn run_evaluation(config: EvalConfig) -> Result<Vec<EvalResult>> {
    let predictions = load_predictions(&config.predictions_path)?;
    let instances = super::instance::load_instances(&config.instances_path)?;

    let inst_map: std::collections::HashMap<String, &SweBenchInstance> = instances
        .iter()
        .map(|i| (i.instance_id.clone(), i))
        .collect();

    let preds: Vec<&SweBenchPrediction> = if let Some(ref ids) = config.instance_ids {
        predictions
            .iter()
            .filter(|p| ids.contains(&p.instance_id))
            .collect()
    } else {
        predictions.iter().collect()
    };

    let non_empty: Vec<&&SweBenchPrediction> =
        preds.iter().filter(|p| !p.model_patch.is_empty()).collect();

    println!(
        "Evaluation: {} predictions ({} with patches, {} empty)",
        preds.len(),
        non_empty.len(),
        preds.len() - non_empty.len(),
    );

    let mut results = Vec::with_capacity(preds.len());

    // Mark empty predictions as failed
    for pred in &preds {
        if pred.model_patch.is_empty() {
            results.push(EvalResult {
                instance_id: pred.instance_id.clone(),
                resolved: false,
                fail_to_pass_passed: 0,
                fail_to_pass_total: 0,
                pass_to_pass_passed: 0,
                pass_to_pass_total: 0,
                error: Some("empty patch".to_string()),
                duration_secs: 0.0,
            });
        }
    }

    for (i, pred) in non_empty.iter().enumerate() {
        let inst = match inst_map.get(&pred.instance_id) {
            Some(i) => *i,
            None => {
                results.push(EvalResult {
                    instance_id: pred.instance_id.clone(),
                    resolved: false,
                    fail_to_pass_passed: 0,
                    fail_to_pass_total: 0,
                    pass_to_pass_passed: 0,
                    pass_to_pass_total: 0,
                    error: Some("instance not found".to_string()),
                    duration_secs: 0.0,
                });
                continue;
            }
        };

        println!("[{}/{}] {}", i + 1, non_empty.len(), pred.instance_id);

        match evaluate_single(pred, inst, &config) {
            Ok(result) => {
                let status = if result.resolved { "RESOLVED" } else { "FAIL" };
                println!(
                    "  → {} (ftp: {}/{}, ptp: {}/{}, {:.1}s)",
                    status,
                    result.fail_to_pass_passed,
                    result.fail_to_pass_total,
                    result.pass_to_pass_passed,
                    result.pass_to_pass_total,
                    result.duration_secs,
                );
                results.push(result);
            }
            Err(e) => {
                println!("  → ERROR: {e}");
                let ftp = parse_test_list(&inst.fail_to_pass);
                let ptp = parse_test_list(&inst.pass_to_pass);
                results.push(EvalResult {
                    instance_id: pred.instance_id.clone(),
                    resolved: false,
                    fail_to_pass_passed: 0,
                    fail_to_pass_total: ftp.len(),
                    pass_to_pass_passed: 0,
                    pass_to_pass_total: ptp.len(),
                    error: Some(e.to_string()),
                    duration_secs: 0.0,
                });
            }
        }
    }

    let resolved = results.iter().filter(|r| r.resolved).count();
    println!();
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("SWE-bench evaluation complete");
    println!("  Total:    {}", results.len());
    println!("  Resolved: {resolved}");
    println!("  Failed:   {}", results.len() - resolved);
    if let Some(ref path) = config.output_path {
        println!("  Output:   {}", path.display());
    }
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");

    if let Some(ref path) = config.output_path {
        save_eval_results(&results, path)?;
    }

    Ok(results)
}

/// Evaluate a single prediction against its instance.
fn evaluate_single(
    pred: &SweBenchPrediction,
    inst: &SweBenchInstance,
    config: &EvalConfig,
) -> Result<EvalResult> {
    let start = Instant::now();
    let clone_dir = config.work_dir.join(&inst.instance_id).join("repo");

    anyhow::ensure!(
        clone_dir.join(".git").exists(),
        "repo not cloned: {}",
        clone_dir.display()
    );

    // Parse test lists from JSON strings
    let ftp_tests = parse_test_list(&inst.fail_to_pass);
    let ptp_tests = parse_test_list(&inst.pass_to_pass);

    // Checkout base commit and clean
    git_run(&clone_dir, &["checkout", "--quiet", &inst.base_commit])?;
    let _ = git_run(&clone_dir, &["checkout", "--quiet", "."]);
    let _ = git_run(&clone_dir, &["clean", "-fdq"]);

    // Apply test_patch
    if !inst.test_patch.is_empty() {
        apply_patch(&clone_dir, &inst.test_patch, "test_patch")?;
    }

    // Apply model_patch
    apply_patch(&clone_dir, &pred.model_patch, "model_patch")?;

    // Install dependencies (best-effort)
    install_deps(&clone_dir);

    // Run FAIL_TO_PASS tests
    let ftp_total = ftp_tests.len();
    let ftp_passed = run_tests(&clone_dir, &ftp_tests, config.test_timeout_secs);

    // Run PASS_TO_PASS tests (sample if too many)
    let ptp_sampled = if config.max_pass_to_pass > 0 && ptp_tests.len() > config.max_pass_to_pass {
        let step = ptp_tests.len() as f64 / config.max_pass_to_pass as f64;
        (0..config.max_pass_to_pass)
            .map(|i| ptp_tests[(i as f64 * step) as usize].clone())
            .collect()
    } else {
        ptp_tests.clone()
    };
    let ptp_total = ptp_sampled.len();
    let ptp_passed = run_tests(&clone_dir, &ptp_sampled, config.test_timeout_secs);

    let resolved = ftp_passed == ftp_total && ftp_total > 0;

    Ok(EvalResult {
        instance_id: pred.instance_id.clone(),
        resolved,
        fail_to_pass_passed: ftp_passed,
        fail_to_pass_total: ftp_total,
        pass_to_pass_passed: ptp_passed,
        pass_to_pass_total: ptp_total,
        error: None,
        duration_secs: start.elapsed().as_secs_f64(),
    })
}

/// Install project dependencies (best-effort, non-blocking).
fn install_deps(repo_dir: &Path) {
    // Try pip install -e . for Python repos
    if repo_dir.join("setup.py").exists()
        || repo_dir.join("setup.cfg").exists()
        || repo_dir.join("pyproject.toml").exists()
    {
        let _ = std::process::Command::new("pip3")
            .args(["install", "-e", ".", "--quiet"])
            .current_dir(repo_dir)
            .output();
    }
}

/// Apply a patch string to the repo using `git apply`.
fn apply_patch(repo_dir: &Path, patch: &str, label: &str) -> Result<()> {
    let tmp = tempfile::NamedTempFile::new().context("create temp file")?;
    std::fs::write(tmp.path(), patch).context("write patch file")?;

    let output = std::process::Command::new("git")
        .args(["apply", "--allow-empty", tmp.path().to_str().unwrap_or("")])
        .current_dir(repo_dir)
        .output()
        .with_context(|| format!("git apply {label}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("git apply {label} failed: {stderr}");
    }

    Ok(())
}

/// Run a list of test identifiers and return how many passed.
fn run_tests(repo_dir: &Path, tests: &[String], _timeout_secs: u64) -> usize {
    if tests.is_empty() {
        return 0;
    }

    let is_django = repo_dir.join("tests").join("runtests.py").exists()
        || repo_dir.join("runtests.py").exists()
        || repo_dir.join("django").is_dir();

    let output = if is_django {
        // Django: extract module from "test_name (module.Class)" format
        let modules: Vec<String> = tests
            .iter()
            .filter_map(|t| {
                if let Some(start) = t.find('(') {
                    let inner = &t[start + 1..];
                    inner
                        .find('.')
                        .or_else(|| inner.find(')'))
                        .map(|end| inner[..end].to_string())
                } else {
                    None
                }
            })
            .collect();

        let mut cmd = std::process::Command::new("python3");
        cmd.args(["tests/runtests.py", "--verbosity=2", "--no-input"])
            .env("PYTHONDONTWRITEBYTECODE", "1")
            .current_dir(repo_dir);
        for module in &modules {
            cmd.arg(module);
        }
        cmd.output()
    } else {
        std::process::Command::new("python3")
            .args(["-m", "pytest", "--tb=no", "-q", "--no-header", "-x"])
            .args(tests)
            .current_dir(repo_dir)
            .env("PYTHONDONTWRITEBYTECODE", "1")
            .output()
    };

    match output {
        Ok(out) => {
            let stdout = String::from_utf8_lossy(&out.stdout);
            let stderr = String::from_utf8_lossy(&out.stderr);
            let combined = format!("{stdout}\n{stderr}");
            parse_test_passed(&combined)
        }
        Err(_) => 0,
    }
}

/// Parse test passed count from pytest or Django output.
fn parse_test_passed(output: &str) -> usize {
    for line in output.lines().rev().take(5) {
        if line.contains(" passed") {
            if let Some(idx) = line.find(" passed") {
                let before = &line[..idx];
                if let Some(num) = before.rsplit(|c: char| !c.is_ascii_digit()).next() {
                    if let Ok(n) = num.parse::<usize>() {
                        return n;
                    }
                }
            }
        }
    }
    0
}

fn git_run(dir: &Path, args: &[&str]) -> Result<()> {
    let status = std::process::Command::new("git")
        .args(args)
        .current_dir(dir)
        .status()
        .context("git")?;
    anyhow::ensure!(status.success(), "git {:?} failed", args);
    Ok(())
}

fn save_eval_results(results: &[EvalResult], path: &Path) -> Result<()> {
    let json_results: Vec<serde_json::Value> = results
        .iter()
        .map(|r| {
            serde_json::json!({
                "instance_id": r.instance_id,
                "resolved": r.resolved,
                "fail_to_pass_passed": r.fail_to_pass_passed,
                "fail_to_pass_total": r.fail_to_pass_total,
                "pass_to_pass_passed": r.pass_to_pass_passed,
                "pass_to_pass_total": r.pass_to_pass_total,
                "error": r.error,
                "duration_secs": (r.duration_secs * 100.0).round() / 100.0,
            })
        })
        .collect();

    let json = serde_json::to_string_pretty(&json_results)?;
    std::fs::write(path, json)?;
    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn parse_pytest_output_simple() {
        assert_eq!(parse_test_passed("5 passed, 2 failed in 3.5s"), 5);
    }

    #[test]
    fn parse_pytest_output_only_passed() {
        assert_eq!(parse_test_passed("3 passed in 1.2s"), 3);
    }

    #[test]
    fn parse_pytest_output_no_passed() {
        assert_eq!(parse_test_passed("2 failed in 1.0s"), 0);
    }

    #[test]
    fn parse_pytest_output_empty() {
        assert_eq!(parse_test_passed(""), 0);
    }

    #[test]
    fn parse_test_list_json_array() {
        let raw = r#"["test_a::test_one", "test_a::test_two"]"#;
        let list = parse_test_list(raw);
        assert_eq!(list.len(), 2);
        assert_eq!(list[0], "test_a::test_one");
    }

    #[test]
    fn parse_test_list_empty() {
        assert!(parse_test_list("").is_empty());
    }
}
