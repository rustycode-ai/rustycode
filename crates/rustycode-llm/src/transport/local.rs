//! In-process transport for local LiteRT-LM models.

use anyhow::Result;
use async_trait::async_trait;
use futures::Stream;
use serde_json::Value;
use std::pin::Pin;

use super::Transport;

#[cfg(feature = "litert")]
use {futures::StreamExt, rustycode_litert::LitManager, std::sync::Arc, tokio::sync::OnceCell};

pub struct LocalTransport {
    #[cfg(feature = "litert")]
    manager: OnceCell<Arc<LitManager>>,
    #[allow(dead_code)]
    model_name: String,
}

impl LocalTransport {
    pub fn new(model_name: String) -> Self {
        Self {
            #[cfg(feature = "litert")]
            manager: OnceCell::new(),
            model_name,
        }
    }

    #[cfg(feature = "litert")]
    async fn ensure_manager(&self) -> Result<Arc<LitManager>> {
        self.manager
            .get_or_try_init(|| async {
                LitManager::new()
                    .await
                    .map(Arc::new)
                    .map_err(|err| anyhow::anyhow!("failed to initialize LiteRT manager: {}", err))
            })
            .await
            .map(Arc::clone)
    }
}

#[async_trait]
impl Transport for LocalTransport {
    async fn send(
        &self,
        _url: &str,
        _body: Value,
        _headers: reqwest::header::HeaderMap,
    ) -> Result<Value> {
        #[cfg(feature = "litert")]
        {
            let prompt = _body
                .get("prompt")
                .and_then(|p| p.as_str())
                .ok_or_else(|| anyhow::anyhow!("missing prompt in body"))?;
            let manager = self.ensure_manager().await?;

            let content = manager
                .run_completion(&self.model_name, prompt)
                .await
                .map_err(|err| anyhow::anyhow!("LiteRT completion failed: {}", err))?;

            Ok(serde_json::json!({ "content": content }))
        }
        #[cfg(not(feature = "litert"))]
        {
            Err(anyhow::anyhow!(
                "LiteRT feature is not enabled. Recompile with --features litert."
            ))
        }
    }

    async fn stream(
        &self,
        _url: &str,
        _body: Value,
        _headers: reqwest::header::HeaderMap,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<String>> + Send>>> {
        #[cfg(feature = "litert")]
        {
            let prompt = _body
                .get("prompt")
                .and_then(|p| p.as_str())
                .ok_or_else(|| anyhow::anyhow!("missing prompt in body"))?;
            let manager = self.ensure_manager().await?;

            let stream = manager
                .run_completion_stream(&self.model_name, prompt)
                .await
                .map_err(|err| anyhow::anyhow!("LiteRT stream initiation failed: {}", err))?
                .map(|chunk| chunk.map_err(|err| anyhow::anyhow!("LiteRT stream error: {}", err)));

            Ok(Box::pin(stream))
        }
        #[cfg(not(feature = "litert"))]
        {
            Err(anyhow::anyhow!(
                "LiteRT feature is not enabled. Recompile with --features litert."
            ))
        }
    }
}
