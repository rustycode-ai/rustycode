use super::*;
use crate::{ToolOutput, ToolPermission, ToolTag};
use serde_json::json;

rustycode_tools_api::define_tool! {
    pub struct DockerImagesTool;

    name: "DockerImages",
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

        Ok(ToolOutput::text(result.text).with_metadata(ctx, || structured))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Tool;

    #[test]
    fn test_docker_images_tool_metadata() {
        let tool = DockerImagesTool;
        assert_eq!(tool.name(), "DockerImages");
        assert!(tool.description().contains("images"));
        assert_eq!(tool.permission(), ToolPermission::Read);
    }
}
