//! Chrome MCP server — visible browser for E2E app testing via MCP protocol.
//!
//! Exposes browser actions as MCP tools over stdio JSON-RPC.
//!
//! Usage:
//!   chrome-mcp                  # headed (visible browser)
//!   chrome-mcp --headless       # headless (CI mode)

use anyhow::{anyhow, Context, Result};
use base64::Engine;
use chromiumoxide::browser::{Browser, BrowserConfig};
use chromiumoxide::page::ScreenshotParams;
use chromiumoxide::Page;
use clap::Parser;
use futures::StreamExt;
use rustycode_mcp::types::{McpContent, McpTool, McpToolResult};
use rustycode_mcp::{McpError, McpResult, McpServer};
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};
use tokio::runtime::Handle;
use tokio::task::JoinHandle;

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

struct BrowserInstance {
    browser: Browser,
    handler: JoinHandle<()>,
}

struct BrowserState {
    inner: Mutex<Option<BrowserInstance>>,
    page: Mutex<Option<Page>>,
    headless: bool,
}

impl BrowserState {
    fn new(headless: bool) -> Self {
        Self {
            inner: Mutex::new(None),
            page: Mutex::new(None),
            headless,
        }
    }

    fn get_page(&self) -> Result<Page> {
        // Try to reuse existing page
        if let Some(page) = self.page.lock().expect("lock").clone() {
            return Ok(page);
        }

        // Launch browser if needed
        {
            let mut inner = self.inner.lock().expect("lock");
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
                    .map_err(|e| anyhow!("config error: {e}"))?;

                let (browser, mut handler) = Browser::launch(config)
                    .await_context("failed to launch Chrome")?;

                let handler_handle = tokio::spawn(async move {
                    while let Some(event) = handler.next().await {
                        if event.is_err() {
                            break;
                        }
                    }
                });

                *inner = Some(BrowserInstance {
                    browser,
                    handler: handler_handle,
                });
            }
        }

        // Create new page
        let inner = self.inner.lock().expect("lock");
        let instance = inner.as_ref().context("no browser instance")?;
        let page = instance
            .browser
            .new_page("about:blank")
            .await_context("failed to create page")?;

        *self.page.lock().expect("lock") = Some(page.clone());
        Ok(page)
    }
}

/// Helper trait to convert anyhow errors in sync context
trait AwaitContext<T> {
    fn await_context(self, msg: &str) -> Result<T>;
}

impl<F: std::future::Future<Output = Result<T>>, T> AwaitContext<T> for F {
    fn await_context(self, msg: &str) -> Result<T> {
        // This doesn't actually await - it's a type alias for documentation
        // Real awaiting happens in block_on below
        unimplemented!("use run_async instead")
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

fn err_result(msg: String) -> McpToolResult {
    McpToolResult {
        content: vec![McpContent::Text { text: format!("Error: {msg}") }],
        structured_content: None,
        is_error: Some(true),
        meta: None,
    }
}

#[tokio::main]
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
                    let page = s.get_page()?;
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
                    let page = s.get_page()?;
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
                    let page = s.get_page()?;
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
                    let page = s.get_page()?;
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
                    let page = s.get_page()?;
                    let result = page
                        .evaluate(&script)
                        .await
                        .map_err(|e| anyhow!("{e}"))?;
                    let output = result
                        .into_value()
                        .map(|v| v.to_string())
                        .unwrap_or_else(|_| "undefined".into());
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
                    let page = s.get_page()?;
                    let html = if let Some(sel) = &selector {
                        let el = page
                            .find_element(sel)
                            .await
                            .map_err(|e| anyhow!("{e}"))?;
                        el.inner_html().await.map_err(|e| anyhow!("{e}"))?
                    } else {
                        page.content().await.map_err(|e| anyhow!("{e}"))?
                    };
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
                    let page = s.get_page()?;
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
                        result
                            .into_value()
                            .map(|v| v.as_str().unwrap_or("").to_string())
                            .unwrap_or_default()
                    };
                    Ok(text_result(text))
                }))
            },
        )
        .await;

    server.run_stdio().await?;
    Ok(())
}

// Tool definitions
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
