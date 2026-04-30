//! Docker/podman environment using `docker compose` CLI.

use std::path::{Path, PathBuf};
use std::process::Stdio;

use anyhow::{bail, Context};
use tokio::process::Command;

use super::{BenchEnvironment, ExecResult};

/// Container paths used by Harbor-compatible tasks.
pub mod container_paths {
    pub const LOGS_DIR: &str = "/logs";
    pub const VERIFIER_DIR: &str = "/logs/verifier";
    pub const AGENT_DIR: &str = "/logs/agent";
    pub const ARTIFACTS_DIR: &str = "/artifacts";
}

/// Configuration for a benchmark container environment.
#[derive(Debug, Clone)]
pub struct EnvironmentConfig {
    /// Path to the directory containing the Dockerfile.
    pub environment_dir: PathBuf,
    /// Number of CPUs allocated to the container.
    pub cpus: u32,
    /// Memory limit (e.g. "2G", "512M").
    pub memory: String,
    /// Optional prebuilt Docker image name.
    pub docker_image: Option<String>,
    /// Build timeout in seconds.
    pub build_timeout_secs: u64,
}

/// Host-side paths for trial artifacts.
#[derive(Debug, Clone)]
pub struct TrialPaths {
    /// Root directory for this trial's output.
    pub trial_dir: PathBuf,
    /// Host path for verifier logs (mounted into container).
    pub verifier_dir: PathBuf,
    /// Host path for agent logs (mounted into container).
    pub agent_dir: PathBuf,
    /// Host path for artifacts (mounted into container).
    pub artifacts_dir: PathBuf,
}

impl TrialPaths {
    #[must_use]
    pub fn new(trial_dir: PathBuf) -> Self {
        let verifier_dir = trial_dir.join("verifier");
        let agent_dir = trial_dir.join("agent");
        let artifacts_dir = trial_dir.join("artifacts");
        Self {
            trial_dir,
            verifier_dir,
            agent_dir,
            artifacts_dir,
        }
    }

    /// Create all directories.
    pub fn create_dirs(&self) -> anyhow::Result<()> {
        std::fs::create_dir_all(&self.verifier_dir)?;
        std::fs::create_dir_all(&self.agent_dir)?;
        std::fs::create_dir_all(&self.artifacts_dir)?;
        Ok(())
    }
}

/// Docker/podman environment using `docker compose` CLI.
///
/// Compatible with both Docker and podman (which provides a `docker` alias).
/// Uses `docker compose` (v2) for container lifecycle management.
pub struct DockerEnvironment {
    /// Unique session/project name for docker compose.
    session_id: String,
    /// Task-specific environment configuration.
    config: EnvironmentConfig,
    /// Host-side trial paths for mounted volumes.
    trial_paths: TrialPaths,
}

impl DockerEnvironment {
    #[must_use]
    pub const fn new(
        session_id: String,
        config: EnvironmentConfig,
        trial_paths: TrialPaths,
    ) -> Self {
        Self {
            session_id,
            config,
            trial_paths,
        }
    }

