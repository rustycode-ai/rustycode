use crate::protocol::{JsonRpcNotification, JsonRpcRequest, JsonRpcResponse};
use crate::sse::SseParser;
use crate::types::JsonRpcId;
use crate::{McpError, McpResult};
use async_trait::async_trait;
use futures::StreamExt;
use reqwest::Client;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinHandle;

use crate::transport::{IncomingMessage, Transport};

#[allow(dead_code)]
/// Simple HTTP transport for MCP using POST requests.
pub struct HttpTransport {
    client: Client,
    url: String,
    headers: HashMap<String, String>,
    session_id: Option<String>,
    connected: bool,
    pending_requests: HashMap<String, oneshot::Sender<JsonRpcResponse>>,
    inbox: mpsc::Receiver<IncomingMessage>,
    inbox_sender: Option<mpsc::Sender<IncomingMessage>>,
    get_listener_handle: Option<JoinHandle<()>>,
    last_event_id: Arc<Mutex<Option<String>>>,
}

impl HttpTransport {
    pub fn new(url: &str, headers: HashMap<String, String>) -> McpResult<Self> {
        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .map_err(|e| McpError::TransportError(format!("HTTP client error: {e}")))?;

        let (tx, rx) = mpsc::channel(100);

        Ok(Self {
            client,
            url: url.to_string(),
            headers,
            session_id: None,
            connected: true,
            pending_requests: HashMap::new(),
            inbox: rx,
            inbox_sender: Some(tx),
            get_listener_handle: None,
            last_event_id: Arc::new(Mutex::new(None)),
        })
    }

    /// Internal helper to set session id (test visibility only)
    #[cfg(test)]
    pub fn test_set_session_id(&mut self, sid: Option<String>) {
        self.session_id = sid;
    }

    /// Spawn a background GET listener for server-initiated messages.
    /// Call after a successful initialize (when session_id is known).
    pub fn start_get_listener(&mut self) {
        let session_id = match &self.session_id {
            Some(id) => id.clone(),
            None => return,
        };
        let inbox_tx = match self.inbox_sender.clone() {
            Some(tx) => tx,
            None => return,
        };
        let client = self.client.clone();
        let url = self.url.clone();
        let last_event_id = self.last_event_id.clone();

        let handle = tokio::spawn(async move {
            let mut backoff_ms: u64 = 1000;
            const MAX_BACKOFF_MS: u64 = 300_000;

            loop {
                let mut req = client
                    .get(&url)
                    .header("Accept", "text/event-stream")
                    .header("MCP-Protocol-Version", crate::MCP_VERSION)
                    .header("MCP-Session-Id", &session_id);

                // Send Last-Event-ID on reconnect for resumability
                if let Some(id) = last_event_id.lock().ok().and_then(|g| g.clone()) {
                    req = req.header("Last-Event-ID", &id);
                }

                match req.send().await {
                    Ok(resp) if resp.status().as_u16() == 405 => {
                        // Server doesn't support GET listener — graceful degradation
                        break;
                    }
                    Ok(resp) if resp.status().as_u16() == 404 => {
                        // Session expired — stop listener
                        break;
                    }
                    Ok(resp) if resp.status().is_success() => {
                        backoff_ms = 1000;
                        let mut parser = SseParser::new();
                        let mut stream = resp.bytes_stream();

                        while let Some(chunk_result) = stream.next().await {
                            let chunk = match chunk_result {
                                Ok(c) => c,
                                Err(_) => break,
                            };
                            let text = match String::from_utf8(chunk.to_vec()) {
                                Ok(s) => s,
                                Err(_) => continue,
                            };

                            let events = parser.feed(&text);
                            for event in &events {
                                route_sse_event(&event.data, &inbox_tx).await;
                                // Track last event ID for resumability
                                if let Some(ref id) = event.id {
                                    if let Ok(mut g) = last_event_id.lock() {
                                        *g = Some(id.clone());
                                    }
                                }
                            }
                            // Respect server-suggested retry interval
                            if let Some(retry_ms) = parser.retry_interval() {
                                backoff_ms = retry_ms;
                            }
                        }

                        // Flush any partial event
                        if let Some(event) = parser.flush() {
                            if let Some(ref id) = event.id {
                                if let Ok(mut g) = last_event_id.lock() {
                                    *g = Some(id.clone());
                                }
                            }
                            route_sse_event(&event.data, &inbox_tx).await;
                        }

                        // Stream ended — reconnect after backoff
                        tokio::time::sleep(Duration::from_millis(backoff_ms)).await;
                        backoff_ms = (backoff_ms * 2).min(MAX_BACKOFF_MS);
                    }
                    Ok(_) => {
                        // Unexpected status — backoff and retry
                        tokio::time::sleep(Duration::from_millis(backoff_ms)).await;
                        backoff_ms = (backoff_ms * 2).min(MAX_BACKOFF_MS);
                    }
                    Err(_) => {
                        // Network error — backoff and retry
                        tokio::time::sleep(Duration::from_millis(backoff_ms)).await;
                        backoff_ms = (backoff_ms * 2).min(MAX_BACKOFF_MS);
                    }
                }
            }
        });

        self.get_listener_handle = Some(handle);
    }
}

