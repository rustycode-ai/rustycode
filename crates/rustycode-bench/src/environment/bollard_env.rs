//! Docker environment using bollard (Rust Docker API).
//!
//! This is the BollardEnvironment implementation used by the TB2 harness.
//! It talks directly to the Docker daemon through bollard, unlike the
//! existing DockerEnvironment which shells out to `docker compose`.

use std::path::{Path, PathBuf};

use anyhow::{bail, Context};
use bollard::container::{
    Config, CreateContainerOptions, RemoveContainerOptions, StartContainerOptions,
};
use bollard::exec::{CreateExecOptions, StartExecOptions, StartExecResults};
use bollard::image::BuildImageOptions;
use bollard::models::BuildInfo;
use bollard::Docker;

use super::{BenchEnvironment, ExecResult};

/// bollard-based Docker environment implementing `BenchEnvironment`.
pub struct BollardEnvironment {
    docker: Docker,
    container_name: String,
    dockerfile_dir: PathBuf,
    image_tag: String,
    cpus: u32,
    memory: String,
    container_id: Option<String>,
    #[allow(dead_code)]
    build_timeout_secs: f64,
}

impl BollardEnvironment {
    pub fn new(
        container_name: String,
        dockerfile_dir: PathBuf,
        image_tag: String,
        cpus: u32,
        memory: String,
        build_timeout_secs: f64,
    ) -> anyhow::Result<Self> {
        let docker =
            Docker::connect_with_local_defaults().context("Failed to connect to Docker daemon")?;
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
        let tar_bytes = create_context_tar(&self.dockerfile_dir)?;

        let options = BuildImageOptions {
            dockerfile: "Dockerfile",
            t: self.image_tag.as_str(),
            forcerm: true,
            ..Default::default()
        };

        let mut stream = self
            .docker
            .build_image(options, None, Some(tar_bytes.into()));

        use futures::StreamExt;
        while let Some(msg) = stream.next().await {
            match msg {
                Ok(BuildInfo {
                    stream: Some(s), ..
                }) => {
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
impl BenchEnvironment for BollardEnvironment {
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
            name: self.container_name.clone(),
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
            cmd: Some(vec![
                "bash".to_string(),
                "-c".to_string(),
                command.to_string(),
            ]),
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

                let mut stdout = String::new();
                let mut stderr = String::new();

                let timeout_result =
                    tokio::time::timeout(std::time::Duration::from_secs(timeout_secs), async {
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
                    })
                    .await;

                if timeout_result.is_err() {
                    return Ok(ExecResult {
                        stdout,
                        stderr: format!("{stderr}\nexec timed out after {timeout_secs}s"),
                        exit_code: -1,
                    });
                }

                let inspect = self
                    .docker
                    .inspect_exec(&exec_result.id)
                    .await
                    .context("Failed to inspect exec")?;

                let exit_code = inspect.exit_code.unwrap_or(-1) as i32;

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

        let content =
            std::fs::read(src).with_context(|| format!("Failed to read {}", src.display()))?;

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

        let options = bollard::container::UploadToContainerOptions {
            path: Path::new(dest)
                .parent()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|| "/".to_string()),
            ..Default::default()
        };

        self.docker
            .upload_to_container(id, Some(options), tar_buf.into())
            .await
            .context("Failed to upload file to container")?;

        Ok(())
    }

    async fn download_file(&self, src: &str, dest: &Path) -> anyhow::Result<()> {
        let id = self
            .container_id
            .as_ref()
            .context("Container not started")?;

        let options = bollard::container::DownloadFromContainerOptions::<String> {
            path: src.to_string(),
        };

        let stream = self.docker.download_from_container(id, Some(options));

        use futures::StreamExt;
        use tokio::io::AsyncWriteExt;

        let mut writer = tokio::io::BufWriter::new(Vec::new());
        let mut stream = std::pin::pin!(stream);
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.context("download stream error")?;
            writer.write_all(&chunk).await?;
        }
        writer.flush().await?;
        let bytes = writer.into_inner();

        let mut archive = tar::Archive::new(bytes.as_slice());
        let mut found = false;
        for entry in archive.entries()? {
            let mut entry = entry?;
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
    num_part
        .parse::<u64>()
        .unwrap_or(2 * 1024 * 1024 * 1024)
        .saturating_mul(multiplier)
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
