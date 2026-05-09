use super::*;
use crate::{ToolOutput, ToolPermission, ToolTag};
use serde_json::json;

rustycode_tools_api::define_tool! {
    pub struct DockerPsTool;

    name: "DockerPs",
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

        Ok(ToolOutput::text(result.text).with_metadata(ctx, || structured))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Tool;

    #[test]
    fn test_docker_ps_tool_metadata() {
        let tool = DockerPsTool;
        assert_eq!(tool.name(), "DockerPs");
        assert!(tool.description().contains("List"));
        assert_eq!(tool.permission(), ToolPermission::Read);
    }
}
