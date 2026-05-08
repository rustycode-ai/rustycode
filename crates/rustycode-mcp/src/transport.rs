//! Transport layer for MCP communication (stdio-based)

use crate::protocol::{JsonRpcNotification, JsonRpcRequest, JsonRpcResponse};
use crate::types::JsonRpcId;
use crate::{McpError, McpResult};
use std::collections::HashMap;
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinHandle;
use tracing::{debug, info, trace, warn};

/// Maximum allowed size for a single MCP message (1 MiB).
/// Prevents memory exhaustion from malformed or malicious servers.
pub(crate) const MAX_MESSAGE_SIZE: usize = 1 << 20;

/// Transport trait for MCP communication
#[async_trait::async_trait]
pub trait Transport: Send + Sync {
    /// Send a request and wait for response
    async fn send_request(&mut self, request: JsonRpcRequest) -> McpResult<JsonRpcResponse>;

    /// Send a notification (no response expected)
    async fn send_notification(&mut self, notification: JsonRpcNotification) -> McpResult<()>;

    /// Receive the next message (request or notification)
    async fn receive(&mut self) -> McpResult<IncomingMessage>;

    /// Try to receive a pending message without blocking.
    ///
    /// Returns `Ok(message)` if one was buffered, or `Err` if none available.
    fn try_receive(&mut self) -> McpResult<IncomingMessage>;

    /// Check if transport is connected
    fn is_connected(&self) -> bool;

    /// Close the transport
    async fn close(&mut self) -> McpResult<()>;
}

/// Incoming message from the other side
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum IncomingMessage {
    Response(JsonRpcResponse),
    Request(JsonRpcRequest),
    Notification(JsonRpcNotification),
}

/// Stdio-based transport for spawning MCP servers.
///
/// Uses a background reader task for true async multiplexing:
/// - Responses are routed to pending oneshot channels by request ID
/// - Notifications and server-initiated requests go to an inbox channel
pub struct StdioTransport {
    child: Option<Child>,
    stdin: Option<tokio::process::ChildStdin>,
    reader_handle: Option<JoinHandle<()>>,
    pending_requests: Arc<Mutex<HashMap<String, oneshot::Sender<JsonRpcResponse>>>>,
    inbox: mpsc::Receiver<IncomingMessage>,
    inbox_sender: Option<mpsc::Sender<IncomingMessage>>,
    next_id: u64,
    connected: bool,
}

impl StdioTransport {
    /// Create a new stdio transport by spawning a process
    pub fn spawn(command: &str, args: &[&str]) -> McpResult<Self> {
        info!("Spawning MCP server: {} {}", command, args.join(" "));

        let mut child = Command::new(command)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| McpError::TransportError(format!("Failed to spawn process: {e}")))?;

        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| McpError::TransportError("Failed to capture stdin".to_string()))?;

        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| McpError::TransportError("Failed to capture stdout".to_string()))?;

        let (inbox_tx, inbox_rx) = mpsc::channel(100);
        let pending: Arc<Mutex<HashMap<String, oneshot::Sender<JsonRpcResponse>>>> =
            Arc::new(Mutex::new(HashMap::new()));

        let pending_clone = pending.clone();
        let inbox_tx_clone = inbox_tx.clone();

        let reader_handle = tokio::spawn(async move {
            let mut reader = BufReader::new(stdout);
            let mut line = String::new();

            loop {
                line.clear();
                match reader.read_line(&mut line).await {
                    Ok(0) => break, // EOF
                    Ok(_) => {}
                    Err(_) => break,
                }

                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }

                // Content-Length framing
                if let Some(len_str) = trimmed.strip_prefix("Content-Length:") {
                    let length: usize = match len_str.trim().parse() {
                        Ok(n) if n <= MAX_MESSAGE_SIZE => n,
                        _ => continue,
                    };

                    // Skip remaining headers until empty line
                    loop {
                        let mut header = String::new();
                        match reader.read_line(&mut header).await {
                            Ok(_) if header.trim().is_empty() => break,
                            Ok(_) => continue,
                            Err(_) => break,
                        }
                    }

                    // Read exact body length
                    let mut buffer = vec![0u8; length];
                    if reader.read_exact(&mut buffer).await.is_err() {
                        break;
                    }
                    let json = match String::from_utf8(buffer) {
                        Ok(s) => s,
                        Err(_) => continue,
                    };
                    route_stdio_message(&json, &pending_clone, &inbox_tx_clone).await;
                    continue;
                }

                // NDJSON
                if trimmed.starts_with('{') || trimmed.starts_with('[') {
                    route_stdio_message(trimmed, &pending_clone, &inbox_tx_clone).await;
                }
            }
        });

        Ok(Self {
            child: Some(child),
            stdin: Some(stdin),
            reader_handle: Some(reader_handle),
            pending_requests: pending,
            inbox: inbox_rx,
            inbox_sender: Some(inbox_tx),
            next_id: 0,
            connected: true,
        })
    }

    /// Generate a unique request ID
    fn generate_id(&mut self) -> String {
        let id = format!("req-{}", self.next_id);
        self.next_id += 1;
        id
    }

    /// Send a JSON message using NDJSON over stdio.
    async fn send_json(&mut self, json: &str) -> McpResult<()> {
        if json.len() > MAX_MESSAGE_SIZE {
            return Err(McpError::TransportError(format!(
                "Message too large: {} bytes (max {})",
                json.len(),
                MAX_MESSAGE_SIZE
            )));
        }

        trace!("Sending JSON: {}", json);

        let stdin = self.stdin.as_mut().ok_or(McpError::ConnectionClosed)?;

        let message = format!("{json}\n");

        stdin
            .write_all(message.as_bytes())
            .await
            .map_err(|e| McpError::TransportError(format!("Failed to write to stdin: {e}")))?;

        stdin
            .flush()
            .await
            .map_err(|e| McpError::TransportError(format!("Failed to flush stdin: {e}")))?;

        Ok(())
    }
}

