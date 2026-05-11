//! JSON-RPC response handling for LSP communication
//!
//! Provides async response reading and parsing from LSP servers

use lsp_types::*;
use serde_json::Value as JsonValue;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, BufReader};
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};

/// LSP server response
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum LspResponse {
    /// Response to a request
    Response {
        id: i64,
        result: Option<JsonValue>,
        error: Option<JsonValue>,
    },
    /// Request from server to client
    Request {
        id: i64,
        method: String,
        params: JsonValue,
    },
    /// Notification from the server
    Notification { method: String, params: JsonValue },
}

impl LspResponse {
    /// Parse JSON into an [`LspResponse`]
    pub fn from_json(json: JsonValue) -> Self {
        if let Some(method) = json.get("method").and_then(JsonValue::as_str) {
            // This has a method field
            let params = json.get("params").cloned().unwrap_or(JsonValue::Null);
            if let Some(id) = json.get("id").and_then(JsonValue::as_i64) {
                // This is a request from server to client
                Self::Request {
                    id,
                    method: method.to_string(),
                    params,
                }
            } else {
                // This is a notification
                Self::Notification {
                    method: method.to_string(),
                    params,
                }
            }
        } else if let Some(id) = json.get("id").and_then(JsonValue::as_i64) {
            // This is a response (has id but no method)
            let result = json.get("result").cloned();
            let error = json.get("error").cloned();
            Self::Response { id, result, error }
        } else {
            warn!("Received invalid JSON-RPC message: {json:?}");
            Self::Notification {
                method: "unknown".to_string(),
                params: json,
            }
        }
    }
}

/// Parsed LSP notification with specific types
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum LspNotification {
    PublishDiagnostics(PublishDiagnosticsParams),
    /// Add more notification types as needed
    Unknown(String, JsonValue),
}

/// Maximum allowed Content-Length for a single LSP message (100 MB).
///
/// Prevents OOM from malicious or broken LSP servers sending absurdly large headers.
const MAX_CONTENT_LENGTH: usize = 100 * 1024 * 1024;

/// Response reader for LSP server communication
pub struct LspResponseReader {
    response_tx: mpsc::Sender<LspResponse>,
}

impl LspResponseReader {
    pub fn new() -> (Self, mpsc::Receiver<LspResponse>) {
        let (response_tx, response_rx) = mpsc::channel(64);
        let reader = Self { response_tx };
        (reader, response_rx)
    }

    /// Spawn a background task to read responses from the LSP server
    pub fn spawn_reader(&self, stdout: tokio::process::ChildStdout) -> tokio::task::JoinHandle<()> {
        let tx = self.response_tx.clone();

        tokio::spawn(async move {
            let mut reader = BufReader::new(stdout);
            let mut content_length: Option<usize> = None;
            let mut headers_complete = false;

            loop {
                let mut line = String::new();

                match reader.read_line(&mut line).await {
                    Ok(0) => {
                        info!("LSP server closed stdout");
                        break;
                    }
                    Ok(_) => {
                        if line.trim().is_empty() {
                            headers_complete = true;
                        } else if let Some(len_str) = line.strip_prefix("Content-Length:") {
                            let len_str = len_str.trim();
                            if let Ok(len) = len_str.parse::<usize>() {
                                if len <= MAX_CONTENT_LENGTH {
                                    content_length = Some(len);
                                } else {
                                    warn!(
                                        len,
                                        max = MAX_CONTENT_LENGTH,
                                        "LSP Content-Length exceeds limit, skipping message"
                                    );
                                }
                            }
                        }
                    }
                    Err(e) => {
                        error!("Error reading from LSP server: {}", e);
                        break;
                    }
                }

                if headers_complete {
                    if let Some(len) = content_length {
                        let mut body = vec![0u8; len];
                        match reader.read_exact(&mut body).await {
                            Ok(_) => {
                                if let Ok(response_str) = String::from_utf8(body) {
                                    debug!(response = %response_str, "Raw response from LSP");

                                    if let Ok(json) =
                                        serde_json::from_str::<JsonValue>(&response_str)
                                    {
                                        let response = LspResponse::from_json(json.clone());
                                        match &response {
                                            LspResponse::Response { id, .. } => {
                                                debug!(id = id, "Parsed response");
                                            }
                                            LspResponse::Notification { method, .. } => {
                                                debug!(method = %method, "Parsed notification");
                                            }
                                            LspResponse::Request { id, method, .. } => {
                                                debug!(id = id, method = %method, "Parsed request from server");
                                            }
                                        }
                                        if let Err(e) = tx.send(response).await {
                                            warn!(error = %e, "Failed to send LSP response");
                                            break;
                                        }
                                    } else {
                                        warn!(response = %response_str, "Failed to parse JSON from response");
                                    }
                                }
                            }
                            Err(e) => {
                                error!("Error reading LSP response body: {}", e);
                                break;
                            }
                        }
                    }

                    // Reset for next message
                    content_length = None;
                    headers_complete = false;
                }
            }

            info!("LSP response reader task ending");
        })
    }
}

