use std::net::SocketAddr;
use std::sync::Arc;

use anyhow::Context;
use rustycode_orchestration::config::OrchestrationConfig;
use rustycode_orchestration::pipeline::OrchestrationPipeline;
use rustycode_ws_server::{SessionManager, WsRouter};
use tower_http::services::{ServeDir, ServeFile};
use tracing::info;

pub async fn start_web_server(port: u16) -> anyhow::Result<()> {
    let dist_dir = find_web_dist()?;

    let pipeline = Arc::new(OrchestrationPipeline::new(OrchestrationConfig::default()));
    let session_manager = SessionManager::new(pipeline, "default".to_string(), "default".to_string());
    let ws_router = WsRouter::build(session_manager).await;

    let static_files = ServeDir::new(&dist_dir)
        .fallback(ServeFile::new(dist_dir.join("index.html")))
        .append_index_html_on_directories(true);

    let app = ws_router.fallback_service(static_files);

    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    info!("web server listening on http://{addr}");

    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .with_context(|| format!("failed to bind to port {port}"))?;

    axum::serve(listener, app)
        .await
        .context("web server error")?;

    Ok(())
}

fn find_web_dist() -> anyhow::Result<std::path::PathBuf> {
    // Walk up from CWD to find workspace root (contains Cargo.toml with [workspace])
    let mut candidates = Vec::new();

    if let Ok(workspace_root) = find_workspace_root() {
        candidates.push(workspace_root.join("crates/rustycode-web/dist"));
    }

    candidates.extend([
        // Workspace-relative (cargo run from repo root)
        std::path::PathBuf::from("crates/rustycode-web/dist"),
        // Adjacent crate (running from crates/rustycode-cli)
        std::path::PathBuf::from("../rustycode-web/dist"),
        // Installed alongside binary
        std::env::current_exe()?
            .parent()
            .map(|p| p.join("web-dist"))
            .unwrap_or_default(),
    ]);

    for dir in &candidates {
        if dir.is_dir() {
            info!(path = %dir.display(), "serving web frontend");
            return Ok(dir.clone());
        }
    }

    anyhow::bail!(
        "web frontend not found. Build it first:\n  cd crates/rustycode-web && npm run build"
    )
}

/// Walk up from CWD to find a directory containing a workspace Cargo.toml.
fn find_workspace_root() -> anyhow::Result<std::path::PathBuf> {
    let mut dir = std::env::current_dir()?;
    loop {
        let cargo_toml = dir.join("Cargo.toml");
        if cargo_toml.is_file() {
            if let Ok(content) = std::fs::read_to_string(&cargo_toml) {
                if content.contains("[workspace]") {
                    return Ok(dir);
                }
            }
        }
        dir = dir
            .parent()
            .ok_or_else(|| anyhow::anyhow!("reached filesystem root"))?
            .to_path_buf();
    }
}