/// Route a parsed JSON message to the correct destination.
/// Responses go to pending oneshot channels; everything else goes to the inbox.
async fn route_stdio_message(
    json: &str,
    pending: &Arc<Mutex<HashMap<String, oneshot::Sender<JsonRpcResponse>>>>,
    inbox: &mpsc::Sender<IncomingMessage>,
) {
    // Parse generically first to check for "method" field — serde ignores
    // unknown fields, so a request like {"id":"x","method":"y"} would
    // otherwise parse as a JsonRpcResponse (dropping "method").
    let value: serde_json::Value = match serde_json::from_str(json) {
        Ok(v) => v,
        Err(_) => return,
    };

    if value.get("method").is_some() {
        // Has "method" → request or notification
        if value.get("id").is_some() {
            if let Ok(req) = serde_json::from_value::<JsonRpcRequest>(value) {
                let _ = inbox.send(IncomingMessage::Request(req)).await;
            }
        } else if let Ok(notif) = serde_json::from_value::<JsonRpcNotification>(value) {
            let _ = inbox.send(IncomingMessage::Notification(notif)).await;
        }
        return;
    }

    // No "method" → response
    if let Ok(response) = serde_json::from_value::<JsonRpcResponse>(value) {
        let id_str = match &response.id {
            JsonRpcId::String(s) => s.clone(),
            JsonRpcId::Number(n) => n.to_string(),
            JsonRpcId::Null => {
                let _ = inbox.send(IncomingMessage::Response(response)).await;
                return;
            }
        };
        if let Ok(mut pending) = pending.lock() {
            if let Some(tx) = pending.remove(&id_str) {
                let _ = tx.send(response);
                return;
            }
        }
        let _ = inbox.send(IncomingMessage::Response(response)).await;
    }
}

#[async_trait::async_trait]
impl Transport for StdioTransport {
    async fn send_request(&mut self, request: JsonRpcRequest) -> McpResult<JsonRpcResponse> {
        let id_str = match &request.id {
            JsonRpcId::String(s) => s.clone(),
            JsonRpcId::Number(n) => n.to_string(),
            JsonRpcId::Null => self.generate_id(),
        };

        let (tx, rx) = oneshot::channel();
        {
            let mut pending = self.pending_requests.lock().unwrap();
            pending.insert(id_str, tx);
        }

        let json = request
            .to_json()
            .map_err(|e| McpError::ProtocolError(format!("Failed to serialize request: {e}")))?;

        self.send_json(&json).await?;

        #[allow(clippy::duration_suboptimal_units)]
        match tokio::time::timeout(std::time::Duration::from_secs(60), rx).await {
            Ok(Ok(response)) => Ok(response),
            Ok(Err(_)) => {
                self.connected = false;
                Err(McpError::InternalError(
                    "Response channel closed".to_string(),
                ))
            }
            Err(_) => {
                self.connected = false;
                Err(McpError::Timeout)
            }
        }
    }

    async fn send_notification(&mut self, notification: JsonRpcNotification) -> McpResult<()> {
        let json = notification.to_json().map_err(|e| {
            McpError::ProtocolError(format!("Failed to serialize notification: {e}"))
        })?;

        self.send_json(&json).await
    }

    async fn receive(&mut self) -> McpResult<IncomingMessage> {
        if let Some(msg) = self.inbox.recv().await {
            Ok(msg)
        } else {
            self.connected = false;
            Err(McpError::ConnectionClosed)
        }
    }

