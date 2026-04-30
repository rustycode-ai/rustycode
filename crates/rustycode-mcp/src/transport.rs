//! Transport layer for MCP communication (stdio-based)

use crate::protocol::{JsonRpcNotification, JsonRpcRequest, JsonRpcResponse};
use crate::{McpError, McpResult};
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
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

/// Stdio-based transport for spawning MCP servers
pub struct StdioTransport {
    child: Option<Child>,
    stdin: Option<tokio::process::ChildStdin>,
    stdout: Option<BufReader<tokio::process::ChildStdout>>,
    pending_requests:
        std::collections::HashMap<String, tokio::sync::oneshot::Sender<JsonRpcResponse>>,
    next_id: u64,
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

        Ok(Self {
            child: Some(child),
            stdin: Some(stdin),
            stdout: Some(BufReader::new(stdout)),
            pending_requests: std::collections::HashMap::new(),
            next_id: 0,
        })
    }

    /// Generate a unique request ID
    fn generate_id(&mut self) -> String {
        let id = format!("req-{}", self.next_id);
        self.next_id += 1;
        id
    }

    /// Send a JSON message using NDJSON (newline-delimited JSON) over stdio.
    ///
    /// This is the most widely supported framing format for MCP servers.
    /// The message is terminated with a newline character.
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

    /// Receive a JSON message using MCP stdio framing.
    ///
    /// Supports two framing modes (auto-detected per message):
    /// 1. **Content-Length header framing** — reads `Content-Length: N\\r\\n\\r\\n` then N bytes.
    /// 2. **Newline-delimited JSON (NDJSON)** — reads a single line ending in `\\n`.
    ///
    /// Most MCP servers send NDJSON. The Content-Length mode handles servers
    /// that follow the HTTP-like framing from the MCP specification.
    async fn receive_json(&mut self) -> McpResult<String> {
        let reader = self.stdout.as_mut().ok_or(McpError::ConnectionClosed)?;

        let mut first_line = String::new();
        reader
            .read_line(&mut first_line)
            .await
            .map_err(|e| McpError::TransportError(format!("Failed to read from server: {e}")))?;

        if first_line.is_empty() {
            return Err(McpError::ConnectionClosed);
        }

        let trimmed = first_line.trim();

        // Check if the first line is a Content-Length header
        if let Some(len_str) = trimmed.strip_prefix("Content-Length:") {
            let len_str = len_str.trim();
            let length: usize = len_str.parse().map_err(|_| {
                McpError::TransportError(format!("Invalid Content-Length value: {len_str}"))
            })?;

            if length > MAX_MESSAGE_SIZE {
                return Err(McpError::TransportError(format!(
                    "Received message too large: {length} bytes (max {MAX_MESSAGE_SIZE})"
                )));
            }

            // Skip any remaining headers until empty line
            loop {
                let mut header = String::new();
                reader
                    .read_line(&mut header)
                    .await
                    .map_err(|e| McpError::TransportError(format!("Failed to read header: {e}")))?;
                if header.trim().is_empty() {
                    break;
                }
            }

            // Read exact body length
            let mut buffer = vec![0u8; length];
            reader.read_exact(&mut buffer).await.map_err(|e| {
                McpError::TransportError(format!("Failed to read message body: {e}"))
            })?;

            let json = String::from_utf8(buffer)
                .map_err(|e| McpError::TransportError(format!("Invalid UTF-8 in message: {e}")))?;

            trace!("Received JSON (Content-Length framed): {}", json);
            return Ok(json);
        }

        // NDJSON mode: the first line IS the JSON message
        if trimmed.starts_with('{') || trimmed.starts_with('[') {
            let json = trimmed.to_string();
            trace!("Received JSON (NDJSON): {}", json);
            return Ok(json);
        }

        // Unknown format
        Err(McpError::TransportError(format!(
            "Unexpected message format (expected JSON or Content-Length header): {trimmed:?}"
        )))
    }

    /// Try to handle incoming messages (server->client requests)
    async fn try_handle_incoming_messages(&mut self) -> McpResult<()> {
        // Set up a timeout for reading
        let read_result =
            tokio::time::timeout(tokio::time::Duration::from_millis(50), self.receive()).await;

        match read_result {
            Ok(Ok(IncomingMessage::Request(request))) => {
                // Handle server->client requests automatically
                self.handle_incoming_request(&request).await?;
            }
            Ok(Ok(IncomingMessage::Notification(_))) => {
                // Notifications are fine, just consume them
            }
            Ok(Ok(IncomingMessage::Response(response))) => {
                // Response received - check if we have a pending request for it
                let id_str = match &response.id {
                    crate::types::JsonRpcId::String(s) => Some(s.clone()),
                    crate::types::JsonRpcId::Number(n) => Some(n.to_string()),
                    crate::types::JsonRpcId::Null => None,
                };

                if let Some(id) = id_str {
                    let mut pending = std::mem::take(&mut self.pending_requests);
                    if let Some(tx) = pending.remove(&id) {
                        if let Err(e) = tx.send(response) {
                            debug!("Failed to send MCP response to pending request: {:?}", e);
                        }
                    }
                    self.pending_requests = pending;
                }
            }
            _ => {
                // Timeout or other errors - ignore during polling
            }
        }

        Ok(())
    }

    /// Handle incoming requests from the server (server->client)
    async fn handle_incoming_request(&mut self, request: &JsonRpcRequest) -> McpResult<()> {
        debug!("Handling incoming request: {}", request.method);

        let response = if request.method.as_str() == "roots/list" {
            // Return empty roots list (client has no workspace roots configured)
            serde_json::json!({
                "jsonrpc": "2.0",
                "id": &request.id,
                "result": {
                    "roots": []
                }
            })
        } else {
            warn!("Unknown incoming request method: {}", request.method);
            serde_json::json!({
                "jsonrpc": "2.0",
                "id": &request.id,
                "error": {
                    "code": -32601,
                    "message": "Method not found"
                }
            })
        };

        // Send the response back
        let response_json = response.to_string();
        self.send_json(&response_json).await?;
        debug!("Sent response to incoming request: {}", request.method);

        Ok(())
    }
}

