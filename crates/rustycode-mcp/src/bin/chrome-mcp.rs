//! Chrome MCP server — visible browser for E2E app testing via MCP protocol.
//!
//! Exposes browser actions as MCP tools over stdio JSON-RPC.
//!
//! Usage:
//!   chrome-mcp                  # headed (visible browser)
//!   chrome-mcp --headless       # headless (CI mode)

use anyhow::{anyhow, Context, Result};
use base64::Engine;
use chromiumoxide::page::ScreenshotParams;
use chromiumoxide::Page;
use clap::Parser;
use rustycode_mcp::types::{McpContent, McpTool, McpToolResult};
use rustycode_mcp::{McpError, McpResult, McpServer};
use rustycode_tools::browser_pool::BrowserPool;
use serde_json::{json, Value};
use std::sync::Arc;
use tokio::runtime::Handle;
use tokio::sync::Mutex;

#[derive(Debug, Parser)]
#[command(name = "chrome-mcp", about = "Chrome browser MCP server for E2E testing")]
struct Cli {
    /// Run in headless mode (no visible browser window)
    #[arg(long, default_value_t = false)]
    headless: bool,

    /// MCP server name
    #[arg(long, default_value = "chrome-mcp")]
    server_name: String,

    /// MCP server version
    #[arg(long, default_value = env!("CARGO_PKG_VERSION"))]
    server_version: String,
}

/// Holds a shared BrowserPool (lifecycle) plus a cached page for reuse.
struct BrowserState {
    pool: BrowserPool,
    page: Mutex<Option<Page>>,
}

impl BrowserState {
    fn new(headless: bool) -> Self {
        Self {
            pool: BrowserPool::new(headless),
            page: Mutex::const_new(None),
        }
    }

    async fn get_page(&self) -> Result<Page> {
        let existing = self.page.lock().await.clone();
        if let Some(page) = existing {
            return Ok(page);
        }
        let page = self.pool.get_page().await?;
        *self.page.lock().await = Some(page.clone());
        Ok(page)
    }
}

fn run_async<F, T>(fut: F) -> Result<T>
where
    F: std::future::Future<Output = Result<T>>,
{
    tokio::task::block_in_place(|| Handle::current().block_on(fut))
}

fn to_mcp(result: Result<McpToolResult>) -> McpResult<McpToolResult> {
    result.map_err(|e| McpError::InternalError(e.to_string()))
}

fn text_result(text: String) -> McpToolResult {
    McpToolResult {
        content: vec![McpContent::Text { text }],
        structured_content: None,
        is_error: None,
        meta: None,
    }
}

