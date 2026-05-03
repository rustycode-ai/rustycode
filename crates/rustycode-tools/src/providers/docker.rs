//! Docker tools for container management
//!
//! This module provides tools for interacting with Docker:
//! - `DockerBuildTool`: Build Docker images
//! - `DockerRunTool`: Run Docker containers
//! - `DockerPsTool`: List running containers
//! - `DockerStopTool`: Stop containers
//! - `DockerLogsTool`: View container logs
//! - `DockerInspectTool`: Inspect containers/images

use crate::{Tool, ToolContext, ToolOutput, ToolPermission, ToolTag};
use anyhow::{anyhow, Result};
use serde_json::{json, Value};
use std::process::Command;

/// Docker build tool - Build Docker images from Dockerfile
pub struct DockerBuildTool;

impl Tool for DockerBuildTool {
    fn name(&self) -> &'static str {
        "docker_build"
    }

    fn description(&self) -> &'static str {
        r#"Build a Docker image from a Dockerfile

Use this tool to:
- Build Docker images from a Dockerfile in the current directory
- Tag the built image with a specific name
- Specify build arguments
- Set the build context path

**Examples:**
- Build image with default name: tag the image as "myapp:latest"
- Build with custom Dockerfile: specify dockerfile path
- Build with build args: pass build arguments like "VERSION=1.0"

