//! `BrowserFetchTool` — headless Chrome for fetching JS-rendered pages.
//!
//! Uses a lazily-initialized headless `BrowserPool` singleton. The sync `execute()`
//! bridges to async chromiumoxide via `tokio::task::block_in_place`.

use crate::security::validation::validate_url;
use crate::{ToolOutput, ToolPermission, ToolTag};
use anyhow::anyhow;
use base64::Engine;
use schemars::JsonSchema;
use serde::Deserialize;
use std::sync::OnceLock;

/// Parameters for the browser fetch tool.
#[derive(Deserialize, JsonSchema)]
pub struct BrowserFetchParams {
    /// URL to fetch
    url: String,
    /// 'content' returns page as markdown, 'screenshot' returns base64 PNG. Default: content
    action: Option<String>,
    /// CSS selector to extract specific element
    selector: Option<String>,
    /// CSS selector to wait for before extracting
    wait_for: Option<String>,
    /// Page load timeout in ms (default 30000)
    timeout_ms: Option<u64>,
}

static HEADLESS_POOL: OnceLock<crate::browser_pool::BrowserPool> = OnceLock::new();

fn pool() -> &'static crate::browser_pool::BrowserPool {
    HEADLESS_POOL.get_or_init(|| crate::browser_pool::BrowserPool::new(true))
}

rustycode_tools_api::define_tool! {
    pub struct BrowserFetchTool;

    name: "BrowserFetch",
    namespace: "web",
    description: "Fetch a URL using headless Chrome (supports JS-rendered pages). Use for SPA content that requires JavaScript.",
    permission: ToolPermission::Network,
    tags: [ToolTag::Explore],
    defer_loading: true,

    execute(params: BrowserFetchParams, _ctx) {
        let url = &params.url;

        validate_url(url)?;

        let action = params.action.as_deref().unwrap_or("content");
        let selector = params.selector;
        let wait_for = params.wait_for;
        let timeout_ms = params.timeout_ms.unwrap_or(30_000);
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
