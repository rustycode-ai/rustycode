// Copyright 2025 The RustyCode Authors. All rights reserved.
// Use of this source code is governed by an MIT-style license.

//! Common test helpers and utilities for integration tests

use std::path::{Path, PathBuf};
use std::fs;
use std::time::Duration;
use tempfile::TempDir;
use tokio::time::sleep;

/// Test configuration builder
pub struct TestConfig {
    pub temp_dir: TempDir,
    pub project_dir: PathBuf,
    pub config_dir: PathBuf,
    pub data_dir: PathBuf,
}

impl TestConfig {
    /// Create a new test configuration with temporary directories
    pub fn new() -> Self {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let project_dir = temp_dir.path().join("test_project");
        let config_dir = project_dir.join(".rustycode");
        let data_dir = temp_dir.path().join("data");

        fs::create_dir_all(&config_dir).expect("Failed to create config dir");
        fs::create_dir_all(&data_dir).expect("Failed to create data dir");

        Self {
            temp_dir,
            project_dir,
            config_dir,
            data_dir,
        }
    }

    /// Write a config file to the test project
    pub fn write_config(&self, name: &str, content: &str) -> PathBuf {
        let config_path = self.config_dir.join(name);
        fs::write(&config_path, content).expect("Failed to write config");
        config_path
    }

    /// Get the project directory path
    pub fn project_dir(&self) -> &Path {
        &self.project_dir
    }

    /// Get the config directory path
    pub fn config_dir(&self) -> &Path {
        &self.config_dir
    }

    /// Get the data directory path
    pub fn data_dir(&self) -> &Path {
        &self.data_dir
    }
}

/// Test environment helper for managing environment variables
pub struct TestEnv {
    preserved_vars: Vec<(String, Option<String>)>,
}

impl TestEnv {
    /// Create a new test environment helper
    pub fn new() -> Self {
        Self {
            preserved_vars: Vec::new(),
        }
    }

    /// Set an environment variable for testing
    pub fn set(&mut self, key: &str, value: &str) {
        // Preserve original value if it exists
        let original = std::env::var(key).ok();
        self.preserved_vars.push((key.to_string(), original));

        std::env::set_var(key, value);
    }

    /// Remove an environment variable for testing
    pub fn remove(&mut self, key: &str) {
        let original = std::env::var(key).ok();
        self.preserved_vars.push((key.to_string(), original));

        std::env::remove_var(key);
    }
}

impl Drop for TestEnv {
    fn drop(&mut self) {
        // Restore original environment variables
        for (key, original) in &self.preserved_vars {
            match original {
                Some(value) => std::env::set_var(key, value),
                None => std::env::remove_var(key),
            }
        }
    }
}

/// Retry helper for async operations
pub async fn retry_async<F, Fut, T, E>(
    mut operation: F,
    max_attempts: usize,
    delay: Duration,
) -> Result<T, E>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<T, E>>,
{
    let mut last_error = None;

    for attempt in 0..max_attempts {
        match operation().await {
            Ok(result) => return Ok(result),
            Err(e) => {
                last_error = Some(e);
                if attempt < max_attempts - 1 {
                    sleep(delay).await;
                }
            }
        }
    }

    Err(last_error.expect("Should have error after retries"))
}

/// Assert that two floats are approximately equal
pub fn assert_approx_eq(a: f64, b: f64, epsilon: f64) {
    let diff = (a - b).abs();
    assert!(
        diff < epsilon,
        "Values are not approximately equal: {} vs {} (diff: {})",
        a,
        b,
        diff
    );
}

/// Cleanup test data helper
pub async fn cleanup_test_data(dir: &Path) {
    if dir.exists() {
        let _ = fs::remove_dir_all(dir);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_test_config_creation() {
        let config = TestConfig::new();
        assert!(config.project_dir().exists());
        assert!(config.config_dir().exists());
        assert!(config.data_dir().exists());
    }

    #[test]
    fn test_test_config_write_config() {
        let config = TestConfig::new();
        let config_path = config.write_config("test.json", r#"{"test": true}"#);
        assert!(config_path.exists());
        let content = fs::read_to_string(&config_path).unwrap();
        assert_eq!(content, r#"{"test":true}"#);
    }

    #[test]
    fn test_assert_approx_eq() {
        assert_approx_eq(1.0, 1.0, 0.001);
        assert_approx_eq(1.0, 1.0005, 0.001);
        assert_approx_eq(1.0, 0.9995, 0.001);
    }

    #[test]
    #[should_panic]
    fn test_assert_approx_eq_panics() {
        assert_approx_eq(1.0, 2.0, 0.001);
    }

    #[tokio::test]
    async fn test_retry_async_success() {
        let mut attempts = 0;
        let result = retry_async(
            || {
                attempts += 1;
                async {
                    if attempts < 3 {
                        Err::<(), _>("not ready")
                    } else {
                        Ok(())
                    }
                }
            },
            5,
            Duration::from_millis(10),
        )
        .await;

        assert!(result.is_ok());
        assert_eq!(attempts, 3);
    }

    #[tokio::test]
    async fn test_retry_async_failure() {
        let result = retry_async(
            || async { Err::<(), _>("always fails") },
            3,
            Duration::from_millis(10),
        )
        .await;

        assert!(result.is_err());
    }

    #[test]
    fn test_test_env_set_and_restore() {
        let original = std::env::var("TEST_VAR_RUSTYCODE").ok();

        {
            let mut env = TestEnv::new();
            env.set("TEST_VAR_RUSTYCODE", "test_value");
            assert_eq!(std::env::var("TEST_VAR_RUSTYCODE").unwrap(), "test_value");
        }

        // Should be restored
        match original {
            Some(value) => assert_eq!(std::env::var("TEST_VAR_RUSTYCODE").unwrap(), value),
            None => assert!(std::env::var("TEST_VAR_RUSTYCODE").is_err()),
        }
    }

    #[test]
    fn test_test_env_remove_and_restore() {
        std::env::set_var("TEST_VAR_RUSTYCODE_2", "original");

        {
            let mut env = TestEnv::new();
            env.remove("TEST_VAR_RUSTYCODE_2");
            assert!(std::env::var("TEST_VAR_RUSTYCODE_2").is_err());
        }

        // Should be restored
        assert_eq!(
            std::env::var("TEST_VAR_RUSTYCODE_2").unwrap(),
            "original"
        );
    }
}
