//! Environment abstraction for benchmark containers.

pub mod bollard_env;
pub mod docker;
pub mod native;

pub use bollard_env::BollardEnvironment;
pub use docker::{container_paths, DockerEnvironment, EnvironmentConfig, TrialPaths};
pub use native::NativeEnvironment;

use std::path::{Path, PathBuf};

/// Result of executing a command inside a container.
#[derive(Debug, Clone)]
pub struct ExecResult {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
}

impl ExecResult {
    #[must_use]
    pub const fn success(&self) -> bool {
        self.exit_code == 0
    }
}

/// Container environment for running benchmark tasks.
///
/// Implementations manage container lifecycle (docker/podman compose)
/// and provide command execution, file upload/download capabilities.
#[async_trait::async_trait]
pub trait BenchEnvironment: Send + Sync {
    /// Start the container. If `force_build`, rebuild the image from Dockerfile
    /// instead of pulling a prebuilt image.
    async fn start(&mut self, force_build: bool) -> anyhow::Result<()>;

    /// Stop the container. If `delete`, remove images and volumes.
    async fn stop(&mut self, delete: bool) -> anyhow::Result<()>;

    /// Execute a command inside the container.
    async fn exec(&self, command: &str) -> anyhow::Result<ExecResult>;

    /// Execute a command with a timeout in seconds.
    async fn exec_with_timeout(
        &self,
        command: &str,
        timeout_secs: u64,
    ) -> anyhow::Result<ExecResult>;

    /// Upload a file from host into the container.
    async fn upload_file(&self, src: &Path, dest: &str) -> anyhow::Result<()>;

    /// Download a file from the container to the host.
    async fn download_file(&self, src: &str, dest: &Path) -> anyhow::Result<()>;

    /// Execute a script file, optionally rewriting container paths for native mode.
    ///
    /// Default implementation simply runs `bash <path>`. Native environments
    /// override to rewrite container paths in the script content.
    async fn exec_script(
        &self,
        script_path: &Path,
        timeout_secs: u64,
    ) -> anyhow::Result<ExecResult> {
        self.exec_with_timeout(&format!("bash {}", script_path.display()), timeout_secs)
            .await
    }

    /// Get the workspace directory path (for native environments).
    /// Returns None for container environments where the concept doesn't apply.
    fn workspace_path(&self) -> Option<PathBuf> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exec_result_success_zero_exit() {
        let r = ExecResult {
            stdout: "hello".to_string(),
            stderr: String::new(),
            exit_code: 0,
        };
        assert!(r.success());
    }

    #[test]
    fn exec_result_failure_nonzero_exit() {
        let r = ExecResult {
            stdout: String::new(),
            stderr: "error".to_string(),
            exit_code: 1,
        };
        assert!(!r.success());
    }

    #[test]
    fn exec_result_negative_exit_code() {
        let r = ExecResult {
            stdout: String::new(),
            stderr: "killed".to_string(),
            exit_code: -9,
        };
        assert!(!r.success());
    }

    #[test]
    fn exec_result_debug_format() {
        let r = ExecResult {
            stdout: "out".to_string(),
            stderr: "err".to_string(),
            exit_code: 0,
        };
        let debug = format!("{r:?}");
        assert!(debug.contains("out"));
        assert!(debug.contains("err"));
    }
}
