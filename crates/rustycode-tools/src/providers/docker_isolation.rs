//! Docker-based per-execution isolation for tool commands.
//!
//! Provides container-level isolation for bash command execution:
//! - Each command runs in a fresh, ephemeral Docker container
//! - Workspace directory is mounted read-write at the same path
//! - Network isolation by default (opt-in per command)
//! - Resource limits (memory, CPU, timeout)
//! - Automatic cleanup with `--rm`
//!
//! # Architecture
//!
//! When Docker isolation is enabled:
//! 1. `BashTool` checks `SandboxConfig` for isolation mode
//! 2. If Docker isolation is requested, delegates to `DockerIsolation`
//! 3. A container is created, command runs, output captured, container removed
//!
//! Falls back to normal bash execution if Docker is unavailable.

use std::path::{Path, PathBuf};
use std::process::Command;

/// Configuration for Docker-based command isolation.
#[derive(Debug, Clone)]
pub struct DockerIsolationConfig {
    /// Docker image to use for isolation (default: "ubuntu:22.04")
    pub image: String,
    /// Memory limit per container (e.g., "512m", "1g")
    pub memory_limit: String,
    /// CPU period in microseconds (default: 100000 = 100ms)
    pub cpu_period: u64,
    /// CPU quota in microseconds (default: 50000 = 50% of one core)
    pub cpu_quota: u64,
    /// Whether to allow network access (default: false)
    pub network_enabled: bool,
    /// Maximum execution time in seconds (default: 120)
    pub timeout_secs: u64,
    /// Additional read-only mounts
    pub read_only_mounts: Vec<(PathBuf, PathBuf)>,
    /// Docker user to run as (e.g., "1000:1000")
    pub user: Option<String>,
}

impl Default for DockerIsolationConfig {
    fn default() -> Self {
        Self {
            image: "ubuntu:22.04".to_string(),
            memory_limit: "512m".to_string(),
            cpu_period: 100_000,
            cpu_quota: 50_000,
            network_enabled: false,
            timeout_secs: 120,
            read_only_mounts: Vec::new(),
            user: None,
        }
    }
}

impl DockerIsolationConfig {
    pub fn new() -> Self {
        Self::default()
    }

    /// Use a specific Docker image.
    #[must_use]
    pub fn with_image(mut self, image: impl Into<String>) -> Self {
        self.image = image.into();
        self
    }

    /// Set memory limit.
    pub fn with_memory(mut self, limit: impl Into<String>) -> Self {
        self.memory_limit = limit.into();
        self
    }

    /// Enable network access inside the container.
    pub const fn with_network(mut self) -> Self {
        self.network_enabled = true;
        self
    }

    /// Set the Docker user (UID:GID format).
    pub fn with_user(mut self, user: impl Into<String>) -> Self {
        self.user = Some(user.into());
        self
    }
}

/// Result of a Docker-isolated command execution.
#[derive(Debug)]
pub struct IsolatedCommandResult {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
    pub container_id: String,
    pub duration_ms: u128,
}

/// Docker-based command isolation executor.
///
/// Runs commands in ephemeral Docker containers for complete isolation.
#[derive(Debug, Clone)]
pub struct DockerIsolation {
    config: DockerIsolationConfig,
}

impl DockerIsolation {
    pub const fn new(config: DockerIsolationConfig) -> Self {
        Self { config }
    }

