use super::*;
use crate::{ToolOutput, ToolPermission, ToolTag};
use serde_json::{json, Value};

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

        Ok(ToolOutput::text(format!("Inspection result for '{target}'"))
            .with_metadata(ctx, || structured))
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests_common::ctx;
    use super::*;
    use crate::Tool;
    use serde_json::json;

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
}
