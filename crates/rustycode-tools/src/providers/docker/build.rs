use super::*;
use crate::{ToolOutput, ToolPermission, ToolTag};
use serde_json::json;

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

#[cfg(test)]
mod tests {
    use super::super::tests_common::ctx;
    use super::*;
    use crate::Tool;

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
}
