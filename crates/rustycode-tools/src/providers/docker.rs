//! Docker tools for container management
//!
//! This module provides tools for interacting with Docker:
//! - `DockerBuildTool`: Build Docker images
//! - `DockerRunTool`: Run Docker containers
//! - `DockerPsTool`: List running containers
//! - `DockerStopTool`: Stop containers
//! - `DockerLogsTool`: View container logs
//! - `DockerInspectTool`: Inspect containers/images

use crate::{ToolContext, ToolOutput, ToolPermission, ToolTag};
use anyhow::{anyhow, Result};
use schemars::JsonSchema;
use serde_json::{json, Value};
use std::process::Command;

// ── Params structs ──────────────────────────────────────────────────────────

#[derive(serde::Deserialize, JsonSchema)]
pub struct DockerBuildParams {
    /// Image tag (e.g., 'myapp:latest' or 'myapp:v1.0')
    tag: String,
    /// Path to Dockerfile (default: 'Dockerfile' in current directory)
    dockerfile: Option<String>,
    /// Build context path (default: '.')
    context: Option<String>,
    /// Build arguments as key-value pairs (e.g., {"VERSION": "1.0"})
    build_args: Option<Value>,
    /// Target stage for multi-stage builds
    target: Option<String>,
    /// Disable cache (default: false)
    #[serde(default)]
    no_cache: bool,
}

#[derive(serde::Deserialize, JsonSchema)]
pub struct DockerRunParams {
    /// Docker image to run (e.g., 'ubuntu:latest' or 'myapp:v1.0')
    image: String,
    /// Command to run in the container
    command: Option<String>,
    /// Port mappings as host:container (e.g., {"8080": "80"})
    ports: Option<Value>,
    /// Volume mappings (e.g., {"/host/path": "/container/path"})
    volumes: Option<Value>,
    /// Environment variables (e.g., {"API_KEY": "secret"})
    environment: Option<Value>,
    /// Run in detached mode (default: true)
    #[serde(default = "default_true")]
    detach: bool,
    /// Auto-remove container on exit (default: false)
    #[serde(default)]
    remove: bool,
    /// Container name
    name: Option<String>,
    /// Working directory inside the container
    workdir: Option<String>,
    /// User to run as (e.g., "1000:1000")
    user: Option<String>,
    /// Add Linux capabilities (e.g., ["SYS_ADMIN"])
    cap_add: Option<Vec<String>>,
    /// Give extended privileges to the container (default: false)
    #[serde(default)]
    privileged: bool,
    /// Network mode to connect the container to
    network: Option<String>,
    /// Memory limit (e.g., "512m", "1g")
    memory_limit: Option<String>,
    /// CPU limit (e.g., "0.5" for 50% of one CPU)
    cpu_limit: Option<String>,
}

fn default_true() -> bool {
    true
}

#[derive(serde::Deserialize, JsonSchema)]
pub struct DockerPsParams {
    /// Show all containers (including stopped ones)
    #[serde(default)]
    all: bool,
    /// Only display container IDs
    #[serde(default)]
    quiet: bool,
    /// Format output using Go template (e.g., '{{.ID}}: {{.Names}}')
    format: Option<String>,
}

#[derive(serde::Deserialize, JsonSchema)]
pub struct DockerStopParams {
    /// Container ID(s) or name(s) to stop. Can be a single string or array of strings.
    containers: ContainersValue,
    /// Seconds to wait before killing (default: 10)
    time: Option<i64>,
}

/// Helper type to accept either a string or array of strings for containers parameter.
#[derive(serde::Deserialize, JsonSchema)]
#[serde(untagged)]
enum ContainersValue {
    Single(String),
    Multiple(Vec<String>),
}

