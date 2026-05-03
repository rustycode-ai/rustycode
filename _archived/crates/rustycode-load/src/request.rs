//! Request types and execution

use crate::error::Result;
use chrono::{DateTime, Utc};
use reqwest::{Client, Method};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::{Duration, Instant};
use uuid::Uuid;

/// A load test request that can be executed
#[derive(Clone)]
#[non_exhaustive]
pub enum LoadRequest {
    /// HTTP request
    Http(HttpRequest),

    /// Custom async request
    Custom(CustomRequest),
}

/// HTTP request configuration
#[derive(Clone)]
pub struct HttpRequest {
    /// Request ID
    pub id: Uuid,

    /// User ID (for scenarios with multiple users)
    pub user_id: usize,

    /// Request URL
    pub url: String,

    /// HTTP method
    pub method: Method,

    /// Request headers
    pub headers: Vec<(String, String)>,

    /// Request body
    pub body: Option<String>,

    /// Request timeout
    pub timeout: Duration,

    /// Timestamp when request was created
    pub created_at: DateTime<Utc>,
}

impl HttpRequest {
    /// Create a new HTTP request
    pub fn new(url: String, method: Method) -> Self {
        Self {
            id: Uuid::new_v4(),
            user_id: 0,
            url,
            method,
            headers: Vec::new(),
            body: None,
            timeout: Duration::from_secs(30),
            created_at: Utc::now(),
        }
    }

    /// Add a header
    pub fn with_header(mut self, key: String, value: String) -> Self {
        self.headers.push((key, value));
        self
    }

    /// Set the request body
    pub fn with_body(mut self, body: String) -> Self {
        self.body = Some(body);
        self
    }

    /// Set the timeout
    pub const fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Set the user ID
    pub const fn with_user_id(mut self, user_id: usize) -> Self {
        self.user_id = user_id;
        self
    }
}

/// Custom async request
pub struct CustomRequest {
    /// Request ID
    pub id: Uuid,

    /// User ID
    pub user_id: usize,

    /// Async function to execute
    pub execute: Arc<
        dyn Fn(ExecutionContext) -> futures::future::BoxFuture<'static, Result<LoadResult>>
            + Send
            + Sync,
    >,

    /// Timestamp when request was created
    pub created_at: DateTime<Utc>,
}

impl Clone for CustomRequest {
    fn clone(&self) -> Self {
        Self {
            id: self.id,
            user_id: self.user_id,
            execute: Arc::clone(&self.execute),
            created_at: self.created_at,
        }
    }
}

/// Execution context for custom requests
#[derive(Clone)]
pub struct ExecutionContext {
    /// User ID
    pub user_id: usize,

    /// Request ID
    pub request_id: Uuid,

    /// HTTP client (for making requests)
    pub http_client: Arc<Client>,
}

impl LoadRequest {
    /// Create a simple HTTP GET request
    pub fn http_get(url: String) -> Self {
        Self::Http(HttpRequest::new(url, Method::GET))
    }

    /// Create a simple HTTP POST request
    pub fn http_post(url: String) -> Self {
        Self::Http(HttpRequest::new(url, Method::POST))
    }

    /// Create an HTTP request with method
    pub fn http(url: String, method: Method) -> Self {
        Self::Http(HttpRequest::new(url, method))
    }

    /// Create a custom async request
    pub fn custom<F>(f: F) -> Self
    where
        F: Fn(ExecutionContext) -> futures::future::BoxFuture<'static, Result<LoadResult>>
            + Send
            + Sync
            + 'static,
    {
        Self::Custom(CustomRequest {
            id: Uuid::new_v4(),
            user_id: 0,
            execute: Arc::new(f),
            created_at: Utc::now(),
        })
    }

    /// Set the user ID
    #[allow(clippy::missing_const_for_fn)]
    pub fn with_user_id(mut self, user_id: usize) -> Self {
        match self {
            Self::Http(ref mut req) => req.user_id = user_id,
            Self::Custom(ref mut req) => req.user_id = user_id,
        }
        self
    }

    /// Execute the request
    pub async fn execute(&self, http_client: &Client) -> LoadResult {
        let start = Instant::now();

        match self {
            Self::Http(req) => {
                let mut builder = http_client
                    .request(req.method.clone(), &req.url)
                    .timeout(req.timeout);

                for (key, value) in &req.headers {
                    builder = builder.header(key, value);
                }

                if let Some(body) = &req.body {
                    builder = builder.body(body.clone());
                }

                match builder.send().await {
                    Ok(response) => {
                        let status = response.status();
                        let duration = start.elapsed();

                        if status.is_success() {
                            LoadResult::success(duration)
                        } else {
                            LoadResult::http_error(
                                duration,
                                status.as_u16(),
                                status.canonical_reason().unwrap_or("Unknown").to_string(),
                            )
                        }
                    }
                    Err(e) => {
                        let duration = start.elapsed();

                        if e.is_timeout() {
                            LoadResult::timeout(duration, req.timeout)
                        } else if e.is_connect() {
                            LoadResult::connection_error(duration, &e.to_string())
                        } else {
                            LoadResult::error(duration, e.to_string())
                        }
                    }
                }
            }
            Self::Custom(req) => {
                let ctx = ExecutionContext {
                    user_id: req.user_id,
                    request_id: req.id,
                    http_client: Arc::new(http_client.clone()),
                };

                match (req.execute)(ctx).await {
                    Ok(result) => result,
                    Err(e) => LoadResult::error(start.elapsed(), e.to_string()),
                }
            }
        }
    }
}

