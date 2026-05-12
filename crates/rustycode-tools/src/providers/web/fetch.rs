//! `WebFetchTool` — fetch content from web URLs.

use crate::security::validation::validate_url;
use crate::{ToolOutput, ToolPermission, ToolTag};
use anyhow::anyhow;
use schemars::JsonSchema;
use serde_json::json;

use super::content::{
    html_to_simple_markdown, is_html_content, truncate_to_char_boundary, WEB_FETCH_MAX_CHARS,
};

#[derive(serde::Deserialize, JsonSchema)]
pub struct WebFetchParams {
    /// The URL to fetch content from (e.g., 'https://docs.anthropic.com', 'https://github.com/user/repo/blob/main/README.md')
    url: String,
    /// The prompt to run on the fetched content
    #[serde(default)]
    prompt: Option<String>,
}

rustycode_tools_api::define_tool! {
    pub struct WebFetchTool;

    name: "WebFetch",
    namespace: "web",
    description: "- Fetches content from a specified URL and processes it using an AI model\n- Takes a URL and a prompt as input\n- Fetches the URL content, converts HTML to markdown\n- Processes the content with the prompt using a small, fast model\n- Returns the model's response about the content\n- Use this tool when you need to retrieve and analyze web content\n\nUsage notes:\n  - IMPORTANT: If an MCP-provided web fetch tool is available, prefer using that tool instead of this one, as it may have fewer restrictions.\n  - The URL must be a fully-formed valid URL\n  - HTTP URLs will be automatically upgraded to HTTPS\n  - The prompt should describe what information you want to extract from the page\n  - This tool is read-only and does not modify any files\n  - Results may be summarized if the content is very large\n  - Includes a self-cleaning 15-minute cache for faster responses when repeatedly accessing the same URL\n  - When a URL redirects to a different host, the tool will inform you and provide the redirect URL in a special format. You should then make a new WebFetch request with the redirect URL to fetch the content.\n  - For GitHub URLs, prefer using the gh CLI via Bash instead (e.g., gh pr view, gh issue view, gh api).",
    permission: ToolPermission::Read,
    tags: [ToolTag::Explore],

    execute(params: WebFetchParams, ctx) {
        validate_url(&params.url)?;

        let _prompt = params.prompt.unwrap_or_else(|| "Return the full content of this page".to_string());
        let convert_markdown = true;

        let start_time = std::time::Instant::now();

        // Async reqwest via block_on_shared — cooperates with tokio runtime
        let (status_code, headers_map, content) =
            rustycode_shared_runtime::block_on_shared(
                super::client::web_fetch_async(&params.url, 30),
            )?;

        if !(200..300).contains(&status_code) {
            return Err(anyhow!("HTTP error {status_code}: {content}"));
        }

        let total_time = start_time.elapsed();

        let (content, converted) = if convert_markdown && is_html_content(&content) {
            (html_to_simple_markdown(&content), true)
        } else {
            (content, false)
        };

        let (content, truncated) = if content.len() > WEB_FETCH_MAX_CHARS {
            (
                truncate_to_char_boundary(&content, WEB_FETCH_MAX_CHARS),
                true,
            )
        } else {
            (&content[..], false)
        };

        let output = if truncated {
            format!("{content}\n\n[Content truncated at {WEB_FETCH_MAX_CHARS} characters]")
        } else {
            content.to_string()
        };

        let mut metadata = json!({
            "url": &params.url,
            "chars": content.len(),
            "truncated": truncated,
            "converted": converted,
            "status_code": status_code,
            "total_time_ms": total_time.as_millis(),
        });

        if !headers_map.is_empty() {
            metadata["headers"] = json!(headers_map);
        }

        Ok(ToolOutput::text(output).with_metadata(ctx, || metadata))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Tool, ToolContext};
    use serde_json::json;

    #[test]
    fn test_web_fetch_tool_metadata() {
        let tool = WebFetchTool;
        assert_eq!(tool.name(), "WebFetch");
        assert!(tool
            .description()
            .contains("Fetches content from a specified URL"));
        assert_eq!(tool.permission(), ToolPermission::Read);
    }

    #[test]
    fn test_web_fetch_parameters_schema() {
        let tool = WebFetchTool;
        let schema = tool.parameters_schema();
        assert_eq!(schema["type"], "object");
        assert!(schema["required"].is_array());
        let required = schema["required"].as_array().unwrap();
        assert_eq!(required.len(), 1);
        assert_eq!(required[0], "url");
        assert_eq!(schema["properties"]["url"]["type"], "string");
        assert!(schema["properties"]["prompt"].is_object());
    }

    #[test]
    fn test_web_fetch_missing_required_url() {
        let tool = WebFetchTool;
        let ctx = ToolContext::new("/tmp");
        let result = tool.execute(json!({}), &ctx);
        assert!(result.is_err());
    }

    #[test]
    fn test_web_fetch_blocks_file_url() {
        let tool = WebFetchTool;
        let ctx = ToolContext::new("/tmp");
        let result = tool.execute(json!({ "url": "file:///etc/passwd" }), &ctx);
        assert!(result.is_err());
    }

    #[test]
    fn test_web_fetch_blocks_missing_scheme() {
        let tool = WebFetchTool;
        let ctx = ToolContext::new("/tmp");
        let result = tool.execute(json!({ "url": "example.com" }), &ctx);
        assert!(result.is_err());
    }

    #[test]
    fn test_web_fetch_blocks_ftp_scheme() {
        let tool = WebFetchTool;
        let ctx = ToolContext::new("/tmp");
        let result = tool.execute(json!({ "url": "ftp://example.com/file.txt" }), &ctx);
        assert!(result.is_err());
    }

    #[test]
    fn test_web_fetch_url_case_insensitive() {
        let tool = WebFetchTool;
        let ctx = ToolContext::new("/tmp");
        let result = tool.execute(json!({ "url": "HTTPS://EXAMPLE.COM" }), &ctx);
        if let Err(e) = result {
            let msg = e.to_string();
            assert!(!msg.contains("scheme"));
        }
    }

    #[test]
    fn test_web_fetch_empty_url() {
        let tool = WebFetchTool;
        let ctx = ToolContext::new("/tmp");
        let result = tool.execute(json!({ "url": "" }), &ctx);
        assert!(result.is_err());
    }

    #[test]
    fn test_web_fetch_max_chars_constant() {
        assert_eq!(WEB_FETCH_MAX_CHARS, 50_000);
    }
}
