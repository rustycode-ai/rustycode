use std::sync::Arc;

use axum::{
    extract::{Path, Query, State, WebSocketUpgrade, ws::{Message, WebSocket}},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    routing::{delete, get, post},
    Json,
    Router,
};
use futures_util::{SinkExt, StreamExt};
use tokio::sync::mpsc;
use tracing::{error, info, warn};

use crate::auth::{self, AuthConfig, WsQuery};
use crate::error::{ErrorCode, ErrorPayload};
use crate::protocol::{
    Capabilities, ClientMessage, Envelope, HeartbeatAckPayload, ServerMessage,
    SessionCreatedPayload, SessionResumedPayload,
};
use crate::session::{
    SessionManager, SessionInfo,
    ProviderInfo, ProviderEntry, ProviderListResponse, SwitchProviderRequest,
    SkillListResponse, SkillInfo, SkillExecuteRequest,
    McpServerListResponse, McpServerInfo, McpAddServerRequest, McpServerConfig,
};
use crate::bridge::EventBridge;

#[derive(Clone)]
pub struct AppState {
    pub session_manager: Arc<SessionManager>,
    pub skill_registry: Arc<rustycode_skill::SkillRegistry>,
    pub auth_config: AuthConfig,
    pub shutdown_token: tokio_util::sync::CancellationToken,
    pub event_bridge: Arc<EventBridge>,
}

pub struct WsRouter;

impl WsRouter {
    pub fn build(session_manager: SessionManager) -> Router {
        Self::build_with_config(session_manager, AuthConfig::default(), tokio_util::sync::CancellationToken::new())
    }

    pub fn build_with_config(
        session_manager: SessionManager,
        auth_config: AuthConfig,
        shutdown_token: tokio_util::sync::CancellationToken,
    ) -> Router {
        let mut skill_registry = rustycode_skill::SkillRegistry::new();
        for skill in rustycode_skill::bundled::get_bundled_skills() {
            skill_registry.register_bundled(skill);
        }

        let bus_handle = session_manager.pipeline().bus_handle();
        let event_bridge = Arc::new(EventBridge::new(bus_handle));
        event_bridge.start();

        let state = AppState {
            session_manager: Arc::new(session_manager),
            skill_registry: Arc::new(skill_registry),
            auth_config: auth_config.clone(),
            shutdown_token,
            event_bridge,
        };
        let auth_state = crate::auth::AuthState { config: auth_config };

        Router::new()
            .route("/ws", get(ws_upgrade_handler))
            .route("/api/providers", get(get_providers))
            .route("/api/providers/switch", post(switch_provider))
            .route("/api/skills", get(list_skills))
            .route("/api/skills/execute", post(execute_skill))
            .route("/api/sessions", get(list_sessions))
            .route("/api/sessions/{id}", delete(delete_session))
            .route("/api/sessions/new", get(create_session))
            .route("/api/mcp/servers", get(list_mcp_servers))
            .route("/api/mcp/servers/add", post(add_mcp_server))
            .route("/api/mcp/servers/{name}", delete(remove_mcp_server))
            .route("/api/mcp/servers/{name}/restart", post(restart_mcp_server))
            .layer(axum::middleware::from_fn_with_state(auth_state, crate::auth::auth_middleware))
            .with_state(state)
    }
}

async fn ws_upgrade_handler(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
    Query(query): Query<WsQuery>,
    headers: HeaderMap,
) -> impl IntoResponse {
    if state.auth_config.is_enabled() {
        let provided = auth::extract_api_key(&headers, query.token.as_deref());
        let expected = state.auth_config.api_key.as_deref().unwrap_or("");
        if provided.as_deref() != Some(expected) {
            return (StatusCode::UNAUTHORIZED, "unauthorized").into_response();
        }
    }
    ws.on_upgrade(move |socket| handle_socket(socket, state))
}

enum Outbound {
    Server(ServerMessage),
    Close,
}

