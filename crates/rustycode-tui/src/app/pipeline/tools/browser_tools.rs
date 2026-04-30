use crate::app::pipeline::browser_manager::BrowserManager;
use crate::app::pipeline::tool_registry::Tool;
use anyhow::{anyhow, Result};
use std::sync::Arc;

pub struct BrowserGotoTool {
    manager: Arc<BrowserManager>,
}

impl BrowserGotoTool {
    pub fn new(manager: Arc<BrowserManager>) -> Self {
        Self { manager }
    }
}

impl Tool for BrowserGotoTool {
    fn execute(&self, _args: serde_json::Value) -> Result<serde_json::Value> {
        let rt = tokio::runtime::Handle::current();
        rt.block_on(self.manager.get_browser())?;
        Err(anyhow!("Browser tool support is not yet implemented"))
    }
}
