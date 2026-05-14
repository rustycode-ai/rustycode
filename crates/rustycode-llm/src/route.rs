//! Route: Protocol + Transport + AuthMethod + Endpoint.
//!
//! A Route is a complete request pipeline. Providers compose one or more Routes.

use anyhow::Result;
use futures::{Stream, StreamExt};
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use serde_json::Value;
use std::pin::Pin;
use std::sync::atomic::Ordering;

use crate::auth::AuthMethod;
use crate::schema::normalizer::WireFormat;
use crate::schema::tool_schema::ToolSchema;
use crate::transport::Transport;
use crate::types::request::CompletionRequest;
use crate::types::response::CompletionResponse;
use crate::types::streaming::StreamEvent;
use crate::wire::Protocol;

/// Type alias for boxed stream of SSE events.
pub type EventStream = Pin<Box<dyn Stream<Item = Result<StreamEvent>> + Send>>;

/// Per-provider extension options.
#[derive(Debug, Clone)]
pub enum ProviderOptions {
    Anthropic(AnthropicOptions),
    OpenAI(OpenAIOptions),
    Gemini(GeminiOptions),
    Bedrock(BedrockOptions),
    Ollama(OllamaOptions),
    OpenRouter(OpenRouterOptions),
    Azure(AzureOptions),
    HttpOverrides(HttpOverrides),
}

#[derive(Debug, Clone, Default)]
pub struct AnthropicOptions {
    pub cache_control: bool,
    pub beta_features: Vec<String>,
    pub thinking_budget: Option<u32>,
    pub defer_tool_loading: bool,
}

#[derive(Debug, Clone, Default)]
pub struct OpenAIOptions {
    pub api_preference: Option<crate::types::message::ApiMode>,
    pub reasoning_effort: Option<crate::types::config::EffortLevel>,
}

#[derive(Debug, Clone, Default)]
pub struct GeminiOptions {
    pub grounding: bool,
    pub thinking_budget: Option<u32>,
}

#[derive(Debug, Clone, Default)]
pub struct BedrockOptions {
    pub region: Option<String>,
    pub model_prefix: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct OllamaOptions {
    pub keep_alive: Option<u64>,
    pub strip_tools: bool,
}

#[derive(Debug, Clone, Default)]
pub struct OpenRouterOptions {
    pub max_tools: Option<usize>,
}

#[derive(Debug, Clone, Default)]
pub struct AzureOptions {
    pub api_version: Option<String>,
    pub deployment_name: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct HttpOverrides {
    pub extra_headers: Vec<(String, String)>,
    pub query_params: Vec<(String, String)>,
}

/// A complete request pipeline: wire format + delivery + auth + URL.
pub struct Route {
    /// Where to send requests.
    pub endpoint: String,
    /// How to serialize/deserialize messages.
    pub protocol: Box<dyn Protocol>,
    /// How to deliver requests.
    pub transport: Box<dyn Transport>,
    /// How to authenticate.
    pub auth: Box<dyn AuthMethod>,
    /// Provider-specific options consulted during serialization.
    pub options: Option<ProviderOptions>,
    /// Extra HTTP headers.
    pub extra_headers: Vec<(String, String)>,
    /// Tracking for LeastLoaded strategy.
    pub(crate) in_flight: std::sync::atomic::AtomicUsize,
    /// Human-readable name for selection/logging.
    pub(crate) name: String,
}

impl Route {
    pub fn builder() -> RouteBuilder {
        RouteBuilder::default()
    }

    pub fn new(
        endpoint: String,
        protocol: Box<dyn Protocol>,
        transport: Box<dyn Transport>,
        auth: Box<dyn AuthMethod>,
    ) -> Self {
        Self {
            endpoint,
            protocol,
            transport,
            auth,
            options: None,
            extra_headers: Vec::new(),
            in_flight: std::sync::atomic::AtomicUsize::new(0),
            name: "default".to_string(),
        }
    }

    /// Set a name for this route.
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = name.into();
        self
    }

    /// Add extra headers to this route.
    pub fn with_extra_headers(mut self, headers: Vec<(String, String)>) -> Self {
        self.extra_headers.extend(headers);
        self
    }

    /// Return the current number of in-flight requests.
    pub fn in_flight(&self) -> usize {
        self.in_flight.load(std::sync::atomic::Ordering::Relaxed)
    }

    /// Return the name of this route.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Create a minimal route for testing selection strategies.
    #[cfg(test)]
    pub fn for_test(name: impl Into<String>) -> Self {
        use crate::auth::NoAuth;
        use crate::transport::HttpTransport;
        use crate::wire::openai_chat::OpenAIChatProtocol;
        Self::new(
            "http://localhost".to_string(),
            Box::new(OpenAIChatProtocol),
            Box::new(HttpTransport::new(30).unwrap()),
            Box::new(NoAuth),
        )
        .with_name(name)
    }

    /// Serialize a request using this route's protocol.
    pub fn serialize_body(
        &self,
        request: &CompletionRequest,
        tools: Option<&[ToolSchema]>,
    ) -> Result<Value> {
        self.protocol.serialize_body(request, tools)
    }

    /// Return the wire format for this route.
    pub fn wire_format(&self) -> WireFormat {
        self.protocol.format()
    }

