//! Standard non-streaming HTTP transport.

use anyhow::{Context, Result};
use async_trait::async_trait;
use futures::Stream;
use serde_json::Value;
use std::pin::Pin;
use std::time::Duration;

use super::Transport;

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
        _url: &str,
        _body: Value,
        _headers: reqwest::header::HeaderMap,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<String>> + Send>>> {
        Err(anyhow::anyhow!(
            "HttpTransport does not support streaming. Use HttpSseTransport."
        ))
    }
}