#[allow(clippy::too_many_lines)]
async fn handle_socket(socket: WebSocket, state: AppState) {
    let (mut ws_sender, mut ws_receiver) = socket.split();
    info!("websocket connection opened");

    let session_token;

    if let Some(hello) = wait_for_hello(&mut ws_receiver).await {
        let (session_state, resumed) = match state
            .session_manager
            .get_or_create(hello.session_token.as_deref())
            .await
        {
            Ok(result) => result,
            Err(e) => {
                error!("session creation failed: {e}");
                let err_msg = ServerMessage::Error(ErrorPayload {
                    code: ErrorCode::InternalError,
                    message: format!("session creation failed: {e}"),
                });
                let _ = send_server_message(&mut ws_sender, &err_msg, "err").await;
                return;
            }
        };

        session_token = session_state.id.to_string();

        let msg = if resumed {
            ServerMessage::SessionResumed(SessionResumedPayload {
                session_token: session_token.clone(),
            })
        } else {
            ServerMessage::SessionCreated(SessionCreatedPayload {
                session_token: session_token.clone(),
                capabilities: Capabilities::default(),
            })
        };

        if let Err(e) = send_server_message(&mut ws_sender, &msg, &correlation_id(0)).await {
            warn!("failed to send session response: {e}");
            return;
        }

        state
            .session_manager
            .client_connected(&session_token)
            .await
            .unwrap_or_else(|e| warn!("client tracking failed: {e}"));

        if let Ok(snapshot) = state.session_manager.snapshot(&session_token).await {
            let snap_msg = ServerMessage::StateSnapshot(snapshot);
            if let Err(e) = send_server_message(&mut ws_sender, &snap_msg, &correlation_id(0)).await {
                warn!("failed to send state snapshot: {e}");
            }
        }
    } else {
        let err_msg = ServerMessage::Error(ErrorPayload {
            code: ErrorCode::Unauthorized,
            message: "expected hello message".to_string(),
        });
        let _ = send_server_message(&mut ws_sender, &err_msg, "err").await;
        return;
    }

    let (out_tx, mut out_rx) = mpsc::unbounded_channel::<Outbound>();

    // Register with EventBridge to receive streaming events
    let (bridge_tx, mut bridge_rx) = mpsc::unbounded_channel::<rustycode_protocol::StreamEvent>();
    state.event_bridge.register(&session_token, bridge_tx).await;

    let bridge_out_tx = out_tx.clone();
    let bridge_forward_handle: tokio::task::JoinHandle<()> = tokio::spawn(async move {
        while let Some(stream_event) = bridge_rx.recv().await {
            let event_id = uuid::Uuid::new_v4().to_string();
            let event_payload = crate::protocol::EventPayload {
                seq: 0, // seq is managed by the session
                event_id,
                event: stream_event,
            };
            let _ = bridge_out_tx.send(Outbound::Server(ServerMessage::Event(event_payload)));
        }
    });

    let forward_handle: tokio::task::JoinHandle<()> = tokio::spawn(async move {
        while let Some(outbound) = out_rx.recv().await {
            match outbound {
                Outbound::Server(msg) => {
                    if let Err(e) = send_server_message(&mut ws_sender, &msg, "auto").await {
                        warn!("ws send failed: {e}");
                        break;
                    }
                }
                Outbound::Close => break,
            }
        }
    });

    let cancel = state.shutdown_token.clone();

    loop {
        tokio::select! {
            msg = ws_receiver.next() => {
                let Some(msg) = msg else { break };
                match msg {
                    Ok(Message::Text(text)) => {
                        if let Err(e) = handle_text_message(
                            &text,
                            &state,
                            &session_token,
                            &out_tx,
                        )
                        .await
                        {
                            warn!("message handling error: {e}");
                            let _ = out_tx.send(Outbound::Server(ServerMessage::Error(ErrorPayload {
                                code: ErrorCode::InternalError,
                                message: e.to_string(),
                            })));
                        }
                    }
                    Ok(Message::Binary(_)) => {
                        let _ = out_tx.send(Outbound::Server(ServerMessage::Error(ErrorPayload {
                            code: ErrorCode::InvalidMessage,
                            message: "binary frames are not supported".to_string(),
                        })));
                    }
                    Ok(Message::Close(_)) => break,
                    Ok(Message::Ping(_data)) => {
                        let _ = out_tx.send(Outbound::Server(
                            ServerMessage::HeartbeatAck(HeartbeatAckPayload {
                                ts: 0,
                                server_ts: chrono::Utc::now().timestamp_millis(),
                            }),
                        ));
                    }
                    Err(e) => {
                        warn!("websocket receive error: {e}");
                        break;
                    }
                    _ => {}
                }
            }
            () = cancel.cancelled() => {
                info!(session_id = %session_token, "shutdown requested, closing connection");
                break;
            }
        }
    }

    let _ = out_tx.send(Outbound::Close);
    let _ = forward_handle.await;

    state.event_bridge.unregister(&session_token).await;
    drop(bridge_forward_handle);

    state
        .session_manager
        .client_disconnected(&session_token)
        .await;

    info!(session_id = %session_token, "websocket connection closed");
}

