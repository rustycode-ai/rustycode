#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::unreadable_literal,
    clippy::needless_pass_by_value
)]

use std::sync::Arc;

use axum::Router;
use futures_util::{SinkExt, StreamExt};
use rustycode_ws_server::{Envelope, SessionManager, WsRouter};
use serde_json::json;
use tokio_tungstenite::tungstenite::Message;

fn test_session_manager() -> SessionManager {
    let config = rustycode_orchestration::config::OrchestrationConfig::default();
    let pipeline = Arc::new(rustycode_orchestration::pipeline::OrchestrationPipeline::new(config));
    SessionManager::new(pipeline, "test".to_string(), "test-model".to_string())
}

fn create_test_app(session_manager: Arc<SessionManager>) -> Router {
    WsRouter::build((*session_manager).clone())
}

async fn ws_connect_with(shared: Option<Arc<SessionManager>>) -> (
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
    let app = create_test_app(mgr.clone());

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

    sink.send(make_envelope("hello", "1", json!({}))).await.unwrap();

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
    sink.send(make_envelope("hello", "1", json!({}))).await.unwrap();
    let created = recv_envelope(&mut recv).await;
    let token = created.payload["session_token"].as_str().unwrap().to_string();

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
    sink.send(make_envelope("hello", "1", json!({}))).await.unwrap();
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

    sink.send(make_envelope("hello", "1", json!({}))).await.unwrap();
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

    sink.send(make_envelope("hello", "1", json!({}))).await.unwrap();
    let _created = recv_envelope(&mut recv).await;
    let _snapshot = recv_envelope(&mut recv).await;

    // Send another hello
    sink.send(make_envelope("hello", "2", json!({}))).await.unwrap();

    let err = recv_envelope(&mut recv).await;
    assert_eq!(err.r#type, "error");
    assert_eq!(err.payload["code"], "invalid_message");
}

#[tokio::test]
async fn unknown_message_type_returns_error() {
    let (mut sink, mut recv) = ws_connect().await;

    sink.send(make_envelope("hello", "1", json!({}))).await.unwrap();
    let _created = recv_envelope(&mut recv).await;
    let _snapshot = recv_envelope(&mut recv).await;

    // Send unknown type
    sink.send(make_envelope("unknown_type", "2", json!({})))
        .await
        .unwrap();

    let err = recv_envelope(&mut recv).await;
    assert_eq!(err.r#type, "error");
}
