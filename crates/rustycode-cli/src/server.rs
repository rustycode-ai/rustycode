#![allow(clippy::doc_markdown)]

//! Web server implementation for RustyCode

use anyhow::Result;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::{
    extract::State,
    response::{Html, IntoResponse},
    routing::{get, post},
    Json, Router,
};
use futures_util::{SinkExt, StreamExt};
use rustycode_protocol::ToolCall;
use rustycode_tools::ToolDispatcher;
use serde_json::Value;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::Mutex;
use tower_http::services::ServeDir;

#[derive(Clone)]
pub struct AppState {
    pub executor: Arc<Mutex<ToolDispatcher>>,
    pub cache: Arc<Mutex<HashMap<String, Value>>>,
}

pub async fn serve_web(port: u16, dir: Option<String>) -> Result<()> {
    let cwd = std::env::current_dir()?;
    let executor = Arc::new(Mutex::new(ToolDispatcher::from_cwd(cwd)));
    let cache: Arc<Mutex<HashMap<String, Value>>> = Arc::new(Mutex::new(HashMap::new()));
    let state = AppState {
        executor: executor.clone(),
        cache: cache.clone(),
    };

    // Determine static file directory
    let www_dir = dir.unwrap_or_else(|| "crates/ratzilla-wasm/www".to_string());

    let app = Router::new()
        .route("/call", post(handle_call))
        .route("/ws", get(ws_handler))
        .route("/api/health", get(health_check))
        .route("/", get(get_index))
        .nest_service("/www", ServeDir::new(&www_dir))
        .with_state(state);

    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    tracing::info!("🚀 RustyCode web server starting on http://{}", addr);
    tracing::info!("📁 Serving static files from: {}", www_dir);
    tracing::info!("🔧 API endpoints:");
    tracing::info!("   POST /call    - Execute tool calls");
    tracing::info!("   GET  /ws      - WebSocket for streaming tool output");
    tracing::info!("   GET  /        - Web UI");

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

async fn health_check() -> Json<Value> {
    Json(serde_json::json!({"status": "ok"}))
}

async fn handle_call(State(state): State<AppState>, Json(payload): Json<Value>) -> Json<Value> {
    let call: ToolCall = match serde_json::from_value(payload) {
        Ok(c) => c,
        Err(e) => return Json(serde_json::json!({"error": e.to_string()})),
    };

    let result = {
        let exec = state.executor.lock().await;
        exec.execute(&call)
    };

    state.cache.lock().await.insert(
        call.call_id.clone(),
        serde_json::to_value(&result).unwrap_or_else(|_| serde_json::json!({})),
    );

    Json(
        serde_json::to_value(result)
            .unwrap_or_else(|_| serde_json::json!({"error": "serialization failed"})),
    )
}

async fn ws_handler(State(state): State<AppState>, ws: WebSocketUpgrade) -> impl IntoResponse {
    ws.on_upgrade(|socket| handle_socket(socket, state))
}

async fn handle_socket(socket: WebSocket, state: AppState) {
    let (mut sender, mut receiver) = socket.split();

    while let Some(Ok(msg)) = receiver.next().await {
        if let Message::Text(txt) = msg {
            let call_res: Result<ToolCall, _> = serde_json::from_str(&txt);
            if let Ok(call) = call_res {
                let call_id = call.call_id.clone();
                let name = call.name.clone();

                // Send tool_started event
                let started = serde_json::json!({
                    "type": "tool_started",
                    "call_id": call_id,
                    "name": name,
                });
                if let Err(e) = sender.send(Message::Text(started.to_string().into())).await {
                    tracing::debug!("Failed to send tool_started via WebSocket: {}", e);
                }

                // Execute tool
                let result = {
                    let exec = state.executor.lock().await;
                    exec.execute(&call)
                };

                // Cache result
                let _ = state.cache.lock().await.insert(
                    call_id.clone(),
                    serde_json::to_value(&result).unwrap_or_else(|_| serde_json::json!({})),
                );

                // Send completion
                let completed = serde_json::json!({
                    "type": "tool_completed",
                    "call_id": call_id,
                    "result": result,
                });
                if let Err(e) = sender
                    .send(Message::Text(completed.to_string().into()))
                    .await
                {
                    tracing::debug!("Failed to send tool_completed via WebSocket: {}", e);
                }
            }
        }
    }
}

async fn get_index() -> Html<&'static str> {
    Html(
        r#"<!DOCTYPE html>
<html>
<head>
    <title>RustyCode Web</title>
    <meta charset="utf-8">
    <style>
        body { font-family: system-ui, sans-serif; margin: 2rem; background: #1a1a2e; color: #eee; }
        h1 { color: #0ff; }
        a { color: #0af; }
        code { background: #333; padding: 0.2em 0.4em; border-radius: 3px; }
        .endpoint { margin: 1em 0; padding: 1em; background: #16213e; border-radius: 8px; }
    </style>
</head>
<body>
    <h1>🦀 RustyCode Web Server</h1>
    <p>Welcome to RustyCode! The web UI is available at <a href="/www/index.html">/www/index.html</a></p>
    
    <h2>API Endpoints</h2>
    <div class="endpoint">
        <h3><code>POST /call</code></h3>
        <p>Execute a tool call. Send JSON with <code>call_id</code>, <code>name</code>, and <code>arguments</code>.</p>
    </div>
    <div class="endpoint">
        <h3><code>GET /ws</code></h3>
        <p>WebSocket endpoint for streaming tool execution with real-time updates.</p>
    </div>
    <div class="endpoint">
        <h3><code>GET /api/health</code></h3>
        <p>Health check endpoint.</p>
    </div>
</body>
</html>"#,
    )
}
