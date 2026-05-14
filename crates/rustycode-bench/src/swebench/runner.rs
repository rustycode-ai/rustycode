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
    /// Max verification retries after agent finishes (default: 1).
    pub verify_retries: u32,
}

/// Detected test runner type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TestRunner {
    Pytest,
    Django,
    Unittest,
    Unknown,
}

/// Detect the test runner used by a repo.
fn detect_test_runner(repo_dir: &Path) -> TestRunner {
    if repo_dir.join("tests").join("runtests.py").exists()
        || repo_dir.join("runtests.py").exists()
        || repo_dir.join("django").is_dir()
    {
        TestRunner::Django
    } else if repo_dir.join("pytest.ini").exists()
        || repo_dir.join("pyproject.toml").exists()
        || repo_dir.join("setup.cfg").exists()
    {
        TestRunner::Pytest
    } else if repo_dir.join("tests").is_dir()
        && !repo_dir.join("pytest.ini").exists()
        && !repo_dir.join("pyproject.toml").exists()
        && !repo_dir.join("setup.cfg").exists()
    {
        TestRunner::Unittest
    } else {
        TestRunner::Unknown
    }
}

/// Build a test invocation command for the given runner and test names.
fn build_test_command(runner: TestRunner, test_names: &[String], repo_dir: &Path) -> Vec<String> {
    if test_names.is_empty() {
        return vec![];
    }
    match runner {
        TestRunner::Django => {
            let runtests = if repo_dir.join("tests").join("runtests.py").exists() {
                "tests/runtests.py"
            } else {
                "runtests.py"
            };
            let modules: Vec<String> = test_names
                .iter()
                .filter_map(|t| {
                    if let Some(start) = t.find('(') {
                        let inner = &t[start + 1..];
                        let end = inner.find(')').unwrap_or(inner.len());
                        inner[..end].split('.').next().map(|s| s.to_string())
                    } else {
                        Some(t.clone())
                    }
                })
                .collect();
            let mut cmd = vec!["python3".to_string(), runtests.to_string()];
            cmd.extend(modules);
            cmd
        }
        TestRunner::Unittest => {
            let mut cmd = vec![
                "python3".to_string(),
                "-m".to_string(),
                "unittest".to_string(),
            ];
            cmd.extend(test_names.iter().cloned());
            cmd
        }
        TestRunner::Pytest | TestRunner::Unknown => {
            let mut cmd = vec![
                "python3".to_string(),
                "-m".to_string(),
                "pytest".to_string(),
                "-x".to_string(),
                "--no-header".to_string(),
                "-q".to_string(),
                "--tb".to_string(),
                "short".to_string(),
            ];
            cmd.extend(test_names.iter().cloned());
            cmd
        }
    }
}

/// Result of running FAIL_TO_PASS tests against a patched repo.
enum VerifyResult {
    /// All tests passed.
    Pass,
    /// Tests failed — contains truncated output.
    Fail(String),
    /// Could not run tests.
    Error(String),
    /// No FAIL_TO_PASS tests to run.
    NoTests,
}

/// Run FAIL_TO_PASS tests against the patched repo to verify the fix.
fn verify_patch(repo_dir: &Path, test_names: &[String]) -> VerifyResult {
    if test_names.is_empty() {
        return VerifyResult::NoTests;
    }
    let runner = detect_test_runner(repo_dir);
    let cmd_args = build_test_command(runner, test_names, repo_dir);
    if cmd_args.is_empty() {
        return VerifyResult::NoTests;
    }
    let output = std::process::Command::new(&cmd_args[0])
        .args(&cmd_args[1..])
        .current_dir(repo_dir)
        .env("PYTHONPATH", "..")
        .output();
    match output {
        Ok(out) => {
            let stdout = String::from_utf8_lossy(&out.stdout).to_string();
            let stderr = String::from_utf8_lossy(&out.stderr).to_string();
            let combined = format!("{stdout}\n{stderr}");
            if out.status.success() {
                VerifyResult::Pass
            } else {
                let truncated = if combined.len() > 4000 {
                    format!("...\n{}", &combined[combined.len() - 4000..])
                } else {
                    combined
                };
                VerifyResult::Fail(truncated)
            }
        }
        Err(e) => VerifyResult::Error(e.to_string()),
    }
}