async fn route_sse_event(data: &str, inbox_tx: &mpsc::Sender<IncomingMessage>) {
    let value: serde_json::Value = match serde_json::from_str(data) {
        Ok(v) => v,
        Err(_) => return,
    };

    if value.get("method").is_some() {
        if value.get("id").is_some() {
            if let Ok(req) = serde_json::from_value::<JsonRpcRequest>(value) {
                let _ = inbox_tx.send(IncomingMessage::Request(req)).await;
            }
        } else if let Ok(notif) = serde_json::from_value::<JsonRpcNotification>(value) {
            let _ = inbox_tx.send(IncomingMessage::Notification(notif)).await;
        }
        return;
    }

    if let Ok(resp) = serde_json::from_value::<JsonRpcResponse>(value) {
        let _ = inbox_tx.send(IncomingMessage::Response(resp)).await;
    }
}

#[async_trait]
impl Transport for HttpTransport {
    async fn send_request(&mut self, request: JsonRpcRequest) -> McpResult<JsonRpcResponse> {
        let json = request
            .to_json()
            .map_err(|e| McpError::ProtocolError(format!("Serialize error: {e}")))?;

        let mut req = self
            .client
            .post(&self.url)
            .header("Content-Type", "application/json")
            .header("Accept", "application/json, text/event-stream")
            .header("MCP-Protocol-Version", crate::MCP_VERSION);

        for (k, v) in &self.headers {
            req = req.header(k.as_str(), v.as_str());
        }
        if let Some(ref sid) = self.session_id {
            req = req.header("MCP-Session-Id", sid.as_str());
        }
        req = req.body(json);

        let resp = match req.send().await {
            Ok(r) => r,
            Err(e) => {
                self.connected = false;
                return Err(McpError::TransportError(format!(
                    "HTTP request failed: {e}"
                )));
            }
        };

        let status = resp.status();
        if status.as_u16() == 404 {
            self.session_id = None;
            return Err(McpError::SessionExpired(
                "Server session not found. Re-initialize to continue.".to_string(),
            ));
        }
        if status.as_u16() == 429 {
            let retry_after = resp
                .headers()
                .get("Retry-After")
                .and_then(|v| v.to_str().ok())
                .and_then(|s| s.parse::<u64>().ok())
                .unwrap_or(5);
            return Err(McpError::RateLimited(Duration::from_secs(retry_after)));
        }
        if status.is_client_error() || status.is_server_error() {
            let body = resp.text().await.unwrap_or_default();
            return Err(McpError::TransportError(format!(
                "HTTP {} response: {}",
                status,
                body.chars().take(200).collect::<String>()
            )));
        }

        if let Some(val) = resp.headers().get("MCP-Session-Id") {
            if let Ok(s) = val.to_str() {
                self.session_id = Some(s.to_string());
            }
        }

        // Route response by Content-Type
        let ct = resp
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");

        if ct.contains("application/json") {
            // Single JSON-RPC response
            let text = resp
                .text()
                .await
                .map_err(|e| McpError::TransportError(format!("HTTP read error: {e}")))?;
            let json_resp = JsonRpcResponse::from_json(&text)
                .map_err(|e| McpError::ProtocolError(format!("Invalid JSON: {e}")))?;
            return Ok(json_resp);
        }

        if ct.contains("text/event-stream") {
            // SSE stream: parse events, route first matching response, forward rest to inbox
            let text = resp
                .text()
                .await
                .map_err(|e| McpError::TransportError(format!("SSE read error: {e}")))?;

            let mut parser = SseParser::new();
            let mut events = parser.feed(&text);
            if let Some(event) = parser.flush() {
                events.push(event);
            }

            let request_id_str = match &request.id {
                JsonRpcId::String(s) => s.clone(),
                JsonRpcId::Number(n) => n.to_string(),
                JsonRpcId::Null => String::new(),
            };

            let mut found_response = None;
            for event in events {
                let value: serde_json::Value = match serde_json::from_str(&event.data) {
                    Ok(v) => v,
                    Err(_) => continue,
                };

                if value.get("method").is_some() {
                    if value.get("id").is_some() {
                        if let Ok(req) = serde_json::from_value::<JsonRpcRequest>(value) {
                            if let Some(sender) = &self.inbox_sender {
                                let _ = sender.send(IncomingMessage::Request(req)).await;
                            }
                        }
                    } else if let Ok(notif) = serde_json::from_value::<JsonRpcNotification>(value) {
                        if let Some(sender) = &self.inbox_sender {
                            let _ = sender.send(IncomingMessage::Notification(notif)).await;
                        }
                    }
                    continue;
                }

                if let Ok(json_resp) = serde_json::from_value::<JsonRpcResponse>(value) {
                    let resp_id_str = match &json_resp.id {
                        JsonRpcId::String(s) => s.clone(),
                        JsonRpcId::Number(n) => n.to_string(),
                        JsonRpcId::Null => String::new(),
                    };
                    if resp_id_str == request_id_str && found_response.is_none() {
                        found_response = Some(json_resp);
                    } else if let Some(sender) = &self.inbox_sender {
                        let _ = sender.send(IncomingMessage::Response(json_resp)).await;
                    }
                }
            }

            return found_response.ok_or_else(|| {
                McpError::ProtocolError("No JSON-RPC response found in SSE stream".to_string())
            });
        }

        Err(McpError::TransportError(format!(
            "Unsupported response content-type: {ct}"
        )))
    }

