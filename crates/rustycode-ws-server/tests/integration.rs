#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::unreadable_literal,
    clippy::needless_pass_by_value
)]

use std::sync::Arc;

use axum::Router;
use futures_util::{SinkExt, StreamExt};
use http_body_util::BodyExt;
use rustycode_ws_server::{Envelope, SessionManager, WsRouter};
use serde_json::json;
use tokio_tungstenite::tungstenite::Message;
use tower::ServiceExt;

fn test_session_manager() -> SessionManager {
    let config = rustycode_orchestration::config::OrchestrationConfig::default();
    let pipeline = Arc::new(rustycode_orchestration::pipeline::OrchestrationPipeline::new(config));
    SessionManager::new(pipeline, "test".to_string(), "test-model".to_string())
}

async fn create_test_app(session_manager: Arc<SessionManager>) -> Router {
    WsRouter::build((*session_manager).clone()).await
}

async fn ws_connect_with(
    shared: Option<Arc<SessionManager>>,
) -> (
    futures_util::stream::SplitSink<
        tokio_tungstenite::WebSocketStream<
            tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
        >,
        Message,
    >,
    futures_util::stream::SplitStream<
        tokio_tungstenite::WebSocketStream<
            tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
        >,
    >,
    Arc<SessionManager>,
) {
    let mgr = shared.unwrap_or_else(|| Arc::new(test_session_manager()));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let app = create_test_app(mgr.clone()).await;

    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    let url = format!("ws://127.0.0.1:{}/ws", addr.port());
    let (stream, _) = tokio_tungstenite::connect_async(&url).await.unwrap();
    let (sink, recv) = stream.split();
    (sink, recv, mgr)
}

async fn ws_connect() -> (
    futures_util::stream::SplitSink<
        tokio_tungstenite::WebSocketStream<
            tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
        >,
        Message,
    >,
    futures_util::stream::SplitStream<
        tokio_tungstenite::WebSocketStream<
            tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
        >,
    >,
) {
    let (sink, recv, _) = ws_connect_with(None).await;
    (sink, recv)
}

fn make_envelope(msg_type: &str, id: &str, payload: serde_json::Value) -> Message {
    let envelope = json!({
        "v": 2,
        "type": msg_type,
        "id": id,
        "payload": payload,
    });
    Message::Text(serde_json::to_string(&envelope).unwrap().into())
}

async fn recv_envelope(
    recv: &mut futures_util::stream::SplitStream<
        tokio_tungstenite::WebSocketStream<
            tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
        >,
    >,
) -> Envelope {
    let msg = recv
        .next()
        .await
        .expect("expected message")
        .expect("expected ok");
    match msg {
        Message::Text(text) => Envelope::decode(&text).unwrap(),
        other => panic!("expected text message, got: {other:?}"),
    }
}