    /// Check if Docker is available on this system.
    pub fn is_docker_available() -> bool {
        Command::new("docker")
            .arg("info")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    /// Execute a command in an isolated Docker container.
    ///
    pub fn execute(
        &self,
        command: &str,
        workspace: &Path,
    ) -> anyhow::Result<IsolatedCommandResult> {
        let start = std::time::Instant::now();
        let container_id = format!("rustycode-{}", uuid::Uuid::new_v4().as_simple());

        let workspace_str = workspace.to_string_lossy().to_string();

        let mut docker_args = vec![
            "run".to_string(),
            "--name".to_string(),
            container_id.clone(),
            "--rm".to_string(), // Auto-remove on exit
            "-v".to_string(),
            format!("{}:{}", workspace_str, workspace_str),
            "-w".to_string(),
            workspace_str.clone(),
            "--memory".to_string(),
            self.config.memory_limit.clone(),
            "--cpu-period".to_string(),
            self.config.cpu_period.to_string(),
            "--cpu-quota".to_string(),
            self.config.cpu_quota.to_string(),
        ];

        // Network isolation
        if !self.config.network_enabled {
            docker_args.push("--network".to_string());
            docker_args.push("none".to_string());
        }

        // User mapping
        if let Some(ref user) = self.config.user {
            docker_args.push("--user".to_string());
            docker_args.push(user.clone());
        }

        // Read-only mounts
        for (host_path, container_path) in &self.config.read_only_mounts {
            docker_args.push("-v".to_string());
            docker_args.push(format!(
                "{}:{}:ro",
                host_path.to_string_lossy(),
                container_path.to_string_lossy()
            ));
        }

        // Security options
        docker_args.push("--security-opt".to_string());
        docker_args.push("no-new-privileges:true".to_string());

        // Prevent container from gaining additional capabilities
        docker_args.push("--cap-drop".to_string());
        docker_args.push("ALL".to_string());

        // Image and command
        docker_args.push(self.config.image.clone());
        docker_args.push("Bash".to_string());
        docker_args.push("-c".to_string());
        docker_args.push(command.to_string());

        let timeout = std::time::Duration::from_secs(self.config.timeout_secs);
        let docker_args_clone = docker_args.clone();

        let (tx, rx) = std::sync::mpsc::channel();
        let handle = std::thread::spawn(move || {
            let result = Command::new("docker").args(&docker_args_clone).output();
            let _ = tx.send(result);
        });

        let output = match rx.recv_timeout(timeout) {
            Ok(result) => result.map_err(|e| anyhow::anyhow!("failed to execute docker: {e}"))?,
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                // Try to stop the container to clean up
                let _ = Command::new("docker")
                    .args(["stop", &container_id])
                    .output();
                let _ = handle.join();
                return Ok(IsolatedCommandResult {
                    stdout: String::new(),
                    stderr: format!("command timed out after {}s", self.config.timeout_secs),
                    exit_code: 124,
                    container_id,
                    duration_ms: start.elapsed().as_millis(),
                });
            }
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                let _ = handle.join();
                return Err(anyhow::anyhow!("docker execution thread panicked"));
            }
        };

        let duration_ms = start.elapsed().as_millis();
        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        let exit_code = output.status.code().unwrap_or(-1);

