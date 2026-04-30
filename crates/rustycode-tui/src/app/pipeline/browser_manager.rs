//! Browser manager for pipeline browser-based tools.
//!
//! NOTE: This is a stub module. The chromiumoxide API integration is not yet
//! complete. Re-enable when browser tool support is implemented.

use anyhow::{anyhow, Result};

/// Stub browser manager — browser tool support not yet implemented.
pub struct BrowserManager {
    _private: (),
}

impl BrowserManager {
    pub fn new() -> Self {
        Self { _private: () }
    }

    pub async fn get_browser(&self) -> Result<()> {
        Err(anyhow!("Browser tool support is not yet implemented"))
    }
}

impl Default for BrowserManager {
    fn default() -> Self {
        Self::new()
    }
}