#[tokio::test]
async fn hello_creates_new_session() {
    let (mut sink, mut recv) = ws_connect().await;

    sink.send(make_envelope("hello", "1", json!({})))
        .await
        .unwrap();

    let envelope = recv_envelope(&mut recv).await;
    assert_eq!(envelope.r#type, "session_created");
    assert_eq!(envelope.v, 2);

    let payload = envelope.payload;
    let token = payload["session_token"].as_str().unwrap();
    assert!(token.starts_with("sess_"));
}

#[tokio::test]
async fn hello_with_token_resumes_session() {
    let shared_mgr = Arc::new(test_session_manager());

    // First connection: create session
    let (mut sink, mut recv, _) = ws_connect_with(Some(shared_mgr.clone())).await;
    sink.send(make_envelope("hello", "1", json!({})))
        .await
        .unwrap();
    let created = recv_envelope(&mut recv).await;
    let token = created.payload["session_token"]
        .as_str()
        .unwrap()
        .to_string();

    // Receive state_snapshot
    let snapshot = recv_envelope(&mut recv).await;
    assert_eq!(snapshot.r#type, "state_snapshot");

    // Second connection: resume with token using same session manager
    let (mut sink2, mut recv2, _) = ws_connect_with(Some(shared_mgr)).await;
    sink2
        .send(make_envelope("hello", "2", json!({"session_token": token})))
        .await
        .unwrap();

    let resumed = recv_envelope(&mut recv2).await;
    assert_eq!(resumed.r#type, "session_resumed");
}

#[tokio::test]
async fn heartbeat_roundtrip() {
    let (mut sink, mut recv) = ws_connect().await;

    // Hello first
    sink.send(make_envelope("hello", "1", json!({})))
        .await
        .unwrap();
    let _created = recv_envelope(&mut recv).await;
    let _snapshot = recv_envelope(&mut recv).await;

    // Send heartbeat
    let ts = 1234567890i64;
    sink.send(make_envelope("heartbeat", "2", json!({"ts": ts})))
        .await
        .unwrap();

    let ack = recv_envelope(&mut recv).await;
    assert_eq!(ack.r#type, "heartbeat_ack");
    assert_eq!(ack.payload["ts"], ts);
    assert!(ack.payload["server_ts"].as_i64().unwrap() > 0);
}

#[tokio::test]
async fn state_snapshot_contains_empty_session() {
    let (mut sink, mut recv) = ws_connect().await;

    sink.send(make_envelope("hello", "1", json!({})))
        .await
        .unwrap();
    let _created = recv_envelope(&mut recv).await;

    let snapshot = recv_envelope(&mut recv).await;
    assert_eq!(snapshot.r#type, "state_snapshot");

    let payload = snapshot.payload;
    assert!(payload["messages"].as_array().unwrap().is_empty());
    assert_eq!(payload["input"].as_str().unwrap(), "");
    assert!(!payload["pending_request"].as_bool().unwrap());
}

#[tokio::test]
async fn duplicate_hello_returns_error() {
    let (mut sink, mut recv) = ws_connect().await;

    sink.send(make_envelope("hello", "1", json!({})))
        .await
        .unwrap();
    let _created = recv_envelope(&mut recv).await;
    let _snapshot = recv_envelope(&mut recv).await;

    // Send another hello
    sink.send(make_envelope("hello", "2", json!({})))
        .await
        .unwrap();

    let err = recv_envelope(&mut recv).await;
    assert_eq!(err.r#type, "error");
    assert_eq!(err.payload["code"], "invalid_message");
}

#[tokio::test]
async fn unknown_message_type_returns_error() {
    let (mut sink, mut recv) = ws_connect().await;

    sink.send(make_envelope("hello", "1", json!({})))
        .await
        .unwrap();
    let _created = recv_envelope(&mut recv).await;
    let _snapshot = recv_envelope(&mut recv).await;

    // Send unknown type
    sink.send(make_envelope("unknown_type", "2", json!({})))
        .await
        .unwrap();

    let err = recv_envelope(&mut recv).await;
    assert_eq!(err.r#type, "error");
}

#[tokio::test]
async fn abort_cancels_session_task() {
    let shared_mgr = Arc::new(test_session_manager());

    // Connect and create session
    let (mut sink, mut recv, mgr) = ws_connect_with(Some(shared_mgr)).await;
    sink.send(make_envelope("hello", "1", json!({})))
        .await
        .unwrap();
    let created = recv_envelope(&mut recv).await;
    let token = created.payload["session_token"]
        .as_str()
        .unwrap()
        .to_string();

    // Receive state_snapshot
    let _snapshot = recv_envelope(&mut recv).await;

    // Abort via session manager (same path the router uses)
    mgr.abort(&token).await.unwrap();

    // Session should still be queryable (abort doesn't delete it)
    let snapshot = mgr.snapshot(&token).await.unwrap();
    assert!(snapshot.messages.is_empty());
}

// ── REST API Integration Tests ─────────────────────────────

#[allow(clippy::unused_async)]
async fn rest_app() -> (Router, Arc<SessionManager>) {
    let mgr = Arc::new(test_session_manager());
    let app = create_test_app(mgr.clone()).await;
    (app, mgr)
}

async fn body_to_json(body: axum::body::Body) -> serde_json::Value {
    let bytes = body.collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).unwrap()
}

#[tokio::test]
async fn get_providers_returns_current() {
    let (app, _) = rest_app().await;
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/api/providers")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), axum::http::StatusCode::OK);
    let body = body_to_json(response.into_body()).await;
    assert_eq!(body["current"]["provider"], "test");
    assert_eq!(body["current"]["model"], "test-model");
    assert!(body["providers"].is_array());
}

#[tokio::test]
async fn switch_provider_updates_info() {
    let (app, _) = rest_app().await;
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .method("POST")
                .uri("/api/providers/switch")
                .header("content-type", "application/json")
                .body(axum::body::Body::from(
                    serde_json::to_string(&json!({"provider": "ollama", "model": "llama3"}))
                        .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), axum::http::StatusCode::OK);
    let body = body_to_json(response.into_body()).await;
    assert_eq!(body["provider"], "ollama");
    assert_eq!(body["model"], "llama3");
}

#[tokio::test]
async fn list_skills_returns_array() {
    let (app, _) = rest_app().await;
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/api/skills")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), axum::http::StatusCode::OK);
    let body = body_to_json(response.into_body()).await;
    assert!(body["skills"].is_array());
}

#[tokio::test]
async fn create_and_list_sessions() {
    let (app, mgr) = rest_app().await;

    // Create session via REST
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/api/sessions/new")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), axum::http::StatusCode::CREATED);
    let body = body_to_json(response.into_body()).await;
    let token = body["session_token"].as_str().unwrap();
    assert!(!token.is_empty());

    // List sessions
    let app = create_test_app(mgr.clone()).await;
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/api/sessions")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), axum::http::StatusCode::OK);
    let sessions: Vec<serde_json::Value> =
        serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert!(!sessions.is_empty());
}

#[tokio::test]
async fn delete_session_returns_no_content() {
    let (_app, mgr) = rest_app().await;

    // Create session
    let session = mgr.create_session().await.unwrap();
    let token = session.id.to_string();

    // Delete it
    let app = create_test_app(mgr.clone()).await;
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .method("DELETE")
                .uri(format!("/api/sessions/{token}"))
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), axum::http::StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn delete_nonexistent_session_returns_404() {
    let (app, _mgr) = rest_app().await;
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .method("DELETE")
                .uri("/api/sessions/nonexistent-id")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), axum::http::StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn delete_session_rejects_empty_id() {
    let (app, _mgr) = rest_app().await;
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .method("DELETE")
                .uri("/api/sessions/")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    // Trailing slash without ID won't match the route
    assert_eq!(response.status(), axum::http::StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn list_mcp_servers_returns_empty() {
    let (app, _mgr) = rest_app().await;
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/api/mcp/servers")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), axum::http::StatusCode::OK);
    let body = body_to_json(response.into_body()).await;
    assert!(body["servers"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn add_mcp_server_and_list() {
    let (app, mgr) = rest_app().await;

    // Add server
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .method("POST")
                .uri("/api/mcp/servers/add")
                .header("content-type", "application/json")
                .body(axum::body::Body::from(
                    serde_json::to_string(&json!({
                        "name": "test-fs",
                        "command": "npx",
                        "args": ["mcp-fs"]
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), axum::http::StatusCode::OK);
    let body = body_to_json(response.into_body()).await;
    assert_eq!(body["status"], "added");
    assert_eq!(body["name"], "test-fs");

    // Verify in list
    let app = create_test_app(mgr).await;
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/api/mcp/servers")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let body = body_to_json(response.into_body()).await;
    let servers = body["servers"].as_array().unwrap();
    assert_eq!(servers.len(), 1);
    assert_eq!(servers[0]["name"], "test-fs");
}

#[tokio::test]
async fn remove_mcp_server() {
    let (_app, mgr) = rest_app().await;

    // Add server first
    mgr.add_mcp_server(rustycode_ws_server::McpServerConfig {
        name: "to-remove".into(),
        command: "echo".into(),
        args: vec![],
        env: std::collections::HashMap::new(),
    })
    .await
    .unwrap();

    // Remove it
    let app = create_test_app(mgr.clone()).await;
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .method("DELETE")
                .uri("/api/mcp/servers/to-remove")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), axum::http::StatusCode::NO_CONTENT);

    // Verify it's gone
    let app = create_test_app(mgr).await;
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/api/mcp/servers")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let body = body_to_json(response.into_body()).await;
    assert!(body["servers"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn remove_nonexistent_mcp_server_returns_404() {
    let (app, _mgr) = rest_app().await;
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .method("DELETE")
                .uri("/api/mcp/servers/no-such-server")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), axum::http::StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn restart_nonexistent_mcp_server_returns_404() {
    let (app, _mgr) = rest_app().await;
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .method("POST")
                .uri("/api/mcp/servers/no-such-server/restart")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), axum::http::StatusCode::NOT_FOUND);
}
