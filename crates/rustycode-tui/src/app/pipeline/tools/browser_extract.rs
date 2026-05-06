use crate::app::pipeline::browser_manager::BrowserManager;
use crate::app::pipeline::tool_registry::Tool;
use anyhow::{anyhow, Result};
use std::sync::Arc;

pub struct BrowserExtractTool {
    manager: Arc<BrowserManager>,
}

impl BrowserExtractTool {
    pub fn new(manager: Arc<BrowserManager>) -> Self {
        Self { manager }
    }
}

impl Tool for BrowserExtractTool {
    fn execute(&self, args: serde_json::Value) -> Result<serde_json::Value> {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                let page = self.manager.get_page().await?;

                let selector = args["selector"].as_str();

                if args
                    .get("screenshot")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false)
                {
                    let bytes = page
                        .screenshot(
                            chromiumoxide::page::ScreenshotParams::builder()
                                .full_page(true)
                                .build(),
                        )
                        .await
                        .map_err(|e| anyhow!("screenshot failed: {e}"))?;

                    let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
                    return Ok(serde_json::json!({
                        "type": "screenshot",
                        "data": b64,
                        "mime_type": "image/png",
                        "size_bytes": bytes.len(),
                    }));
                }

                let (content, text) = if let Some(sel) = selector {
                    let el = page
                        .find_element(sel)
                        .await
                        .map_err(|e| anyhow!("element not found '{sel}': {e}"))?;
                    let html = el
                        .inner_html()
                        .await
                        .map_err(|e| anyhow!("failed to get inner HTML: {e}"))?
                        .unwrap_or_default();
                    let txt = el
                        .inner_text()
                        .await
                        .map_err(|e| anyhow!("failed to get text: {e}"))?
                        .unwrap_or_default();
                    (html, txt)
                } else {
                    let html = page
                        .content()
                        .await
                        .map_err(|e| anyhow!("failed to get page content: {e}"))?;
                    let result = page
                        .evaluate("document.body.innerText")
                        .await
                        .map_err(|e| anyhow!("failed to evaluate: {e}"))?;
                    let txt = result
                        .into_value()
                        .map(|v: serde_json::Value| v.as_str().unwrap_or("").to_string())
                        .unwrap_or_default();
                    (html, txt)
                };

                Ok(serde_json::json!({
                    "type": "content",
                    "html": content,
                    "text": text,
                }))
            })
        })
    }
}
