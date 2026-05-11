use serde::{Deserialize, Serialize};
use std::fmt;

/// Resolve an ImageSource to base64 data.
///
/// If the source is already base64, return as-is.
/// If the source is a file path, read the file and base64-encode it.
/// If the source is a URL, return None (provider should handle URL passthrough).
pub fn resolve_image_to_base64(
    source: &rustycode_protocol::ImageSource,
) -> Option<(String, String)> {
    match source.source_type.as_str() {
        "base64" => Some((source.media_type.clone(), source.data.clone())),
        "file" => {
            let path = std::path::Path::new(&source.data);
            match std::fs::read(path) {
                Ok(bytes) => {
                    use base64::{engine::general_purpose::STANDARD, Engine};
                    let mime = if source.media_type.is_empty() {
                        match path.extension().and_then(|e| e.to_str()) {
                            Some("png") => "image/png",
                            Some("gif") => "image/gif",
                            Some("webp") => "image/webp",
                            _ => "image/jpeg",
                        }
                        .to_string()
                    } else {
                        source.media_type.clone()
                    };
                    Some((mime, STANDARD.encode(&bytes)))
                }
                Err(e) => {
                    tracing::warn!("Failed to read image file {}: {}", source.data, e);
                    None
                }
            }
        }
        "url" => None,
        _ => None,
    }
}

/// Reference to an Anthropic Agent Skill
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillRef {
    #[serde(rename = "type")]
    pub skill_type: String,
    pub skill_id: String,
    pub version: String,
}

/// API mode for providers that support multiple endpoints.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ApiMode {
    /// Try Responses API first, fall back to Chat Completions on failure.
    /// Caches the result per provider so subsequent requests skip the probe.
    Auto,
    /// Chat Completions API (`POST /v1/chat/completions`) — default.
    ChatCompletions,
    /// Responses API (`POST /v1/responses`) — HTTP.
    Responses,
    /// Responses API via WebSocket (OpenAI only, feature-gated).
    #[cfg(feature = "ws")]
    ResponsesWs,
}

/// Message role in a chat conversation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, strum::AsRefStr)]
#[serde(rename_all = "lowercase")]
#[strum(serialize_all = "lowercase")]
#[non_exhaustive]
pub enum MessageRole {
    User,
    Assistant,
    System,
    /// For tool/function calling responses
    Tool(String),
}

impl MessageRole {
    /// Create from string (for API responses)
    pub fn from_role_str(s: &str) -> Result<Self, super::super::ProviderError> {
        match s.to_lowercase().as_str() {
            "user" => Ok(MessageRole::User),
            "assistant" => Ok(MessageRole::Assistant),
            "system" => Ok(MessageRole::System),
            other if other.starts_with("tool_") || other.starts_with("tool:") => {
                Ok(MessageRole::Tool(other[5..].to_string()))
            }
            _ => Err(super::super::ProviderError::Api(format!(
                "unknown message role: {}",
                s
            ))),
        }
    }
}

/// A chat message with role and content.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: MessageRole,
    pub content: rustycode_protocol::MessageContent,
}

impl ChatMessage {
    pub fn user(content: impl Into<rustycode_protocol::MessageContent>) -> Self {
        Self {
            role: MessageRole::User,
            content: content.into(),
        }
    }

    pub fn assistant(content: impl Into<rustycode_protocol::MessageContent>) -> Self {
        Self {
            role: MessageRole::Assistant,
            content: content.into(),
        }
    }

    pub fn system(content: impl Into<rustycode_protocol::MessageContent>) -> Self {
        Self {
            role: MessageRole::System,
            content: content.into(),
        }
    }

    pub fn tool(content: impl Into<rustycode_protocol::MessageContent>, tool_name: String) -> Self {
        Self {
            role: MessageRole::Tool(tool_name),
            content: content.into(),
        }
    }

    pub fn tool_result(content: String, tool_use_id: String) -> Self {
        Self::tool_result_with_error(content, tool_use_id, false)
    }

    /// Create a tool result message with error flag
    pub fn tool_result_with_error(content: String, tool_use_id: String, is_error: bool) -> Self {
        Self {
            role: MessageRole::User,
            content: rustycode_protocol::MessageContent::blocks(vec![
                rustycode_protocol::ContentBlock::ToolResult {
                    tool_use_id,
                    content,
                    is_error,
                },
            ]),
        }
    }

    /// Get the text content for backward compatibility
    pub fn text(&self) -> String {
        self.content.as_text()
    }

    /// Check if message contains images
    pub fn has_images(&self) -> bool {
        self.content.has_images()
    }
}

impl From<rustycode_protocol::Message> for ChatMessage {
    fn from(msg: rustycode_protocol::Message) -> Self {
        let role = match msg.role.as_str() {
            "user" => MessageRole::User,
            "assistant" => MessageRole::Assistant,
            "system" => MessageRole::System,
            other => MessageRole::Tool(other.to_string()),
        };
        ChatMessage {
            role,
            content: msg.content,
        }
    }
}

impl From<ChatMessage> for rustycode_protocol::Message {
    fn from(msg: ChatMessage) -> Self {
        // Convert LLM MessageRole to protocol MessageRole
        let protocol_role = match &msg.role {
            MessageRole::User => rustycode_protocol::MessageRole::User,
            MessageRole::Assistant => rustycode_protocol::MessageRole::Assistant,
            MessageRole::System => rustycode_protocol::MessageRole::System,
            MessageRole::Tool(name) => rustycode_protocol::MessageRole::Tool(name.clone()),
        };
        rustycode_protocol::Message {
            role: protocol_role,
            content: msg.content,
            timestamp: chrono::Utc::now(),
            metadata: rustycode_protocol::MessageMetadata::default(),
        }
    }
}

/// Supported LLM provider types
#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[non_exhaustive]
pub enum ProviderType {
    Ollama,
    OpenAI,
    Anthropic,
    Gemini,      // Google Gemini
    Copilot,     // GitHub Copilot
    Bedrock,     // AWS Bedrock
    Azure,       // Azure OpenAI
    Cohere,      // Cohere
    Mistral,     // Mistral AI
    Together,    // Together AI
    Perplexity,  // Perplexity AI
    HuggingFace, // Hugging Face Inference API
    OpenRouter,  // OpenRouter
    Nvidia,      // NVIDIA NIM API
    Custom,      // OpenAI-compatible custom provider
}

impl fmt::Display for ProviderType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Ollama => write!(f, "ollama"),
            Self::OpenAI => write!(f, "openai"),
            Self::Anthropic => write!(f, "anthropic"),
            Self::Gemini => write!(f, "gemini"),
            Self::Copilot => write!(f, "copilot"),
            Self::Bedrock => write!(f, "bedrock"),
            Self::Azure => write!(f, "azure"),
            Self::Cohere => write!(f, "cohere"),
            Self::Mistral => write!(f, "mistral"),
            Self::Together => write!(f, "together"),
            Self::Perplexity => write!(f, "perplexity"),
            Self::HuggingFace => write!(f, "huggingface"),
            Self::OpenRouter => write!(f, "openrouter"),
            Self::Nvidia => write!(f, "nvidia"),
            Self::Custom => write!(f, "custom"),
        }
    }
}
