use crate::protocol::{JsonRpcNotification, JsonRpcRequest, JsonRpcResponse};
use crate::{McpError, McpResult};
use async_trait::async_trait;
use reqwest::Client;
use std::collections::HashMap;
use std::time::Duration;
use tokio::sync::{mpsc, oneshot};

use crate::transport::{IncomingMessage, Transport};

#[allow(dead_code)]
/// Simple HTTP transport for MCP using POST requests.
pub struct HttpTransport {
    client: Client,
    url: String,
    headers: HashMap<String, String>,
    session_id: Option<String>,
    connected: bool,
    pending_requests: HashMap<String, oneshot::Sender<JsonRpcResponse>>, // map of id -> responder
    inbox: mpsc::Receiver<IncomingMessage>,
    // Optional sender to push into inbox from internal listeners (not required for tests)
    inbox_sender: Option<mpsc::Sender<IncomingMessage>>,
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
        })
    }

    /// Internal helper to set session id (test visibility only)
    #[cfg(test)]
    pub fn test_set_session_id(&mut self, sid: Option<String>) {
        self.session_id = sid;
    }
}

#[async_trait]
impl Transport for HttpTransport {
    async fn send_request(&mut self, request: JsonRpcRequest) -> McpResult<JsonRpcResponse> {
        // Serialize request to JSON
        let json = request
            .to_json()
            .map_err(|e| McpError::ProtocolError(format!("Serialize error: {e}")))?;

        // Build request with headers and optional session id
        let mut req = self
            .client
            .post(&self.url)
            .header("Content-Type", "application/json")
            .header("MCP-Protocol-Version", crate::MCP_VERSION);

        for (k, v) in &self.headers {
            req = req.header(k.as_str(), v.as_str());
        }
        if let Some(ref sid) = self.session_id {
            req = req.header("Mcp-Session-Id", sid.as_str());
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
        if status.is_client_error() || status.is_server_error() {
            let body = resp.text().await.unwrap_or_default();
            return Err(McpError::TransportError(format!(
                "HTTP {} response: {}",
                status,
                body.chars().take(200).collect::<String>()
            )));
        }

        if let Some(val) = resp.headers().get("Mcp-Session-Id") {
            if let Ok(s) = val.to_str() {
                self.session_id = Some(s.to_string());
            }
        }

        // Infer content type
        if let Some(ct) = resp.headers().get(reqwest::header::CONTENT_TYPE) {
            if let Ok(ct_str) = ct.to_str() {
                if ct_str.contains("application/json") {
                    let text = resp
                        .text()
                        .await
                        .map_err(|e| McpError::TransportError(format!("HTTP read error: {e}")))?;
                    let json_resp = JsonRpcResponse::from_json(&text)
                        .map_err(|e| McpError::ProtocolError(format!("Invalid JSON: {e}")))?;
                    return Ok(json_resp);
                } else if ct_str.contains("text/event-stream") {
                    let text = resp.text().await.map_err(|e| {
                        McpError::TransportError(format!("BOM SSE read error: {e}"))
                    })?;
                    // Try to parse as a single JSON-RPC response
                    let json_resp = JsonRpcResponse::from_json(&text)
                        .map_err(|e| McpError::ProtocolError(format!("Invalid JSON: {e}")))?;
                    return Ok(json_resp);
                }
            }
        }

        Err(McpError::TransportError(
            "Unsupported response content-type".to_string(),
        ))
    }

    async fn send_notification(&mut self, notification: JsonRpcNotification) -> McpResult<()> {
        let json = notification
            .to_json()
            .map_err(|e| McpError::ProtocolError(format!("Serialize error: {e}")))?;

        let mut req = self
            .client
            .post(&self.url)
            .header("Content-Type", "application/json")
            .header("MCP-Protocol-Version", crate::MCP_VERSION);
        for (k, v) in &self.headers {
            req = req.header(k.as_str(), v.as_str());
        }
        if let Some(ref sid) = self.session_id {
            req = req.header("Mcp-Session-Id", sid.as_str());
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

    fn is_connected(&self) -> bool {
        self.connected
    }

    async fn close(&mut self) -> McpResult<()> {
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
}