#[derive(serde::Deserialize, JsonSchema)]
pub struct DockerLogsParams {
    /// Container ID or name
    container: String,
    /// Follow log output (default: false)
    #[serde(default)]
    follow: bool,
    /// Number of lines to show from the end (default: 'all'). Use '100' for last 100 lines.
    tail: Option<String>,
    /// Show timestamps (default: false)
    #[serde(default)]
    timestamps: bool,
    /// Show logs since timestamp (e.g., '2023-01-01T00:00:00Z') or relative time (e.g., '10m')
    since: Option<String>,
}

#[derive(serde::Deserialize, JsonSchema)]
pub struct DockerInspectParams {
    /// Container ID, image name, or other Docker object to inspect
    target: String,
    /// Format output using Go template (e.g., '{{.Config.Image}}')
    format: Option<String>,
    /// Return JSON for specified type
    #[serde(rename = "type")]
    inspect_type: Option<String>,
}

#[derive(serde::Deserialize, JsonSchema)]
pub struct DockerImagesParams {
    /// Show all images (including intermediate layers)
    #[serde(default)]
    all: bool,
    /// Show only dangling images (untagged)
    #[serde(default)]
    dangling: bool,
    /// Only show image IDs
    #[serde(default)]
    quiet: bool,
    /// Format output using Go template
    format: Option<String>,
}

// ── Tool definitions ────────────────────────────────────────────────────────

rustycode_tools_api::define_tool! {
    pub struct DockerBuildTool;

    name: "docker_build",
    description: r#"Build a Docker image from a Dockerfile

Use this tool to:
- Build Docker images from a Dockerfile in the current directory
- Tag the built image with a specific name
- Specify build arguments
- Set the build context path

**Examples:**
- Build image with default name: tag the image as "myapp:latest"
- Build with custom Dockerfile: specify dockerfile path
- Build with build args: pass build arguments like "VERSION=1.0"

**Note:** This requires Docker to be installed and running on the system."#,
    permission: ToolPermission::Execute,
    tags: [ToolTag::Ops],

    execute(params: DockerBuildParams, ctx) {
        let tag = &params.tag;
        let dockerfile = params.dockerfile.as_deref().unwrap_or("Dockerfile");
        let context = params.context.as_deref().unwrap_or(".");
        let no_cache = params.no_cache;
        let target = params.target.as_deref();

        // Build command args as owned strings
        let mut args_vec: Vec<String> =
            vec!["build".to_string(), "-t".to_string(), tag.to_string()];

        // Add Dockerfile if specified
        if dockerfile != "Dockerfile" {
            args_vec.push("-f".to_string());
            args_vec.push(dockerfile.to_string());
        }

        // Add no-cache flag
        if no_cache {
            args_vec.push("--no-cache".to_string());
        }

        // Add build args
        if let Some(ref build_args) = params.build_args {
            if let Some(build_args_obj) = build_args.as_object() {
                for (key, value) in build_args_obj {
                    if let Some(value_str) = value.as_str() {
                        args_vec.push("--build-arg".to_string());
                        args_vec.push(format!("{key}={value_str}"));
                    }
                }
            }
        }

        // Add target if specified
        if let Some(target_stage) = target {
            args_vec.push("--target".to_string());
            args_vec.push(target_stage.to_string());
        }

        // Add context path last
        args_vec.push(context.to_string());

        // Convert to string slices for run_docker
        let args: Vec<&str> = args_vec.iter().map(String::as_str).collect();
        let result = run_docker(ctx, &args)?;

        // Extract image ID from output
        let image_id = extract_image_id(result.text.as_str());

        let mut structured = result.structured.unwrap_or(json!({}));
        structured["tag"] = json!(tag);
        structured["dockerfile"] = json!(dockerfile);
        structured["context"] = json!(context);
        if let Some(id) = image_id {
            structured["image_id"] = json!(id);
        }

        Ok(ToolOutput::with_structured(result.text, structured))
    }
}

