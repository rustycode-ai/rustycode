use crate::app::pipeline::browser_manager::BrowserManager;
use crate::app::pipeline::tool_registry::Tool;
use anyhow::{anyhow, Result};
use rustycode_tools::security::validation::validate_url;
use std::sync::Arc;

#[non_exhaustive]
pub struct BrowserGotoTool {
    manager: Arc<BrowserManager>,
}

impl BrowserGotoTool {
    pub fn new(manager: Arc<BrowserManager>) -> Self {
        Self { manager }
    }
}

impl Tool for BrowserGotoTool {
    fn execute(&self, args: serde_json::Value) -> Result<serde_json::Value> {
        let url = args["url"]
            .as_str()
            .ok_or_else(|| anyhow!("'url' parameter required"))?;
        validate_url(url)?;

        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                let page = self.manager.get_page().await?;
                page.goto(url)
                    .await
                    .map_err(|e| anyhow!("navigation failed: {e}"))?;

                tokio::time::sleep(std::time::Duration::from_millis(500)).await;

                let title = page.get_title().await.ok().flatten().unwrap_or_default();
                let final_url = page.url().await.ok().flatten().unwrap_or_default();

                Ok(serde_json::json!({
                    "title": title,
                    "url": final_url,
                }))
            })
        })
    }
}