impl LspResponse {
    /// Parse a notification into a specific type
    pub fn parse_notification(&self) -> Option<LspNotification> {
        match self {
            Self::Notification { method, params } => match method.as_str() {
                "textDocument/publishDiagnostics" => {
                    serde_json::from_value::<PublishDiagnosticsParams>(params.clone()).map_or_else(
                        |_| {
                            warn!("Failed to parse PublishDiagnosticsParams");
                            Some(LspNotification::Unknown(method.clone(), params.clone()))
                        },
                        |diagnostics| Some(LspNotification::PublishDiagnostics(diagnostics)),
                    )
                }
                _ => Some(LspNotification::Unknown(method.clone(), params.clone())),
            },
            Self::Request { .. } => {
                // Server requests are not notifications
                None
            }
            Self::Response { .. } => {
                // Responses are not notifications
                None
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_response_from_json() {
        let json = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "result": {
                "capabilities": {}
            }
        });

        let response = LspResponse::from_json(json);
        assert!(matches!(response, LspResponse::Response { id: 1, .. }));
    }

    #[test]
    fn test_notification_from_json() {
        let json = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "textDocument/publishDiagnostics",
            "params": {
                "uri": "file:///test.rs",
                "diagnostics": []
            }
        });

        let response = LspResponse::from_json(json);
        match response {
            LspResponse::Notification { method, .. } => {
                assert_eq!(method, "textDocument/publishDiagnostics");
            }
            _ => panic!("Expected notification"),
        }
    }

    #[test]
    fn test_parse_diagnostics_notification() {
        let params = serde_json::json!({
            "uri": "file:///test.rs",
            "diagnostics": []
        });

        let response = LspResponse::Notification {
            method: "textDocument/publishDiagnostics".to_string(),
            params,
        };

        if let Some(LspNotification::PublishDiagnostics(diagnostics)) =
            response.parse_notification()
        {
            assert_eq!(diagnostics.uri.to_string(), "file:///test.rs");
            assert!(diagnostics.diagnostics.is_empty());
        } else {
            panic!("Failed to parse diagnostics notification");
        }
    }

    #[test]
    fn test_request_from_json() {
        let json = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 42,
            "method": "window/showMessageRequest",
            "params": {"actions": []}
        });

        let response = LspResponse::from_json(json);
        match response {
            LspResponse::Request { id, method, .. } => {
                assert_eq!(id, 42);
                assert_eq!(method, "window/showMessageRequest");
            }
            _ => panic!("Expected request"),
        }
    }

    #[test]
    fn test_unknown_notification_parse() {
        let response = LspResponse::Notification {
            method: "custom/notification".to_string(),
            params: serde_json::json!({"key": "value"}),
        };

        match response.parse_notification() {
            Some(LspNotification::Unknown(method, params)) => {
                assert_eq!(method, "custom/notification");
                assert_eq!(params["key"], "value");
            }
            _ => panic!("Expected Unknown notification"),
        }
    }

    #[test]
    fn test_response_not_notification() {
        let response = LspResponse::Response {
            id: 1,
            result: Some(serde_json::json!({"ok": true})),
            error: None,
        };
        assert!(response.parse_notification().is_none());
    }

    #[test]
    fn test_request_not_notification() {
        let response = LspResponse::Request {
            id: 1,
            method: "test".to_string(),
            params: serde_json::Value::Null,
        };
        assert!(response.parse_notification().is_none());
    }

    #[test]
    fn test_response_with_error() {
        let json = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 5,
            "error": {"code": -32600, "message": "Invalid Request"}
        });

        let response = LspResponse::from_json(json);
        match response {
            LspResponse::Response { id, result, error } => {
                assert_eq!(id, 5);
                assert!(result.is_none());
                assert!(error.is_some());
            }
            _ => panic!("Expected response"),
        }
    }

    #[test]
    fn test_response_reader_new() {
        let (reader, rx) = LspResponseReader::new();
        drop(reader);
        drop(rx);
    }

    #[test]
    fn test_invalid_json_falls_back_to_notification() {
        let json = serde_json::json!("just a string");
        let response = LspResponse::from_json(json);
        match response {
            LspResponse::Notification { method, .. } => {
                assert_eq!(method, "unknown");
            }
            _ => panic!("Expected unknown notification for invalid JSON-RPC"),
        }
    }

    #[test]
    fn test_diagnostics_notification_with_errors() {
        let params = serde_json::json!({
            "uri": "file:///broken.rs",
            "diagnostics": [
                {
                    "range": {
                        "start": {"line": 0, "character": 0},
                        "end": {"line": 0, "character": 5}
                    },
                    "severity": 1,
                    "message": "expected `;`"
                }
            ]
        });

        let response = LspResponse::Notification {
            method: "textDocument/publishDiagnostics".to_string(),
            params,
        };

        if let Some(LspNotification::PublishDiagnostics(diagnostics)) =
            response.parse_notification()
        {
            assert_eq!(diagnostics.diagnostics.len(), 1);
            assert_eq!(diagnostics.diagnostics[0].message, "expected `;`");
        } else {
            panic!("Failed to parse diagnostics with errors");
        }
    }

    #[test]
    fn test_notification_with_null_params() {
        let json = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "some/notification",
            "params": null
        });
        let response = LspResponse::from_json(json);
        match response {
            LspResponse::Notification { method, params } => {
                assert_eq!(method, "some/notification");
                assert!(params.is_null());
            }
            _ => panic!("Expected notification"),
        }
    }

    #[test]
    fn test_notification_missing_params_defaults_to_null() {
        let json = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "event/noParams"
        });
        let response = LspResponse::from_json(json);
        match response {
            LspResponse::Notification { method, params } => {
                assert_eq!(method, "event/noParams");
                assert!(params.is_null());
            }
            _ => panic!("Expected notification"),
        }
    }

    #[test]
    fn test_response_with_both_result_and_error() {
        // JSON-RPC spec says one or the other, but we should handle both present
        let json = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 10,
            "result": {"data": 42},
            "error": {"code": -32603, "message": "Internal error"}
        });
        let response = LspResponse::from_json(json);
        match response {
            LspResponse::Response { id, result, error } => {
                assert_eq!(id, 10);
                assert!(result.is_some());
                assert!(error.is_some());
            }
            _ => panic!("Expected response"),
        }
    }

    #[test]
    fn test_response_with_null_result() {
        let json = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 7,
            "result": null
        });
        let response = LspResponse::from_json(json);
        match response {
            LspResponse::Response { id, result, error } => {
                assert_eq!(id, 7);
                // result is present but null in the JSON; cloned Option<JsonValue>
                assert!(result.is_some());
                assert!(error.is_none());
            }
            _ => panic!("Expected response"),
        }
    }

    #[test]
    fn test_request_with_complex_params() {
        let json = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 99,
            "method": "workspace/applyEdit",
            "params": {
                "label": "Refactor",
                "edit": {
                    "changes": {}
                }
            }
        });
        let response = LspResponse::from_json(json);
        match response {
            LspResponse::Request { id, method, params } => {
                assert_eq!(id, 99);
                assert_eq!(method, "workspace/applyEdit");
                assert!(params["label"] == "Refactor");
            }
            _ => panic!("Expected request"),
        }
    }

    #[test]
    #[allow(clippy::redundant_clone)]
    fn test_lsp_response_clone() {
        let response = LspResponse::Response {
            id: 1,
            result: Some(serde_json::json!({"ok": true})),
            error: None,
        };
        // Verify Clone trait compiles and produces matching data
        let cloned = response.clone();
        assert!(matches!(cloned, LspResponse::Response { id: 1, .. }));
    }

    #[test]
    #[allow(clippy::redundant_clone)]
    fn test_lsp_notification_clone() {
        let notif = LspNotification::Unknown("test".to_string(), serde_json::json!({}));
        // Verify Clone trait compiles and produces matching data
        let cloned = notif.clone();
        match cloned {
            LspNotification::Unknown(method, _) => assert_eq!(method, "test"),
            _ => panic!("Expected Unknown"),
        }
    }

    #[test]
    fn test_parse_diagnostics_with_invalid_params() {
        // Pass invalid params for PublishDiagnostics
        let response = LspResponse::Notification {
            method: "textDocument/publishDiagnostics".to_string(),
            params: serde_json::json!("not an object"),
        };
        // Should fall back to Unknown since parsing fails
        match response.parse_notification() {
            Some(LspNotification::Unknown(method, _)) => {
                assert_eq!(method, "textDocument/publishDiagnostics");
            }
            _ => panic!("Expected Unknown fallback for invalid diagnostics params"),
        }
    }

    #[test]
    fn test_response_with_negative_id() {
        let json = serde_json::json!({
            "jsonrpc": "2.0",
            "id": -1,
            "result": null
        });
        let response = LspResponse::from_json(json);
        match response {
            LspResponse::Response { id, .. } => assert_eq!(id, -1),
            _ => panic!("Expected response"),
        }
    }

    #[test]
    fn test_from_json_with_numeric_but_string_id() {
        // id as string should not match (per JSON-RPC spec it must be number or string)
        let json = serde_json::json!({
            "jsonrpc": "2.0",
            "id": "string-id",
            "result": null
        });
        // as_i64() returns None for string, so this falls through to unknown notification
        let response = LspResponse::from_json(json);
        match response {
            LspResponse::Notification { method, .. } => assert_eq!(method, "unknown"),
            _ => panic!("Expected unknown notification fallback"),
        }
    }
}