/// Result of executing a load test request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoadResult {
    /// Request ID
    pub request_id: Uuid,

    /// User ID
    pub user_id: usize,

    /// Whether the request was successful
    pub success: bool,

    /// Time taken to execute the request
    pub duration: Duration,

    /// Error message (if failed)
    pub error: Option<String>,

    /// HTTP status code (if applicable)
    pub status_code: Option<u16>,

    /// Timestamp when the request was executed
    pub timestamp: DateTime<Utc>,
}

impl LoadResult {
    /// Create a successful result
    pub fn success(duration: Duration) -> Self {
        Self {
            request_id: Uuid::new_v4(),
            user_id: 0,
            success: true,
            duration,
            error: None,
            status_code: None,
            timestamp: Utc::now(),
        }
    }

    /// Create a failed result
    pub fn error(duration: Duration, error: String) -> Self {
        Self {
            request_id: Uuid::new_v4(),
            user_id: 0,
            success: false,
            duration,
            error: Some(error),
            status_code: None,
            timestamp: Utc::now(),
        }
    }

    /// Create a timeout result
    pub fn timeout(duration: Duration, timeout: Duration) -> Self {
        Self::error(duration, format!("Request timed out after {timeout:?}"))
    }

    /// Create a connection error result
    pub fn connection_error(duration: Duration, error: &str) -> Self {
        Self::error(duration, format!("Connection error: {error}"))
    }

    /// Create an HTTP error result
    pub fn http_error(duration: Duration, status: u16, message: String) -> Self {
        Self {
            request_id: Uuid::new_v4(),
            user_id: 0,
            success: false,
            duration,
            error: Some(message),
            status_code: Some(status),
            timestamp: Utc::now(),
        }
    }

