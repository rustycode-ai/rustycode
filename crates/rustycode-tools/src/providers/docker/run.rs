use super::*;
use crate::{ToolOutput, ToolPermission, ToolTag};
use serde_json::json;

rustycode_tools_api::define_tool! {
    pub struct DockerRunTool;

    name: "DockerRun",
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

        Ok(ToolOutput::text(result.text).with_metadata(ctx, || structured))
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests_common::ctx;
    use super::*;
    use crate::Tool;

    #[test]
    fn test_docker_run_tool_metadata() {
        let tool = DockerRunTool;
        assert_eq!(tool.name(), "DockerRun");
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
}
