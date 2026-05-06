//! `WebFetchTool` — fetch content from web URLs.

use crate::security::validation::validate_url;
use crate::{Tool, ToolContext, ToolOutput, ToolPermission, ToolTag};
use anyhow::{anyhow, Result};
use serde_json::{json, Value};

use super::fs::{
    html_to_simple_markdown, is_html_content, required_string, truncate_to_char_boundary,
    WEB_FETCH_MAX_CHARS,
};

const USER_AGENT: &str = concat!("RustyCode/", env!("CARGO_PKG_VERSION"));

pub struct WebFetchTool;

impl Tool for WebFetchTool {
    fn name(&self) -> &'static str {
        "web_fetch"
    }

    fn description(&self) -> &'static str {
        "Fetch and read content from a web page or PDF. Use this to read documentation, blog posts, GitHub files, or online articles."
    }

    fn permission(&self) -> ToolPermission {
        ToolPermission::Read
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["url"],
            "properties": {
                "url": {
                    "type": "string",
                    "description": "The URL to fetch content from (e.g., 'https://docs.anthropic.com', 'https://github.com/user/repo/blob/main/README.md')"
                },
                "convert_markdown": {
                    "type": "boolean",
                    "description": "Convert HTML to simplified markdown format"
                }
            }
        })
    }

    fn tags(&self) -> &[ToolTag] {
        &[ToolTag::Explore]
    }

    fn execute(&self, params: Value, ctx: &ToolContext) -> Result<ToolOutput> {
        // Role-based gating
        if let Some(gate) = &ctx.plan_gate {
            gate.check_access(ctx.role, self.name())?;
        }
        let url = required_string(&params, "url")?;

        // Validate URL for security
        validate_url(url)?;

        let convert_markdown = params
            .get("convert_markdown")
            .and_then(Value::as_bool)
            .unwrap_or(false);

        // Track execution time
        let start_time = std::time::Instant::now();

        // Use blocking reqwest for simplicity in tool context
        let client = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .user_agent(USER_AGENT)
            .build()?;

        let response = client.get(url).send()?;
        let time_to_first_byte = start_time.elapsed();

        let status_code = response.status().as_u16();

        if !response.status().is_success() {
            return Err(anyhow!(
                "HTTP error {}: {}",
                response.status(),
                response
                    .text()
                    .unwrap_or_else(|_| "unable to read error".to_string())
            ));
        }

        // Extract response headers for metadata
        let headers_map = response
            .headers()
            .iter()
            .filter_map(|(name, value)| {
                value
                    .to_str()
                    .ok()
                    .map(|v| (name.as_str().to_string(), v.to_string()))
            })
            .collect::<std::collections::HashMap<String, String>>();

        let content = response.text()?;
        let total_time = start_time.elapsed();

        // Convert HTML to simplified markdown if requested
        let (content, converted) = if convert_markdown && is_html_content(&content) {
            (html_to_simple_markdown(&content), true)
        } else {
            (content, false)
        };

        // Truncate content if too large (limit to ~50k chars to avoid overwhelming context)
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

        // Build enhanced metadata with headers and timing
        let mut metadata = json!({
            "url": url,
            "chars": content.len(),
            "truncated": truncated,
            "converted": converted,
            "status_code": status_code,
            "time_to_first_byte_ms": time_to_first_byte.as_millis(),
            "total_time_ms": total_time.as_millis(),
        });

        // Add headers to metadata
        if !headers_map.is_empty() {
            metadata["headers"] = json!(headers_map);
        }

        Ok(ToolOutput::with_structured(output, metadata))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ToolContext;
    use serde_json::json;

    #[test]
    fn test_web_fetch_tool_metadata() {
        let tool = WebFetchTool;
        assert_eq!(tool.name(), "web_fetch");
        assert_eq!(
            tool.description(),
            "Fetch and read content from a web page or PDF. Use this to read documentation, blog posts, GitHub files, or online articles."
        );
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

        // Check url property
        assert_eq!(schema["properties"]["url"]["type"], "string");
        assert!(schema["properties"]["url"]["description"].is_string());

        // Check convert_markdown property (optional)
        assert_eq!(schema["properties"]["convert_markdown"]["type"], "boolean");
        assert!(schema["properties"]["convert_markdown"]["description"].is_string());
    }

    #[test]
    fn test_web_fetch_missing_required_url() {
        let tool = WebFetchTool;
        let ctx = ToolContext::new("/tmp");

        let result = tool.execute(json!({}), &ctx);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("url"));
    }

    #[test]
    fn test_web_fetch_blocks_file_url() {
        let tool = WebFetchTool;
        let ctx = ToolContext::new("/tmp");

        let result = tool.execute(json!({ "url": "file:///etc/passwd" }), &ctx);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not allowed"));
    }

    #[test]
    fn test_web_fetch_blocks_missing_scheme() {
        let tool = WebFetchTool;
        let ctx = ToolContext::new("/tmp");

        let result = tool.execute(json!({ "url": "example.com" }), &ctx);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("scheme"));
    }

    #[test]
    fn test_web_fetch_blocks_ftp_scheme() {
        let tool = WebFetchTool;
        let ctx = ToolContext::new("/tmp");

        let result = tool.execute(json!({ "url": "ftp://example.com/file.txt" }), &ctx);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("only http:// and https://"));
    }

    #[test]
    fn test_web_fetch_allows_http_scheme() {
        let tool = WebFetchTool;
        let ctx = ToolContext::new("/tmp");

        let result = tool.execute(json!({ "url": "http://example.com" }), &ctx);

        // Should pass validation (may fail on actual request, but shouldn't fail on scheme)
        if let Err(e) = result {
            let err_msg = e.to_string();
            assert!(!err_msg.contains("scheme"));
            assert!(!err_msg.contains("not allowed"));
        }
    }

    #[test]
    fn test_web_fetch_allows_https_scheme() {
        let tool = WebFetchTool;
        let ctx = ToolContext::new("/tmp");

        let result = tool.execute(json!({ "url": "https://example.com" }), &ctx);

        // Should pass validation
        if let Err(e) = result {
            let err_msg = e.to_string();
            assert!(!err_msg.contains("scheme"));
            assert!(!err_msg.contains("not allowed"));
        }
    }

    #[test]
    fn test_web_fetch_convert_markdown_default() {
        let tool = WebFetchTool;
        let ctx = ToolContext::new("/tmp");

        // Without convert_markdown parameter, defaults to false
        let result = tool.execute(json!({ "url": "https://example.com" }), &ctx);

        // Should pass validation (actual fetch may fail, but params are valid)
        if let Err(e) = result {
            let err_msg = e.to_string();
            assert!(!err_msg.contains("convert_markdown"));
        }
    }

    #[test]
    fn test_web_fetch_convert_markdown_explicit_false() {
        let tool = WebFetchTool;
        let ctx = ToolContext::new("/tmp");

        let result = tool.execute(
            json!({ "url": "https://example.com", "convert_markdown": false }),
            &ctx,
        );

        if let Err(e) = result {
            let err_msg = e.to_string();
            assert!(!err_msg.contains("convert_markdown"));
        }
    }

    #[test]
    fn test_web_fetch_convert_markdown_explicit_true() {
        let tool = WebFetchTool;
        let ctx = ToolContext::new("/tmp");

        let result = tool.execute(
            json!({ "url": "https://example.com", "convert_markdown": true }),
            &ctx,
        );

        if let Err(e) = result {
            let err_msg = e.to_string();
            assert!(!err_msg.contains("convert_markdown"));
        }
    }

    #[test]
    fn test_web_fetch_url_case_insensitive() {
        let tool = WebFetchTool;
        let ctx = ToolContext::new("/tmp");

        let result = tool.execute(json!({ "url": "HTTPS://EXAMPLE.COM" }), &ctx);

        if let Err(e) = result {
            let err_msg = e.to_string();
            assert!(!err_msg.contains("scheme"));
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
    fn test_web_fetch_url_with_fragment() {
        let tool = WebFetchTool;
        let ctx = ToolContext::new("/tmp");

        let result = tool.execute(json!({ "url": "https://example.com/page#section" }), &ctx);

        if let Err(e) = result {
            let err_msg = e.to_string();
            assert!(!err_msg.contains("scheme"));
            assert!(!err_msg.contains("not allowed"));
        }
    }

    #[test]
    fn test_web_fetch_url_with_query_params() {
        let tool = WebFetchTool;
        let ctx = ToolContext::new("/tmp");

        let result = tool.execute(
            json!({ "url": "https://example.com?query=value&other=123" }),
            &ctx,
        );

        // URL with query params should pass validation
        if let Err(e) = result {
            let err_msg = e.to_string();
            assert!(!err_msg.contains("scheme"));
            assert!(!err_msg.contains("not allowed"));
        }
    }

    #[test]
    fn test_web_fetch_url_with_port() {
        let tool = WebFetchTool;
        let ctx = ToolContext::new("/tmp");

        let result = tool.execute(json!({ "url": "https://example.com:8443/path" }), &ctx);

        // URL with port should pass validation
        if let Err(e) = result {
            let err_msg = e.to_string();
            assert!(!err_msg.contains("scheme"));
            assert!(!err_msg.contains("not allowed"));
        }
    }

    #[test]
    fn test_web_fetch_url_with_ipv4() {
        let tool = WebFetchTool;
        let ctx = ToolContext::new("/tmp");

        let result = tool.execute(json!({ "url": "https://192.168.1.1/path" }), &ctx);

        // IPv4 URL should pass validation
        if let Err(e) = result {
            let err_msg = e.to_string();
            assert!(!err_msg.contains("scheme"));
            assert!(!err_msg.contains("not allowed"));
        }
    }

    #[test]
    fn test_web_fetch_url_with_localhost() {
        let tool = WebFetchTool;
        let ctx = ToolContext::new("/tmp");

        let result = tool.execute(json!({ "url": "http://localhost:3000/api" }), &ctx);

        // localhost URL should pass validation
        if let Err(e) = result {
            let err_msg = e.to_string();
            assert!(!err_msg.contains("scheme"));
            assert!(!err_msg.contains("not allowed"));
        }
    }

    #[test]
    fn test_web_fetch_invalid_url_type() {
        let tool = WebFetchTool;
        let ctx = ToolContext::new("/tmp");

        // Pass number instead of string
        let result = tool.execute(json!({ "url": 12345 }), &ctx);
        assert!(result.is_err());
    }

    #[test]
    fn test_is_html_content_doctype() {
        assert!(super::super::fs::is_html_content(
            "<!DOCTYPE html><html><body>Test</body></html>"
        ));
    }

    #[test]
    fn test_is_html_content_html_tag() {
        assert!(super::super::fs::is_html_content(
            "<html><head><title>Test</title></head><body>Content</body></html>"
        ));
    }

    #[test]
    fn test_is_html_content_xmlns() {
        assert!(super::super::fs::is_html_content(
            "<div xmlns='http://www.w3.org/1999/xhtml'>Content</div>"
        ));
    }

    #[test]
    fn test_is_html_content_false_plain_text() {
        assert!(!super::super::fs::is_html_content("Just plain text"));
    }

    #[test]
    fn test_is_html_content_false_json() {
        assert!(!super::super::fs::is_html_content("{\"key\": \"value\"}"));
    }

    #[test]
    fn test_is_html_content_case_insensitive() {
        assert!(super::super::fs::is_html_content(
            "<!DOCTYPE HTML>\n<HTML><BODY>Test</BODY></HTML>"
        ));
    }

    #[test]
    fn test_is_html_content_with_whitespace() {
        assert!(super::super::fs::is_html_content(
            "  \n  <!DOCTYPE html>\n  <html>Test</html>  \n"
        ));
    }

    #[test]
    fn test_html_to_simple_markdown_basic() {
        let html = "<html><body><h1>Title</h1><p>Paragraph</p></body></html>";
        let markdown = super::super::fs::html_to_simple_markdown(html);
        assert!(!markdown.is_empty());
        assert!(markdown.contains("Title") || markdown.contains("Paragraph"));
    }

    #[test]
    fn test_html_to_simple_markdown_trims_whitespace() {
        let html = "  \n  <html><body>Content</body></html>  \n  ";
        let markdown = super::super::fs::html_to_simple_markdown(html);
        assert_eq!(markdown, markdown.trim());
    }

    #[test]
    fn test_html_to_simple_markdown_handles_empty() {
        let html = "";
        let markdown = super::super::fs::html_to_simple_markdown(html);
        assert!(markdown.is_empty());
    }

    #[test]
    fn test_web_fetch_max_chars_constant() {
        // Verify the constant is set to expected value
        assert_eq!(WEB_FETCH_MAX_CHARS, 50_000);
    }
}
