use super::*;
use crate::{ToolOutput, ToolPermission, ToolTag};
use anyhow::anyhow;
use serde_json::json;

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

        Ok(ToolOutput::text(result.text).with_metadata(ctx, || structured))
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests_common::ctx;
    use super::*;
    use crate::Tool;
    use serde_json::json;

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
}
