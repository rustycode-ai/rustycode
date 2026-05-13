//! Browser manager backed by BrowserPool for pipeline browser tools.

use anyhow::Result;
use chromiumoxide::Page;
use rustycode_tools::browser_pool::BrowserPool;
use std::sync::Arc;
use tokio::sync::Mutex;

/// Manages a lazily-launched browser for the TUI pipeline.
///
/// Headed by default (visible browser) so the user can see what's happening.
/// The browser starts on first `get_page()` call and stays alive until dropped.
pub struct BrowserManager {
    pool: Arc<BrowserPool>,
    page: Mutex<Option<Page>>,
}

impl BrowserManager {
    pub fn new() -> Self {
        Self {
            pool: Arc::new(BrowserPool::new(false)),
            page: Mutex::new(None),
        }
    }

    /// Create a headless variant (for testing or CI).
    pub fn new_headless() -> Self {
        Self {
            pool: Arc::new(BrowserPool::new(true)),
            page: Mutex::new(None),
        }
    }

    pub async fn get_page(&self) -> Result<Page> {
        // Fast path: check cached page
        if let Some(page) = self.page.lock().await.clone() {
            if page.evaluate("1").await.is_ok() {
                return Ok(page);
            }
        }

        // Slow path: allocate a new page from the pool
        let page = self.pool.get_page().await?;
        let mut cached = self.page.lock().await;
        // Re-check: another task may have stored a valid page while we
        // were allocating, so prefer the existing one to avoid leaking
        // a browser page.
        if let Some(existing) = cached.clone() {
            if existing.evaluate("1").await.is_ok() {
                return Ok(existing);
            }
        }
        *cached = Some(page.clone());
        Ok(page)
    }

    /// Shut down the browser process.
    pub async fn shutdown(&self) -> Result<()> {
        self.page.lock().await.take();
        self.pool.shutdown().await
    }
}

impl Default for BrowserManager {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for BrowserManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BrowserManager").finish()
    }
}
