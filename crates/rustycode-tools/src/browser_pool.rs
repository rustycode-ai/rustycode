//! Browser pool for lazy Chrome process management.
//!
//! Shared by `BrowserFetchTool` (headless) and TUI `BrowserManager` (headed).
//! The browser launches on first `get_page()` call and stays alive until dropped.
//!
//! Uses `tokio::sync::Mutex` so the guard is `Send` and can be held across `.await`.

use anyhow::{anyhow, Context, Result};
use chromiumoxide::browser::{Browser, BrowserConfig};
use chromiumoxide::Page;
use futures::StreamExt;
use tokio::sync::Mutex;

pub struct BrowserPool {
    inner: Mutex<Option<BrowserInstance>>,
    headless: bool,
}

struct BrowserInstance {
    browser: Browser,
    #[allow(dead_code)]
    handler: tokio::task::JoinHandle<()>,
}

impl BrowserPool {
    pub fn new(headless: bool) -> Self {
        Self {
            inner: Mutex::new(None),
            headless,
        }
    }

    pub async fn get_page(&self) -> Result<Page> {
        {
            let mut inner = self.inner.lock().await;
            if inner.is_none() {
                let mut config_builder = BrowserConfig::builder()
                    .window_size(1280, 1024)
                    .arg("--disable-gpu")
                    .arg("--no-sandbox")
                    .arg("--disable-dev-shm-usage");

                if !self.headless {
                    config_builder = config_builder.with_head();
                }

                let config = config_builder
                    .build()
                    .map_err(|e| anyhow!("browser config error: {e}"))?;

                let (browser, mut handler) = Browser::launch(config)
                    .await
                    .map_err(|e| anyhow!("failed to launch Chrome: {e}"))?;

                let handler_handle = tokio::spawn(async move {
                    while let Some(event) = handler.next().await {
                        if let Err(e) = event {
                            tracing::debug!("CDP event error (continuing): {e}");
                        }
                    }
                });

                *inner = Some(BrowserInstance {
                    browser,
                    handler: handler_handle,
                });
            }
        }

        let inner = self.inner.lock().await;
        let instance = inner.as_ref().context("no browser instance")?;
        instance
            .browser
            .new_page("about:blank")
            .await
            .map_err(|e| anyhow!("failed to create page: {e}"))
    }

    pub async fn shutdown(&self) -> Result<()> {
        let mut inner = self.inner.lock().await;
        if let Some(instance) = inner.take() {
            drop(inner);
            drop(instance.browser);
            let _ = instance.handler.await;
            return Ok(());
        }
        drop(inner);
        Ok(())
    }
}