    /// Set the user ID
    pub const fn with_user_id(mut self, user_id: usize) -> Self {
        self.user_id = user_id;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_http_request_creation() {
        let req = HttpRequest::new("https://example.com".to_string(), Method::GET)
            .with_header("Authorization".to_string(), "Bearer token".to_string())
            .with_user_id(5);

        assert_eq!(req.user_id, 5);
        assert_eq!(req.headers.len(), 1);
        assert_eq!(req.method, Method::GET);
    }

    #[test]
    fn test_load_result_creation() {
        let success = LoadResult::success(Duration::from_millis(100));
        assert!(success.success);
        assert!(success.error.is_none());

        let error = LoadResult::error(
            Duration::from_millis(50),
            "Something went wrong".to_string(),
        );
        assert!(!error.success);
        assert_eq!(error.error, Some("Something went wrong".to_string()));
    }

    #[test]
    fn test_load_request_types() {
        let http_req = LoadRequest::http_get("https://example.com".to_string());
        match http_req {
            LoadRequest::Http(_) => {}
            LoadRequest::Custom(_) => panic!("Expected HTTP request"),
        }

        let custom_req = LoadRequest::custom(|_| {
            Box::pin(async { Ok(LoadResult::success(Duration::from_millis(10))) })
        });
        match custom_req {
            LoadRequest::Http(_) => panic!("Expected Custom request"),
            LoadRequest::Custom(_) => {}
        }
    }

    #[test]
    fn test_http_request_with_body() {
        let req = HttpRequest::new("https://example.com".to_string(), Method::POST)
            .with_body(r#"{"key":"value"}"#.to_string());
        assert_eq!(req.body, Some(r#"{"key":"value"}"#.to_string()));
        assert_eq!(req.method, Method::POST);
    }

    #[test]
    fn test_http_request_with_timeout() {
        let req = HttpRequest::new("https://example.com".to_string(), Method::GET)
            .with_timeout(Duration::from_secs(5));
        assert_eq!(req.timeout, Duration::from_secs(5));
    }

    #[test]
    fn test_http_request_default_timeout() {
        let req = HttpRequest::new("https://example.com".to_string(), Method::GET);
        assert_eq!(req.timeout, Duration::from_secs(30));
    }

    #[test]
    fn test_http_request_multiple_headers() {
        let req = HttpRequest::new("https://example.com".to_string(), Method::GET)
            .with_header("Auth".to_string(), "token".to_string())
            .with_header("Accept".to_string(), "application/json".to_string())
            .with_header("X-Custom".to_string(), "value".to_string());
        assert_eq!(req.headers.len(), 3);
    }

    #[test]
    fn test_http_request_default_user_id() {
        let req = HttpRequest::new("https://example.com".to_string(), Method::GET);
        assert_eq!(req.user_id, 0);
    }

    #[test]
    fn test_load_request_http_post() {
        let req = LoadRequest::http_post("https://example.com/api".to_string());
        match req {
            LoadRequest::Http(http) => assert_eq!(http.method, Method::POST),
            LoadRequest::Custom(_) => panic!("Expected HTTP request"),
        }
    }

    #[test]
    fn test_load_request_http_with_method() {
        let req = LoadRequest::http("https://example.com".to_string(), Method::DELETE);
        match req {
            LoadRequest::Http(http) => assert_eq!(http.method, Method::DELETE),
            LoadRequest::Custom(_) => panic!("Expected HTTP request"),
        }
    }

    #[test]
    fn test_load_request_with_user_id_http() {
        let req = LoadRequest::http_get("https://example.com".to_string()).with_user_id(42);
        match req {
            LoadRequest::Http(http) => assert_eq!(http.user_id, 42),
            LoadRequest::Custom(_) => panic!("Expected HTTP request"),
        }
    }

    #[test]
    fn test_load_request_with_user_id_custom() {
        let req = LoadRequest::custom(|_| {
            Box::pin(async { Ok(LoadResult::success(Duration::from_millis(10))) })
        })
        .with_user_id(7);
        match req {
            LoadRequest::Custom(custom) => assert_eq!(custom.user_id, 7),
            LoadRequest::Http(_) => panic!("Expected Custom request"),
        }
    }

    #[test]
    fn test_load_result_success() {
        let result = LoadResult::success(Duration::from_millis(150));
        assert!(result.success);
        assert_eq!(result.duration, Duration::from_millis(150));
        assert!(result.error.is_none());
        assert!(result.status_code.is_none());
        assert_eq!(result.user_id, 0);
    }

    #[test]
    fn test_load_result_error() {
        let result = LoadResult::error(Duration::from_millis(50), "Something failed".to_string());
        assert!(!result.success);
        assert_eq!(result.error, Some("Something failed".to_string()));
        assert!(result.status_code.is_none());
    }

    #[test]
    fn test_load_result_timeout() {
        let result = LoadResult::timeout(Duration::from_secs(30), Duration::from_secs(30));
        assert!(!result.success);
        assert!(result.error.as_ref().unwrap().contains("timed out"));
        assert!(result.error.as_ref().unwrap().contains("30"));
    }

    #[test]
    fn test_load_result_connection_error() {
        let result = LoadResult::connection_error(Duration::from_millis(10), "refused");
        assert!(!result.success);
        assert!(result.error.as_ref().unwrap().contains("Connection error"));
        assert!(result.error.as_ref().unwrap().contains("refused"));
    }

    #[test]
    fn test_load_result_http_error() {
        let result = LoadResult::http_error(
            Duration::from_millis(200),
            503,
            "Service Unavailable".to_string(),
        );
        assert!(!result.success);
        assert_eq!(result.status_code, Some(503));
        assert_eq!(result.error, Some("Service Unavailable".to_string()));
    }

    #[test]
    fn test_load_result_with_user_id() {
        let result = LoadResult::success(Duration::from_millis(100)).with_user_id(5);
        assert_eq!(result.user_id, 5);
    }

    #[test]
    fn test_load_result_serialization_roundtrip() {
        let result = LoadResult::http_error(
            Duration::from_millis(250),
            429,
            "Too Many Requests".to_string(),
        )
        .with_user_id(3);
        let json = serde_json::to_string(&result).unwrap();
        let decoded: LoadResult = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.user_id, 3);
        assert!(!decoded.success);
        assert_eq!(decoded.status_code, Some(429));
        assert_eq!(decoded.error, Some("Too Many Requests".to_string()));
    }

    #[test]
    fn test_http_request_has_uuid() {
        let req = HttpRequest::new("https://example.com".to_string(), Method::GET);
        assert!(!req.id.is_nil());
    }

    #[test]
    fn test_load_result_has_uuid() {
        let result = LoadResult::success(Duration::from_millis(100));
        assert!(!result.request_id.is_nil());
    }

    #[test]
    fn test_http_request_has_timestamp() {
        let before = Utc::now();
        let req = HttpRequest::new("https://example.com".to_string(), Method::GET);
        let after = Utc::now();
        assert!(req.created_at >= before);
        assert!(req.created_at <= after);
    }

    #[test]
    fn test_load_result_has_timestamp() {
        let before = Utc::now();
        let result = LoadResult::success(Duration::from_millis(100));
        let after = Utc::now();
        assert!(result.timestamp >= before);
        assert!(result.timestamp <= after);
    }

    #[test]
    fn test_execution_context_clone() {
        let ctx = ExecutionContext {
            user_id: 1,
            request_id: Uuid::new_v4(),
            http_client: Arc::new(Client::new()),
        };
        let cloned = ctx.clone();
        assert_eq!(cloned.user_id, ctx.user_id);
        assert_eq!(cloned.request_id, ctx.request_id);
    }

    #[test]
    fn test_custom_request_clone() {
        let req = LoadRequest::custom(|_| {
            Box::pin(async { Ok(LoadResult::success(Duration::from_millis(10))) })
        });
        if let LoadRequest::Custom(custom) = req {
            let cloned = custom.clone();
            assert_eq!(cloned.user_id, custom.user_id);
            assert_eq!(cloned.id, custom.id);
        }
    }
}
