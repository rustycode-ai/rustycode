//! Native verifier — runs test.sh on the host filesystem.
//!
//! Works with the NativeEnvironment. Copies test files to the workspace,
//! runs test.sh with container paths rewritten for native execution,
//! and parses reward.txt from the verifier directory.

use std::path::PathBuf;

use anyhow::Context;

use super::Verifier;
use crate::environment::BenchEnvironment;

/// Verifier that works with both Docker and native environments.
///
/// For Docker: uploads test files and runs test.sh in the container.
/// For native: copies test files to workspace and runs test.sh locally.
pub struct NativeVerifier {
    /// Path to the tests directory (contains test.sh).
    tests_dir: PathBuf,
    /// Timeout for the test script in seconds.
    timeout_secs: u64,
}

impl NativeVerifier {
    #[must_use]
    pub const fn new(tests_dir: PathBuf, timeout_secs: u64) -> Self {
        Self {
            tests_dir,
            timeout_secs,
        }
    }
}

#[async_trait::async_trait]
impl Verifier for NativeVerifier {
    async fn verify(&self, env: &mut dyn BenchEnvironment) -> anyhow::Result<f64> {
        // Upload/copy test files
        let test_sh = self.tests_dir.join("test.sh");
        if test_sh.exists() {
            env.upload_file(&test_sh, "/tests/test.sh").await?;
            env.exec("chmod +x $WORKSPACE/tests/test.sh").await?;
        }

        // Upload additional test files
        let entries = std::fs::read_dir(&self.tests_dir)?;
        for entry in entries {
            let entry = entry?;
            let path = entry.path();
            if path.is_file() {
                let file_name = path.file_name().unwrap_or_default().to_string_lossy();
                if file_name != "test.sh" {
                    let dest = format!("/tests/{file_name}");
                    env.upload_file(&path, &dest).await?;
                }
            }
        }

        // Ensure verifier log directory exists
        env.exec("mkdir -p $WORKSPACE/logs/verifier").await?;

        // Run test script — native env rewrites container paths automatically
        tracing::info!("Running verification...");
        let test_script_path = self.tests_dir.join("test.sh");
        let result = env
            .exec_script(&test_script_path, self.timeout_secs)
            .await
            .context("test script execution failed")?;

        tracing::info!("Test script completed (exit {})", result.exit_code);

        // Try reward.json first (dict of scores)
        let json_result = env
            .exec("cat $WORKSPACE/logs/verifier/reward.json 2>/dev/null")
            .await;
        if let Ok(r) = &json_result {
            if r.success() && !r.stdout.trim().is_empty() {
                if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&r.stdout) {
                    // If it's a simple number, use it directly
                    if let Some(num) = parsed.as_f64() {
                        let reward = num.clamp(0.0, 1.0);
                        tracing::info!("Reward (json number): {reward}");
                        return Ok(reward);
                    }
                    // If it's an object with "reward" key, use that
                    if let Some(obj) = parsed.as_object() {
                        if let Some(reward_val) = obj.get("reward") {
                            if let Some(num) = reward_val.as_f64() {
                                let reward = num.clamp(0.0, 1.0);
                                tracing::info!("Reward (json object): {reward}");
                                return Ok(reward);
                            }
                        }
                        // Try "mean" or "score" keys
                        for key in ["mean", "score", "total", "average"] {
                            if let Some(val) = obj.get(key) {
                                if let Some(num) = val.as_f64() {
                                    let reward = num.clamp(0.0, 1.0);
                                    tracing::info!("Reward (json.{key}): {reward}");
                                    return Ok(reward);
                                }
                            }
                        }
                    }
                }
            }
        }

        // Fallback: parse reward.txt
        let reward_result = env
            .exec("cat $WORKSPACE/logs/verifier/reward.txt 2>/dev/null")
            .await;
        let reward = match reward_result {
            Ok(r) if r.success() && !r.stdout.trim().is_empty() => {
                let text = r.stdout.trim();
                // Try parsing as a simple float
                text.parse::<f64>().unwrap_or_else(|_| {
                    // Could be "X/Y tests passed" format
                    if let Some(frac) = text.split_once('/') {
                        let num: f64 = frac.0.trim().parse().unwrap_or(0.0);
                        let den: f64 = frac.1.trim().parse().unwrap_or(1.0);
                        if den > 0.0 {
                            num / den
                        } else {
                            0.0
                        }
                    } else {
                        // If "1" or "0" integer
                        match text {
                            "1" | "pass" | "true" | "yes" => 1.0,
                            "0" | "fail" | "false" | "no" => 0.0,
                            _ => 0.0,
                        }
                    }
                })
            }
            _ => {
                // If test script succeeded but no reward file, infer from exit code
                if result.success() {
                    tracing::info!("No reward file but tests passed, assuming reward=1.0");
                    1.0
                } else {
                    tracing::warn!("No reward file and tests failed, defaulting to 0.0");
                    0.0
                }
            }
        };

        let reward = reward.clamp(0.0, 1.0);
        tracing::info!("Verification reward: {reward}");
        Ok(reward)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    #[test]
    fn native_verifier_creation() {
        use super::*;
        let v = NativeVerifier::new("/tmp/tests".into(), 300);
        assert_eq!(v.tests_dir, PathBuf::from("/tmp/tests"));
        assert_eq!(v.timeout_secs, 300);
    }
}
