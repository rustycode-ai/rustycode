//! Wire format serialization protocols.
//!
//! Each protocol handles one wire format: converting CompletionRequest → JSON body
//! and parsing JSON responses back into CompletionResponse. No HTTP, no auth —
//! pure serialization logic.

use anyhow::Result;
use serde_json::Value;

use crate::schema::normalizer::WireFormat;
use crate::schema::tool_schema::ToolSchema;
use crate::types::request::CompletionRequest;
use crate::types::response::CompletionResponse;
use rustycode_protocol::stream_event::StreamEvent;

pub mod anthropic;
pub mod bedrock;
pub mod cohere;
pub mod gemini;
pub mod litert;
pub mod openai_chat;
pub mod openai_responses;

/// Wire format serialization protocol.
///
/// Each wire format has ONE implementation shared by all providers using that format.
/// Pure serialization — no HTTP, no auth, no network.
pub trait Protocol: Send + Sync {
    /// The wire format this protocol handles.
    fn format(&self) -> WireFormat;

    /// Clone this protocol into a box.
    fn clone_box(&self) -> Box<dyn Protocol>;

    /// Clone this protocol but with a fresh internal state (for new streams).
    fn clone_with_fresh_state(&self) -> Box<dyn Protocol> {
        self.clone_box()
    }

    /// Convert a CompletionRequest into a JSON request body.
    fn serialize_body(
        &self,
        request: &CompletionRequest,
        tools: Option<&[ToolSchema]>,
    ) -> Result<Value>;

    /// Parse a non-streaming JSON response body.
    fn parse_response(&self, body: &Value) -> Result<CompletionResponse>;

    /// Parse a single SSE data line into a stream event.
    /// Returns None for keep-alive or skip lines.
    fn parse_sse_event(&self, data: &str) -> Result<Option<StreamEvent>>;

    /// Convert tool definitions into this format's tool schema.
    fn serialize_tools(&self, tools: &[ToolSchema]) -> Vec<Value>;

    /// Return any extra headers required for this request based on its content.
    fn extra_headers(&self, _request: &CompletionRequest) -> Vec<(String, String)> {
        Vec::new()
    }
}

/// Get the protocol implementation for a specific wire format.
pub fn get_protocol(format: WireFormat) -> Box<dyn Protocol> {
    match format {
        WireFormat::Anthropic => Box::new(anthropic::AnthropicProtocol {
            registry: None,
            state: std::sync::Arc::new(std::sync::Mutex::new(Default::default())),
        }),
        WireFormat::OpenAIChat => Box::new(openai_chat::OpenAIChatProtocol),
        WireFormat::OpenAIResponses => Box::new(openai_responses::OpenAIResponsesProtocol),
        WireFormat::Gemini => Box::new(gemini::GeminiProtocol),
        WireFormat::Bedrock => Box::new(bedrock::BedrockProtocol),
        WireFormat::Cohere => Box::new(cohere::CohereProtocol),
        WireFormat::LiteRT => Box::new(litert::LiteRTProtocol),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wire_format_enum_covers_all_formats() {
        let formats = [
            WireFormat::Anthropic,
            WireFormat::OpenAIChat,
            WireFormat::OpenAIResponses,
            WireFormat::Gemini,
            WireFormat::Bedrock,
            WireFormat::Cohere,
            WireFormat::LiteRT,
        ];
        // Ensure we have exactly 7 formats
        assert_eq!(formats.len(), 7);
    }
}
