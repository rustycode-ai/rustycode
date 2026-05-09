#![cfg_attr(test, allow(clippy::unwrap_used))]
#![allow(
    clippy::expect_used,
    clippy::too_many_lines,
    clippy::option_if_let_else,
    clippy::semicolon_if_nothing_returned,
    clippy::uninlined_format_args
)]

use axum::extract::ws::Utf8Bytes;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::{
    extract::State,
    response::Html,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use futures_util::{SinkExt, StreamExt};
use rustycode_protocol::ToolCall;
use rustycode_tools::ToolExecutor;
use serde_json::Value;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;
use tower_http::services::ServeDir;

#[derive(Clone)]
struct AppState {
    executor: Arc<Mutex<ToolExecutor>>,
    cache: Arc<Mutex<HashMap<String, Value>>>,
}

/// Tool server entry point with optimized tokio runtime configuration.
///
/// The runtime uses all available CPU cores for maximum throughput when
/// executing tools concurrently.
fn main() {
    // Build runtime with CPU-based worker thread count
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(num_cpus::get())
        .thread_name_fn(|| {
            static ATOMIC_ID: std::sync::atomic::AtomicUsize =
                std::sync::atomic::AtomicUsize::new(0);
            let id = ATOMIC_ID.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            format!("tool-server-worker-{}", id)
        })
        .enable_all()
        .build()
        .expect("failed to build tokio runtime");

    rt.block_on(async_main())
}

async fn async_main() {
    tracing_subscriber::fmt::init();

    // Shared executor (workspace root)
    let executor = Arc::new(Mutex::new(
        ToolExecutor::from_cwd(PathBuf::from(".")).with_structured_output(true),
    ));
    let cache: Arc<Mutex<HashMap<String, Value>>> = Arc::new(Mutex::new(HashMap::new()));
    let state = AppState {
        executor: executor.clone(),
        cache: cache.clone(),
    };

    let app = Router::new()
        .route("/call", post(handle_call))
        .route("/ws", get(ws_handler))
        .route("/cache/{call_id}", get(get_cached))
        .route("/", get(get_index))
        .nest_service("/www", ServeDir::new("crates/ratzilla-wasm/www"))
        .with_state(state.clone());

    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));
    tracing::info!("listening on {}", addr);
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .unwrap_or_else(|e| {
            eprintln!("error: failed to bind tool server to {addr}: {e}");
            std::process::exit(1);
        });
    axum::serve(listener, app).await.unwrap_or_else(|e| {
        eprintln!("error: tool server failed: {e}");
        std::process::exit(1);
    });
}

async fn handle_call(State(state): State<AppState>, Json(payload): Json<Value>) -> Json<Value> {
    // Simple passthrough example: accept ToolCall JSON and run via executor
    let call: ToolCall = match serde_json::from_value(payload) {
        Ok(c) => c,
        Err(e) => return Json(serde_json::json!({"error": e.to_string()})),
    };

    // Execute synchronously using the executor
    let result = {
        let exec = state.executor.lock().await;
        exec.execute(&call)
    };

    // Cache full result for lookup via /cache/:call_id
    let _ = state.cache.lock().await.insert(
        call.call_id.clone(),
        serde_json::to_value(&result).unwrap_or_else(|_| serde_json::json!({})),
    );

    Json(
        serde_json::to_value(result)
            .unwrap_or_else(|_| serde_json::json!({"error":"serialization failed"})),
    )
}

async fn ws_handler(State(state): State<AppState>, ws: WebSocketUpgrade) -> impl IntoResponse {
    ws.on_upgrade(|socket| handle_socket(socket, state))
}