#[tokio::main]
#[allow(clippy::too_many_lines)]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter("info,chrome_mcp=debug")
        .init();

    let cli = Cli::parse();
    let headless = cli.headless;

    let config = rustycode_mcp::McpServerConfig {
        server_name: cli.server_name,
        server_version: cli.server_version,
        enable_tools: true,
        enable_resources: false,
        enable_prompts: false,
        timeout_secs: 60,
    };

    let mut server = McpServer::new("chrome-mcp", config);
    let state = Arc::new(BrowserState::new(headless));

    // browser_navigate
    let s = state.clone();
    server
        .register_tool(
            navigate_tool(),
            move |params: Value| -> McpResult<McpToolResult> {
                let url = params["url"]
                    .as_str()
                    .ok_or_else(|| McpError::InvalidRequest("'url' parameter required".into()))?;
                let s = s.clone();
                to_mcp(run_async(async move {
                    let page = s.get_page().await?;
                    page.goto(url)
                        .await
                        .map_err(|e| anyhow!("navigation failed: {e}"))?;
                    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
                    let title = page.get_title().await.ok().flatten().unwrap_or_default();
                    let final_url = page.url().await.ok().flatten().unwrap_or_default();
                    Ok(text_result(format!("Navigated to {final_url}\nTitle: {title}")))
                }))
            },
        )
        .await;

    // browser_screenshot
    let s = state.clone();
    server
        .register_tool(
            screenshot_tool(),
            move |_params: Value| -> McpResult<McpToolResult> {
                let s = s.clone();
                to_mcp(run_async(async move {
                    let page = s.get_page().await?;
                    let bytes = page
                        .screenshot(ScreenshotParams::builder().full_page(true).build())
                        .await
                        .map_err(|e| anyhow!("screenshot failed: {e}"))?;
                    let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
                    Ok(McpToolResult {
                        content: vec![McpContent::Image {
                            data: b64,
                            mime_type: "image/png".into(),
                        }],
                        structured_content: None,
                        is_error: None,
                        meta: None,
                    })
                }))
            },
        )
        .await;

    // browser_click
    let s = state.clone();
    server
        .register_tool(
            click_tool(),
            move |params: Value| -> McpResult<McpToolResult> {
                let selector = params["selector"]
                    .as_str()
                    .ok_or_else(|| McpError::InvalidRequest("'selector' required".into()))?
                    .to_string();
                let s = s.clone();
                to_mcp(run_async(async move {
                    let page = s.get_page().await?;
                    let el = page
                        .find_element(&selector)
                        .await
                        .map_err(|e| anyhow!("{e}"))?;
                    el.click().await.map_err(|e| anyhow!("{e}"))?;
                    Ok(text_result(format!("Clicked: {selector}")))
                }))
            },
        )
        .await;

    // browser_type
    let s = state.clone();
    server
        .register_tool(
            type_tool(),
            move |params: Value| -> McpResult<McpToolResult> {
                let selector = params["selector"]
                    .as_str()
                    .ok_or_else(|| McpError::InvalidRequest("'selector' required".into()))?
                    .to_string();
                let text = params["text"]
                    .as_str()
                    .ok_or_else(|| McpError::InvalidRequest("'text' required".into()))?
                    .to_string();
                let s = s.clone();
                to_mcp(run_async(async move {
                    let page = s.get_page().await?;
                    let el = page
                        .find_element(&selector)
                        .await
                        .map_err(|e| anyhow!("{e}"))?;
                    el.type_str(&text).await.map_err(|e| anyhow!("{e}"))?;
                    Ok(text_result(format!("Typed '{text}' into {selector}")))
                }))
            },
        )
        .await;

    // browser_evaluate
    let s = state.clone();
    server
        .register_tool(
            evaluate_tool(),
            move |params: Value| -> McpResult<McpToolResult> {
                let script = params["script"]
                    .as_str()
                    .ok_or_else(|| McpError::InvalidRequest("'script' required".into()))?
                    .to_string();
                let s = s.clone();
                to_mcp(run_async(async move {
                    let page = s.get_page().await?;
                    let result = page
                        .evaluate(script.as_str())
                        .await
                        .map_err(|e| anyhow!("{e}"))?;
                    #[allow(clippy::option_if_let_else)]
                    let output = match result.into_value::<Value>() {
                        Ok(v) => v.to_string(),
                        Err(_) => "undefined".into(),
                    };
                    Ok(text_result(output))
                }))
            },
        )
        .await;

    // browser_get_content
    let s = state.clone();
    server
        .register_tool(
            get_content_tool(),
            move |params: Value| -> McpResult<McpToolResult> {
                let selector = params["selector"].as_str().map(String::from);
                let s = s.clone();
                to_mcp(run_async(async move {
                    let page = s.get_page().await?;
                    let html = if let Some(sel) = &selector {
                        let el = page
                            .find_element(sel)
                            .await
                            .map_err(|e| anyhow!("{e}"))?;
                        el.inner_html().await.map_err(|e| anyhow!("{e}"))?
                    } else {
                        Some(
                            page.content()
                                .await
                                .map_err(|e| anyhow!("{e}"))?,
                        )
                    }
                    .unwrap_or_default();
                    Ok(text_result(html))
                }))
            },
        )
        .await;

    // browser_get_text
    let s = state.clone();
    server
        .register_tool(
            get_text_tool(),
            move |params: Value| -> McpResult<McpToolResult> {
                let selector = params["selector"].as_str().map(String::from);
                let s = s.clone();
                to_mcp(run_async(async move {
                    let page = s.get_page().await?;
                    let text = if let Some(sel) = &selector {
                        let el = page
                            .find_element(sel)
                            .await
                            .map_err(|e| anyhow!("{e}"))?;
                        el.inner_text().await.map_err(|e| anyhow!("{e}"))?
                    } else {
                        let result = page
                            .evaluate("document.body.innerText")
                            .await
                            .map_err(|e| anyhow!("{e}"))?;
                        #[allow(clippy::option_if_let_else)]
                        let val = match result.into_value::<Value>() {
                            Ok(v) => v.as_str().unwrap_or("").to_string(),
                            Err(_) => String::new(),
                        };
                        Some(val)
                    }
                    .unwrap_or_default();
                    Ok(text_result(text))
                }))
            },
        )
        .await;

    server.run_stdio().await?;
    Ok(())
}