/// Result of running a single SWE-bench instance.
struct InstanceResult {
    patch: String,
    attempts: u32,
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
            Ok(result) => {
                let has_patch = !result.patch.is_empty();
                println!(
                    "  → {} ({} bytes, {} attempt{})",
                    if has_patch { "PATCH" } else { "EMPTY" },
                    result.patch.len(),
                    result.attempts,
                    if result.attempts > 1 { "s" } else { "" }
                );
                predictions.push(SweBenchPrediction {
                    instance_id: inst.instance_id.clone(),
                    model_patch: result.patch,
                    attempts: result.attempts,
                });
            }
            Err(e) => {
                tracing::warn!("[{}] Failed: {e:#}", inst.instance_id);
                println!("  → ERROR: {e:#}");
                predictions.push(SweBenchPrediction {
                    instance_id: inst.instance_id.clone(),
                    model_patch: String::new(),
                    attempts: 1,
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

/// Run a single SWE-bench instance: clone → agent → verify → (retry) → diff.
async fn run_single_instance(
    inst: &SweBenchInstance,
    config: &SweBenchConfig,
) -> Result<InstanceResult> {
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
    let file_tree = build_file_tree(&clone_dir, 3);

    // Build hints section if available
    let hints_section = if inst.hints_text.is_empty() {
        String::new()
    } else {
        format!("\n## Hints\n\n{}\n", inst.hints_text)
    };

    // Build test names section — critical for verification
    let ftp_tests = parse_test_list(&inst.fail_to_pass);
    let ptp_tests = parse_test_list(&inst.pass_to_pass);
    let tests_section = if ftp_tests.is_empty() {
        String::new()
    } else {
        let ftp_list = ftp_tests
            .iter()
            .map(|t| format!("- {t}"))
            .collect::<Vec<_>>()
            .join("\n");
        let ptp_info = if ptp_tests.is_empty() {
            String::new()
        } else {
            format!(
                "\n\n### Tests that must STAY passing (sample)\n{}",
                ptp_tests
                    .iter()
                    .take(10)
                    .map(|t| format!("- {t}"))
                    .collect::<Vec<_>>()
                    .join("\n")
            )
        };
        format!(
            "\n## Tests to Fix (FAIL_TO_PASS)\n\
             These tests currently FAIL. Your fix must make them PASS.\n\
             {ftp_list}\
             {ptp_info}\n"
        )
    };

    // Build install hint — tells the agent to use the repo's code, not system packages
    let install_hint = if clone_dir.join("setup.py").exists()
        || clone_dir.join("pyproject.toml").exists()
    {
        "\n- When testing, ALWAYS use `PYTHONPATH=.. python3 test.py` (or `pip install -e .`) to import from THIS repo, not the system package."
    } else {
        ""
    };

    // Detect test runner for the prompt
    let test_runner_hint = detect_test_runner_hint(&clone_dir);

    // Build prompt — honest, no tricks, but includes context to reduce exploration turns
    let prompt = format!(
        "Please fix the following issue in this repository.\n\n\
         ## Repository Structure\n\n```\n{file_tree}\n```\n\n\
         ## Issue\n\n{}\n\
         {hints_section}\
         {tests_section}\
         ## Instructions\n\n\
         1. **Reproduce first**: Run the FAIL_TO_PASS tests above to see the exact error.{test_runner_hint}\n\
         2. **Read relevant code**: Use grep/find_symbol to locate the bug. Read only files related to the error.\n\
         3. **Make minimal fix**: Edit only the source code needed. Do NOT add or modify tests.\n\
         4. **Verify**: Re-run the FAIL_TO_PASS tests. If they pass, stop. If not, fix and retry.\n\
         5. Use MULTIPLE tool calls per turn (e.g. read several files at once, or grep + glob together).{install_hint}",
        inst.problem_statement
    );

    // Retry loop: initial attempt + optional verification retries
    let max_attempts = 1u32 + config.verify_retries;
    let mut current_prompt = prompt;
    let mut patch = String::new();
    let mut attempts = 0u32;

    for attempt in 0..max_attempts {
        attempts = attempt + 1;

        let agent = crate::config::create_agent(
            &config.agent_name,
            &config.agent_config.model,
            clone_dir.clone(),
            Some(&config.agent_config.provider),
            config.agent_config.with_symbol_tools,
            config.agent_config.with_thinking_guide,
        )
        .with_context(|| format!("Failed to create '{}' agent", config.agent_name))?;

        if let Err(e) =
            run_agent_with_timeout(agent, &current_prompt, &clone_dir, config.timeout_secs).await
        {
            if attempt == 0 {
                return Err(e);
            }
            tracing::warn!("[{}] Retry {attempts} failed: {e:#}", inst.instance_id);
            break;
        }

        patch = capture_diff(&clone_dir)?;

        if patch.is_empty() || attempt + 1 >= max_attempts || ftp_tests.is_empty() {
            break;
        }

        // Verify fix against FAIL_TO_PASS tests
        let verify = verify_patch(&clone_dir, &ftp_tests);
        match verify {
            VerifyResult::Pass => {
                tracing::info!("[{}] Tests PASS on attempt {attempts}", inst.instance_id);
                break;
            }
            VerifyResult::NoTests => break,
            VerifyResult::Error(msg) => {
                tracing::warn!("[{}] Verification error: {msg}", inst.instance_id);
                break;
            }
            VerifyResult::Fail(output) => {
                tracing::info!(
                    "[{}] Tests FAIL on attempt {attempts}, retrying",
                    inst.instance_id
                );
                current_prompt = format!(
                    "{current_prompt}\n\n\
                     ## Verification Result (Attempt {attempts})\n\n\
                     The FAIL_TO_PASS tests produced the following output:\n\n\
                     ```\n{output}\n```\n\n\
                     Please fix the remaining issues. The code already has changes \
                     from a previous attempt — modify them or make new changes."
                );
            }
        }
    }

    Ok(InstanceResult { patch, attempts })
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

/// Detect the test runner and return a prompt hint with the correct invocation.
fn detect_test_runner_hint(repo_dir: &Path) -> String {
    match detect_test_runner(repo_dir) {
        TestRunner::Django => " Use `python3 tests/runtests.py <module>` for Django projects.",
        TestRunner::Pytest => " Use `python3 -m pytest <test_path>` for pytest projects.",
        TestRunner::Unittest | TestRunner::Unknown => {
            " Use `python3 -m pytest` or `python3 -m unittest` as appropriate."
        }
    }
    .to_string()
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
            verify_retries: 1,
        };
        assert_eq!(config.format, "json");
        assert_eq!(config.timeout_secs, 600);
        assert_eq!(config.verify_retries, 1);
    }

    #[test]
    fn detect_django_runner() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(tmp.path().join("tests")).unwrap();
        std::fs::write(tmp.path().join("tests").join("runtests.py"), "").unwrap();
        assert_eq!(detect_test_runner(tmp.path()), TestRunner::Django);
    }

    #[test]
    fn detect_pytest_runner() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("pytest.ini"), "[pytest]\n").unwrap();
        assert_eq!(detect_test_runner(tmp.path()), TestRunner::Pytest);
    }