async fn handle_socket(socket: WebSocket, state: AppState) {
    // Split socket into sender/receiver so we can spawn tasks that send messages
    let (sender, mut receiver) = socket.split();
    let sender = Arc::new(Mutex::new(sender));

    while let Some(Ok(msg)) = receiver.next().await {
        match msg {
            Message::Text(txt) => {
                let call_res: Result<ToolCall, _> = serde_json::from_str(&txt);
                match call_res {
                    Ok(call) => {
                        let call_id = call.call_id.clone();
                        let name = call.name.clone();

                        // Send tool_started event immediately
                        let started = serde_json::json!({
                            "type": "tool_started",
                            "call_id": call_id,
                            "name": name,
                        });
                        let _ = sender
                            .lock()
                            .await
                            .send(Message::Text(Utf8Bytes::from(started.to_string())))
                            .await;

                        // Spawn a task to execute the tool and stream back chunks
                        let sender_clone = sender.clone();
                        let exec_clone = state.executor.clone();
                        let cache_clone = state.cache.clone();
                        tokio::spawn(async move {
                            // Execute using executor (acquire lock asynchronously)
                            let result = {
                                let exec = exec_clone.lock().await;
                                exec.execute(&call)
                            };

                            // Stream output in small chunks (by line, limited size)
                            let out = rustycode_tools::truncation::truncate_bash_output(
                                &result.output,
                                &result.error.clone().unwrap_or_default(),
                                result.exit_code.unwrap_or(-1),
                            );
                            let out_str = out.as_str();
                            let max_chunk = 1024usize;

                            if !out_str.is_empty() {
                                let mut buf = String::with_capacity(max_chunk);
                                for line in out_str.lines() {
                                    if !buf.is_empty() {
                                        buf.push('\n');
                                    }
                                    buf.push_str(line);
                                    if buf.len() >= max_chunk {
                                        let chunk = serde_json::json!({
                                            "type": "tool_chunk",
                                            "call_id": result.call_id,
                                            "chunk_type": "stdout",
                                            "text": buf,
                                        });
                                        let _ = sender_clone
                                            .lock()
                                            .await
                                            .send(Message::Text(chunk.to_string().into()))
                                            .await;
                                        buf.clear();
                                    }
                                }
                                if !buf.is_empty() {
                                    let chunk = serde_json::json!({
                                        "type": "tool_chunk",
                                        "call_id": result.call_id,
                                        "chunk_type": "stdout",
                                        "text": buf,
                                    });
                                    let _ = sender_clone
                                        .lock()
                                        .await
                                        .send(Message::Text(chunk.to_string().into()))
                                        .await;
                                }
                            }

                            // If structured transformed output exists, emit as transform_chunk
                            if let Some(data) = &result.data {
                                if data.get("transformed").is_some() {
                                    let tchunk = serde_json::json!({
                                        "type": "tool_chunk",
                                        "call_id": result.call_id,
                                        "chunk_type": "transform_chunk",
                                        "structured": data,
                                    });
                                    let _ = sender_clone
                                        .lock()
                                        .await
                                        .send(Message::Text(tchunk.to_string().into()))
                                        .await;
                                }
                            }

                            // Store full result in server cache with robust serialization error handling
                            if let Ok(serialized) = serde_json::to_value(&result) {
                                let _ = cache_clone
                                    .lock()
                                    .await
                                    .insert(result.call_id.clone(), serialized);
                            }

                            // Final tool_done event with full ToolResult
                            let done = serde_json::json!({
                                "type": "tool_done",
                                "call_id": result.call_id,
                                "result": result,
                            });
                            let _ = sender_clone
                                .lock()
                                .await
                                .send(Message::Text(done.to_string().into()))
                                .await;
                        });
                    }
                    Err(e) => {
                        let _ = sender
                            .lock()
                            .await
                            .send(Message::Text(
                                serde_json::json!({"error": e.to_string()})
                                    .to_string()
                                    .into(),
                            ))
                            .await;
                    }
                }
            }
            Message::Binary(_) => {
                let _ = sender
                    .lock()
                    .await
                    .send(Message::Text(
                        serde_json::json!({"error":"binary not supported"})
                            .to_string()
                            .into(),
                    ))
                    .await;
            }
            Message::Close(_) => break,
            Message::Ping(p) => {
                let _ = sender.lock().await.send(Message::Pong(p)).await;
            }
            Message::Pong(_) => {}
        }
    }
}

async fn get_cached(
    State(state): State<AppState>,
    axum::extract::Path(call_id): axum::extract::Path<String>,
) -> impl IntoResponse {
    let cache = state.cache.lock().await;
    if let Some(v) = cache.get(&call_id) {
        (axum::http::StatusCode::OK, Json(v.clone()))
    } else {
        (
            axum::http::StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error":"not found"})),
        )
    }
}

async fn get_index() -> impl IntoResponse {
    match std::fs::read_to_string("crates/ratzilla-wasm/www/index.html") {
        Ok(s) => Html(s).into_response(),
        Err(_) => (axum::http::StatusCode::NOT_FOUND, "index not found").into_response(),
    }
}