        Ok(IsolatedCommandResult {
            stdout,
            stderr,
            exit_code,
            container_id,
            duration_ms,
        })
    }

    /// Stop a running container by ID.
    pub fn stop_container(container_id: &str) -> anyhow::Result<()> {
        let status = Command::new("docker")
            .args(["stop", container_id])
            .status()
            .map_err(|e| anyhow::anyhow!("failed to stop container {container_id}: {e}"))?;

        if status.success() {
            Ok(())
        } else {
            Err(anyhow::anyhow!("failed to stop container {container_id}"))
        }
    }

    /// Get the current configuration.
    pub const fn config(&self) -> &DockerIsolationConfig {
        &self.config
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = DockerIsolationConfig::default();
        assert_eq!(config.image, "ubuntu:22.04");
        assert_eq!(config.memory_limit, "512m");
        assert!(!config.network_enabled);
        assert_eq!(config.timeout_secs, 120);
        assert!(config.user.is_none());
    }

    #[test]
    fn test_config_builder() {
        let config = DockerIsolationConfig::new()
            .with_image("alpine:3.19")
            .with_memory("1g")
            .with_network()
            .with_user("1000:1000");

        assert_eq!(config.image, "alpine:3.19");
        assert_eq!(config.memory_limit, "1g");
        assert!(config.network_enabled);
        assert_eq!(config.user.as_deref(), Some("1000:1000"));
    }

    #[test]
    fn test_isolation_creation() {
        let config = DockerIsolationConfig::new();
        let isolation = DockerIsolation::new(config);
        assert_eq!(isolation.config().image, "ubuntu:22.04");
    }

    #[test]
    fn test_container_id_format() {
        // Verify the container ID prefix
        let id = format!("rustycode-{}", uuid::Uuid::new_v4().as_simple());
        assert!(id.starts_with("rustycode-"));
        assert!(id.len() > "rustycode-".len());
    }

    #[test]
    fn test_docker_args_construction() {
        let config = DockerIsolationConfig::new();
        let isolation = DockerIsolation::new(config);

        // Verify config values that feed into docker args
        assert_eq!(isolation.config().memory_limit, "512m");
        assert_eq!(isolation.config().cpu_period, 100_000);
        assert_eq!(isolation.config().cpu_quota, 50_000);
        assert_eq!(isolation.config().image, "ubuntu:22.04");
    }

    #[test]
    fn test_network_disabled_by_default() {
        let config = DockerIsolationConfig::default();
        assert!(
            !config.network_enabled,
            "Network should be disabled by default"
        );
    }

    #[test]
    fn test_security_options() {
        let config = DockerIsolationConfig::new();

        // Verify security defaults are configured
        // no-new-privileges + cap-drop ALL is applied in execute()
        assert!(!config.network_enabled);
        assert!(config.memory_limit.contains('m') || config.memory_limit.contains('g'));
    }

    #[test]
    fn test_read_only_mounts() {
        let config = DockerIsolationConfig {
            read_only_mounts: vec![(
                PathBuf::from("/usr/local/bin"),
                PathBuf::from("/usr/local/bin"),
            )],
            ..Default::default()
        };

        assert_eq!(config.read_only_mounts.len(), 1);
        assert_eq!(
            config.read_only_mounts[0].0,
            PathBuf::from("/usr/local/bin")
        );
    }

    // Integration tests (require Docker)
    #[test]
    #[ignore = "requires Docker"]
    fn test_docker_available() {
        // This test checks if Docker is running on the system
        let available = DockerIsolation::is_docker_available();
        // Don't assert true - Docker may not be installed in CI
        // Just verify the check doesn't panic
        let _ = available;
    }

    #[test]
    #[ignore = "requires Docker"]
    fn test_isolated_command_execution() {
        let isolation = DockerIsolation::new(DockerIsolationConfig::new());
        let workspace = PathBuf::from("/tmp");

        let result = isolation
            .execute("echo 'hello from container'", &workspace)
            .unwrap();
        assert!(result.stdout.contains("hello from container"));
        assert_eq!(result.exit_code, 0);
        assert!(result.duration_ms > 0);
    }

    #[test]
    #[ignore = "requires Docker"]
    fn test_isolated_command_failure() {
        let isolation = DockerIsolation::new(DockerIsolationConfig::new());
        let workspace = PathBuf::from("/tmp");

        let result = isolation.execute("exit 42", &workspace).unwrap();
        assert_eq!(result.exit_code, 42);
    }

    #[test]
    #[ignore = "requires Docker"]
    fn test_isolated_command_no_network() {
        let isolation = DockerIsolation::new(DockerIsolationConfig::new());
        let workspace = PathBuf::from("/tmp");

        // curl should fail with no network
        let result = isolation
            .execute(
                "curl -s https://example.com 2>&1 || echo 'no network'",
                &workspace,
            )
            .unwrap();
        assert!(result.stdout.contains("no network") || result.exit_code != 0);
    }

    #[test]
    #[ignore = "requires Docker"]
    fn test_isolated_command_with_network() {
        let config = DockerIsolationConfig::new().with_network();
        let isolation = DockerIsolation::new(config);
        let workspace = PathBuf::from("/tmp");

        let result = isolation
            .execute(
                "curl -s -o /dev/null -w '%{http_code}' https://example.com",
                &workspace,
            )
            .unwrap();
        assert!(result.stdout.contains("200") || result.exit_code == 0);
    }

    #[test]
    #[ignore = "requires Docker"]
    fn test_workspace_mount() {
        let isolation = DockerIsolation::new(DockerIsolationConfig::new());
        let workspace = PathBuf::from("/tmp");

        // Write a file and verify it's visible inside container
        let test_file = "/tmp/rustycode_docker_test.txt";
        let result = isolation
            .execute(
                &format!("echo 'test' > {} && cat {}", test_file, test_file),
                &workspace,
            )
            .unwrap();
        assert!(result.stdout.contains("test"));

        // Cleanup
        let _ = std::fs::remove_file(test_file);
    }
}
