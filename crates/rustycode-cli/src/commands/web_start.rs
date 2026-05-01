use std::net::SocketAddr;
use std::sync::Arc;

use anyhow::Context;
use rustycode_llm::{create_provider_with_config, load_provider_config_from_env};
use rustycode_orchestration::config::OrchestrationConfig;
use rustycode_orchestration::pipeline::OrchestrationPipeline;
use rustycode_ws_server::{SessionManager, WsRouter};
use tower_http::services::{ServeDir, ServeFile};
use tracing::info;

pub async fn start_web_server(port: u16) -> anyhow::Result<()> {
    let dist_dir = find_web_dist()?;

    let (provider_type, model_name, v2_config) =
        load_provider_config_from_env().context("Failed to load LLM provider config")?;
    let provider = create_provider_with_config(&provider_type, &model_name, v2_config)
        .context("Failed to create LLM provider")?;

    let config = OrchestrationConfig::default();
    let pipeline =
        OrchestrationPipeline::with_provider_and_model(config, provider, &model_name);
    let pipeline = Arc::new(pipeline);

    let session_manager = SessionManager::new(
        pipeline,
        provider_type.to_string(),
        model_name.clone(),
    );
    let ws_router = WsRouter::build(session_manager);

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
    let candidates = [
        // Workspace-relative (cargo run from repo root)
        std::path::PathBuf::from("crates/rustycode-web/dist"),
        // Adjacent crate (running from crates/rustycode-cli)
        std::path::PathBuf::from("../rustycode-web/dist"),
        // Installed alongside binary
        std::env::current_exe()?
            .parent()
            .map(|p| p.join("web-dist"))
            .unwrap_or_default(),
    ];

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
