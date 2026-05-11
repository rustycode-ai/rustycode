//! Fallback strategy for transports.

use anyhow::Result;
use async_trait::async_trait;
use futures::Stream;
use serde_json::Value;
use std::pin::Pin;

use super::Transport;

pub struct TransportFallback {
    primary: Box<dyn Transport>,
    secondary: Box<dyn Transport>,
}

impl TransportFallback {
    pub fn new(primary: Box<dyn Transport>, secondary: Box<dyn Transport>) -> Self {
        Self { primary, secondary }
    }
}

#[async_trait]
impl Transport for TransportFallback {
    async fn send(
        &self,
        url: &str,
        body: Value,
        headers: reqwest::header::HeaderMap,
    ) -> Result<Value> {
        match self.primary.send(url, body.clone(), headers.clone()).await {
            Ok(res) => Ok(res),
            Err(e) => {
                tracing::warn!(
                    "Primary transport failed: {}. Falling back to secondary.",
                    e
                );
                self.secondary.send(url, body, headers).await
            }
        }
    }

    async fn stream(
        &self,
        url: &str,
        body: Value,
        headers: reqwest::header::HeaderMap,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<String>> + Send>>> {
        match self
            .primary
            .stream(url, body.clone(), headers.clone())
            .await
        {
            Ok(stream) => Ok(stream),
            Err(e) => {
                tracing::warn!(
                    "Primary streaming transport failed: {}. Falling back to secondary.",
                    e
                );
                self.secondary.stream(url, body, headers).await
            }
        }
    }
}