rustycode_tools_api::define_tool! {
    pub struct DockerRunTool;

    name: "docker_run",
    description: r"Run a Docker container

Use this tool to:
- Run a container from an image
- Configure ports, volumes, and environment variables
- Run in detached or interactive mode
- Auto-remove the container after exit

**Examples:**
- Run basic container: specify image name
- Run with ports: map host ports to container ports
- Run with volumes: mount host directories into container
- Run with environment: pass environment variables

**Note:** This requires Docker to be installed and running on the system.",
    permission: ToolPermission::Execute,
    tags: [ToolTag::Ops],

    execute(params: DockerRunParams, ctx) {
        let image = &params.image;
        let detach = params.detach;
        let remove = params.remove;
        let privileged = params.privileged;

        // Build command args as owned strings
        let mut args_vec: Vec<String> = vec!["run".to_string()];

        // Add detach flag
        if detach {
            args_vec.push("-d".to_string());
        }

        // Add remove flag
        if remove {
            args_vec.push("--rm".to_string());
        }

        // Add name if specified
        if let Some(ref container_name) = params.name {
            args_vec.push("--name".to_string());
            args_vec.push(container_name.to_string());
        }

        // Add workdir if specified
        if let Some(ref wd) = params.workdir {
            args_vec.push("-w".to_string());
            args_vec.push(wd.to_string());
        }

        // Add user if specified
        if let Some(ref u) = params.user {
            args_vec.push("-u".to_string());
            args_vec.push(u.to_string());
        }

        // Add privileged flag
        if privileged {
            args_vec.push("--privileged".to_string());
        }

        // Add network if specified
        if let Some(ref net) = params.network {
            args_vec.push("--network".to_string());
            args_vec.push(net.to_string());
        }

        // Add memory limit
        if let Some(ref mem) = params.memory_limit {
            args_vec.push("-m".to_string());
            args_vec.push(mem.to_string());
        }

        // Add CPU limit
        if let Some(ref cpu) = params.cpu_limit {
            args_vec.push("--cpus".to_string());
            args_vec.push(cpu.to_string());
        }

        // Add port mappings
        if let Some(ref ports) = params.ports {
            if let Some(ports_obj) = ports.as_object() {
                for (host_port, container_port) in ports_obj {
                    if let Some(container_port_str) = container_port.as_str() {
                        args_vec.push("-p".to_string());
                        args_vec.push(format!("{host_port}:{container_port_str}"));
                    }
                }
            }
        }

        // Add volume mappings
        if let Some(ref volumes) = params.volumes {
            if let Some(volumes_obj) = volumes.as_object() {
                for (host_path, container_path) in volumes_obj {
                    if let Some(container_path_str) = container_path.as_str() {
                        args_vec.push("-v".to_string());
                        args_vec.push(format!("{host_path}:{container_path_str}"));
                    }
                }
            }
        }

        // Add environment variables
        if let Some(ref env) = params.environment {
            if let Some(env_obj) = env.as_object() {
                for (key, value) in env_obj {
                    if let Some(value_str) = value.as_str() {
                        args_vec.push("-e".to_string());
                        args_vec.push(format!("{key}={value_str}"));
                    }
                }
            }
        }

        // Add capabilities
        if let Some(ref cap_adds) = params.cap_add {
            for cap in cap_adds {
                args_vec.push("--cap-add".to_string());
                args_vec.push(cap.to_string());
            }
        }

        // Add image
        args_vec.push(image.to_string());

        // Add command if specified
        if let Some(ref cmd) = params.command {
            args_vec.push("sh".to_string());
            args_vec.push("-c".to_string());
            args_vec.push(cmd.to_string());
        }

        // Convert to string slices for run_docker
        let args: Vec<&str> = args_vec.iter().map(String::as_str).collect();
        let result = run_docker(ctx, &args)?;

        // Extract container ID from output
        let container_id = result.text.trim().to_string();

        let mut structured = result.structured.unwrap_or(json!({}));
        structured["image"] = json!(image);
        structured["detach"] = json!(detach);
        structured["remove"] = json!(remove);
        if !container_id.is_empty() {
            structured["container_id"] = json!(container_id);
        }

        Ok(ToolOutput::with_structured(result.text, structured))
    }
}

