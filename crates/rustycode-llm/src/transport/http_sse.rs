//! Streaming SSE (Server-Sent Events) HTTP transport.

use anyhow::{Context, Result};
use async_trait::async_trait;
use futures::{Stream, StreamExt};
use serde_json::Value;
use std::pin::Pin;
use std::time::Duration;

use super::Transport;
use crate::sse::SseByteBuffer;

pub struct HttpSseTransport {
    client: reqwest::Client,
}

impl HttpSseTransport {
    pub fn new(timeout_seconds: u64) -> Result<Self> {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(timeout_seconds))
            .connect_timeout(Duration::from_secs(10))
            .build()
            .context("failed to build HTTP client for SSE")?;
        Ok(Self { client })
    }
}

#[async_trait]
impl Transport for HttpSseTransport {
    async fn send(
        &self,
        _url: &str,
        _body: Value,
        _headers: reqwest::header::HeaderMap,
    ) -> Result<Value> {
        Err(anyhow::anyhow!("HttpSseTransport only supports streaming. Use HttpTransport for non-streaming requests."))
    }

    async fn stream(
        &self,
        url: &str,
        body: Value,
        headers: reqwest::header::HeaderMap,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<String>> + Send>>> {
        let response = self
            .client
            .post(url)
            .headers(headers)
            .json(&body)
            .send()
            .await
            .context("SSE request failed")?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "unable to read error".to_string());
            return Err(anyhow::anyhow!("SSE HTTP error {}: {}", status, error_text));
        }

        let bytes_stream = response.bytes_stream();
        let sse_buffer = SseByteBuffer::new();

        let sse_stream = bytes_stream.flat_map(move |chunk_result| {
            let lines: Vec<Result<String>> = match chunk_result {
                Ok(bytes) => {
                    let lines = sse_buffer.feed_chunk(&bytes);
                    lines.into_iter().map(Ok).collect()
                }
                Err(e) => {
                    vec![Err(anyhow::anyhow!("failed to read SSE chunk: {}", e))]
                }
            };
            futures::stream::iter(lines)
        });

        Ok(Box::pin(sse_stream))
    }
}