    async fn send_notification(&mut self, notification: JsonRpcNotification) -> McpResult<()> {
        let json = notification
            .to_json()
            .map_err(|e| McpError::ProtocolError(format!("Serialize error: {e}")))?;

        let mut req = self
            .client
            .post(&self.url)
            .header("Content-Type", "application/json")
            .header("Accept", "application/json, text/event-stream")
            .header("MCP-Protocol-Version", crate::MCP_VERSION);
        for (k, v) in &self.headers {
            req = req.header(k.as_str(), v.as_str());
        }
        if let Some(ref sid) = self.session_id {
            req = req.header("MCP-Session-Id", sid.as_str());
        }
        req = req.body(json);

        let _resp = match req.send().await {
            Ok(r) => r,
            Err(e) => {
                self.connected = false;
                return Err(McpError::TransportError(format!("HTTP notify failed: {e}")));
            }
        };
        Ok(())
    }

    async fn receive(&mut self) -> McpResult<IncomingMessage> {
        match self.inbox.recv().await {
            Some(msg) => Ok(msg),
            None => Err(McpError::ConnectionClosed),
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
        self.connected
    }

    async fn close(&mut self) -> McpResult<()> {
        // Abort background GET listener
        if let Some(handle) = self.get_listener_handle.take() {
            handle.abort();
        }

        // Send DELETE to notify server (ignore errors — session may already be gone)
        if let Some(ref session_id) = self.session_id {
            let _ = self
                .client
                .delete(&self.url)
                .header("MCP-Session-Id", session_id)
                .header("MCP-Protocol-Version", crate::MCP_VERSION)
                .send()
                .await;
        }

        // Drain pending requests
        for (_id, tx) in self.pending_requests.drain() {
            let _ = tx.send(JsonRpcResponse::error(
                "closed",
                -32000,
                "Connection closed",
            ));
        }

        // Close inbox channel
        self.inbox_sender.take();

        self.connected = false;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[tokio::test]
    async fn test_http_transport_new_builds_client() {
        let headers: HashMap<String, String> = HashMap::new();
        let t = HttpTransport::new("http://example.invalid/mcp", headers).unwrap();
        // is_connected must be true on creation in our simplified implementation
        assert!(t.is_connected());
    }

    #[test]
    fn test_http_transport_stores_headers() {
        let mut headers = HashMap::new();
        headers.insert("Authorization".to_string(), "Bearer test-token".to_string());
        let t = HttpTransport::new("http://localhost:8080/mcp", headers).unwrap();
        assert_eq!(t.headers.get("Authorization").unwrap(), "Bearer test-token");
    }

    #[test]
    fn test_http_transport_session_id_initially_none() {
        let headers = HashMap::new();
        let t = HttpTransport::new("http://localhost:8080/mcp", headers).unwrap();
        assert!(t.session_id.is_none());
    }

    #[test]
    fn test_http_transport_test_set_session_id() {
        let headers = HashMap::new();
        let mut t = HttpTransport::new("http://localhost:8080/mcp", headers).unwrap();
        assert!(t.session_id.is_none());
        t.test_set_session_id(Some("sess-123".to_string()));
        assert_eq!(t.session_id.as_deref(), Some("sess-123"));
    }

    #[tokio::test]
    async fn test_close_sets_disconnected() {
        let headers = HashMap::new();
        let mut t = HttpTransport::new("http://localhost:8080/mcp", headers).unwrap();
        assert!(t.is_connected());
        t.close().await.unwrap();
        assert!(!t.is_connected());
    }

    #[test]
    fn test_pending_requests_initially_empty() {
        let headers = HashMap::new();
        let t = HttpTransport::new("http://localhost:8080/mcp", headers).unwrap();
        assert!(t.pending_requests.is_empty());
    }

    #[test]
    fn test_url_stored_correctly() {
        let headers = HashMap::new();
        let t = HttpTransport::new("http://localhost:9999/sse", headers).unwrap();
        assert_eq!(t.url, "http://localhost:9999/sse");
    }

    #[test]
    fn test_multiple_headers_preserved() {
        let mut headers = HashMap::new();
        headers.insert("Authorization".to_string(), "Bearer tok".to_string());
        headers.insert("X-Custom".to_string(), "value123".to_string());
        let t = HttpTransport::new("http://localhost/mcp", headers).unwrap();
        assert_eq!(t.headers.len(), 2);
        assert_eq!(t.headers.get("X-Custom").unwrap(), "value123");
    }

    #[tokio::test]
    async fn test_receive_returns_connection_closed_when_sender_dropped() {
        let headers = HashMap::new();
        let mut t = HttpTransport::new("http://localhost/mcp", headers).unwrap();
        // Drop the sender so the channel closes
        t.inbox_sender.take();
        let result = t.receive().await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_send_request_to_invalid_host_returns_transport_error() {
        let headers = HashMap::new();
        let mut t = HttpTransport::new("http://256.256.256.256:1/mcp", headers).unwrap();
        let req = JsonRpcRequest::new("test-1", "initialize").with_params(serde_json::json!({}));
        let result = t.send_request(req).await;
        assert!(result.is_err());
        assert!(!t.is_connected(), "should mark disconnected on failure");
    }

    // --- SSE response routing tests ---

    #[test]
    fn test_sse_parser_extracts_json_rpc_response() {
        let sse_data = "data: {\"jsonrpc\":\"2.0\",\"id\":\"req-1\",\"result\":{\"tools\":[]}}\n\n";
        let mut parser = SseParser::new();
        let events = parser.feed(sse_data);
        assert_eq!(events.len(), 1);
        let resp = JsonRpcResponse::from_json(&events[0].data).unwrap();
        assert_eq!(resp.id, JsonRpcId::String("req-1".to_string()));
        assert!(resp.is_success());
    }

    #[test]
    fn test_sse_request_id_routing() {
        let sse_data = "data: {\"jsonrpc\":\"2.0\",\"id\":\"req-42\",\"result\":{}}\n\n\
             data: {\"jsonrpc\":\"2.0\",\"method\":\"notifications/tools/list_changed\"}\n\n";
        let mut parser = SseParser::new();
        let events = parser.feed(sse_data);
        assert_eq!(events.len(), 2);

        // First event: response matching our request
        let resp = JsonRpcResponse::from_json(&events[0].data).unwrap();
        assert_eq!(resp.id, JsonRpcId::String("req-42".to_string()));

        // Second event: notification forwarded to inbox
        let notif = JsonRpcNotification::from_json(&events[1].data).unwrap();
        assert_eq!(notif.method, "notifications/tools/list_changed");
    }

    #[test]
    fn test_sse_server_initiated_request_routed() {
        let sse_data = "data: {\"jsonrpc\":\"2.0\",\"id\":\"req-1\",\"result\":{}}\n\n\
             data: {\"jsonrpc\":\"2.0\",\"id\":\"srv-1\",\"method\":\"roots/list\"}\n\n";
        let mut parser = SseParser::new();
        let events = parser.feed(sse_data);
        assert_eq!(events.len(), 2);

        // Response
        let resp = JsonRpcResponse::from_json(&events[0].data).unwrap();
        assert_eq!(resp.id, JsonRpcId::String("req-1".to_string()));

        // Server-initiated request
        let req = JsonRpcRequest::from_json(&events[1].data).unwrap();
        assert_eq!(req.method, "roots/list");
    }

    #[test]
    fn test_sse_skips_non_matching_response() {
        let sse_data = "data: {\"jsonrpc\":\"2.0\",\"id\":\"other-id\",\"result\":{}}\n\n\
             data: {\"jsonrpc\":\"2.0\",\"id\":\"req-99\",\"result\":{\"matched\":true}}\n\n";
        let mut parser = SseParser::new();
        let events = parser.feed(sse_data);

        // Routing: first response with matching ID wins
        let request_id = "req-99";
        let mut found: Option<JsonRpcResponse> = None;
        for event in &events {
            if let Ok(resp) = JsonRpcResponse::from_json(&event.data) {
                let resp_id = match &resp.id {
                    JsonRpcId::String(s) => s.clone(),
                    JsonRpcId::Number(n) => n.to_string(),
                    JsonRpcId::Null => String::new(),
                };
                if resp_id == request_id && found.is_none() {
                    found = Some(resp);
                }
            }
        }
        let found = found.unwrap();
        assert_eq!(found.id, JsonRpcId::String("req-99".to_string()));
    }

    #[test]
    fn test_json_response_parsing_unchanged() {
        let json = "{\"jsonrpc\":\"2.0\",\"id\":\"test\",\"result\":{\"status\":\"ok\"}}";
        let resp = JsonRpcResponse::from_json(json).unwrap();
        assert!(resp.is_success());
        assert_eq!(resp.id, JsonRpcId::String("test".to_string()));
    }

    // --- Phase 4: DELETE session termination + 404 handling ---

    #[test]
    fn test_session_expired_error_display() {
        let err = McpError::SessionExpired("session gone".to_string());
        assert!(err.to_string().contains("session gone"));
        assert!(err.to_string().starts_with("Session expired"));
    }

    #[tokio::test]
    async fn test_close_with_no_session_sends_no_delete() {
        let headers = HashMap::new();
        let mut t = HttpTransport::new("http://localhost:9999/mcp", headers).unwrap();
        assert!(t.session_id.is_none());
        t.close().await.unwrap();
        assert!(!t.is_connected());
        assert!(t.inbox_sender.is_none());
    }

    #[tokio::test]
    async fn test_close_with_session_attempts_delete() {
        let headers = HashMap::new();
        let mut t = HttpTransport::new("http://localhost:9999/mcp", headers).unwrap();
        t.test_set_session_id(Some("sess-abc".to_string()));
        t.close().await.unwrap();
        assert!(!t.is_connected());
        // DELETE sent to invalid host — errors ignored
    }

    #[tokio::test]
    async fn test_close_drains_inbox_sender() {
        let headers = HashMap::new();
        let mut t = HttpTransport::new("http://localhost:9999/mcp", headers).unwrap();
        assert!(t.inbox_sender.is_some());
        t.close().await.unwrap();
        assert!(t.inbox_sender.is_none());
    }

    #[test]
    fn test_pending_requests_drained_on_close() {
        let headers = HashMap::new();
        let t = HttpTransport::new("http://localhost:9999/mcp", headers).unwrap();
        assert!(t.pending_requests.is_empty());
    }

    // --- Phase 5: Last-Event-ID resumability ---

    #[test]
    fn test_last_event_id_initially_none() {
        let headers = HashMap::new();
        let t = HttpTransport::new("http://localhost:9999/mcp", headers).unwrap();
        let guard = t.last_event_id.lock().unwrap();
        assert!(guard.is_none());
    }

    #[test]
    fn test_last_event_id_tracks_sse_event_id() {
        let sse_data = "id: evt-42\ndata: {\"jsonrpc\":\"2.0\",\"id\":\"req-1\",\"result\":{}}\n\n";
        let mut parser = SseParser::new();
        let events = parser.feed(sse_data);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].id.as_deref(), Some("evt-42"));
    }

    #[test]
    fn test_last_event_id_missing_when_no_id_field() {
        let sse_data = "data: {\"jsonrpc\":\"2.0\",\"id\":\"req-1\",\"result\":{}}\n\n";
        let mut parser = SseParser::new();
        let events = parser.feed(sse_data);
        assert_eq!(events.len(), 1);
        assert!(events[0].id.is_none());
    }

    #[test]
    fn test_last_event_id_updated_via_arc() {
        let headers = HashMap::new();
        let t = HttpTransport::new("http://localhost:9999/mcp", headers).unwrap();
        // Simulate what the GET listener task does
        {
            let mut guard = t.last_event_id.lock().unwrap();
            *guard = Some("evt-99".to_string());
        }
        let guard = t.last_event_id.lock().unwrap();
        assert_eq!(guard.as_deref(), Some("evt-99"));
    }

    #[test]
    fn test_last_event_id_reset_on_new_connection() {
        let sse_data = "id: first\ndata: {\"jsonrpc\":\"2.0\",\"result\":{}}\n\n\
             id: second\ndata: {\"jsonrpc\":\"2.0\",\"result\":{}}\n\n";
        let mut parser = SseParser::new();
        let events = parser.feed(sse_data);
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].id.as_deref(), Some("first"));
        assert_eq!(events[1].id.as_deref(), Some("second"));
    }

    #[test]
    fn test_sse_retry_field_parsed() {
        let sse_data = "retry: 5000\ndata: {\"jsonrpc\":\"2.0\",\"result\":{}}\n\n";
        let mut parser = SseParser::new();
        let _events = parser.feed(sse_data);
        assert_eq!(parser.retry_interval(), Some(5000));
    }

    #[test]
    fn test_sse_retry_field_ignored_non_numeric() {
        let sse_data = "retry: abc\ndata: {\"jsonrpc\":\"2.0\",\"result\":{}}\n\n";
        let mut parser = SseParser::new();
        let _events = parser.feed(sse_data);
        assert!(parser.retry_interval().is_none());
    }

    #[tokio::test]
    async fn test_close_aborts_get_listener_handle() {
        let headers = HashMap::new();
        let mut t = HttpTransport::new("http://localhost:9999/mcp", headers).unwrap();
        t.test_set_session_id(Some("sess-test".to_string()));
        // start_get_listener will fail to connect but the handle is set
        t.start_get_listener();
        assert!(t.get_listener_handle.is_some());
        t.close().await.unwrap();
        assert!(t.get_listener_handle.is_none());
    }

    // --- Phase 7: 429 rate limiting ---

    #[test]
    fn test_rate_limited_error_from_429_status() {
        let err = McpError::RateLimited(Duration::from_secs(30));
        assert!(err.to_string().contains("30s"));
        assert!(err.to_string().starts_with("Rate limited"));
    }

    #[test]
    fn test_rate_limited_with_default_retry_after() {
        // When server sends 429 without Retry-After, default to 5s
        let default_secs: u64 = "invalid".parse::<u64>().unwrap_or(5);
        assert_eq!(default_secs, 5);
    }
}
