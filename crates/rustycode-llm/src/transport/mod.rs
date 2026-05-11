//! LLM delivery transports (HTTP, SSE, Local).

use anyhow::Result;
use async_trait::async_trait;
use futures::Stream;
use serde_json::Value;
use std::pin::Pin;

/// Transport identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransportType {
    Http,
    HttpSse,
    Local,
}

/// Generic transport for sending requests and receiving responses.
#[async_trait]
pub trait Transport: Send + Sync {
    /// Send a non-streaming request.
    async fn send(
        &self,
        url: &str,
        body: Value,
        headers: reqwest::header::HeaderMap,
    ) -> Result<Value>;

    /// Send a streaming request.
    async fn stream(
        &self,
        url: &str,
        body: Value,
        headers: reqwest::header::HeaderMap,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<String>> + Send>>>;
}

pub mod fallback;
pub mod http;
pub mod http_sse;
pub mod local;

pub use http::HttpTransport;
pub use http_sse::HttpSseTransport;
pub use local::LocalTransport;