rustycode_tools_api::define_tool! {
    pub struct DockerPsTool;

    name: "docker_ps",
    description: r"List Docker containers

Use this tool to:
- List running containers
- List all containers (including stopped ones)
- Get detailed information about containers

**Note:** This requires Docker to be installed and running on the system.",
    permission: ToolPermission::Read,
    tags: [ToolTag::Ops],

    execute(params: DockerPsParams, ctx) {
        let all = params.all;
        let quiet = params.quiet;
        let format_str = params.format.as_deref();

        let mut args = vec!["ps"];

        if all {
            args.push("-a");
        }

        if quiet {
            args.push("-q");
        }

        if let Some(fmt) = format_str {
            args.extend_from_slice(&["--format", fmt]);
        }

        let result = run_docker(ctx, &args)?;

        let mut structured = result.structured.unwrap_or(json!({}));
        structured["all"] = json!(all);
        structured["quiet"] = json!(quiet);

        Ok(ToolOutput::with_structured(result.text, structured))
    }
}

rustycode_tools_api::define_tool! {
    pub struct DockerStopTool;

    name: "docker_stop",
    description: r"Stop one or more running Docker containers

Use this tool to:
- Stop running containers by ID or name
- Gracefully stop containers (SIGTERM)
- Force stop containers after timeout

**Note:** This requires Docker to be installed and running on the system.",
    permission: ToolPermission::Execute,
    tags: [ToolTag::Ops],

    execute(params: DockerStopParams, ctx) {
        let time = params.time.unwrap_or(10);

        let containers: Vec<String> = match &params.containers {
            ContainersValue::Single(s) => vec![s.clone()],
            ContainersValue::Multiple(arr) => arr.clone(),
        };

        if containers.is_empty() {
            return Err(anyhow!("at least one container must be specified"));
        }

        let time_str = time.to_string();
        let mut args = vec!["stop", "-t", &time_str];
        args.extend_from_slice(&containers.iter().map(String::as_str).collect::<Vec<_>>());

        let result = run_docker(ctx, &args)?;

        let mut structured = result.structured.unwrap_or(json!({}));
        structured["containers"] = json!(containers);
        structured["time"] = json!(time);

        Ok(ToolOutput::with_structured(result.text, structured))
    }
}

rustycode_tools_api::define_tool! {
    pub struct DockerLogsTool;

    name: "docker_logs",
    description: r"View logs from a Docker container

Use this tool to:
- View container logs
- Follow logs in real-time
- View logs from a specific number of lines
- View logs with timestamps

**Note:** This requires Docker to be installed and running on the system.",
    permission: ToolPermission::Read,
    tags: [ToolTag::Ops],

    execute(params: DockerLogsParams, ctx) {
        let container = &params.container;
        let follow = params.follow;
        let timestamps = params.timestamps;
        let tail = params.tail.as_deref();
        let since = params.since.as_deref();

        let mut args = vec!["logs"];

        if follow {
            args.push("-f");
        }

        if timestamps {
            args.push("-t");
        }

        if let Some(tail_val) = tail {
            args.extend_from_slice(&["--tail", tail_val]);
        }

        if let Some(since_val) = since {
            args.extend_from_slice(&["--since", since_val]);
        }

        args.push(container);

        let result = run_docker(ctx, &args)?;

        let mut structured = result.structured.unwrap_or(json!({}));
        structured["container"] = json!(container);
        structured["follow"] = json!(follow);
        structured["timestamps"] = json!(timestamps);

        Ok(ToolOutput::with_structured(result.text, structured))
    }
}