#[async_trait::async_trait]
impl Transport for StdioTransport {
    async fn send_request(&mut self, request: JsonRpcRequest) -> McpResult<JsonRpcResponse> {
        // Generate a unique ID if needed
        let id_str = match &request.id {
            crate::types::JsonRpcId::String(s) => s.clone(),
            crate::types::JsonRpcId::Number(n) => n.to_string(),
            crate::types::JsonRpcId::Null => self.generate_id(),
        };

        // Create a channel for the response
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.pending_requests.insert(id_str.clone(), tx);

        // Send the request
        let json = request
            .to_json()
            .map_err(|e| McpError::ProtocolError(format!("Failed to serialize request: {e}")))?;

        self.send_json(&json).await?;

        // Wait for response, handling incoming requests while waiting
        let mut rx = rx;
        let max_iterations = 120; // 60 seconds max (120 * 500ms)
        for _ in 0..max_iterations {
            // First, try to receive and handle any incoming messages
            if let Err(e) = self.try_handle_incoming_messages().await {
                debug!("Error handling incoming messages: {}", e);
            }

            // Check if we've received the response
            match tokio::time::timeout(tokio::time::Duration::from_millis(500), &mut rx).await {
                Ok(Ok(response)) => return Ok(response),
                Ok(Err(_)) => {
                    return Err(McpError::InternalError(
                        "Response channel closed".to_string(),
                    ))
                }
                Err(_) => continue, // Timeout, continue handling incoming messages
            }
        }

        Err(McpError::InternalError(
            "Response timed out after 60 seconds".to_string(),
        ))
    }

    async fn send_notification(&mut self, notification: JsonRpcNotification) -> McpResult<()> {
        let json = notification.to_json().map_err(|e| {
            McpError::ProtocolError(format!("Failed to serialize notification: {e}"))
        })?;

        self.send_json(&json).await
    }

    async fn receive(&mut self) -> McpResult<IncomingMessage> {
        loop {
            let json = self.receive_json().await?;

            // Try to parse as response first
            if let Ok(response) = JsonRpcResponse::from_json(&json) {
                let id_str = match &response.id {
                    crate::types::JsonRpcId::String(s) => Some(s.clone()),
                    crate::types::JsonRpcId::Number(n) => Some(n.to_string()),
                    crate::types::JsonRpcId::Null => None,
                };

                // Check if this is a response to a pending request
                if let Some(id) = id_str {
                    if let Some(tx) = self.pending_requests.remove(&id) {
                        if let Err(e) = tx.send(response) {
                            debug!("Failed to send MCP response to pending request: {:?}", e);
                        }
                        // Skip dispatched responses and read next message
                        continue;
                    }
                }

                return Ok(IncomingMessage::Response(response));
            }

            // Try to parse as request
            if let Ok(request) = JsonRpcRequest::from_json(&json) {
                return Ok(IncomingMessage::Request(request));
            }

            // Try to parse as notification
            if let Ok(notification) = JsonRpcNotification::from_json(&json) {
                return Ok(IncomingMessage::Notification(notification));
            }

            break Err(McpError::ProtocolError(format!(
                "Unknown message format: {json}"
            )));
        } // end loop
    }

    fn is_connected(&self) -> bool {
        self.stdin.is_some() && self.stdout.is_some()
    }

    async fn close(&mut self) -> McpResult<()> {
        debug!("Closing stdio transport");

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

        self.stdout = None;

        Ok(())
    }
}

impl Drop for StdioTransport {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            warn!("StdioTransport dropped without explicit close — killing child process");
            // Best-effort kill; start_kill() is synchronous (sends SIGKILL)
            // We can't await in Drop, so we use the non-blocking version
            let _ = child.start_kill();
        }
    }
}

#[cfg(test)]
mod tests {

    #[tokio::test]
    async fn test_stdio_transport_spawn_echo() {
        // This test requires an echo server, skip in CI
        // In real tests, you'd spawn a simple echo server
    }
}
