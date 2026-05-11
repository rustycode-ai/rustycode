use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

/// A single GA4 analytics event.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalyticsEvent {
    pub name: String,
    pub params: HashMap<String, serde_json::Value>,
}

impl AnalyticsEvent {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            params: HashMap::new(),
        }
    }

    pub fn param(mut self, key: impl Into<String>, value: impl Into<serde_json::Value>) -> Self {
        self.params.insert(key.into(), value.into());
        self
    }

    pub fn param_str(self, key: impl Into<String>, value: impl AsRef<str>) -> Self {
        self.param(key, value.as_ref().to_string())
    }

    pub fn param_u64(self, key: impl Into<String>, value: u64) -> Self {
        self.param(key, value)
    }

    pub fn param_bool(self, key: impl Into<String>, value: bool) -> Self {
        self.param(key, value)
    }
}

/// Common context attached to every event.
#[derive(Debug, Clone)]
pub struct EventContext {
    pub app_version: String,
    pub provider: String,
    pub model: String,
    pub os: String,
    pub session_id: String,
    pub install_id: String,
}

impl EventContext {
    pub fn new(install_id: String, session_id: String) -> Self {
        Self {
            app_version: env!("CARGO_PKG_VERSION").to_string(),
            provider: String::new(),
            model: String::new(),
            os: std::env::consts::OS.to_string(),
            session_id,
            install_id,
        }
    }

    /// Attach common context params to an event.
    pub fn enrich(&self, event: AnalyticsEvent) -> AnalyticsEvent {
        event
            .param_str("app_version", &self.app_version)
            .param_str("provider", &self.provider)
            .param_str("model", &self.model)
            .param_str("os", &self.os)
            .param_str("session_id", &self.session_id)
            .param_str("install_id", &self.install_id)
    }
}

/// Payload sent to GA4 Measurement Protocol.
#[derive(Debug, Serialize)]
pub struct Ga4Payload {
    pub client_id: String,
    pub timestamp_micros: u64,
    pub non_personalized_ads: bool,
    pub events: Vec<AnalyticsEvent>,
}

impl Ga4Payload {
    pub fn new(client_id: String, events: Vec<AnalyticsEvent>) -> Self {
        let micros = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_micros() as u64);

        Self {
            client_id,
            timestamp_micros: micros,
            non_personalized_ads: true,
            events,
        }
    }
}

// ── Event constructors ──────────────────────────────────────────────

/// App launched.
pub fn app_start(mode: &str, args_count: usize) -> AnalyticsEvent {
    AnalyticsEvent::new("app_start")
        .param_str("mode", mode)
        .param_u64("args_count", args_count as u64)
}

/// Session created.
pub fn session_start() -> AnalyticsEvent {
    AnalyticsEvent::new("session_start")
}

/// Session ended.
pub fn session_end(
    duration_secs: f64,
    message_count: u64,
    tool_call_count: u64,
    total_tokens: u64,
) -> AnalyticsEvent {
    AnalyticsEvent::new("session_end")
        .param("duration_secs", duration_secs)
        .param_u64("message_count", message_count)
        .param_u64("tool_call_count", tool_call_count)
        .param_u64("total_tokens", total_tokens)
}

/// LLM API request made.
pub fn llm_request(model: &str, provider: &str, streaming: bool) -> AnalyticsEvent {
    AnalyticsEvent::new("llm_request")
        .param_str("model", model)
        .param_str("provider", provider)
        .param_bool("streaming", streaming)
}

/// LLM API error occurred.
pub fn llm_error(error_type: &str, status_code: Option<u16>, provider: &str) -> AnalyticsEvent {
    let mut ev = AnalyticsEvent::new("llm_error")
        .param_str("error_type", error_type)
        .param_str("provider", provider);
    if let Some(code) = status_code {
        ev = ev.param_u64("status_code", code as u64);
    }
    ev
}

/// Tool executed.
pub fn tool_use(tool_name: &str, success: bool, duration_ms: u64) -> AnalyticsEvent {
    AnalyticsEvent::new("tool_use")
        .param_str("tool_name", tool_name)
        .param_bool("success", success)
        .param_u64("duration_ms", duration_ms)
}

/// Tool execution error.
pub fn tool_error(tool_name: &str, error_type: &str) -> AnalyticsEvent {
    AnalyticsEvent::new("tool_error")
        .param_str("tool_name", tool_name)
        .param_str("error_type", error_type)
}

/// Unhandled/critical application error.
pub fn app_error(error_type: &str, error_message: &str) -> AnalyticsEvent {
    // Truncate error message to avoid sending large payloads
    let truncated = if error_message.len() > 200 {
        &error_message[..200]
    } else {
        error_message
    };
    AnalyticsEvent::new("app_error")
        .param_str("error_type", error_type)
        .param_str("error_message", truncated)
}

/// User switched mode (cli/tui/autonomous).
pub fn mode_switch(from: &str, to: &str) -> AnalyticsEvent {
    AnalyticsEvent::new("mode_switch")
        .param_str("from_mode", from)
        .param_str("to_mode", to)
}

/// Context compaction occurred.
pub fn compaction(messages_before: usize, messages_after: usize) -> AnalyticsEvent {
    AnalyticsEvent::new("compaction")
        .param_u64("messages_before", messages_before as u64)
        .param_u64("messages_after", messages_after as u64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_builder() {
        let ev = AnalyticsEvent::new("test_event")
            .param_str("key", "value")
            .param_u64("count", 42)
            .param_bool("flag", true);

        assert_eq!(ev.name, "test_event");
        assert_eq!(ev.params.len(), 3);
    }

    #[test]
    fn enrich_adds_context() {
        let ctx = EventContext::new("install-123".into(), "sess-456".into());
        let ev = ctx.enrich(AnalyticsEvent::new("test"));

        assert_eq!(ev.params.get("install_id").unwrap(), "install-123");
        assert_eq!(ev.params.get("session_id").unwrap(), "sess-456");
        assert!(ev.params.contains_key("app_version"));
        assert!(ev.params.contains_key("os"));
    }

    #[test]
    fn ga4_payload_serializes() {
        let events = vec![AnalyticsEvent::new("test")];
        let payload = Ga4Payload::new("client-1".into(), events);
        let json = serde_json::to_string(&payload).unwrap();
        assert!(json.contains("non_personalized_ads"));
        assert!(json.contains("client-1"));
    }

    #[test]
    fn all_event_constructors() {
        let _ = app_start("cli", 3);
        let _ = session_start();
        let _ = session_end(60.0, 10, 5, 1000);
        let _ = llm_request("claude-sonnet-4-6", "anthropic", true);
        let _ = llm_error("timeout", Some(429), "anthropic");
        let _ = tool_use("bash", true, 150);
        let _ = tool_error("bash", "permission_denied");
        let _ = app_error("panic", "index out of bounds");
        let _ = mode_switch("cli", "tui");
        let _ = compaction(100, 20);
    }

    #[test]
    fn app_error_truncates_long_message() {
        let long_msg = "x".repeat(500);
        let ev = app_error("test", &long_msg);
        let msg = ev.params.get("error_message").unwrap().as_str().unwrap();
        assert_eq!(msg.len(), 200);
    }
}
