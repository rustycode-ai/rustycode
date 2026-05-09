//! Docker tools for container management

use crate::{ToolContext, ToolOutput};
use anyhow::Result;
use schemars::JsonSchema;
use serde_json::{json, Value};
use std::process::Command;

// Re-export all tools
pub use build::*;
pub use images::*;
pub use inspect::*;
pub use logs::*;
pub use ps::*;
pub use run::*;
pub use stop::*;

pub mod build;
pub mod images;
pub mod inspect;
pub mod logs;
pub mod ps;
pub mod run;
pub mod stop;

// ── Params structs ──────────────────────────────────────────────────────────

#[derive(serde::Deserialize, JsonSchema)]
pub struct DockerBuildParams {
    /// Image tag (e.g., 'myapp:latest' or 'myapp:v1.0')
    pub tag: String,
    /// Path to Dockerfile (default: 'Dockerfile' in current directory)
    pub dockerfile: Option<String>,
    /// Build context path (default: '.')
    pub context: Option<String>,
    /// Build arguments as key-value pairs (e.g., {"VERSION": "1.0"})
    pub build_args: Option<Value>,
    /// Target stage for multi-stage builds
    pub target: Option<String>,
    /// Disable cache (default: false)
    #[serde(default)]
    pub no_cache: bool,
}

#[derive(serde::Deserialize, JsonSchema)]
pub struct DockerRunParams {
    /// Docker image to run (e.g., 'ubuntu:latest' or 'myapp:v1.0')
    pub image: String,
    /// Command to run in the container
    pub command: Option<String>,
    /// Port mappings as host:container (e.g., {"8080": "80"})
    pub ports: Option<Value>,
    /// Volume mappings (e.g., {"/host/path": "/container/path"})
    pub volumes: Option<Value>,
    /// Environment variables (e.g., {"API_KEY": "secret"})
    pub environment: Option<Value>,
    /// Run in detached mode (default: true)
    #[serde(default = "default_true")]
    pub detach: bool,
    /// Auto-remove container on exit (default: false)
    #[serde(default)]
    pub remove: bool,
    /// Container name
    pub name: Option<String>,
    /// Working directory inside the container
    pub workdir: Option<String>,
    /// User to run as (e.g., "1000:1000")
    pub user: Option<String>,
    /// Add Linux capabilities (e.g., ["SYS_ADMIN"])
    pub cap_add: Option<Vec<String>>,
    /// Give extended privileges to the container (default: false)
    #[serde(default)]
    pub privileged: bool,
    /// Network mode to connect the container to
    pub network: Option<String>,
    /// Memory limit (e.g., "512m", "1g")
    pub memory_limit: Option<String>,
    /// CPU limit (e.g., "0.5" for 50% of one CPU)
    pub cpu_limit: Option<String>,
}

fn default_true() -> bool {
    true
}

#[derive(serde::Deserialize, JsonSchema)]
pub struct DockerPsParams {
    /// Show all containers (including stopped ones)
    #[serde(default)]
    pub all: bool,
    /// Only display container IDs
    #[serde(default)]
    pub quiet: bool,
    /// Format output using Go template (e.g., '{{.ID}}: {{.Names}}')
    pub format: Option<String>,
}

#[derive(serde::Deserialize, JsonSchema)]
pub struct DockerStopParams {
    /// Container ID(s) or name(s) to stop. Can be a single string or array of strings.
    pub containers: ContainersValue,
    /// Seconds to wait before killing (default: 10)
    pub time: Option<i64>,
}

/// Helper type to accept either a string or array of strings for containers parameter.
#[derive(serde::Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum ContainersValue {
    Single(String),
    Multiple(Vec<String>),
}

#[derive(serde::Deserialize, JsonSchema)]
pub struct DockerLogsParams {
    /// Container ID or name
    pub container: String,
    /// Follow log output (default: false)
    #[serde(default)]
    pub follow: bool,
    /// Number of lines to show from the end (default: 'all'). Use '100' for last 100 lines.
    pub tail: Option<String>,
    /// Show timestamps (default: false)
    #[serde(default)]
    pub timestamps: bool,
    /// Show logs since timestamp (e.g., '2023-01-01T00:00:00Z') or relative time (e.g., '10m')
    pub since: Option<String>,
}

#[derive(serde::Deserialize, JsonSchema)]
pub struct DockerInspectParams {
    /// Container ID, image name, or other Docker object to inspect
    pub target: String,
    /// Format output using Go template (e.g., '{{.Config.Image}}')
    pub format: Option<String>,
    /// Return JSON for specified type
    #[serde(rename = "type")]
    pub inspect_type: Option<String>,
}

#[derive(serde::Deserialize, JsonSchema)]
pub struct DockerImagesParams {
    /// Show all images (including intermediate layers)
    #[serde(default)]
    pub all: bool,
    /// Show only dangling images (untagged)
    #[serde(default)]
    pub dangling: bool,
    /// Only show image IDs
    #[serde(default)]
    pub quiet: bool,
    /// Format output using Go template
    pub format: Option<String>,
}

/// Helper function to run docker commands
pub(crate) fn run_docker(ctx: &ToolContext, args: &[&str]) -> Result<ToolOutput> {
    let output = Command::new("docker")
        .args(args)
        .current_dir(&ctx.cwd)
        .output()?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    if !output.status.success() && !stdout.is_empty() {
        // Docker commands sometimes output to stdout even on error
        return Ok(ToolOutput::text(stdout));
    }

    anyhow::ensure!(
        output.status.success(),
        "docker command failed: {}",
        stderr.trim()
    );

    let text = if stdout.is_empty() { stderr } else { stdout };

    let metadata = json!({
        "args": args,
        "exit_code": output.status.code().unwrap_or(-1)
    });

    Ok(ToolOutput::text(text).with_metadata(ctx, || metadata))
}

/// Extract image ID from docker build output
pub(crate) fn extract_image_id(output: &str) -> Option<String> {
    // Look for "Successfully built <sha256>" pattern
    for line in output.lines() {
        if line.contains("Successfully built") {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if let Some(id) = parts.last() {
                return Some(id.to_string());
            }
        }
        // Also look for SHA256: pattern
        if line.contains("SHA256:") {
            let parts: Vec<&str> = line.split_whitespace().collect();
            for part in parts {
                if part.starts_with("sha256:") || part.starts_with("SHA256:") {
                    return Some(part.to_string());
                }
            }
        }
    }
    None
}

#[cfg(test)]
pub(crate) mod tests_common {
    use super::*;

    /// Helper to create a ToolContext
    pub fn ctx() -> ToolContext {
        ToolContext::new(std::env::temp_dir())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_image_id() {
        let output =
            "Step 1/2 : FROM alpine\nStep 2/2 : RUN echo hello\nSuccessfully built abc123def456\n";
        let id = extract_image_id(output);
        assert_eq!(id, Some("abc123def456".to_string()));
    }

    #[test]
    fn test_extract_image_id_sha256() {
        let output = "Build result: SHA256:abc123def456";
        let id = extract_image_id(output);
        assert_eq!(id, Some("SHA256:abc123def456".to_string()));
    }

    #[test]
    fn test_extract_image_id_none() {
        let output = "No image ID in this output";
        let id = extract_image_id(output);
        assert_eq!(id, None);
    }
}
