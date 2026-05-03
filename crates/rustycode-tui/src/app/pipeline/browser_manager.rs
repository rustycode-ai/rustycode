//! Browser manager backed by BrowserPool for pipeline browser tools.

use anyhow::Result;
use chromiumoxide::Page;
use rustycode_tools::browser_pool::BrowserPool;
use std::sync::Arc;

/// Manages a lazily-launched browser for the TUI pipeline.
///
/// Headed by default (visible browser) so the user can see what's happening.
/// The browser starts on first `get_page()` call and stays alive until dropped.
pub struct BrowserManager {
    pool: Arc<BrowserPool>,
}

impl BrowserManager {
    pub fn new() -> Self {
        Self {
            pool: Arc::new(BrowserPool::new(false)),
        }
    }

    /// Create a headless variant (for testing or CI).
    pub fn new_headless() -> Self {
        Self {
            pool: Arc::new(BrowserPool::new(true)),
        }
    }

    /// Get a new browser page, launching the browser if needed.
    pub async fn get_page(&self) -> Result<Page> {
        self.pool.get_page().await
    }

    /// Shut down the browser process.
    pub async fn shutdown(&self) -> Result<()> {
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
