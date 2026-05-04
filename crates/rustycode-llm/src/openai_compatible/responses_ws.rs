//! WebSocket transport for the OpenAI Responses API.
//!
//! The Responses API can be accessed via WebSocket for persistent connections.
//! Event payloads are the same JSON shapes as SSE — this module parses raw
//! WebSocket text messages using the shared `dispatch_responses_event` logic.

use std::pin::Pin;

use futures::{SinkExt, Stream, StreamExt};
use rustycode_protocol::stream_event::StreamEvent;
use tokio_tungstenite::{connect_async, tungstenite::Message};

use crate::openai_compatible::{dispatch_responses_event, extract_responses_error, ResponsesSseState};
use crate::provider::{ProviderError, StreamChunk};

/// Parse a single WebSocket text message into stream events.
///
/// WebSocket delivers complete JSON objects (no SSE `event:/data:` framing),
/// so we extract the `type` field and dispatch directly.
fn parse_ws_message(
    msg: &str,
    state: &ResponsesSseState,
) -> Vec<StreamChunk> {
    let data: serde_json::Value = match serde_json::from_str(msg) {
        Ok(d) => d,
        Err(_) => return Vec::new(),
    };

    let evt_type = match data.get("type").and_then(|t| t.as_str()) {
        Some(t) => t,
        None => {
            if let Some(err) = extract_responses_error(&data) {
                return vec![err];
            }
            return Vec::new();
        }
    };

    dispatch_responses_event(&data, evt_type, state)
}

/// Stream a Responses API request over WebSocket.
///
/// Connects to the given WebSocket URL, sends the request as a JSON message,
/// and yields `StreamChunk` values as events arrive.
pub async fn stream_responses_ws(
    ws_url: &str,
    api_key: &str,
    request_body: serde_json::Value,
) -> Result<Pin<Box<dyn Stream<Item = StreamChunk> + Send>>, ProviderError> {
    use tokio_tungstenite::tungstenite::client::IntoClientRequest;

    let mut req = ws_url
        .into_client_request()
        .map_err(|e| ProviderError::Network(format!("invalid WS URL: {e}")))?;
    req.headers_mut().insert(
        "Authorization",
        format!("Bearer {api_key}")
            .parse()
            .map_err(|e| ProviderError::Network(format!("invalid auth header: {e}")))?,
    );
    req.headers_mut().insert(
        "OpenAI-Beta",
        "responses=v1"
            .parse()
            .map_err(|e| ProviderError::Network(format!("invalid header: {e}")))?,
    );

    let (ws_stream, _response) = connect_async(req)
        .await
        .map_err(|e| ProviderError::Network(format!("WS connect failed: {e}")))?;

    let (mut write, read) = ws_stream.split();

    let payload = serde_json::to_string(&request_body)
        .map_err(|e| ProviderError::Serialization(e.to_string()))?;
    write
        .send(Message::text(payload))
        .await
        .map_err(|e| ProviderError::Network(format!("WS send failed: {e}")))?;

    let state = ResponsesSseState::default();

    let stream = read.flat_map(move |msg_result| {
        let state = state.clone();
        let events = match msg_result {
            Ok(Message::Text(text)) => parse_ws_message(text.as_ref(), &state),
            Ok(Message::Close(_)) => vec![Ok(StreamEvent::Done)],
            Err(e) => vec![Err(ProviderError::Network(format!("WS read error: {e}")))],
            _ => Vec::new(),
        };
        futures::stream::iter(events)
    });

    Ok(Box::pin(stream))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustycode_protocol::stream_event::StreamEvent;

    #[test]
    fn ws_parse_text_delta() {
        let msg = r#"{"type":"response.output_text.delta","delta":"Hello"}"#;
        let state = ResponsesSseState::default();
        let events = parse_ws_message(msg, &state);
        assert_eq!(events.len(), 1);
        match &events[0] {
            Ok(StreamEvent::TextDelta { content }) => assert_eq!(content, "Hello"),
            other => panic!("expected TextDelta, got {:?}", other),
        }
    }

    #[test]
    fn ws_parse_tool_call_started() {
        let msg = r#"{"type":"response.output_item.added","item":{"type":"function_call","id":"fc_1","call_id":"call_1","name":"read_file"}}"#;
        let state = ResponsesSseState::default();
        let events = parse_ws_message(msg, &state);
        assert_eq!(events.len(), 1);
        match &events[0] {
            Ok(StreamEvent::ToolCallStarted { id, name }) => {
                assert_eq!(id, "call_1");
                assert_eq!(name, "read_file");
            }
            other => panic!("expected ToolCallStarted, got {:?}", other),
        }
    }

    #[test]
    fn ws_parse_response_completed() {
        let msg = r#"{"type":"response.completed","response":{"id":"resp_1","status":"completed","usage":{"input_tokens":10,"output_tokens":5}}}"#;
        let state = ResponsesSseState::default();
        let events = parse_ws_message(msg, &state);
        assert_eq!(events.len(), 2); // TokenUsage + TurnCompleted
        assert!(matches!(&events[0], Ok(StreamEvent::TokenUsage { .. })));
        assert!(matches!(&events[1], Ok(StreamEvent::TurnCompleted { .. })));
    }

    #[test]
    fn ws_parse_error() {
        let msg = r#"{"type":"error","code":"rate_limit","message":"Too many requests"}"#;
        let state = ResponsesSseState::default();
        let events = parse_ws_message(msg, &state);
        assert_eq!(events.len(), 1);
        match &events[0] {
            Err(ProviderError::Api(msg)) => assert!(msg.contains("rate_limit")),
            other => panic!("expected Api error, got {:?}", other),
        }
    }

    #[test]
    fn ws_parse_invalid_json_returns_empty() {
        let msg = "not json";
        let state = ResponsesSseState::default();
        let events = parse_ws_message(msg, &state);
        assert!(events.is_empty());
    }

    #[test]
    fn ws_parse_unknown_type_returns_empty() {
        let msg = r#"{"type":"response.created","response":{"id":"resp_1"}}"#;
        let state = ResponsesSseState::default();
        let events = parse_ws_message(msg, &state);
        assert!(events.is_empty()); // informational event, no output
    }
}
