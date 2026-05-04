//! `BrowserFetchTool` — headless Chrome for fetching JS-rendered pages.
//!
//! Uses a lazily-initialized headless `BrowserPool` singleton. The sync `execute()`
//! bridges to async chromiumoxide via `tokio::task::block_in_place`.

use crate::security::validation::validate_url;
use crate::{Tool, ToolContext, ToolOutput, ToolPermission, ToolTag};
use anyhow::anyhow;
use base64::Engine;
use serde_json::{json, Value};
use std::sync::OnceLock;

static HEADLESS_POOL: OnceLock<crate::browser_pool::BrowserPool> = OnceLock::new();

fn pool() -> &'static crate::browser_pool::BrowserPool {
    HEADLESS_POOL.get_or_init(|| crate::browser_pool::BrowserPool::new(true))
}

pub struct BrowserFetchTool;

impl Tool for BrowserFetchTool {
    fn defer_loading(&self) -> Option<bool> {
        Some(true)
    }

    fn name(&self) -> &str {
        "browser_fetch"
    }

    fn description(&self) -> &str {
        "Fetch a URL using headless Chrome (supports JS-rendered pages). Use for SPA content that requires JavaScript."
    }

    fn permission(&self) -> ToolPermission {
        ToolPermission::Network
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["url"],
            "properties": {
                "url": { "type": "string", "description": "URL to fetch" },
                "action": {
                    "type": "string",
                    "enum": ["content", "screenshot"],
                    "description": "'content' returns page as markdown, 'screenshot' returns base64 PNG. Default: content"
                },
                "selector": { "type": "string", "description": "CSS selector to extract specific element" },
                "wait_for": { "type": "string", "description": "CSS selector to wait for before extracting" },
                "timeout_ms": { "type": "number", "description": "Page load timeout in ms (default 30000)" }
            }
        })
    }

    fn tags(&self) -> &[ToolTag] {
        &[ToolTag::Explore]
    }

    fn execute(&self, params: Value, _ctx: &ToolContext) -> anyhow::Result<ToolOutput> {
        let url = params["url"]
            .as_str()
            .ok_or_else(|| anyhow!("'url' parameter required"))?;

        validate_url(url)?;

        let action = params["action"].as_str().unwrap_or("content");
        let selector = params["selector"].as_str().map(String::from);
        let wait_for = params["wait_for"].as_str().map(String::from);
        let timeout_ms = params["timeout_ms"].as_u64().unwrap_or(30_000);
        let pool = pool();

        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async move {
                let page = pool.get_page().await?;

                let timeout = std::time::Duration::from_millis(timeout_ms);
                tokio::time::timeout(timeout, page.goto(url))
                    .await
                    .map_err(|_| anyhow!("page load timed out after {timeout_ms}ms"))?
                    .map_err(|e| anyhow!("navigation failed: {e}"))?;

                if let Some(sel) = &wait_for {
                    tokio::time::timeout(std::time::Duration::from_millis(timeout_ms), async {
                        loop {
                            if page.find_element(sel).await.is_ok() {
                                break;
                            }
                            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
                        }
                    })
                    .await
                    .map_err(|_| anyhow!("timed out waiting for selector '{sel}'"))?;
                } else {
                    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                }

                let result = match action {
                    "screenshot" => {
                        let bytes = page
                            .screenshot(
                                chromiumoxide::page::ScreenshotParams::builder()
                                    .full_page(true)
                                    .build(),
                            )
                            .await
                            .map_err(|e| anyhow!("screenshot failed: {e}"))?;
                        let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
                        Ok(ToolOutput::text(b64))
                    }
                    _ => {
                        let html = if let Some(sel) = &selector {
                            let el = page
                                .find_element(sel)
                                .await
                                .map_err(|e| anyhow!("element '{sel}': {e}"))?;
                            el.inner_html()
                                .await
                                .map_err(|e| anyhow!("inner html: {e}"))?
                        } else {
                            Some(
                                page.content()
                                    .await
                                    .map_err(|e| anyhow!("page content: {e}"))?,
                            )
                        }
                        .unwrap_or_default();

                        let markdown = html2md::parse_html(&html);
                        let output = if markdown.len() > 50_000 {
                            let mut s = markdown;
                            s.truncate(s.floor_char_boundary(50_000));
                            s.push_str("\n\n[... truncated at 50K characters ...]");
                            s
                        } else {
                            markdown
                        };

                        Ok(ToolOutput::text(output))
                    }
                };

                let _ = page.close().await;
                result
            })
        })
    }
}