rustycode_tools_api::define_tool! {
    pub struct DockerInspectTool;

    name: "docker_inspect",
    description: r"Inspect Docker containers or images

Use this tool to:
- View detailed configuration of containers
- View image metadata
- Get low-level information about Docker objects

**Note:** This requires Docker to be installed and running on the system.",
    permission: ToolPermission::Read,
    tags: [ToolTag::Ops],

    execute(params: DockerInspectParams, ctx) {
        let target = &params.target;
        let format_str = params.format.as_deref();
        let inspect_type = params.inspect_type.as_deref();

        let mut args = vec!["inspect"];

        if let Some(fmt) = format_str {
            args.extend_from_slice(&["--format", fmt]);
        }

        if let Some(typ) = inspect_type {
            args.extend_from_slice(&["--type", typ]);
        }

        args.push(target);

        let result = run_docker(ctx, &args)?;

        // Try to parse JSON for structured output
        let structured_output: Value = if let Ok(json_val) = serde_json::from_str(&result.text) {
            json_val
        } else {
            json!({ "raw": result.text })
        };

        let mut structured = result.structured.unwrap_or(json!({}));
        structured["target"] = json!(target);
        if let Some(typ) = inspect_type {
            structured["type"] = json!(typ);
        }
        structured["inspection"] = structured_output;

        Ok(ToolOutput::with_structured(
            format!("Inspection result for '{target}'"),
            structured,
        ))
    }
}

rustycode_tools_api::define_tool! {
    pub struct DockerImagesTool;

    name: "docker_images",
    description: r"List Docker images

Use this tool to:
- List all locally available Docker images
- Show image sizes and tags
- Find dangling images

**Note:** This requires Docker to be installed and running on the system.",
    permission: ToolPermission::Read,
    tags: [ToolTag::Ops],

    execute(params: DockerImagesParams, ctx) {
        let all = params.all;
        let dangling = params.dangling;
        let quiet = params.quiet;
        let format_str = params.format.as_deref();

        let mut args = vec!["images"];

        if all {
            args.push("-a");
        }

        if dangling {
            args.extend_from_slice(&["-f", "dangling=true"]);
        }

        if quiet {
            args.push("-q");
        }

        if let Some(fmt) = format_str {
            args.extend_from_slice(&["--format", fmt]);
        }

        let result = run_docker(ctx, &args)?;

        let mut structured = result.structured.unwrap_or(json!({}));
        structured["all"] = json!(all);
        structured["dangling"] = json!(dangling);
        structured["quiet"] = json!(quiet);

        Ok(ToolOutput::with_structured(result.text, structured))
    }
}

/// Helper function to run docker commands
fn run_docker(ctx: &ToolContext, args: &[&str]) -> Result<ToolOutput> {
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

    Ok(ToolOutput::with_structured(text, metadata))
}