    /// Sanitize a name to be a valid docker compose project name.
    /// See: <https://docs.docker.com/compose/how-tos/project-name/>
    fn sanitize_project_name(name: &str) -> String {
        let mut name = name.to_lowercase();
        if name.is_empty() || !name.as_bytes()[0].is_ascii_alphanumeric() {
            name = format!("0{name}");
        }
        name.chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                    c
                } else {
                    '-'
                }
            })
            .collect()
    }

    /// Generate a docker-compose.yaml for this trial.
    fn generate_compose_file(&self, use_prebuilt: bool) -> String {
        let image_name = self.config.docker_image.as_deref().unwrap_or("");
        let build_or_image = if use_prebuilt && !image_name.is_empty() {
            format!(
                r"    image: {image_name}
    pull_policy: always"
            )
        } else {
            let context = self.config.environment_dir.display();
            format!(
                r"    build:
      context: {context}
    pull_policy: build"
            )
        };

        // Docker compose requires absolute paths for bind mounts.
        // Canonicalize to resolve relative paths and symlinks.
        let verifier_dir = std::fs::canonicalize(&self.trial_paths.verifier_dir)
            .unwrap_or_else(|_| self.trial_paths.verifier_dir.clone());
        let agent_dir = std::fs::canonicalize(&self.trial_paths.agent_dir)
            .unwrap_or_else(|_| self.trial_paths.agent_dir.clone());
        let artifacts_dir = std::fs::canonicalize(&self.trial_paths.artifacts_dir)
            .unwrap_or_else(|_| self.trial_paths.artifacts_dir.clone());

        let verifier_mount = format!(
            "{}:{}",
            verifier_dir.display(),
            container_paths::VERIFIER_DIR
        );
        let agent_mount = format!("{}:{}", agent_dir.display(), container_paths::AGENT_DIR);
        let artifacts_mount = format!(
            "{}:{}",
            artifacts_dir.display(),
            container_paths::ARTIFACTS_DIR
        );

        format!(
            r#"services:
  main:
{build_or_image}
    command: ["sh", "-c", "sleep infinity"]
    volumes:
      - {verifier_mount}
      - {agent_mount}
      - {artifacts_mount}
    deploy:
      resources:
        limits:
          cpus: {}
          memory: {}"#,
            self.config.cpus, self.config.memory,
        )
    }

    /// Write the compose file to disk for this trial.
    fn write_compose_file(&self, use_prebuilt: bool) -> anyhow::Result<PathBuf> {
        let compose_path = self.trial_paths.trial_dir.join("docker-compose.yaml");
        let content = self.generate_compose_file(use_prebuilt);
        std::fs::write(&compose_path, content)?;
        Ok(compose_path)
    }

    /// Run a docker compose command.
    async fn run_compose(
        &self,
        args: &[&str],
        timeout_secs: Option<u64>,
    ) -> anyhow::Result<ExecResult> {
        let project_name = Self::sanitize_project_name(&self.session_id);
        let compose_file = self.trial_paths.trial_dir.join("docker-compose.yaml");

        let mut full_args = vec![
            "compose",
            "--project-name",
            project_name.as_str(),
            "-f",
            compose_file.to_str().unwrap_or(""),
        ];
        full_args.extend_from_slice(args);

        let mut cmd = Command::new("docker");
        cmd.args(&full_args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        tracing::debug!("docker {}", full_args.join(" "));

        let output = if let Some(secs) = timeout_secs {
            let duration = std::time::Duration::from_secs(secs);
            tokio::time::timeout(duration, cmd.output()).await??
        } else {
            cmd.output().await?
        };

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();

        Ok(ExecResult {
            stdout,
            stderr,
            exit_code: output.status.code().unwrap_or(-1),
        })
    }

    /// Run `docker compose exec` to execute a command inside the running container.
    async fn compose_exec(
        &self,
        command: &str,
        timeout_secs: Option<u64>,
    ) -> anyhow::Result<ExecResult> {
        let project_name = Self::sanitize_project_name(&self.session_id);
        let compose_file = self.trial_paths.trial_dir.join("docker-compose.yaml");

        let args = vec![
            "compose",
            "--project-name",
            project_name.as_str(),
            "-f",
            compose_file.to_str().unwrap_or(""),
            "exec",
            "main",
            "bash",
            "-c",
            command,
        ];

        let mut cmd = Command::new("docker");
        cmd.args(&args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let output = if let Some(secs) = timeout_secs {
            let duration = std::time::Duration::from_secs(secs);
            tokio::time::timeout(duration, cmd.output()).await??
        } else {
            cmd.output().await?
        };

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();

        Ok(ExecResult {
            stdout,
            stderr,
            exit_code: output.status.code().unwrap_or(-1),
        })
    }
}

#[async_trait::async_trait]
impl BenchEnvironment for DockerEnvironment {
    async fn start(&mut self, force_build: bool) -> anyhow::Result<()> {
        self.trial_paths.create_dirs()?;

        let use_prebuilt = !force_build && self.config.docker_image.is_some();
        self.write_compose_file(use_prebuilt)?;

        // Build if needed
        if !use_prebuilt {
            tracing::info!("Building image for task {}...", self.session_id);
            let result = self
                .run_compose(&["build"], Some(self.config.build_timeout_secs))
                .await
                .context("docker compose build failed")?;

            if !result.success() {
                bail!(
                    "Build failed (exit {}): {}",
                    result.exit_code,
                    result.stderr
                );
            }
        }

        // Clean up any stale containers from previous runs
        let _ = self.run_compose(&["down", "--remove-orphans"], None).await;

        // Start the container
        tracing::info!("Starting container for task {}...", self.session_id);
        let result = self
            .run_compose(&["up", "--detach", "--wait"], Some(600))
            .await
            .context("docker compose up failed")?;

        if !result.success() {
            bail!(
                "Container start failed (exit {}): {}",
                result.exit_code,
                result.stderr
            );
        }

        // Make log directories world-writable inside container
        self.exec(&format!(
            "chmod 777 {} {}",
            container_paths::AGENT_DIR,
            container_paths::VERIFIER_DIR
        ))
        .await?;

        tracing::info!("Container for task {} is running", self.session_id);
        Ok(())
    }

    async fn stop(&mut self, delete: bool) -> anyhow::Result<()> {
        let args: &[&str] = if delete {
            &["down", "--rmi", "all", "--volumes", "--remove-orphans"]
        } else {
            &["down", "--remove-orphans"]
        };

        match self.run_compose(args, None).await {
            Ok(result) => {
                tracing::info!(
                    "Container stopped for task {} (exit {})",
                    self.session_id,
                    result.exit_code
                );
            }
            Err(e) => {
                tracing::warn!("Container stop failed for task {}: {}", self.session_id, e);
            }
        }
        Ok(())
    }

    async fn exec(&self, command: &str) -> anyhow::Result<ExecResult> {
        self.compose_exec(command, None).await
    }

    async fn exec_with_timeout(
        &self,
        command: &str,
        timeout_secs: u64,
    ) -> anyhow::Result<ExecResult> {
        self.compose_exec(command, Some(timeout_secs)).await
    }

    async fn upload_file(&self, src: &Path, dest: &str) -> anyhow::Result<()> {
        let project_name = Self::sanitize_project_name(&self.session_id);
        let compose_file = self.trial_paths.trial_dir.join("docker-compose.yaml");
        let dest_path = format!("main:{dest}");

        let args = vec![
            "compose",
            "--project-name",
            project_name.as_str(),
            "-f",
            compose_file.to_str().unwrap_or(""),
            "cp",
            src.to_str().unwrap_or(""),
            &dest_path,
        ];

        let output = Command::new("docker")
            .args(&args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await?;

        if !output.status.success() {
            bail!(
                "docker cp failed: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
        Ok(())
    }

    async fn download_file(&self, src: &str, dest: &Path) -> anyhow::Result<()> {
        let project_name = Self::sanitize_project_name(&self.session_id);
        let compose_file = self.trial_paths.trial_dir.join("docker-compose.yaml");
        let src_path = format!("main:{src}");

        let args = vec![
            "compose",
            "--project-name",
            project_name.as_str(),
            "-f",
            compose_file.to_str().unwrap_or(""),
            "cp",
            &src_path,
            dest.to_str().unwrap_or(""),
        ];

        let output = Command::new("docker")
            .args(&args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await?;

        if !output.status.success() {
            bail!(
                "docker cp failed: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
        Ok(())
    }
}