    /// Execute a non-streaming request.
    pub async fn execute(
        &self,
        request: &CompletionRequest,
        tools: Option<&[ToolSchema]>,
    ) -> Result<CompletionResponse> {
        self.in_flight.fetch_add(1, Ordering::Relaxed);
        let result = self.execute_internal(request, tools).await;
        self.in_flight.fetch_sub(1, Ordering::Relaxed);
        result
    }

    async fn execute_internal(
        &self,
        request: &CompletionRequest,
        tools: Option<&[ToolSchema]>,
    ) -> Result<CompletionResponse> {
        let body = self.serialize_body(request, tools)?;
        let mut headers = HeaderMap::new();
        // Add static extra headers from route config
        for (k, v) in &self.extra_headers {
            headers.insert(
                HeaderName::from_bytes(k.as_bytes())?,
                HeaderValue::from_str(v)?,
            );
        }
        // Add dynamic extra headers from protocol
        for (k, v) in self.protocol.extra_headers(request) {
            headers.append(
                HeaderName::from_bytes(k.as_bytes())?,
                HeaderValue::from_str(&v)?,
            );
        }
        self.auth.apply(&mut headers).await?;

        let response_value = self.transport.send(&self.endpoint, body, headers).await?;
        self.protocol.parse_response(&response_value)
    }

    /// Execute a streaming request.
    pub async fn execute_stream(
        &self,
        request: &CompletionRequest,
        tools: Option<&[ToolSchema]>,
    ) -> Result<EventStream> {
        let body = self.serialize_body(request, tools)?;
        let mut headers = HeaderMap::new();
        // Add static extra headers from route config
        for (k, v) in &self.extra_headers {
            headers.insert(
                HeaderName::from_bytes(k.as_bytes())?,
                HeaderValue::from_str(v)?,
            );
        }
        // Add dynamic extra headers from protocol
        for (k, v) in self.protocol.extra_headers(request) {
            headers.append(
                HeaderName::from_bytes(k.as_bytes())?,
                HeaderValue::from_str(&v)?,
            );
        }
        self.auth.apply(&mut headers).await?;

        let stream = self.transport.stream(&self.endpoint, body, headers).await?;
        let protocol = self.protocol.clone_with_fresh_state();

        let event_stream = stream
            .then(move |line_result: Result<String>| {
                let protocol = protocol.clone_box();
                async move {
                    match line_result {
                        Ok(line) => {
                            let payload = match line.strip_prefix("data: ") {
                                Some(data) => data,
                                None => {
                                    tracing::debug!(
                                        target: "llm::route",
                                        line = %line,
                                        "SSE line dropped (no 'data: ' prefix)"
                                    );
                                    return vec![];
                                }
                            };
                            tracing::debug!(
                                target: "llm::route",
                                payload_len = payload.len(),
                                payload = %if payload.len() > 300 { &payload[..300] } else { payload },
                                "SSE payload before parse"
                            );
                            match protocol.parse_sse_event(payload) {
                                Ok(events) => events.into_iter().map(Ok).collect(),
                                Err(e) => vec![Err(e)],
                            }
                        }
                        Err(e) => vec![Err(e)],
                    }
                }
            })
            .flat_map(futures::stream::iter);

        Ok(Box::pin(event_stream))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::NoAuth;
    use crate::transport::HttpTransport;
    use crate::wire::openai_chat::OpenAIChatProtocol;

    #[test]
    fn test_route_composition() {
        let protocol = Box::new(OpenAIChatProtocol);
        let transport = Box::new(HttpTransport::new(30).unwrap());
        let auth = Box::new(NoAuth);
        let route = Route::new(
            "https://api.openai.com/v1/chat/completions".to_string(),
            protocol,
            transport,
            auth,
        );
        assert_eq!(route.wire_format(), WireFormat::OpenAIChat);
    }
}

#[derive(Default)]
pub struct RouteBuilder {
    endpoint: Option<String>,
    protocol: Option<Box<dyn Protocol>>,
    transport: Option<Box<dyn Transport>>,
    auth: Option<Box<dyn AuthMethod>>,
    options: Option<ProviderOptions>,
    extra_headers: Vec<(String, String)>,
    name: Option<String>,
}

impl RouteBuilder {
    pub fn endpoint(mut self, endpoint: impl Into<String>) -> Self {
        self.endpoint = Some(endpoint.into());
        self
    }

    pub fn protocol(mut self, protocol: Box<dyn Protocol>) -> Self {
        self.protocol = Some(protocol);
        self
    }

    pub fn transport(mut self, transport: Box<dyn Transport>) -> Self {
        self.transport = Some(transport);
        self
    }

    pub fn auth(mut self, auth: Box<dyn AuthMethod>) -> Self {
        self.auth = Some(auth);
        self
    }

    pub fn options(mut self, options: ProviderOptions) -> Self {
        self.options = Some(options);
        self
    }

    pub fn extra_headers(mut self, headers: Vec<(String, String)>) -> Self {
        self.extra_headers = headers;
        self
    }

    pub fn name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    pub fn build(self) -> Route {
        Route {
            endpoint: self.endpoint.expect("endpoint required"),
            protocol: self.protocol.expect("protocol required"),
            transport: self.transport.expect("transport required"),
            auth: self.auth.expect("auth required"),
            options: self.options,
            extra_headers: self.extra_headers,
            in_flight: std::sync::atomic::AtomicUsize::new(0),
            name: self.name.unwrap_or_else(|| "anonymous".to_string()),
        }
    }
}