fn navigate_tool() -> McpTool {
    McpTool {
        name: "browser_navigate".into(),
        title: Some("Navigate to URL".into()),
        description: "Navigate the browser to a URL.".into(),
        input_schema: json!({"type":"object","required":["url"],"properties":{"url":{"type":"string","description":"URL to navigate to"}}}),
        output_schema: None,
        annotations: None,
        category: Some("browser".into()),
    }
}

fn screenshot_tool() -> McpTool {
    McpTool {
        name: "browser_screenshot".into(),
        title: Some("Take screenshot".into()),
        description: "Capture a full-page screenshot as base64 PNG.".into(),
        input_schema: json!({"type":"object","properties":{}}),
        output_schema: None,
        annotations: None,
        category: Some("browser".into()),
    }
}

fn click_tool() -> McpTool {
    McpTool {
        name: "browser_click".into(),
        title: Some("Click element".into()),
        description: "Click an element by CSS selector.".into(),
        input_schema: json!({"type":"object","required":["selector"],"properties":{"selector":{"type":"string","description":"CSS selector"}}}),
        output_schema: None,
        annotations: None,
        category: Some("browser".into()),
    }
}

fn type_tool() -> McpTool {
    McpTool {
        name: "browser_type".into(),
        title: Some("Type text".into()),
        description: "Type text into an element.".into(),
        input_schema: json!({"type":"object","required":["selector","text"],"properties":{"selector":{"type":"string"},"text":{"type":"string"}}}),
        output_schema: None,
        annotations: None,
        category: Some("browser".into()),
    }
}

fn evaluate_tool() -> McpTool {
    McpTool {
        name: "browser_evaluate".into(),
        title: Some("Execute JavaScript".into()),
        description: "Execute JavaScript and return the result.".into(),
        input_schema: json!({"type":"object","required":["script"],"properties":{"script":{"type":"string"}}}),
        output_schema: None,
        annotations: None,
        category: Some("browser".into()),
    }
}

fn get_content_tool() -> McpTool {
    McpTool {
        name: "browser_get_content".into(),
        title: Some("Get page HTML".into()),
        description: "Get HTML of the page or an element.".into(),
        input_schema: json!({"type":"object","properties":{"selector":{"type":"string"}}}),
        output_schema: None,
        annotations: None,
        category: Some("browser".into()),
    }
}

fn get_text_tool() -> McpTool {
    McpTool {
        name: "browser_get_text".into(),
        title: Some("Get page text".into()),
        description: "Get text content of the page or an element.".into(),
        input_schema: json!({"type":"object","properties":{"selector":{"type":"string"}}}),
        output_schema: None,
        annotations: None,
        category: Some("browser".into()),
    }
}