async fn wait_for_hello(
    receiver: &mut futures_util::stream::SplitStream<WebSocket>,
) -> Option<crate::protocol::HelloPayload> {
    let result = tokio::time::timeout(std::time::Duration::from_secs(10), async {
        while let Some(msg) = receiver.next().await {
            match msg {
                Ok(Message::Text(text)) => {
                    if let Ok(envelope) = Envelope::decode(&text) {
                        if envelope.r#type == "hello" {
                            let hello: crate::protocol::HelloPayload =
                                serde_json::from_value(envelope.payload).ok()?;
                            return Some(hello);
                        }
                    }
                    return None;
                }
                Ok(Message::Close(_)) => return None,
                _ => {}
            }
        }
        None
    })
    .await;

    result.unwrap_or(None)
}

async fn handle_text_message(
    text: &str,
    state: &AppState,
    session_token: &str,
    out_tx: &mpsc::UnboundedSender<Outbound>,
) -> Result<(), crate::error::WsError> {
    let envelope =
        Envelope::decode(text).map_err(|e| crate::error::WsError::Protocol(e.to_string()))?;
    let client_msg = ClientMessage::from_envelope(&envelope)
        .map_err(crate::error::WsError::Protocol)?;

    match client_msg {
        ClientMessage::Hello(_) => {
            let _ = out_tx.send(Outbound::Server(ServerMessage::Error(ErrorPayload {
                code: ErrorCode::InvalidMessage,
                message: "hello already received".to_string(),
            })));
        }
        ClientMessage::Input(payload) => {
            state
                .session_manager
                .submit_input(session_token, &payload.content)
                .await?;
        }
        ClientMessage::Abort => {
            info!(session_id = session_token, "abort requested");
            state.session_manager.abort(session_token).await?;
        }
        ClientMessage::ToolApproval(payload) => {
            info!(
                session_id = session_token,
                request_id = %payload.request_id,
                approved = payload.approved,
                "tool approval response received"
            );
            state
                .session_manager
                .respond_tool_approval(session_token, &payload.request_id, payload.approved)
                .await?;
        }
        ClientMessage::PlanApproval(payload) => {
            info!(
                session_id = session_token,
                plan_id = %payload.plan_id,
                approved = payload.approved,
                "plan approval response received"
            );
            state
                .session_manager
                .respond_plan_approval(session_token, &payload.plan_id, payload.approved)
                .await?;
        }
        ClientMessage::Heartbeat(payload) => {
            let _ = out_tx.send(Outbound::Server(ServerMessage::HeartbeatAck(
                HeartbeatAckPayload {
                    ts: payload.ts,
                    server_ts: chrono::Utc::now().timestamp_millis(),
                },
            )));
        }
    }

    Ok(())
}

// ── REST API Handlers ──────────────────────────────────────

async fn get_providers(
    State(state): State<AppState>,
) -> Json<ProviderListResponse> {
    let current = state.session_manager.provider_info().await;
    let providers = rustycode_llm::registry::ProviderMetadataRegistry::new()
        .get_all_providers()
        .iter()
        .map(|meta| {
            let available = std::env::var(&meta.api_key_env)
                .is_ok_and(|k| !k.is_empty());
            ProviderEntry {
                name: meta.id.clone(),
                display_name: meta.name.clone(),
                models: meta.models.iter().map(|m| m.id.clone()).collect(),
                default_model: meta.default_model.clone(),
                available,
            }
        })
        .collect();
    Json(ProviderListResponse { current, providers })
}

async fn switch_provider(
    State(state): State<AppState>,
    Json(req): Json<SwitchProviderRequest>,
) -> Json<ProviderInfo> {
    let info = state.session_manager.switch_provider(req.provider, req.model).await;
    Json(info)
}

