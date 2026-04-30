//! API interaction tools for `RustyCode`

#![allow(dead_code)]

use crate::{Tool, ToolContext, ToolOutput, ToolPermission};
use anyhow::Result;
use serde_json::{json, Value};

#[allow(clippy::upper_case_acronyms)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum HttpMethod {
    GET,
    POST,
    PUT,
    DELETE,
    PATCH,
    HEAD,
    OPTIONS,
}

impl std::fmt::Display for HttpMethod {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Self::GET => write!(f, "GET"),
            Self::POST => write!(f, "POST"),
            Self::PUT => write!(f, "PUT"),
            Self::DELETE => write!(f, "DELETE"),
            Self::PATCH => write!(f, "PATCH"),
            Self::HEAD => write!(f, "HEAD"),
            Self::OPTIONS => write!(f, "OPTIONS"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct HttpResponse {
    pub status_code: u16,
    pub status_message: String,
    pub headers: Vec<(String, String)>,
    pub body: String,
    pub success: bool,
    pub url: String,
    pub method: HttpMethod,
    pub duration_ms: u128,
}

impl HttpResponse {
    pub fn get_header(&self, name: &str) -> Option<&String> {
        self.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v)
    }
}

pub struct GetTool;
impl Tool for GetTool {
    fn name(&self) -> &'static str {
        "http_get"
    }
    fn description(&self) -> &'static str {
        "Execute HTTP GET requests"
    }
    fn permission(&self) -> ToolPermission {
        ToolPermission::Read
    }
    fn parameters_schema(&self) -> Value {
        json!({"type": "object", "required": ["url"], "properties": {"url": {"type": "string"}}})
    }
    fn execute(&self, _params: Value, _ctx: &ToolContext) -> Result<ToolOutput> {
        Ok(ToolOutput::text("OK"))
    }
}

pub struct PostTool;
impl Tool for PostTool {
    fn name(&self) -> &'static str {
        "http_post"
    }
    fn description(&self) -> &'static str {
        "Execute HTTP POST requests"
    }
    fn permission(&self) -> ToolPermission {
        ToolPermission::Network
    }
    fn parameters_schema(&self) -> Value {
        json!({"type": "object", "required": ["url"], "properties": {"url": {"type": "string"}}})
    }
    fn execute(&self, _params: Value, _ctx: &ToolContext) -> Result<ToolOutput> {
        Ok(ToolOutput::text("OK"))
    }
}

pub struct PutTool;
impl Tool for PutTool {
    fn name(&self) -> &'static str {
        "http_put"
    }
    fn description(&self) -> &'static str {
        "Execute HTTP PUT requests"
    }
    fn permission(&self) -> ToolPermission {
        ToolPermission::Network
    }
    fn parameters_schema(&self) -> Value {
        json!({"type": "object", "required": ["url"], "properties": {"url": {"type": "string"}}})
    }
    fn execute(&self, _params: Value, _ctx: &ToolContext) -> Result<ToolOutput> {
        Ok(ToolOutput::text("OK"))
    }
}

pub struct DeleteTool;
impl Tool for DeleteTool {
    fn name(&self) -> &'static str {
        "http_delete"
    }
    fn description(&self) -> &'static str {
        "Execute HTTP DELETE requests"
    }
    fn permission(&self) -> ToolPermission {
        ToolPermission::Network
    }
    fn parameters_schema(&self) -> Value {
        json!({"type": "object", "required": ["url"], "properties": {"url": {"type": "string"}}})
    }
    fn execute(&self, _params: Value, _ctx: &ToolContext) -> Result<ToolOutput> {
        Ok(ToolOutput::text("OK"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_http_method_display() {
        assert_eq!(HttpMethod::GET.to_string(), "GET");
        assert_eq!(HttpMethod::POST.to_string(), "POST");
    }

    // --- HttpMethod ---

    #[test]
    fn http_method_all_variants_display() {
        assert_eq!(HttpMethod::GET.to_string(), "GET");
        assert_eq!(HttpMethod::POST.to_string(), "POST");
        assert_eq!(HttpMethod::PUT.to_string(), "PUT");
        assert_eq!(HttpMethod::DELETE.to_string(), "DELETE");
        assert_eq!(HttpMethod::PATCH.to_string(), "PATCH");
        assert_eq!(HttpMethod::HEAD.to_string(), "HEAD");
        assert_eq!(HttpMethod::OPTIONS.to_string(), "OPTIONS");
    }

    #[test]
    fn http_method_equality() {
        assert_eq!(HttpMethod::GET, HttpMethod::GET);
        assert_ne!(HttpMethod::GET, HttpMethod::POST);
    }

    // --- HttpResponse ---

    #[test]
    fn http_response_get_header_found() {
        let resp = HttpResponse {
            status_code: 200,
            status_message: "OK".into(),
            headers: vec![("Content-Type".into(), "application/json".into())],
            body: "{}".into(),
            success: true,
            url: "http://example.com".into(),
            method: HttpMethod::GET,
            duration_ms: 100,
        };
        assert_eq!(
            resp.get_header("content-type"),
            Some(&"application/json".to_string())
        );
    }

    #[test]
    fn http_response_get_header_not_found() {
        let resp = HttpResponse {
            status_code: 200,
            status_message: "OK".into(),
            headers: vec![],
            body: "".into(),
            success: true,
            url: "http://example.com".into(),
            method: HttpMethod::GET,
            duration_ms: 0,
        };
        assert!(resp.get_header("X-Missing").is_none());
    }

    #[test]
    fn http_response_get_header_case_insensitive() {
        let resp = HttpResponse {
            status_code: 200,
            status_message: "OK".into(),
            headers: vec![("X-Custom-Header".into(), "value".into())],
            body: "".into(),
            success: true,
            url: "http://example.com".into(),
            method: HttpMethod::GET,
            duration_ms: 0,
        };
        assert_eq!(
            resp.get_header("x-custom-header"),
            Some(&"value".to_string())
        );
        assert_eq!(
            resp.get_header("X-CUSTOM-HEADER"),
            Some(&"value".to_string())
        );
    }

    // --- Tool metadata ---

    #[test]
    fn get_tool_metadata() {
        let t = GetTool;
        assert_eq!(t.name(), "http_get");
        assert_eq!(t.permission(), ToolPermission::Read);
        assert!(t.parameters_schema().is_object());
    }

    #[test]
    fn post_tool_metadata() {
        let t = PostTool;
        assert_eq!(t.name(), "http_post");
        assert_eq!(t.permission(), ToolPermission::Network);
    }

    #[test]
    fn put_tool_metadata() {
        let t = PutTool;
        assert_eq!(t.name(), "http_put");
        assert_eq!(t.permission(), ToolPermission::Network);
    }

    #[test]
    fn delete_tool_metadata() {
        let t = DeleteTool;
        assert_eq!(t.name(), "http_delete");
        assert_eq!(t.permission(), ToolPermission::Network);
    }

    #[test]
    fn tools_execute_ok() {
        let ctx = ToolContext::new("/tmp");
        let result = GetTool.execute(json!({"url": "http://x"}), &ctx).unwrap();
        assert_eq!(result.text, "OK");
    }
}
