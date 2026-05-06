//! Response debug context extraction from LLM API response headers.
//!
//! Extracts request IDs, CF-Ray headers, and auth error details from API
//! responses for inclusion in error messages.

use std::fmt;

/// Debug context extracted from an LLM API response's headers.
#[derive(Debug, Clone, Default)]
pub struct ResponseDebugContext {
    /// Request ID from `x-request-id` (Anthropic) or `x-oai-request-id` (OpenAI).
    pub request_id: Option<String>,
    /// Cloudflare ray ID from `cf-ray` header.
    pub cf_ray: Option<String>,
    /// Retry-after duration in seconds, parsed from `retry-after` header.
    pub retry_after: Option<u64>,
    /// Error type classification extracted from the response body or headers.
    pub error_type: Option<String>,
}

impl ResponseDebugContext {
    /// Extract debug context from response headers.
    pub fn from_response_headers(headers: &reqwest::header::HeaderMap) -> Self {
        let request_id = headers
            .get("x-request-id")
            .or_else(|| headers.get("x-oai-request-id"))
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());
        let cf_ray = headers
            .get("cf-ray")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());
        let retry_after = headers
            .get("retry-after")
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.parse::<u64>().ok());
        Self {
            request_id,
            cf_ray,
            retry_after,
            error_type: None,
        }
    }

    /// Append debug context to a base error message.
    ///
    /// If no context is available, returns the base error unchanged.
    pub fn format_error_message(&self, base_error: &str) -> String {
        if !self.has_context() {
            return base_error.to_string();
        }
        let mut parts = vec![];
        if let Some(ref req_id) = self.request_id {
            parts.push(format!("req: {req_id}"));
        }
        if let Some(ref ray) = self.cf_ray {
            parts.push(format!("cf-ray: {ray}"));
        }
        if let Some(secs) = self.retry_after {
            parts.push(format!("retry after {secs}s"));
        }
        if let Some(ref etype) = self.error_type {
            parts.push(format!("type: {etype}"));
        }
        format!("{} ({})", base_error, parts.join(", "))
    }

    /// Returns `true` if any debug field is populated.
    pub fn has_context(&self) -> bool {
        self.request_id.is_some()
            || self.cf_ray.is_some()
            || self.retry_after.is_some()
            || self.error_type.is_some()
    }
}

impl fmt::Display for ResponseDebugContext {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut parts = vec![];
        if let Some(ref req_id) = self.request_id {
            parts.push(format!("request_id={req_id}"));
        }
        if let Some(ref ray) = self.cf_ray {
            parts.push(format!("cf_ray={ray}"));
        }
        if let Some(secs) = self.retry_after {
            parts.push(format!("retry_after={secs}s"));
        }
        if let Some(ref etype) = self.error_type {
            parts.push(format!("error_type={etype}"));
        }
        if parts.is_empty() {
            write!(f, "ResponseDebugContext(empty)")
        } else {
            write!(f, "ResponseDebugContext({})", parts.join(", "))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_headers(pairs: &[(&str, &str)]) -> reqwest::header::HeaderMap {
        let mut map = reqwest::header::HeaderMap::new();
        for (key, val) in pairs {
            map.insert(
                reqwest::header::HeaderName::from_bytes(key.as_bytes())
                    .unwrap_or_else(|_| reqwest::header::HeaderName::from_static("x-unused")),
                val.parse()
                    .unwrap_or_else(|_| reqwest::header::HeaderValue::from_static("")),
            );
        }
        map
    }

    #[test]
    fn test_extract_all_headers() {
        let headers = make_headers(&[
            ("x-request-id", "req-abc123"),
            ("cf-ray", "ray-xyz789"),
            ("retry-after", "30"),
        ]);
        let ctx = ResponseDebugContext::from_response_headers(&headers);
        assert_eq!(ctx.request_id.as_deref(), Some("req-abc123"));
        assert_eq!(ctx.cf_ray.as_deref(), Some("ray-xyz789"));
        assert_eq!(ctx.retry_after, Some(30));
        assert!(ctx.has_context());
    }

    #[test]
    fn test_extract_partial_headers() {
        let headers = make_headers(&[("x-request-id", "req-only")]);
        let ctx = ResponseDebugContext::from_response_headers(&headers);
        assert_eq!(ctx.request_id.as_deref(), Some("req-only"));
        assert!(ctx.cf_ray.is_none());
        assert!(ctx.retry_after.is_none());
        assert!(ctx.has_context());
    }

    #[test]
    fn test_no_headers() {
        let ctx = ResponseDebugContext::from_response_headers(&reqwest::header::HeaderMap::new());
        assert!(ctx.request_id.is_none());
        assert!(ctx.cf_ray.is_none());
        assert!(ctx.retry_after.is_none());
        assert!(!ctx.has_context());
    }

    #[test]
    fn test_format_with_request_id() {
        let mut ctx = ResponseDebugContext::default();
        ctx.request_id = Some("req-abc123".into());
        ctx.retry_after = Some(30);
        let msg = ctx.format_error_message("Rate limited");
        assert!(
            msg.contains("req: req-abc123"),
            "should contain request ID: {msg}"
        );
        assert!(
            msg.contains("retry after 30s"),
            "should contain retry after: {msg}"
        );
    }

    #[test]
    fn test_format_without_context() {
        let msg = ResponseDebugContext::default().format_error_message("Something went wrong");
        assert_eq!(msg, "Something went wrong");
    }

    #[test]
    fn test_retry_after_parsing() {
        let ctx =
            ResponseDebugContext::from_response_headers(&make_headers(&[("retry-after", "60")]));
        assert_eq!(ctx.retry_after, Some(60));
        let ctx = ResponseDebugContext::from_response_headers(&make_headers(&[(
            "retry-after",
            "not-a-number",
        )]));
        assert!(ctx.retry_after.is_none());
    }

    #[test]
    fn test_openai_request_id_header() {
        let ctx = ResponseDebugContext::from_response_headers(&make_headers(&[(
            "x-oai-request-id",
            "oai-123",
        )]));
        assert_eq!(ctx.request_id.as_deref(), Some("oai-123"));
    }

    #[test]
    fn test_display_trait() {
        let mut ctx = ResponseDebugContext::default();
        ctx.request_id = Some("req-abc".into());
        ctx.cf_ray = Some("ray-xyz".into());
        let display = format!("{ctx}");
        assert!(display.contains("request_id=req-abc"));
        assert!(display.contains("cf_ray=ray-xyz"));
        assert_eq!(
            format!("{}", ResponseDebugContext::default()),
            "ResponseDebugContext(empty)"
        );
    }
}
