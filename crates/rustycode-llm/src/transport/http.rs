//! HTTP transport supporting both non-streaming and SSE streaming responses.

use anyhow::{Context, Result};
use async_trait::async_trait;
use futures::{Stream, StreamExt};
use serde_json::Value;
use std::pin::Pin;
use std::time::Duration;

use super::Transport;
use crate::sse::SseByteBuffer;

pub struct HttpTransport {
    client: reqwest::Client,
}

impl HttpTransport {
    pub fn new(timeout_seconds: u64) -> Result<Self> {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(timeout_seconds))
            .connect_timeout(Duration::from_secs(10))
            .build()
            .context("failed to build HTTP client")?;
        Ok(Self { client })
    }
}

#[async_trait]
impl Transport for HttpTransport {
    async fn send(
        &self,
        url: &str,
        body: Value,
        headers: reqwest::header::HeaderMap,
    ) -> Result<Value> {
        let response = self
            .client
            .post(url)
            .headers(headers)
            .json(&body)
            .send()
            .await
            .context("HTTP request failed")?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "unable to read error".to_string());
            return Err(anyhow::anyhow!("HTTP error {}: {}", status, error_text));
        }

        let json = response
            .json()
            .await
            .context("failed to parse JSON response")?;
        Ok(json)
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
            .context("streaming request failed")?;

        let status = response.status();
        let ct = response
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("unknown");

        tracing::info!(
            target: "llm::transport",
            status = %status,
            content_type = ct,
            url,
            "stream response received"
        );

        if !status.is_success() {
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "unable to read error".to_string());
            return Err(anyhow::anyhow!(
                "stream HTTP error {}: {}",
                status,
                error_text
            ));
        }

        if ct.contains("text/event-stream") {
            // SSE response — parse as event stream via byte buffer
            let bytes_stream = response.bytes_stream();
            let sse_buffer = SseByteBuffer::new();

            let sse_stream = bytes_stream.flat_map(move |chunk_result| {
                let lines: Vec<Result<String>> = match chunk_result {
                    Ok(bytes) => {
                        if bytes.len() < 512 {
                            tracing::debug!(
                                target: "llm::transport",
                                bytes_len = bytes.len(),
                                raw = %String::from_utf8_lossy(&bytes),
                                "raw SSE bytes"
                            );
                        } else {
                            tracing::debug!(
                                target: "llm::transport",
                                bytes_len = bytes.len(),
                                "raw SSE bytes (large chunk, content truncated)"
                            );
                        }
                        let lines = sse_buffer.feed_chunk(&bytes);
                        if !lines.is_empty() {
                            tracing::debug!(
                                target: "llm::transport",
                                line_count = lines.len(),
                                "SSE lines extracted"
                            );
                        }
                        lines.into_iter().map(Ok).collect()
                    }
                    Err(e) => {
                        vec![Err(anyhow::anyhow!("failed to read SSE chunk: {}", e))]
                    }
                };
                futures::stream::iter(lines)
            });

            Ok(Box::pin(sse_stream))
        } else {
            // JSON response — emit entire body as a single line so the protocol
            // layer can parse it as a non-streaming response wrapped in a stream.
            let text = response
                .text()
                .await
                .context("failed to read non-SSE stream response")?;
            tracing::debug!(
                target: "llm::transport",
                len = text.len(),
                "non-SSE response body read as single chunk"
            );
            Ok(Box::pin(futures::stream::iter(vec![Ok(text)])))
        }
    }
}
