use super::*;
use crate::{ToolOutput, ToolPermission, ToolTag};
use serde_json::json;

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

#[cfg(test)]
mod tests {
    use super::super::tests_common::ctx;
    use super::*;
    use crate::Tool;
    use serde_json::json;

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
}