/// Extract image ID from docker build output
fn extract_image_id(output: &str) -> Option<String> {
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
mod tests {
    use super::*;
    use crate::{Tool, ToolContext};

    /// Helper to create a ToolContext
    fn ctx() -> ToolContext {
        ToolContext::new(std::env::temp_dir())
    }

    // ============================================================================
    // DockerBuildTool Tests
    // ============================================================================

    #[test]
    fn test_docker_build_tool_metadata() {
        let tool = DockerBuildTool;
        assert_eq!(tool.name(), "docker_build");
        assert!(tool.description().contains("Build"));
        assert_eq!(tool.permission(), ToolPermission::Execute);
    }

    #[test]
    fn test_docker_build_parameters_schema() {
        let tool = DockerBuildTool;
        let schema = tool.parameters_schema();

        assert_eq!(schema["type"], "object");
        assert!(schema["properties"]["tag"].is_object());
    }

    #[test]
    fn test_docker_build_missing_tag() {
        let tool = DockerBuildTool;
        let result = tool.execute(json!({}), &ctx());
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("tag"));
    }

    // ============================================================================
    // DockerRunTool Tests
    // ============================================================================

    #[test]
    fn test_docker_run_tool_metadata() {
        let tool = DockerRunTool;
        assert_eq!(tool.name(), "docker_run");
        assert!(tool.description().contains("Run"));
        assert_eq!(tool.permission(), ToolPermission::Execute);
    }

    #[test]
    fn test_docker_run_parameters_schema() {
        let tool = DockerRunTool;
        let schema = tool.parameters_schema();

        assert_eq!(schema["type"], "object");
        assert!(schema["properties"]["image"].is_object());
    }

    #[test]
    fn test_docker_run_missing_image() {
        let tool = DockerRunTool;
        let result = tool.execute(json!({}), &ctx());
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("image"));
    }

    // ============================================================================
    // DockerPsTool Tests
    // ============================================================================

    #[test]
    fn test_docker_ps_tool_metadata() {
        let tool = DockerPsTool;
        assert_eq!(tool.name(), "docker_ps");
        assert!(tool.description().contains("List"));
        assert_eq!(tool.permission(), ToolPermission::Read);
    }

    // ============================================================================
    // DockerStopTool Tests
    // ============================================================================

    #[test]
    fn test_docker_stop_tool_metadata() {
        let tool = DockerStopTool;
        assert_eq!(tool.name(), "docker_stop");
        assert!(tool.description().contains("Stop"));
        assert_eq!(tool.permission(), ToolPermission::Execute);
    }

    #[test]
    fn test_docker_stop_missing_containers() {
        let tool = DockerStopTool;
        let result = tool.execute(json!({}), &ctx());
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("containers"));
    }

    #[test]
    fn test_docker_stop_single_container() {
        let _tool = DockerStopTool;
        // Just validate parameters parsing - actual execution requires docker
        let params = json!({
            "containers": "abc123"
        });

        let containers_param = params.get("containers").unwrap();
        let containers: Vec<String> = match containers_param {
            Value::String(s) => vec![s.clone()],
            Value::Array(arr) => arr
                .iter()
                .filter_map(|v| v.as_str())
                .map(|s| s.to_string())
                .collect(),
            _ => vec![],
        };

        assert_eq!(containers.len(), 1);
        assert_eq!(containers[0], "abc123");
    }

    #[test]
    fn test_docker_stop_multiple_containers() {
        let _tool = DockerStopTool;
        let params = json!({
            "containers": ["abc123", "def456"]
        });

        let containers_param = params.get("containers").unwrap();
        let containers: Vec<String> = match containers_param {
            Value::String(s) => vec![s.clone()],
            Value::Array(arr) => arr
                .iter()
                .filter_map(|v| v.as_str())
                .map(|s| s.to_string())
                .collect(),
            _ => vec![],
        };

        assert_eq!(containers.len(), 2);
    }

    // ============================================================================
    // DockerLogsTool Tests
    // ============================================================================

    #[test]
    fn test_docker_logs_tool_metadata() {
        let tool = DockerLogsTool;
        assert_eq!(tool.name(), "docker_logs");
        assert!(tool.description().contains("logs"));
        assert_eq!(tool.permission(), ToolPermission::Read);
    }

    #[test]
    fn test_docker_logs_missing_container() {
        let tool = DockerLogsTool;
        let result = tool.execute(json!({}), &ctx());
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("container"));
    }

    // ============================================================================
    // DockerInspectTool Tests
    // ============================================================================

    #[test]
    fn test_docker_inspect_tool_metadata() {
        let tool = DockerInspectTool;
        assert_eq!(tool.name(), "docker_inspect");
        assert!(tool.description().contains("Inspect"));
        assert_eq!(tool.permission(), ToolPermission::Read);
    }

    #[test]
    fn test_docker_inspect_missing_target() {
        let tool = DockerInspectTool;
        let result = tool.execute(json!({}), &ctx());
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("target"));
    }

    // ============================================================================
    // DockerImagesTool Tests
    // ============================================================================

    #[test]
    fn test_docker_images_tool_metadata() {
        let tool = DockerImagesTool;
        assert_eq!(tool.name(), "docker_images");
        assert!(tool.description().contains("images"));
        assert_eq!(tool.permission(), ToolPermission::Read);
    }

    // ============================================================================
    // Helper Function Tests
    // ============================================================================

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