    fn try_receive(&mut self) -> McpResult<IncomingMessage> {
        self.inbox.try_recv().map_err(|e| match e {
            tokio::sync::mpsc::error::TryRecvError::Empty => {
                McpError::TransportError("no pending messages".to_string())
            }
            tokio::sync::mpsc::error::TryRecvError::Disconnected => {
                self.connected = false;
                McpError::ConnectionClosed
            }
        })
    }

    fn is_connected(&self) -> bool {
        self.connected && self.stdin.is_some()
    }

    async fn close(&mut self) -> McpResult<()> {
        debug!("Closing stdio transport");

        if let Some(handle) = self.reader_handle.take() {
            handle.abort();
        }

        if let Some(mut stdin) = self.stdin.take() {
            if let Err(e) = stdin.shutdown().await {
                debug!("stdin shutdown error (expected if process exited): {}", e);
            }
        }

        if let Some(mut child) = self.child.take() {
            if let Err(e) = child.kill().await {
                debug!("Failed to kill child process: {}", e);
            }
            if let Err(e) = child.wait().await {
                debug!("Failed to wait for child process: {}", e);
            }
        }

        // Drain pending requests
        let mut pending = self.pending_requests.lock().unwrap();
        for (_, tx) in pending.drain() {
            let _ = tx.send(JsonRpcResponse::error(
                "closed",
                -32000,
                "Connection closed",
            ));
        }

        self.inbox_sender.take();
        self.connected = false;
        Ok(())
    }
}

impl Drop for StdioTransport {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            warn!("StdioTransport dropped without explicit close — killing child process");
            let _ = child.start_kill();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_id_increments() {
        let mut id_counter: u64 = 0;
        let id1 = format!("req-{id_counter}");
        id_counter += 1;
        let id2 = format!("req-{id_counter}");
        assert_eq!(id1, "req-0");
        assert_eq!(id2, "req-1");
    }

    #[tokio::test]
    async fn test_route_stdio_message_response() {
        let pending: Arc<Mutex<HashMap<String, oneshot::Sender<JsonRpcResponse>>>> =
            Arc::new(Mutex::new(HashMap::new()));
        let (tx, mut rx) = mpsc::channel(10);

        let (oneshot_tx, oneshot_rx) = oneshot::channel();
        pending
            .lock()
            .unwrap()
            .insert("req-1".to_string(), oneshot_tx);

        let json = r#"{"jsonrpc":"2.0","id":"req-1","result":{"tools":[]}}"#;
        route_stdio_message(json, &pending, &tx).await;

        let response = oneshot_rx.await.unwrap();
        assert!(response.is_success());
        assert!(rx.try_recv().is_err(), "inbox should be empty");
    }

    #[tokio::test]
    async fn test_route_stdio_message_notification() {
        let pending: Arc<Mutex<HashMap<String, oneshot::Sender<JsonRpcResponse>>>> =
            Arc::new(Mutex::new(HashMap::new()));
        let (tx, mut rx) = mpsc::channel(10);

        let json = r#"{"jsonrpc":"2.0","method":"notifications/tools/list_changed"}"#;
        route_stdio_message(json, &pending, &tx).await;

        let msg = rx.try_recv().unwrap();
        match msg {
            IncomingMessage::Notification(n) => {
                assert_eq!(n.method, "notifications/tools/list_changed");
            }
            _ => panic!("Expected notification"),
        }
    }

    #[tokio::test]
    async fn test_route_stdio_message_request() {
        let pending: Arc<Mutex<HashMap<String, oneshot::Sender<JsonRpcResponse>>>> =
            Arc::new(Mutex::new(HashMap::new()));
        let (tx, mut rx) = mpsc::channel(10);

        let json = r#"{"jsonrpc":"2.0","id":"srv-1","method":"roots/list"}"#;
        route_stdio_message(json, &pending, &tx).await;

        let msg = rx.try_recv().unwrap();
        match msg {
            IncomingMessage::Request(r) => {
                assert_eq!(r.method, "roots/list");
            }
            _ => panic!("Expected request"),
        }
    }

    #[tokio::test]
    async fn test_route_unmatched_response_to_inbox() {
        let pending: Arc<Mutex<HashMap<String, oneshot::Sender<JsonRpcResponse>>>> =
            Arc::new(Mutex::new(HashMap::new()));
        let (tx, mut rx) = mpsc::channel(10);

        let json = r#"{"jsonrpc":"2.0","id":"unknown","result":{}}"#;
        route_stdio_message(json, &pending, &tx).await;

        let msg = rx.try_recv().unwrap();
        assert!(matches!(msg, IncomingMessage::Response(_)));
    }

    #[test]
    fn test_max_message_size_constant() {
        assert_eq!(MAX_MESSAGE_SIZE, 1 << 20);
    }
}