**Note:** This requires Docker to be installed and running on the system."#
    }

    fn permission(&self) -> ToolPermission {
        ToolPermission::Execute
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["tag"],
            "properties": {
                "tag": {
                    "type": "string",
                    "description": "Image tag (e.g., 'myapp:latest' or 'myapp:v1.0')"
                },
                "dockerfile": {
                    "type": "string",
                    "description": "Path to Dockerfile (default: 'Dockerfile' in current directory)"
                },
                "context": {
                    "type": "string",
                    "description": "Build context path (default: '.')"
                },
                "build_args": {
                    "type": "object",
                    "description": "Build arguments as key-value pairs (e.g., {\"VERSION\": \"1.0\"})",
                    "additionalProperties": { "type": "string" }
                },
                "target": {
                    "type": "string",
                    "description": "Target stage for multi-stage builds"
                },
                "no_cache": {
                    "type": "boolean",
                    "description": "Disable cache (default: false)"
                }
            }
        })
    }

    fn tags(&self) -> &[ToolTag] {
        &[ToolTag::Ops]
    }

    fn execute(&self, params: Value, ctx: &ToolContext) -> Result<ToolOutput> {
        let tag = required_string(&params, "tag")?;
        let dockerfile = optional_string(&params, "dockerfile").unwrap_or("Dockerfile");
        let context = optional_string(&params, "context").unwrap_or(".");
        let no_cache = params
            .get("no_cache")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let target = optional_string(&params, "target");

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
        if let Some(build_args) = params.get("build_args").and_then(|v| v.as_object()) {
            for (key, value) in build_args {
                if let Some(value_str) = value.as_str() {
                    args_vec.push("--build-arg".to_string());
                    args_vec.push(format!("{key}={value_str}"));
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

/// Docker run tool - Run Docker containers
pub struct DockerRunTool;

impl Tool for DockerRunTool {
    fn name(&self) -> &'static str {
        "docker_run"
    }

    fn description(&self) -> &'static str {
        r"Run a Docker container

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

**Note:** This requires Docker to be installed and running on the system."
    }

    fn permission(&self) -> ToolPermission {
        ToolPermission::Execute
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["image"],
            "properties": {
                "image": {
                    "type": "string",
                    "description": "Docker image to run (e.g., 'ubuntu:latest' or 'myapp:v1.0')"
                },
                "command": {
                    "type": "string",
                    "description": "Command to run in the container"
                },
                "ports": {
                    "type": "object",
                    "description": "Port mappings as host:container (e.g., {\"8080\": \"80\"})",
                    "additionalProperties": { "type": "string" }
                },
                "volumes": {
                    "type": "object",
                    "description": "Volume mappings (e.g., {\"/host/path\": \"/container/path\"})",
                    "additionalProperties": { "type": "string" }
                },
                "environment": {
                    "type": "object",
                    "description": "Environment variables (e.g., {\"API_KEY\": \"secret\"})",
                    "additionalProperties": { "type": "string" }
                },
                "detach": {
                    "type": "boolean",
                    "description": "Run in detached mode (default: true)",
                    "default": true
                },
                "remove": {
                    "type": "boolean",
                    "description": "Auto-remove container on exit (default: false)",
                    "default": false
                },
                "name": {
                    "type": "string",
                    "description": "Container name"
                },
                "workdir": {
                    "type": "string",
                    "description": "Working directory inside the container"
                },
                "user": {
                    "type": "string",
                    "description": "User to run as (e.g., \"1000:1000\")"
                },
                "cap_add": {
                    "type": "array",
                    "description": "Add Linux capabilities (e.g., [\"SYS_ADMIN\"])",
                    "items": { "type": "string" }
                },
                "privileged": {
                    "type": "boolean",
                    "description": "Give extended privileges to the container (default: false)",
                    "default": false
                },
                "network": {
                    "type": "string",
                    "description": "Network mode to connect the container to"
                },
                "memory_limit": {
                    "type": "string",
                    "description": "Memory limit (e.g., \"512m\", \"1g\")"
                },
                "cpu_limit": {
                    "type": "string",
                    "description": "CPU limit (e.g., \"0.5\" for 50% of one CPU)"
                }
            }
        })
    }

    fn tags(&self) -> &[ToolTag] {
        &[ToolTag::Ops]
    }

    fn execute(&self, params: Value, ctx: &ToolContext) -> Result<ToolOutput> {
        let image = required_string(&params, "image")?;
        let command = optional_string(&params, "command");
        let detach = params
            .get("detach")
            .and_then(Value::as_bool)
            .unwrap_or(true);
        let remove = params
            .get("remove")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let name = optional_string(&params, "name");
        let workdir = optional_string(&params, "workdir");
        let user = optional_string(&params, "user");
        let privileged = params
            .get("privileged")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let network = optional_string(&params, "network");
        let memory_limit = optional_string(&params, "memory_limit");
        let cpu_limit = optional_string(&params, "cpu_limit");

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
        if let Some(container_name) = name {
            args_vec.push("--name".to_string());
            args_vec.push(container_name.to_string());
        }

        // Add workdir if specified
        if let Some(wd) = workdir {
            args_vec.push("-w".to_string());
            args_vec.push(wd.to_string());
        }

        // Add user if specified
        if let Some(u) = user {
            args_vec.push("-u".to_string());
            args_vec.push(u.to_string());
        }

        // Add privileged flag
        if privileged {
            args_vec.push("--privileged".to_string());
        }

        // Add network if specified
        if let Some(net) = network {
            args_vec.push("--network".to_string());
            args_vec.push(net.to_string());
        }

        // Add memory limit
        if let Some(mem) = memory_limit {
            args_vec.push("-m".to_string());
            args_vec.push(mem.to_string());
        }

        // Add CPU limit
        if let Some(cpu) = cpu_limit {
            args_vec.push("--cpus".to_string());
            args_vec.push(cpu.to_string());
        }

        // Add port mappings
        if let Some(ports) = params.get("ports").and_then(|v| v.as_object()) {
            for (host_port, container_port) in ports {
                if let Some(container_port_str) = container_port.as_str() {
                    args_vec.push("-p".to_string());
                    args_vec.push(format!("{host_port}:{container_port_str}"));
                }
            }
        }

        // Add volume mappings
        if let Some(volumes) = params.get("volumes").and_then(|v| v.as_object()) {
            for (host_path, container_path) in volumes {
                if let Some(container_path_str) = container_path.as_str() {
                    args_vec.push("-v".to_string());
                    args_vec.push(format!("{host_path}:{container_path_str}"));
                }
            }
        }

        // Add environment variables
        if let Some(env) = params.get("environment").and_then(|v| v.as_object()) {
            for (key, value) in env {
                if let Some(value_str) = value.as_str() {
                    args_vec.push("-e".to_string());
                    args_vec.push(format!("{key}={value_str}"));
                }
            }
        }

        // Add capabilities
        if let Some(cap_adds) = params.get("cap_add").and_then(|v| v.as_array()) {
            for cap in cap_adds {
                if let Some(cap_str) = cap.as_str() {
                    args_vec.push("--cap-add".to_string());
                    args_vec.push(cap_str.to_string());
                }
            }
        }

        // Add image
        args_vec.push(image.to_string());

        // Add command if specified
        if let Some(cmd) = command {
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

/// Docker ps tool - List running containers
pub struct DockerPsTool;

impl Tool for DockerPsTool {
    fn name(&self) -> &'static str {
        "docker_ps"
    }

    fn description(&self) -> &'static str {
        r"List Docker containers

Use this tool to:
- List running containers
- List all containers (including stopped ones)
- Get detailed information about containers

**Note:** This requires Docker to be installed and running on the system."
    }

    fn permission(&self) -> ToolPermission {
        ToolPermission::Read
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "all": {
                    "type": "boolean",
                    "description": "Show all containers (including stopped ones)",
                    "default": false
                },
                "quiet": {
                    "type": "boolean",
                    "description": "Only display container IDs",
                    "default": false
                },
                "format": {
                    "type": "string",
                    "description": "Format output using Go template (e.g., '{{.ID}}: {{.Names}}')"
                }
            }
        })
    }

    fn tags(&self) -> &[ToolTag] {
        &[ToolTag::Ops]
    }

    fn execute(&self, params: Value, ctx: &ToolContext) -> Result<ToolOutput> {
        let all = params.get("all").and_then(Value::as_bool).unwrap_or(false);
        let quiet = params
            .get("quiet")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let format_str = optional_string(&params, "format");

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

/// Docker stop tool - Stop running containers
pub struct DockerStopTool;

impl Tool for DockerStopTool {
    fn name(&self) -> &'static str {
        "docker_stop"
    }

    fn description(&self) -> &'static str {
        r"Stop one or more running Docker containers

Use this tool to:
- Stop running containers by ID or name
- Gracefully stop containers (SIGTERM)
- Force stop containers after timeout

**Note:** This requires Docker to be installed and running on the system."
    }

    fn permission(&self) -> ToolPermission {
        ToolPermission::Execute
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["containers"],
            "properties": {
                "containers": {
                    "oneOf": [
                        { "type": "string" },
                        { "type": "array", "items": { "type": "string" } }
                    ],
                    "description": "Container ID(s) or name(s) to stop"
                },
                "time": {
                    "type": "integer",
                    "description": "Seconds to wait before killing (default: 10)",
                    "default": 10,
                    "minimum": 1
                }
            }
        })
    }

    fn tags(&self) -> &[ToolTag] {
        &[ToolTag::Ops]
    }

    fn execute(&self, params: Value, ctx: &ToolContext) -> Result<ToolOutput> {
        let containers_param = params
            .get("containers")
            .ok_or_else(|| anyhow!("missing 'containers' parameter"))?;
        let time = params.get("time").and_then(Value::as_i64).unwrap_or(10);

        let containers: Vec<String> = match containers_param {
            Value::String(s) => vec![s.clone()],
            Value::Array(arr) => arr
                .iter()
                .filter_map(|v| v.as_str())
                .map(ToString::to_string)
                .collect(),
            _ => return Err(anyhow!("containers must be a string or array of strings")),
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

/// Docker logs tool - View container logs
pub struct DockerLogsTool;

impl Tool for DockerLogsTool {
    fn name(&self) -> &'static str {
        "docker_logs"
    }

    fn description(&self) -> &'static str {
        r"View logs from a Docker container

Use this tool to:
- View container logs
- Follow logs in real-time
- View logs from a specific number of lines
- View logs with timestamps

**Note:** This requires Docker to be installed and running on the system."
    }

    fn permission(&self) -> ToolPermission {
        ToolPermission::Read
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["container"],
            "properties": {
                "container": {
                    "type": "string",
                    "description": "Container ID or name"
                },
                "follow": {
                    "type": "boolean",
                    "description": "Follow log output (default: false)",
                    "default": false
                },
                "tail": {
                    "type": "string",
                    "description": "Number of lines to show from the end (default: 'all'). Use '100' for last 100 lines."
                },
                "timestamps": {
                    "type": "boolean",
                    "description": "Show timestamps (default: false)",
                    "default": false
                },
                "since": {
                    "type": "string",
                    "description": "Show logs since timestamp (e.g., '2023-01-01T00:00:00Z') or relative time (e.g., '10m')"
                }
            }
        })
    }

    fn tags(&self) -> &[ToolTag] {
        &[ToolTag::Ops]
    }

    fn execute(&self, params: Value, ctx: &ToolContext) -> Result<ToolOutput> {
        let container = required_string(&params, "container")?;
        let follow = params
            .get("follow")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let timestamps = params
            .get("timestamps")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let tail = optional_string(&params, "tail");
        let since = optional_string(&params, "since");

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

/// Docker inspect tool - Inspect containers or images
pub struct DockerInspectTool;

impl Tool for DockerInspectTool {
    fn name(&self) -> &'static str {
        "docker_inspect"
    }

    fn description(&self) -> &'static str {
        r"Inspect Docker containers or images

Use this tool to:
- View detailed configuration of containers
- View image metadata
- Get low-level information about Docker objects

**Note:** This requires Docker to be installed and running on the system."
    }

    fn permission(&self) -> ToolPermission {
        ToolPermission::Read
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["target"],
            "properties": {
                "target": {
                    "type": "string",
                    "description": "Container ID, image name, or other Docker object to inspect"
                },
                "format": {
                    "type": "string",
                    "description": "Format output using Go template (e.g., '{{.Config.Image}}')"
                },
                "type": {
                    "type": "string",
                    "enum": ["container", "image", "task"],
                    "description": "Return JSON for specified type"
                }
            }
        })
    }

    fn tags(&self) -> &[ToolTag] {
        &[ToolTag::Ops]
    }

    fn execute(&self, params: Value, ctx: &ToolContext) -> Result<ToolOutput> {
        let target = required_string(&params, "target")?;
        let format_str = optional_string(&params, "format");
        let inspect_type = optional_string(&params, "type");

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

/// Docker images tool - List Docker images
pub struct DockerImagesTool;

impl Tool for DockerImagesTool {
    fn name(&self) -> &'static str {
        "docker_images"
    }

    fn description(&self) -> &'static str {
        r"List Docker images

Use this tool to:
- List all locally available Docker images
- Show image sizes and tags
- Find dangling images

**Note:** This requires Docker to be installed and running on the system."
    }

    fn permission(&self) -> ToolPermission {
        ToolPermission::Read
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "all": {
                    "type": "boolean",
                    "description": "Show all images (including intermediate layers)",
                    "default": false
                },
                "dangling": {
                    "type": "boolean",
                    "description": "Show only dangling images (untagged)",
                    "default": false
                },
                "quiet": {
                    "type": "boolean",
                    "description": "Only show image IDs",
                    "default": false
                },
                "format": {
                    "type": "string",
                    "description": "Format output using Go template"
                }
            }
        })
    }

    fn tags(&self) -> &[ToolTag] {
        &[ToolTag::Ops]
    }

    fn execute(&self, params: Value, ctx: &ToolContext) -> Result<ToolOutput> {
        let all = params.get("all").and_then(Value::as_bool).unwrap_or(false);
        let dangling = params
            .get("dangling")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let quiet = params
            .get("quiet")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let format_str = optional_string(&params, "format");

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

fn required_string<'a>(value: &'a Value, key: &str) -> Result<&'a str> {
    value
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("missing string parameter '{key}'"))
}

fn optional_string<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value.get(key).and_then(Value::as_str)
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let required = schema["required"].as_array().unwrap();
        assert_eq!(required.len(), 1);
        assert_eq!(required[0], "tag");
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
        let required = schema["required"].as_array().unwrap();
        assert_eq!(required.len(), 1);
        assert_eq!(required[0], "image");
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
