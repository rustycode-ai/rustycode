//! Script verifier — runs test.sh and parses reward.txt.

use std::path::PathBuf;

use anyhow::Context;
use tracing;

use super::Verifier;
use crate::environment::{container_paths, BenchEnvironment};

/// Verifier that uploads test files, runs test.sh in the container,
/// and parses the reward.txt output.
pub struct ScriptVerifier {
    /// Path to the tests directory (contains test.sh and `test_outputs.py`).
    tests_dir: PathBuf,
    /// Timeout for the test script in seconds.
    timeout_secs: u64,
}

impl ScriptVerifier {
    #[must_use]
    pub const fn new(tests_dir: PathBuf, timeout_secs: u64) -> Self {
        Self {
            tests_dir,
            timeout_secs,
        }
    }
}

#[async_trait::async_trait]
impl Verifier for ScriptVerifier {
    async fn verify(&self, env: &mut dyn BenchEnvironment) -> anyhow::Result<f64> {
        // Upload test files to /tests in container
        let test_sh = self.tests_dir.join("test.sh");
        if test_sh.exists() {
            env.upload_file(&test_sh, "/tests/test.sh").await?;
            env.exec("chmod +x /tests/test.sh").await?;
        }

        // Upload any additional test files (e.g. test_outputs.py)
        let entries = std::fs::read_dir(&self.tests_dir)?;
        for entry in entries {
            let entry = entry?;
            let path = entry.path();
            if path.is_file() {
                let file_name = path.file_name().unwrap_or_default().to_string_lossy();
                // Skip test.sh — already uploaded
                if file_name != "test.sh" {
                    let dest = format!("/tests/{file_name}");
                    env.upload_file(&path, &dest).await?;
                }
            }
        }

        // Ensure verifier log directory exists in container
        env.exec(&format!("mkdir -p {}", container_paths::VERIFIER_DIR))
            .await?;

        // Run the test script
        tracing::info!("Running test verification...");
        let result = env
            .exec_with_timeout("bash /tests/test.sh", self.timeout_secs)
            .await
            .context("test script execution failed")?;

        tracing::info!("Test script completed (exit {})", result.exit_code);

        // Parse reward.txt
        let reward_path = format!("{}/reward.txt", container_paths::VERIFIER_DIR);
        let cat_result = env.exec(&format!("cat {reward_path}")).await;

        let reward = match cat_result {
            Ok(r) if r.success() => {
                let text = r.stdout.trim();
                text.parse::<f64>().unwrap_or(0.0)
            }
            _ => {
                tracing::warn!("Could not read reward.txt, defaulting to 0.0");
                0.0
            }
        };

        let reward = reward.clamp(0.0, 1.0);
        tracing::info!("Verification reward: {}", reward);
        Ok(reward)
    }
}
