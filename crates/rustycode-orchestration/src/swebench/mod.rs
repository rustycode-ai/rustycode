pub mod instance;
pub mod predictor;
pub mod report;

use std::path::PathBuf;

use anyhow::Result;

use instance::SweBenchInstance;
use predictor::save_predictions;

pub use report::SweBenchPrediction;

/// Lightweight runner for SWE-bench predictions via the CLI.
pub struct SweBenchRunner {
    pub instances: PathBuf,
    pub output: PathBuf,
    pub budget: f64,
    pub parallel: usize,
    pub instance_ids: Option<Vec<String>>,
    pub format: String,
}

impl SweBenchRunner {
    pub fn new(
        instances: PathBuf,
        output: PathBuf,
        budget: f64,
        parallel: usize,
        instance_ids: Option<Vec<String>>,
    ) -> Self {
        Self {
            instances,
            output,
            budget,
            parallel,
            instance_ids,
            format: "json".to_string(),
        }
    }

    /// Load instances, run predictions, and save results.
    ///
    /// Uses a no-op agent runner by default — override via `run_all_with_agent`
    /// for real agent execution.
    pub fn run_all(&mut self) -> Result<Vec<SweBenchPrediction>> {
        let all_instances = instance::load_instances(&self.instances)?;
        let instances: Vec<&SweBenchInstance> = if let Some(ref ids) = self.instance_ids {
            all_instances
                .iter()
                .filter(|i| ids.contains(&i.instance_id))
                .collect()
        } else {
            all_instances.iter().collect()
        };

        tracing::info!(
            "Loaded {} instances ({} total in file)",
            instances.len(),
            all_instances.len()
        );

        let work_dir = self
            .output
            .parent()
            .unwrap_or_else(|| std::path::Path::new("."))
            .join("swebench-work");

        let mut predictions = Vec::with_capacity(instances.len());

        for inst in &instances {
            tracing::info!("Processing {}", inst.instance_id);

            // Create per-instance work directory
            let instance_work = work_dir.join(&inst.instance_id);
            std::fs::create_dir_all(&instance_work)?;

            // Build a prompt from the problem statement
            let prompt = build_prompt(inst);

            // Run the agent via rustycode-agent-runtime headless mode
            let patch = match self.run_agent(&prompt, &instance_work) {
                Ok(()) => {
                    // Capture git diff as the model patch
                    let clone_dir = instance_work.join("repo");
                    predictor::predict_instance_direct(&clone_dir).unwrap_or_default()
                }
                Err(e) => {
                    tracing::warn!(
                        "[{}] Agent failed: {e}",
                        inst.instance_id
                    );
                    String::new()
                }
            };

            predictions.push(SweBenchPrediction {
                instance_id: inst.instance_id.clone(),
                model_patch: patch,
            });

            // Incremental save after each instance
            save_predictions(&predictions, &self.output, &self.format, "rustycode")?;
        }

        Ok(predictions)
    }

    /// Run the agent on a cloned repo.
    ///
    /// This clones the repo at the base commit and runs the headless agent.
    fn run_agent(&self, prompt: &str, work_dir: &std::path::Path) -> Result<()> {
        // For now, use a simple shell-based approach.
        // The full agent integration requires rustycode-core headless runtime.
        // This stub allows the CLI to compile and the pipeline to be tested
        // with an external agent runner (e.g. via environment variable).

        let agent_cmd = std::env::var("RUSTYCODE_AGENT_CMD").unwrap_or_default();
        if agent_cmd.is_empty() {
            // No agent configured — skip actual execution
            tracing::info!("No RUSTYCODE_AGENT_CMD set, skipping agent execution");
            return Ok(());
        }

        let status = std::process::Command::new("sh")
            .arg("-c")
            .arg(&format!("{agent_cmd} {prompt:?}"))
            .current_dir(work_dir)
            .status()?;

        if status.success() {
            Ok(())
        } else {
            anyhow::bail!("Agent command exited with status {}", status)
        }
    }
}

/// Build the standard SWE-bench prompt from an instance.
fn build_prompt(inst: &SweBenchInstance) -> String {
    format!(
        "Please fix the following issue in this repository.\n\n\
         ## Issue\n\n{}\n\n\
         ## Instructions\n\n\
         1. Read the codebase to understand the problem.\n\
         2. Make minimal, targeted changes to fix the issue.\n\
         3. Ensure your changes don't break existing functionality.\n\
         4. Do NOT add tests — only fix the source code.",
        inst.problem_statement
    )
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn build_prompt_contains_issue() {
        let inst = SweBenchInstance {
            instance_id: "test__repo-1".to_string(),
            repo: "test/repo".to_string(),
            base_commit: "abc123".to_string(),
            problem_statement: "Fix the bug in foo()".to_string(),
            hints_text: String::new(),
            version: String::new(),
            fail_to_pass: String::new(),
            pass_to_pass: String::new(),
            test_patch: String::new(),
            patch: String::new(),
        };
        let prompt = build_prompt(&inst);
        assert!(prompt.contains("Fix the bug in foo()"));
        assert!(prompt.contains("Instructions"));
    }

    #[test]
    fn runner_new_sets_defaults() {
        let runner = SweBenchRunner::new(
            PathBuf::from("instances.json"),
            PathBuf::from("output.json"),
            1.0,
            2,
            None,
        );
        assert_eq!(runner.format, "json");
        assert!(runner.instance_ids.is_none());
    }
}