async fn list_skills(
    State(state): State<AppState>,
) -> Json<SkillListResponse> {
    let skills: Vec<SkillInfo> = state.skill_registry
        .get_all()
        .into_iter()
        .filter(|s| s.user_invocable)
        .map(|s| SkillInfo {
            id: s.id.clone(),
            name: s.name.clone(),
            description: s.description.clone(),
            categories: s.categories.clone(),
        })
        .collect();
    Json(SkillListResponse { skills })
}

async fn execute_skill(
    State(state): State<AppState>,
    Json(req): Json<SkillExecuteRequest>,
) -> Result<impl IntoResponse, (StatusCode, Json<serde_json::Value>)> {
    let skill = state.skill_registry.get(&req.skill_id).cloned();
    let Some(skill) = skill else {
        return Err((
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": format!("skill not found: {}", req.skill_id) })),
        ));
    };

    let content = skill.content.unwrap_or_default();
    let input = if req.args.is_empty() {
        format!("/{}\n{}", skill.id, content)
    } else {
        format!("/{} {}\n{}", skill.id, req.args, content)
    };

    state
        .session_manager
        .submit_input(&req.session_token, &input)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": e.to_string() })),
            )
        })?;

    Ok((
        StatusCode::OK,
        Json(serde_json::json!({ "status": "executing", "skill_id": req.skill_id })),
    ))
}

async fn list_sessions(
    State(state): State<AppState>,
) -> Json<Vec<SessionInfo>> {
    Json(state.session_manager.list_sessions().await)
}

async fn create_session(
    State(state): State<AppState>,
) -> impl IntoResponse {
    let session_state = state.session_manager.create_session().await;
    (
        StatusCode::CREATED,
        Json(serde_json::json!({ "session_token": session_state.id.to_string() })),
    )
}

async fn delete_session(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, Json<serde_json::Value>> {
    state
        .session_manager
        .delete_session(&id)
        .await
        .map_err(|e| {
            Json(serde_json::json!({ "error": e.to_string() }))
        })?;
    Ok(StatusCode::NO_CONTENT)
}

// ── MCP Server Handlers ───────────────────────────────────

async fn list_mcp_servers(
    State(state): State<AppState>,
) -> Json<McpServerListResponse> {
    let servers: Vec<McpServerInfo> = state
        .session_manager
        .list_mcp_servers()
        .await
        .into_iter()
        .map(|c| McpServerInfo {
            name: c.name,
            command: c.command,
            args: c.args,
            status: "registered".to_string(),
        })
        .collect();
    Json(McpServerListResponse { servers })
}

async fn add_mcp_server(
    State(state): State<AppState>,
    Json(req): Json<McpAddServerRequest>,
) -> Json<serde_json::Value> {
    let config = McpServerConfig {
        name: req.name.clone(),
        command: req.command.clone(),
        args: req.args.clone(),
        env: req.env.clone(),
    };
    state.session_manager.add_mcp_server(config).await;
    Json(serde_json::json!({
        "status": "added",
        "name": req.name
    }))
}

async fn remove_mcp_server(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<serde_json::Value>)> {
    state
        .session_manager
        .remove_mcp_server(&name)
        .await
        .map_err(|e| {
            (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({ "error": e.to_string() })),
            )
        })?;
    Ok(StatusCode::NO_CONTENT)
}

async fn restart_mcp_server(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let config = state
        .session_manager
        .restart_mcp_server(&name)
        .await
        .map_err(|e| {
            (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({ "error": e.to_string() })),
            )
        })?;
    Ok(Json(serde_json::json!({
        "status": "restarted",
        "name": config.name
    })))
}

async fn send_server_message(
    sender: &mut futures_util::stream::SplitSink<WebSocket, Message>,
    msg: &ServerMessage,
    id: &str,
) -> Result<(), crate::error::WsError> {
    let envelope = msg
        .to_envelope(id)
        .map_err(crate::error::WsError::Serialization)?;
    let json = envelope
        .encode()
        .map_err(crate::error::WsError::Serialization)?;
    sender
        .send(Message::Text(json.into()))
        .await
        .map_err(|_| crate::error::WsError::ConnectionClosed)
}

fn correlation_id(seq: u64) -> String {
    format!("corr-{seq}")
}