    #[test]
    fn detect_unittest_runner() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(tmp.path().join("tests")).unwrap();
        // Has tests/ dir but no pytest or Django markers
        assert_eq!(detect_test_runner(tmp.path()), TestRunner::Unittest);
    }

    #[test]
    fn detect_unknown_runner() {
        let tmp = tempfile::tempdir().unwrap();
        assert_eq!(detect_test_runner(tmp.path()), TestRunner::Unknown);
    }

    #[test]
    fn build_pytest_command() {
        let tmp = tempfile::tempdir().unwrap();
        let names = vec!["tests/test_foo.py::test_bar".to_string()];
        let cmd = build_test_command(TestRunner::Pytest, &names, tmp.path());
        assert_eq!(cmd[0], "python3");
        assert!(cmd.contains(&"-m".to_string()));
        assert!(cmd.contains(&"pytest".to_string()));
        assert!(cmd.contains(&"tests/test_foo.py::test_bar".to_string()));
    }

    #[test]
    fn build_django_command_extracts_module() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(tmp.path().join("tests")).unwrap();
        std::fs::write(tmp.path().join("tests").join("runtests.py"), "").unwrap();
        let names = vec![
            "test_something (auth.tests.TestAuth)".to_string(),
            "test_other (contenttypes.tests.TestCT)".to_string(),
        ];
        let cmd = build_test_command(TestRunner::Django, &names, tmp.path());
        assert_eq!(cmd[0], "python3");
        assert_eq!(cmd[1], "tests/runtests.py");
        assert!(cmd.contains(&"auth".to_string()));
        assert!(cmd.contains(&"contenttypes".to_string()));
    }

    #[test]
    fn build_unittest_command() {
        let tmp = tempfile::tempdir().unwrap();
        let names = vec!["tests.test_foo.TestBar.test_baz".to_string()];
        let cmd = build_test_command(TestRunner::Unittest, &names, tmp.path());
        assert_eq!(cmd[0], "python3");
        assert!(cmd.contains(&"-m".to_string()));
        assert!(cmd.contains(&"unittest".to_string()));
        assert!(cmd.contains(&"tests.test_foo.TestBar.test_baz".to_string()));
    }

    #[test]
    fn build_test_command_empty_names() {
        let tmp = tempfile::tempdir().unwrap();
        let cmd = build_test_command(TestRunner::Pytest, &[], tmp.path());
        assert!(cmd.is_empty());
    }

    #[test]
    fn verify_patch_no_tests() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(matches!(
            verify_patch(tmp.path(), &[]),
            VerifyResult::NoTests
        ));
    }

    #[test]
    fn parse_test_list_json_array() {
        let raw = r#"["tests/test_a.py::test_one", "tests/test_a.py::test_two"]"#;
        let list = parse_test_list(raw);
        assert_eq!(list.len(), 2);
        assert_eq!(list[0], "tests/test_a.py::test_one");
    }

    #[test]
    fn parse_test_list_comma_separated() {
        let raw = "test_one, test_two, test_three";
        let list = parse_test_list(raw);
        assert_eq!(list.len(), 3);
    }

    #[test]
    fn parse_test_list_empty() {
        let list = parse_test_list("");
        assert!(list.is_empty());
    }
}
