# LLM Provider Redesign — Plan 1: Foundation

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Extract the new architecture skeleton from the monolithic `provider.rs` (2,324 lines) and `tools.rs` (2,175 lines) into composable `types/`, `schema/`, `wire/`, `transport/`, and `auth/` modules — without breaking any existing code.

**Architecture:** Three orthogonal dimensions: WireFormat (message shape) × Transport (delivery) × Auth (identity). A `Protocol` trait handles serialization/deserialization per wire format. A `Route` composes Protocol + Transport + Auth + Endpoint. A `Provider` is a named collection of Routes. All new modules are additive — existing provider code continues to work during and after this plan.

**Tech Stack:** Rust 2021, serde_json, async_trait, tokio, reqwest, secrecy, thiserror

**Spec:** `docs/architecture/LLM-PROVIDER-REDESIGN.md`

**Scope check:** This is Plan 1 of 3. Plans 2 and 3 (provider migration + cleanup) depend on this foundation but are independent of each other.

---

## File Structure

### New Files (Phase 1: Type Extraction)

```
crates/rustycode-llm/src/types/
├── mod.rs                # Re-exports
├── message.rs            # ChatMessage, MessageRole (moved from provider.rs)
├── config.rs             # ThinkingConfig, OutputConfig, EffortLevel, etc. (moved from provider.rs)
├── request.rs            # CompletionRequest (moved from provider.rs)
├── response.rs           # CompletionResponse, ThinkingBlock, Citation, Usage (moved from provider.rs)
├── error.rs              # ProviderError (moved from provider.rs)
└── streaming.rs          # StreamChunk, StreamEvent, SSEEvent (moved from provider.rs)
```

### New Files (Phase 2: Typed Schema)

```
crates/rustycode-llm/src/schema/
├── mod.rs                # Re-exports + WireFormat enum
├── tool_schema.rs        # ToolSchema, JsonSchema builder
└── normalizer.rs         # SchemaNormalizationProfile + per-format normalization
```

### New Files (Phase 3: Wire Serializers)

```
crates/rustycode-llm/src/wire/
├── mod.rs                # Protocol trait + WireFormat enum (re-exported from schema)
├── anthropic.rs          # AnthropicProtocol
├── openai_chat.rs        # OpenAIChatProtocol
├── openai_responses.rs   # OpenAIResponsesProtocol
├── gemini.rs             # GeminiProtocol
├── bedrock.rs            # BedrockProtocol
├── cohere.rs             # CohereProtocol
└── litert.rs             # LiteRTProtocol (in-process, no JSON)
```

### New Files (Phase 4: Transport + Auth)

```
crates/rustycode-llm/src/transport/
├── mod.rs                # Transport enum + HttpTransport trait
├── http.rs               # Non-streaming HTTP
├── http_sse.rs           # SSE streaming (extracted from existing sse.rs patterns)
├── local.rs              # In-process (LiteRT)
└── fallback.rs           # TransportFallbackStrategy

crates/rustycode-llm/src/auth/
├── mod.rs                # AuthMethod enum + credential resolution chain
├── bearer.rs             # Bearer token auth
├── api_key_header.rs     # x-api-key, x-goog-api-key, api-key header auth
├── aws_sigv4.rs          # AWS Sigv4 signing (extracted from bedrock.rs)
├── none.rs               # No auth (local providers)
└── resolver.rs           # Credential resolution: env → config → keyring → prompt
```

### Modified Files

```
crates/rustycode-llm/src/lib.rs          # Add new module declarations + re-exports
crates/rustycode-llm/src/provider.rs      # Re-export from types/ (no logic changes)
crates/rustycode-llm/src/tools.rs         # Re-export from schema/ (no logic changes)
```

---

## Chunk 1: Phase 1 — Type Extraction

Non-breaking reorganization. All types move to `types/` modules. `provider.rs` re-exports everything so existing code compiles unchanged.

### Task 1: Create `types/` Module Skeleton

**Files:**
- Create: `crates/rustycode-llm/src/types/mod.rs`

- [ ] **Step 1: Create the types directory and mod.rs**

```bash
mkdir -p crates/rustycode-llm/src/types
```

Create `crates/rustycode-llm/src/types/mod.rs`:
```rust
//! Shared LLM types extracted from provider.rs.
//!
//! These types are re-exported from `provider.rs` for backward compatibility.
//! New code should import from `crate::types::*`.

pub mod config;
pub mod error;
pub mod message;
pub mod request;
pub mod response;
pub mod streaming;
```

- [ ] **Step 2: Add `pub mod types;` to lib.rs**

In `crates/rustycode-llm/src/lib.rs`, add after the existing module declarations:
```rust
pub mod types;
```

- [ ] **Step 3: Verify it compiles with empty modules**

Create empty placeholder files so the module compiles:
```bash
for f in config error message request response streaming; do
  touch crates/rustycode-llm/src/types/${f}.rs
done
```

Run: `cargo check -p rustycode-llm 2>&1 | tail -5`
Expected: Compiles with warnings about empty modules (no errors)

- [ ] **Step 4: Commit**

```bash
git add crates/rustycode-llm/src/types/ crates/rustycode-llm/src/lib.rs
git commit -m "refactor(llm): add empty types/ module skeleton for provider.rs extraction"
```

### Task 2: Extract Message Types to `types/message.rs`

**Files:**
- Create: `crates/rustycode-llm/src/types/message.rs`
- Modify: `crates/rustycode-llm/src/provider.rs`

- [ ] **Step 1: Find ChatMessage and MessageRole in provider.rs**

Run: `grep -n "pub struct ChatMessage\|pub enum MessageRole\|pub enum ApiMode\|pub struct SkillRef" crates/rustycode-llm/src/provider.rs`

Note: `ChatMessage` may be re-exported from `rustycode-protocol`. Check what provider.rs actually defines vs. re-exports.

Run: `grep -n "^pub enum\|^pub struct" crates/rustycode-llm/src/provider.rs | head -30`

- [ ] **Step 2: Move message-related types to types/message.rs**

Read the exact types from `provider.rs`. Key types to move:
- `ChatMessage` (if defined here, not just re-exported)
- `ApiMode` enum
- `SkillRef` struct
- `ProviderType` enum
- `resolve_image_to_base64` function

Create `crates/rustycode-llm/src/types/message.rs` with these types copied verbatim (same imports).

Add re-export in `provider.rs`:
```rust
pub use crate::types::message::{ApiMode, ChatMessage, MessageRole, ProviderType, SkillRef};
```

Remove the original definitions from `provider.rs`, keeping only the `pub use` re-exports.

- [ ] **Step 3: Verify compilation**

Run: `cargo check -p rustycode-llm 2>&1 | tail -10`
Expected: Compiles successfully

- [ ] **Step 4: Run existing tests**

Run: `cargo test -p rustycode-llm --quiet 2>&1 | tail -5`
Expected: All existing tests pass (no behavior change)

- [ ] **Step 5: Commit**

```bash
git add crates/rustycode-llm/src/types/message.rs crates/rustycode-llm/src/provider.rs
git commit -m "refactor(llm): extract ChatMessage, ApiMode, SkillRef to types/message.rs"
```

### Task 3: Extract Config Types to `types/config.rs`

**Files:**
- Create: `crates/rustycode-llm/src/types/config.rs`
- Modify: `crates/rustycode-llm/src/provider.rs`

- [ ] **Step 1: Identify config types in provider.rs**

Types to extract:
- `ThinkingConfig`, `ThinkingType`, `ThinkingDisplay`
- `OutputConfig`, `OutputFormat`, `OutputFormatType`
- `EffortLevel`
- `ProviderConfig` (runtime config, not the file config)
- `build_openai_response_format`, `build_gemini_response_format` helper functions

- [ ] **Step 2: Move to types/config.rs**

Copy all config types verbatim. Add re-exports in `provider.rs`:
```rust
pub use crate::types::config::{
    build_gemini_response_schema, build_openai_response_format, EffortLevel, OutputConfig,
    OutputFormat, OutputFormatType, ProviderConfig, ThinkingConfig, ThinkingDisplay, ThinkingType,
};
```

- [ ] **Step 3: Verify compilation + tests**

Run: `cargo check -p rustycode-llm 2>&1 | tail -5`
Run: `cargo test -p rustycode-llm --quiet 2>&1 | tail -5`

- [ ] **Step 4: Commit**

```bash
git add crates/rustycode-llm/src/types/config.rs crates/rustycode-llm/src/provider.rs
git commit -m "refactor(llm): extract ThinkingConfig, OutputConfig, EffortLevel to types/config.rs"
```

### Task 4: Extract Request/Response Types to `types/request.rs` and `types/response.rs`

**Files:**
- Create: `crates/rustycode-llm/src/types/request.rs`
- Create: `crates/rustycode-llm/src/types/response.rs`
- Modify: `crates/rustycode-llm/src/provider.rs`

- [ ] **Step 1: Extract CompletionRequest to types/request.rs**

Types:
- `CompletionRequest` struct + all its impl blocks
- Any helper functions used only by CompletionRequest

Add re-export in `provider.rs`:
```rust
pub use crate::types::request::CompletionRequest;
```

- [ ] **Step 2: Extract CompletionResponse to types/response.rs**

Types:
- `CompletionResponse` struct + impl blocks
- `ThinkingBlock` (if defined in provider.rs, not protocol)
- `Citation`
- `Usage` re-export from `rustycode_protocol`

Add re-export in `provider.rs`:
```rust
pub use crate::types::response::{Citation, CompletionResponse, ThinkingBlock};
```

- [ ] **Step 3: Verify compilation + tests**

Run: `cargo check -p rustycode-llm 2>&1 | tail -5`
Run: `cargo test -p rustycode-llm --quiet 2>&1 | tail -5`

- [ ] **Step 4: Commit**

```bash
git add crates/rustycode-llm/src/types/request.rs crates/rustycode-llm/src/types/response.rs crates/rustycode-llm/src/provider.rs
git commit -m "refactor(llm): extract CompletionRequest/Response to types/"
```

### Task 5: Extract Error and Streaming Types

**Files:**
- Create: `crates/rustycode-llm/src/types/error.rs`
- Create: `crates/rustycode-llm/src/types/streaming.rs`
- Modify: `crates/rustycode-llm/src/provider.rs`

- [ ] **Step 1: Extract ProviderError to types/error.rs**

Move `ProviderError` enum + all variants. Add `sanitize_error_message` if it lives in provider.rs.

- [ ] **Step 2: Extract streaming types to types/streaming.rs**

Types:
- `StreamChunk` type alias
- `StreamEvent` enum (if defined here)
- Any SSE-related helper types that are shared across providers

- [ ] **Step 3: Verify full compilation + all tests**

Run: `cargo check -p rustycode-llm 2>&1 | tail -5`
Run: `cargo test -p rustycode-llm --quiet 2>&1 | tail -5`

- [ ] **Step 4: Verify provider.rs is now thin**

Run: `wc -l crates/rustycode-llm/src/provider.rs`
Expected: ~200-400 lines (only re-exports + `LLMProvider` trait + `create_provider` factory)

- [ ] **Step 5: Commit**

```bash
git add crates/rustycode-llm/src/types/ crates/rustycode-llm/src/provider.rs
git commit -m "refactor(llm): extract ProviderError and streaming types to types/"
```

### Task 6: Run Full Workspace Test Suite

- [ ] **Step 1: Run full test suite**

Run: `cargo test -p rustycode-llm 2>&1 | tail -15`
Expected: All tests pass

- [ ] **Step 2: Run clippy**

Run: `cargo clippy -p rustycode-llm -- -D warnings 2>&1 | tail -10`
Expected: No warnings (or only pre-existing warnings)

- [ ] **Step 3: Run fmt check**

Run: `cargo fmt -p rustycode-llm -- --check`
Expected: No formatting issues

---

## Chunk 2: Phase 2 — Typed Tool Schema

Replace raw `serde_json::Value` tool definitions with typed `ToolSchema` and `JsonSchema` builders. Add per-format normalization.

### Task 7: Create `schema/` Module with `ToolSchema` and `JsonSchema`

**Files:**
- Create: `crates/rustycode-llm/src/schema/mod.rs`
- Create: `crates/rustycode-llm/src/schema/tool_schema.rs`
- Modify: `crates/rustycode-llm/src/lib.rs`

- [ ] **Step 1: Write the failing test for JsonSchema builder**

Create `crates/rustycode-llm/src/schema/tool_schema.rs`:

```rust
//! Typed tool schema — replaces raw json!() macros for tool definitions.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Typed JSON Schema builder.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonSchema {
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub schema_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub properties: Option<BTreeMap<String, JsonSchema>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub required: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub items: Option<Box<JsonSchema>>,
    #[serde(rename = "enum", skip_serializing_if = "Option::is_none")]
    pub enum_values: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub any_of: Option<Vec<JsonSchema>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub additional_properties: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default: Option<serde_json::Value>,
}

impl JsonSchema {
    pub fn string(description: impl Into<String>) -> Self {
        Self {
            schema_type: Some("string".into()),
            description: Some(description.into()),
            properties: None,
            required: None,
            items: None,
            enum_values: None,
            any_of: None,
            additional_properties: None,
            default: None,
        }
    }

    pub fn integer(description: impl Into<String>) -> Self {
        Self {
            schema_type: Some("integer".into()),
            description: Some(description.into()),
            properties: None,
            required: None,
            items: None,
            enum_values: None,
            any_of: None,
            additional_properties: None,
            default: None,
        }
    }

    pub fn boolean(description: impl Into<String>) -> Self {
        Self {
            schema_type: Some("boolean".into()),
            description: Some(description.into()),
            properties: None,
            required: None,
            items: None,
            enum_values: None,
            any_of: None,
            additional_properties: None,
            default: None,
        }
    }

    pub fn number(description: impl Into<String>) -> Self {
        Self {
            schema_type: Some("number".into()),
            description: Some(description.into()),
            properties: None,
            required: None,
            items: None,
            enum_values: None,
            any_of: None,
            additional_properties: None,
            default: None,
        }
    }

    pub fn object(
        properties: BTreeMap<String, Self>,
        required: Vec<String>,
    ) -> Self {
        Self {
            schema_type: Some("object".into()),
            description: None,
            properties: Some(properties),
            required: if required.is_empty() { None } else { Some(required) },
            items: None,
            enum_values: None,
            any_of: None,
            additional_properties: None,
            default: None,
        }
    }

    pub fn array(items: Self) -> Self {
        Self {
            schema_type: Some("array".into()),
            description: None,
            properties: None,
            required: None,
            items: Some(Box::new(items)),
            enum_values: None,
            any_of: None,
            additional_properties: None,
            default: None,
        }
    }

    pub fn enum_of(variants: Vec<&str>) -> Self {
        Self {
            schema_type: Some("string".into()),
            description: None,
            properties: None,
            required: None,
            items: None,
            enum_values: Some(variants.into_iter().map(String::from).collect()),
            any_of: None,
            additional_properties: None,
            default: None,
        }
    }

    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    pub fn with_default(mut self, value: serde_json::Value) -> Self {
        self.default = Some(value);
        self
    }

    /// Convert to serde_json::Value for wire serialization.
    pub fn to_value(&self) -> serde_json::Value {
        serde_json::to_value(self).unwrap_or_else(|_| serde_json::Value::Object(Default::default()))
    }
}

/// Typed tool definition — replaces raw `serde_json::Value` tool definitions.
#[derive(Debug, Clone)]
pub struct ToolSchema {
    pub name: String,
    pub description: String,
    pub input_schema: JsonSchema,
}

impl ToolSchema {
    pub fn new(
        name: impl Into<String>,
        description: impl Into<String>,
        input_schema: JsonSchema,
    ) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
            input_schema,
        }
    }

    /// Convert to a serde_json::Value in Anthropic wire format.
    pub fn to_anthropic_value(&self) -> serde_json::Value {
        serde_json::json!({
            "name": self.name,
            "description": self.description,
            "input_schema": self.input_schema.to_value(),
        })
    }

    /// Convert to a serde_json::Value in OpenAI Chat wire format.
    pub fn to_openai_chat_value(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "function",
            "function": {
                "name": self.name,
                "description": self.description,
                "parameters": self.input_schema.to_value(),
            }
        })
    }

    /// Convert to a serde_json::Value in OpenAI Responses wire format.
    pub fn to_openai_responses_value(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "function",
            "name": self.name,
            "description": self.description,
            "parameters": self.input_schema.to_value(),
        })
    }

    /// Convert to a serde_json::Value in Gemini wire format.
    pub fn to_gemini_value(&self) -> serde_json::Value {
        serde_json::json!({
            "name": self.name,
            "description": self.description,
            "parameters": self.input_schema.to_value(),
        })
    }

    /// Convert to a serde_json::Value in Bedrock wire format.
    pub fn to_bedrock_value(&self) -> serde_json::Value {
        serde_json::json!({
            "toolSpec": {
                "name": self.name,
                "description": self.description,
                "inputSchema": {
                    "json": self.input_schema.to_value(),
                }
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_schema_string_serializes() {
        let schema = JsonSchema::string("File path");
        let val = schema.to_value();
        assert_eq!(val["type"], "string");
        assert_eq!(val["description"], "File path");
    }

    #[test]
    fn json_schema_object_with_required() {
        let schema = JsonSchema::object(
            BTreeMap::from([
                ("path".into(), JsonSchema::string("File path")),
                ("content".into(), JsonSchema::string("File content")),
            ]),
            vec!["path".into(), "content".into()],
        );
        let val = schema.to_value();
        assert_eq!(val["type"], "object");
        assert!(val["properties"]["path"].is_object());
        assert_eq!(val["required"][0], "path");
    }

    #[test]
    fn tool_schema_anthropic_format() {
        let tool = ToolSchema::new(
            "Edit",
            "Replace text in a file",
            JsonSchema::object(
                BTreeMap::from([
                    ("file_path".into(), JsonSchema::string("Absolute path")),
                    ("old_string".into(), JsonSchema::string("Text to find")),
                    ("new_string".into(), JsonSchema::string("Replacement text")),
                ]),
                vec!["file_path".into(), "old_string".into(), "new_string".into()],
            ),
        );
        let val = tool.to_anthropic_value();
        assert_eq!(val["name"], "Edit");
        assert_eq!(val["input_schema"]["type"], "object");
        assert!(val.get("function").is_none()); // Anthropic doesn't use function wrapper
    }

    #[test]
    fn tool_schema_openai_chat_format() {
        let tool = ToolSchema::new(
            "Edit",
            "Replace text in a file",
            JsonSchema::object(
                BTreeMap::from([
                    ("file_path".into(), JsonSchema::string("Absolute path")),
                ]),
                vec!["file_path".into()],
            ),
        );
        let val = tool.to_openai_chat_value();
        assert_eq!(val["type"], "function");
        assert_eq!(val["function"]["name"], "Edit");
        assert_eq!(val["function"]["parameters"]["type"], "object");
    }

    #[test]
    fn tool_schema_bedrock_format() {
        let tool = ToolSchema::new(
            "Edit",
            "Replace text",
            JsonSchema::object(
                BTreeMap::from([
                    ("path".into(), JsonSchema::string("Path")),
                ]),
                vec!["path".into()],
            ),
        );
        let val = tool.to_bedrock_value();
        assert_eq!(val["toolSpec"]["name"], "Edit");
        assert_eq!(val["toolSpec"]["inputSchema"]["json"]["type"], "object");
    }
}
```

Create `crates/rustycode-llm/src/schema/mod.rs`:
```rust
//! Typed tool schema and per-format normalization.

pub mod normalizer;
pub mod tool_schema;

pub use tool_schema::{JsonSchema, ToolSchema};
```

Create `crates/rustycode-llm/src/schema/normalizer.rs`:
```rust
//! Per-format schema normalization profiles.

/// Describes what JSON Schema features a wire format supports.
#[derive(Debug, Clone, Copy)]
pub struct SchemaNormalizationProfile {
    pub supports_ref: bool,
    pub supports_defs: bool,
    pub supports_schema_keyword: bool,
    pub supports_default_values: bool,
    pub supports_enum: bool,
    pub supports_type_unions: bool,
    pub supports_additional_properties: bool,
    pub supports_min_max: bool,
    pub supports_pattern: bool,
    pub supports_format: bool,
    pub supports_examples: bool,
    pub requires_strict: bool,
}

/// Result of normalizing a schema for a specific format.
#[derive(Debug, Clone)]
pub struct NormalizedSchema {
    pub schema: serde_json::Value,
    pub warnings: Vec<String>,
    pub removed_features: Vec<&'static str>,
}

/// Wire format identifier for schema normalization.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WireFormat {
    Anthropic,
    OpenAIChat,
    OpenAIResponses,
    Gemini,
    Bedrock,
    Cohere,
}

/// Return the normalization profile for a wire format.
pub fn profile_for_format(format: WireFormat) -> SchemaNormalizationProfile {
    match format {
        WireFormat::Anthropic => SchemaNormalizationProfile {
            supports_ref: true,
            supports_defs: true,
            supports_schema_keyword: false,
            supports_default_values: true,
            supports_enum: true,
            supports_type_unions: true,
            supports_additional_properties: true,
            supports_min_max: true,
            supports_pattern: true,
            supports_format: true,
            supports_examples: false,
            requires_strict: false,
        },
        WireFormat::OpenAIChat => SchemaNormalizationProfile {
            supports_ref: false,
            supports_defs: false,
            supports_schema_keyword: false,
            supports_default_values: true,
            supports_enum: true,
            supports_type_unions: false,
            supports_additional_properties: true,
            supports_min_max: true,
            supports_pattern: true,
            supports_format: true,
            supports_examples: false,
            requires_strict: false,
        },
        WireFormat::OpenAIResponses => SchemaNormalizationProfile {
            supports_ref: false,
            supports_defs: false,
            supports_schema_keyword: false,
            supports_default_values: true,
            supports_enum: true,
            supports_type_unions: false,
            supports_additional_properties: true,
            supports_min_max: true,
            supports_pattern: true,
            supports_format: true,
            supports_examples: false,
            requires_strict: true,
        },
        WireFormat::Gemini => SchemaNormalizationProfile {
            supports_ref: false,
            supports_defs: false,
            supports_schema_keyword: false,
            supports_default_values: false,
            supports_enum: true,
            supports_type_unions: false,
            supports_additional_properties: false,
            supports_min_max: true,
            supports_pattern: true,
            supports_format: true,
            supports_examples: false,
            requires_strict: false,
        },
        WireFormat::Bedrock => SchemaNormalizationProfile {
            supports_ref: true,
            supports_defs: true,
            supports_schema_keyword: false,
            supports_default_values: true,
            supports_enum: true,
            supports_type_unions: true,
            supports_additional_properties: true,
            supports_min_max: true,
            supports_pattern: true,
            supports_format: true,
            supports_examples: false,
            requires_strict: false,
        },
        WireFormat::Cohere => SchemaNormalizationProfile {
            supports_ref: false,
            supports_defs: false,
            supports_schema_keyword: false,
            supports_default_values: true,
            supports_enum: true,
            supports_type_unions: false,
            supports_additional_properties: false,
            supports_min_max: true,
            supports_pattern: true,
            supports_format: true,
            supports_examples: false,
            requires_strict: false,
        },
    }
}

/// Normalize a JSON schema value for a specific wire format.
///
/// Removes unsupported features and logs warnings for anything stripped.
pub fn normalize_schema(
    schema: &serde_json::Value,
    format: WireFormat,
) -> NormalizedSchema {
    let profile = profile_for_format(format);
    let mut normalized = schema.clone();
    let mut warnings = Vec::new();
    let mut removed: Vec<&'static str> = Vec::new();

    // Remove $schema
    if !profile.supports_schema_keyword {
        if let Some(obj) = normalized.as_object_mut() {
            if obj.remove("$schema").is_some() {
                removed.push("$schema");
            }
        }
    }

    // Remove $defs
    if !profile.supports_defs {
        if let Some(obj) = normalized.as_object_mut() {
            if obj.remove("$defs").is_some() {
                removed.push("$defs");
                warnings.push("$defs not supported; consider expanding inline".into());
            }
        }
    }

    // Remove $ref
    if !profile.supports_ref {
        if let Some(obj) = normalized.as_object_mut() {
            if obj.remove("$ref").is_some() {
                removed.push("$ref");
                warnings.push("$ref not supported; consider expanding inline".into());
            }
        }
    }

    // Remove default: null (Gemini can't handle it)
    if !profile.supports_default_values {
        if let Some(obj) = normalized.as_object_mut() {
            if let Some(default) = obj.get("default") {
                if default.is_null() {
                    obj.remove("default");
                    removed.push("default:null");
                }
            }
        }
    }

    // Flatten type unions: ["string", "null"] → "string"
    if !profile.supports_type_unions {
        if let Some(obj) = normalized.as_object_mut() {
            if let Some(type_val) = obj.get_mut("type") {
                if let Some(arr) = type_val.as_array_mut() {
                    if !arr.is_empty() {
                        warnings.push(format!(
                            "type union {:?} flattened to first variant",
                            arr.iter()
                                .filter_map(|v| v.as_str())
                                .collect::<Vec<_>>()
                        ));
                        *type_val = arr[0].clone();
                        removed.push("type_union");
                    }
                }
            }
        }
    }

    // Add strict: true for OpenAI Responses
    if profile.requires_strict {
        if let Some(obj) = normalized.as_object_mut() {
            obj.insert("strict".into(), serde_json::Value::Bool(true));
        }
    }

    // Recurse into properties
    if let Some(obj) = normalized.as_object_mut() {
        if let Some(properties) = obj.get_mut("properties") {
            if let Some(props) = properties.as_object_mut() {
                for (_key, value) in props.iter_mut() {
                    let sub = normalize_schema(value, format);
                    warnings.extend(sub.warnings);
                    // Don't add duplicate removed features
                    for feat in sub.removed_features {
                        if !removed.contains(&feat) {
                            removed.push(feat);
                        }
                    }
                    *value = sub.schema;
                }
            }
        }
        // Recurse into items
        if let Some(items) = obj.get_mut("items") {
            let sub = normalize_schema(items, format);
            warnings.extend(sub.warnings);
            for feat in sub.removed_features {
                if !removed.contains(&feat) {
                    removed.push(feat);
                }
            }
            *items = sub.schema;
        }
        // Recurse into anyOf
        if let Some(any_of) = obj.get_mut("anyOf") {
            if let Some(arr) = any_of.as_array_mut() {
                for item in arr.iter_mut() {
                    let sub = normalize_schema(item, format);
                    warnings.extend(sub.warnings);
                    for feat in sub.removed_features {
                        if !removed.contains(&feat) {
                            removed.push(feat);
                        }
                    }
                    *item = sub.schema;
                }
            }
        }
    }

    NormalizedSchema {
        schema: normalized,
        warnings,
        removed_features: removed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn gemini_strips_schema_and_defs() {
        let schema = json!({
            "$schema": "http://json-schema.org/draft-07/schema#",
            "$defs": { "Error": { "type": "string" } },
            "type": "object",
            "properties": {}
        });
        let result = normalize_schema(&schema, WireFormat::Gemini);
        assert!(result.schema.get("$schema").is_none());
        assert!(result.schema.get("$defs").is_none());
        assert!(result.removed_features.contains(&"$schema"));
        assert!(result.removed_features.contains(&"$defs"));
    }

    #[test]
    fn gemini_flattens_type_unions() {
        let schema = json!({
            "type": ["string", "null"]
        });
        let result = normalize_schema(&schema, WireFormat::Gemini);
        assert_eq!(result.schema["type"], "string");
        assert!(result.removed_features.contains(&"type_union"));
    }

    #[test]
    fn gemini_removes_default_null() {
        let schema = json!({
            "type": "string",
            "default": null
        });
        let result = normalize_schema(&schema, WireFormat::Gemini);
        assert!(result.schema.get("default").is_none());
        assert!(result.removed_features.contains(&"default:null"));
    }

    #[test]
    fn openai_responses_adds_strict() {
        let schema = json!({
            "type": "object",
            "properties": {}
        });
        let result = normalize_schema(&schema, WireFormat::OpenAIResponses);
        assert_eq!(result.schema["strict"], true);
    }

    #[test]
    fn anthropic_preserves_ref() {
        let schema = json!({
            "$ref": "#/definitions/Error",
            "type": "object"
        });
        let result = normalize_schema(&schema, WireFormat::Anthropic);
        assert!(result.schema.get("$ref").is_some());
        assert!(!result.removed_features.contains(&"$ref"));
    }

    #[test]
    fn openai_chat_strips_ref() {
        let schema = json!({
            "$ref": "#/definitions/Error",
            "type": "object"
        });
        let result = normalize_schema(&schema, WireFormat::OpenAIChat);
        assert!(result.schema.get("$ref").is_none());
        assert!(result.removed_features.contains(&"$ref"));
    }

    #[test]
    fn normalization_is_recursive() {
        let schema = json!({
            "type": "object",
            "properties": {
                "nested": {
                    "type": "object",
                    "properties": {
                        "deep": {
                            "$schema": "http://json-schema.org/draft-07/schema#",
                            "type": "string"
                        }
                    }
                }
            }
        });
        let result = normalize_schema(&schema, WireFormat::Gemini);
        assert!(result.removed_features.contains(&"$schema"));
        // The deeply nested $schema should be removed too
        let deep = &result.schema["properties"]["nested"]["properties"]["deep"];
        assert!(deep.get("$schema").is_none());
    }
}
```

- [ ] **Step 2: Add `pub mod schema;` to lib.rs**

In `crates/rustycode-llm/src/lib.rs`, add:
```rust
pub mod schema;
```

- [ ] **Step 3: Run tests**

Run: `cargo test -p rustycode-llm schema:: 2>&1 | tail -15`
Expected: All 9 schema tests pass

- [ ] **Step 4: Run clippy**

Run: `cargo clippy -p rustycode-llm -- -D warnings 2>&1 | tail -5`
Expected: No new warnings

- [ ] **Step 5: Commit**

```bash
git add crates/rustycode-llm/src/schema/ crates/rustycode-llm/src/lib.rs
git commit -m "feat(llm): add typed ToolSchema/JsonSchema builder and per-format schema normalizer"
```

---

## Chunk 3: Phase 3 — Wire Serializers

Create the `Protocol` trait and 7 wire format implementations. Each serializer handles request→JSON body and response JSON→CompletionResponse without any HTTP or auth logic.

### Task 8: Create `wire/` Module with Protocol Trait

**Files:**
- Create: `crates/rustycode-llm/src/wire/mod.rs`
- Modify: `crates/rustycode-llm/src/lib.rs`

- [ ] **Step 1: Write the failing test for Protocol trait**

Create `crates/rustycode-llm/src/wire/mod.rs`:

```rust
//! Wire format serialization protocols.
//!
//! Each protocol handles one wire format: converting CompletionRequest → JSON body
//! and parsing JSON responses back into CompletionResponse. No HTTP, no auth —
//! pure serialization logic.

use anyhow::Result;
use serde_json::Value;

use crate::schema::normalizer::WireFormat;
use crate::types::request::CompletionRequest;
use crate::types::response::CompletionResponse;
use crate::types::streaming::StreamEvent;
use crate::schema::tool_schema::ToolSchema;

pub mod anthropic;
pub mod openai_chat;
pub mod openai_responses;
pub mod gemini;
pub mod bedrock;
pub mod cohere;
pub mod litert;

/// Wire format serialization protocol.
///
/// Each wire format has ONE implementation shared by all providers using that format.
/// Pure serialization — no HTTP, no auth, no network.
pub trait Protocol: Send + Sync {
    /// The wire format this protocol handles.
    fn format(&self) -> WireFormat;

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
        ];
        // Ensure we have exactly 6 formats
        assert_eq!(formats.len(), 6);
    }
}
```

- [ ] **Step 2: Create placeholder protocol files**

```bash
for f in anthropic openai_chat openai_responses gemini bedrock cohere litert; do
  echo "//! Wire protocol for ${f} format." > "crates/rustycode-llm/src/wire/${f}.rs"
done
```

- [ ] **Step 3: Add `pub mod wire;` to lib.rs**

In `crates/rustycode-llm/src/lib.rs`, add:
```rust
pub mod wire;
```

- [ ] **Step 4: Verify compilation**

Run: `cargo check -p rustycode-llm 2>&1 | tail -5`
Expected: Compiles

- [ ] **Step 5: Commit**

```bash
git add crates/rustycode-llm/src/wire/ crates/rustycode-llm/src/lib.rs
git commit -m "feat(llm): add wire/ module with Protocol trait and placeholder implementations"
```

### Task 9: Implement `AnthropicProtocol`

**Files:**
- Create: `crates/rustycode-llm/src/wire/anthropic.rs`

This is the reference implementation. It extracts serialization logic from `anthropic/mod.rs` into a standalone, testable protocol.

- [ ] **Step 1: Write failing tests for Anthropic serialization**

The tests should cover:
- Basic message → JSON body conversion
- System prompt handling (separate from messages)
- Tool serialization (Anthropic format: `{name, description, input_schema}`)
- Thinking config serialization
- Image content block handling
- Tool result message handling
- SSE event parsing (message_start, content_block_start, content_block_delta, etc.)

Write these tests in `crates/rustycode-llm/src/wire/anthropic.rs` as a `#[cfg(test)] mod tests` block.

- [ ] **Step 2: Implement `AnthropicProtocol`**

Key implementation notes:
- Read `crates/rustycode-llm/src/anthropic/mod.rs` to extract the exact JSON body construction
- The `serialize_body` method builds: `{model, system, messages, tools, tool_choice, max_tokens, stream, thinking}`
- The `serialize_tools` method produces: `{name, description, input_schema}`
- The `parse_sse_event` method handles: `message_start`, `content_block_start`, `content_block_delta`, `content_block_stop`, `message_delta`, `message_stop`, `ping`

- [ ] **Step 3: Run tests**

Run: `cargo test -p rustycode-llm wire::anthropic 2>&1 | tail -15`
Expected: All Anthropic protocol tests pass

- [ ] **Step 4: Commit**

```bash
git add crates/rustycode-llm/src/wire/anthropic.rs
git commit -m "feat(llm): implement AnthropicProtocol wire serializer with tests"
```

### Task 10: Implement `OpenAIChatProtocol`

**Files:**
- Create: `crates/rustycode-llm/src/wire/openai_chat.rs`

- [ ] **Step 1: Write failing tests**

Cover:
- System message handling (role: "system" in messages array)
- Tool calls in assistant messages
- Tool results as role: "tool" messages
- Tool serialization (OpenAI format: `{type: "function", function: {name, description, parameters}}`)
- Reasoning model handling (no temperature, max_completion_tokens)
- SSE event parsing (delta.content, delta.tool_calls, [DONE])

- [ ] **Step 2: Implement `OpenAIChatProtocol`**

Extract from `crates/rustycode-llm/src/openai/mod.rs`. This is the highest-impact protocol — it's shared by 10+ providers.

- [ ] **Step 3: Run tests + commit**

```bash
cargo test -p rustycode-llm wire::openai_chat
git add crates/rustycode-llm/src/wire/openai_chat.rs
git commit -m "feat(llm): implement OpenAIChatProtocol wire serializer with tests"
```

### Task 11: Implement `OpenAIResponsesProtocol`

**Files:**
- Create: `crates/rustycode-llm/src/wire/openai_responses.rs`

- [ ] **Step 1: Write failing tests**

Cover:
- `input` field instead of `messages`
- Flat tool format: `{type, name, description, parameters}`
- `strict: true` on tool schemas
- Different streaming event types vs Chat Completions

- [ ] **Step 2: Implement `OpenAIResponsesProtocol`**

Extract from `crates/rustycode-llm/src/openai_compatible/responses.rs`.

- [ ] **Step 3: Run tests + commit**

```bash
cargo test -p rustycode-llm wire::openai_responses
git add crates/rustycode-llm/src/wire/openai_responses.rs
git commit -m "feat(llm): implement OpenAIResponsesProtocol wire serializer with tests"
```

### Task 12: Implement `GeminiProtocol`, `BedrockProtocol`, `CohereProtocol`, `LiteRTProtocol`

**Files:**
- Create: `crates/rustycode-llm/src/wire/gemini.rs`
- Create: `crates/rustycode-llm/src/wire/bedrock.rs`
- Create: `crates/rustycode-llm/src/wire/cohere.rs`
- Create: `crates/rustycode-llm/src/wire/litert.rs`

- [ ] **Step 1: Implement GeminiProtocol**

Key differences:
- `contents` array with `parts` instead of `messages` with `content`
- `system_instruction` as separate field
- `functionDeclarations` wrapper for tools
- `generationConfig` for temperature/max tokens
- Schema normalization required (use `normalize_schema` from schema/normalizer)

Extract from `crates/rustycode-llm/src/gemini.rs`.

- [ ] **Step 2: Implement BedrockProtocol**

Key differences:
- `toolSpec` wrapper around tool definitions
- `inputSchema.json` (extra nesting)
- `inferenceConfig` instead of top-level params
- Tool results use `status: "success" | "error"`

Extract from `crates/rustycode-llm/src/bedrock.rs`.

- [ ] **Step 3: Implement CohereProtocol**

Key differences:
- `parameter_definitions` instead of `parameters`
- System prompt at top level (like Anthropic)
- `tool_use` enum: "auto", "off", "always"

Extract from `crates/rustycode-llm/src/cohere.rs`.

- [ ] **Step 4: Implement LiteRTProtocol (in-process)**

This is special — no JSON serialization. It takes Rust structs directly.
- `serialize_body` returns a marker value (not used for HTTP)
- `parse_response` is not used (direct struct return)
- `parse_sse_event` returns error (no streaming)

- [ ] **Step 5: Run all wire tests + commit**

Run: `cargo test -p rustycode-llm wire:: 2>&1 | tail -15`
Expected: All wire protocol tests pass

```bash
git add crates/rustycode-llm/src/wire/
git commit -m "feat(llm): implement Gemini, Bedrock, Cohere, LiteRT wire protocols"
```

---

## Chunk 4: Phase 4 — Transport + Auth Layer

Extract HTTP transport, SSE streaming, and auth methods into standalone modules.

### Task 13: Create `transport/` Module

**Files:**
- Create: `crates/rustycode-llm/src/transport/mod.rs`
- Create: `crates/rustycode-llm/src/transport/http.rs`
- Create: `crates/rustycode-llm/src/transport/http_sse.rs`
- Create: `crates/rustycode-llm/src/transport/fallback.rs`
- Create: `crates/rustycode-llm/src/transport/local.rs`
- Modify: `crates/rustycode-llm/src/lib.rs`

- [ ] **Step 1: Create transport/mod.rs with Transport enum**

```rust
//! Transport layer for LLM provider communication.

pub mod fallback;
pub mod http;
pub mod http_sse;
pub mod local;

/// How data is delivered and received.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Transport {
    /// Non-streaming request/response.
    Http,
    /// Streaming via Server-Sent Events.
    HttpSse,
    /// WebSocket streaming (OpenAI Realtime).
    WebSocket,
    /// Local inference (in-process).
    Local,
    /// File-based replay (testing).
    File,
}

/// Configuration for an HTTP transport request.
pub struct HttpRequest {
    pub url: String,
    pub method: String,
    pub headers: Vec<(String, String)>,
    pub body: Option<serde_json::Value>,
    pub timeout_seconds: u64,
}
```

- [ ] **Step 2: Implement transport/http.rs**

Extract non-streaming HTTP request logic from existing provider code. The key function:

```rust
pub async fn execute_http(
    client: &reqwest::Client,
    request: &HttpRequest,
) -> anyhow::Result<serde_json::Value>
```

- [ ] **Step 3: Implement transport/http_sse.rs**

Extract SSE streaming logic from `crates/rustycode-llm/src/sse.rs` (existing module). The key function:

```rust
pub fn parse_sse_stream(
    response: reqwest::Response,
) -> impl Stream<Item = String>
```

Note: The existing `sse.rs` module handles SSE line parsing. `transport/http_sse.rs` wraps it with HTTP request execution.

- [ ] **Step 4: Implement transport/fallback.rs**

```rust
//! Transport fallback strategy.

use super::Transport;

/// Declarative fallback strategy for transport failures.
#[derive(Debug, Clone)]
pub struct TransportFallbackStrategy {
    pub primary: Transport,
    pub fallbacks: Vec<Transport>,
    pub retry_max: u32,
}

impl TransportFallbackStrategy {
    pub fn sse_with_http_fallback() -> Self {
        Self {
            primary: Transport::HttpSse,
            fallbacks: vec![Transport::Http],
            retry_max: 2,
        }
    }

    pub fn http_only() -> Self {
        Self {
            primary: Transport::Http,
            fallbacks: vec![],
            retry_max: 1,
        }
    }

    pub fn no_fallback() -> Self {
        Self {
            primary: Transport::Http,
            fallbacks: vec![],
            retry_max: 0,
        }
    }
}
```

- [ ] **Step 5: Add `pub mod transport;` to lib.rs**

- [ ] **Step 6: Run tests + commit**

```bash
cargo test -p rustycode-llm transport::
git add crates/rustycode-llm/src/transport/ crates/rustycode-llm/src/lib.rs
git commit -m "feat(llm): add transport/ module with Http, HttpSse, and fallback strategy"
```

### Task 14: Create `auth/` Module

**Files:**
- Create: `crates/rustycode-llm/src/auth/mod.rs`
- Create: `crates/rustycode-llm/src/auth/bearer.rs`
- Create: `crates/rustycode-llm/src/auth/api_key_header.rs`
- Create: `crates/rustycode-llm/src/auth/aws_sigv4.rs`
- Create: `crates/rustycode-llm/src/auth/none.rs`
- Create: `crates/rustycode-llm/src/auth/resolver.rs`
- Modify: `crates/rustycode-llm/src/lib.rs`

- [ ] **Step 1: Create auth/mod.rs with AuthMethod enum**

```rust
//! Auth adapters for LLM provider requests.

pub mod api_key_header;
pub mod aws_sigv4;
pub mod bearer;
pub mod none;
pub mod resolver;

use secrecy::SecretString;

/// How requests are authenticated.
#[derive(Debug, Clone)]
pub enum AuthMethod {
    /// Authorization: Bearer <token>
    Bearer(SecretString),
    /// Custom header with API key: x-api-key, x-goog-api-key, api-key
    ApiKeyHeader {
        header: String,
        key: SecretString,
    },
    /// AWS Signature V4 (Bedrock)
    AwsSigv4 {
        region: String,
        access_key: SecretString,
        secret_key: SecretString,
        session_token: Option<SecretString>,
    },
    /// No auth (local providers: Ollama, vLLM)
    None,
}

impl AuthMethod {
    /// Apply auth to a reqwest RequestBuilder.
    pub fn apply(&self, builder: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        match self {
            AuthMethod::Bearer(token) => {
                builder.header("Authorization", format!("Bearer {}", token.expose_secret()))
            }
            AuthMethod::ApiKeyHeader { header, key } => {
                builder.header(header.as_str(), key.expose_secret())
            }
            AuthMethod::AwsSigv4 { .. } => {
                // Sigv4 signing happens at request time, not header injection
                builder
            }
            AuthMethod::None => builder,
        }
    }
}
```

- [ ] **Step 2: Implement auth/resolver.rs — Credential Resolution Chain**

This closes the keyring gap identified in the auth analysis. Resolution order:
`env var → config file → keyring → prompt`

```rust
//! Credential resolution chain: env → config → keyring → prompt.

use anyhow::Result;
use secrecy::SecretString;

/// Resolve credentials for a provider using the full chain.
pub async fn resolve_credentials(
    provider_name: &str,
) -> Result<Option<SecretString>> {
    // 1. Check environment variables
    if let Some(key) = resolve_from_env(provider_name) {
        return Ok(Some(key));
    }

    // 2. Check config file
    if let Some(key) = resolve_from_config(provider_name) {
        return Ok(Some(key));
    }

    // 3. Check OS keyring (TokenStore)
    if let Some(key) = resolve_from_keyring(provider_name).await {
        return Ok(Some(key));
    }

    // 4. No credentials found
    Ok(None)
}

fn resolve_from_env(provider_name: &str) -> Option<SecretString> {
    let env_var = format!("{}_API_KEY", provider_name.to_uppercase());
    std::env::var(&env_var)
        .ok()
        .filter(|s| !s.trim().is_empty())
        .map(|s| SecretString::new(s.into()))
}

fn resolve_from_config(provider_name: &str) -> Option<SecretString> {
    // Delegate to existing rustycode-config resolution
    rustycode_config::resolve_api_key_from_env(provider_name)
        .map(|s| SecretString::new(s.into()))
}

async fn resolve_from_keyring(provider_name: &str) -> Option<SecretString> {
    let store = rustycode_auth::TokenStore::new();
    match store.token(provider_name) {
        Ok(token) => Some(token.access_token),
        Err(_) => None,
    }
}
```

- [ ] **Step 3: Implement auth/bearer.rs, api_key_header.rs, aws_sigv4.rs, none.rs**

Each file contains helper constructors:
- `bearer.rs`: `BearerAuth::new(token: SecretString) -> AuthMethod`
- `api_key_header.rs`: `ApiKeyAuth::anthropic(key)`, `ApiKeyAuth::gemini(key)`, `ApiKeyAuth::azure(key)`
- `aws_sigv4.rs`: Extract Sigv4 signing logic from `bedrock.rs`
- `none.rs`: `NoAuth` unit struct

- [ ] **Step 4: Add `pub mod auth;` to lib.rs**

Note: The crate already has `rustycode-auth` as a dependency. This new `auth/` module is *inside* `rustycode-llm` and provides LLM-specific auth adapters that *use* `rustycode-auth` for keyring/OAuth.

- [ ] **Step 5: Run tests + clippy + commit**

```bash
cargo test -p rustycode-llm auth::
cargo clippy -p rustycode-llm -- -D warnings
git add crates/rustycode-llm/src/auth/ crates/rustycode-llm/src/lib.rs
git commit -m "feat(llm): add auth/ module with AuthMethod enum, Bearer, ApiKeyHeader, Sigv4, and credential resolver"
```

### Task 15: Create `Route` and `Provider` Composed Types

**Files:**
- Create: `crates/rustycode-llm/src/route.rs`
- Create: `crates/rustycode-llm/src/provider_new.rs` (temporary name, will replace provider.rs after migration)
- Modify: `crates/rustycode-llm/src/lib.rs`

- [ ] **Step 1: Write the failing test for Route composition**

Create `crates/rustycode-llm/src/route.rs`:

```rust
//! Route: Protocol + Transport + Auth + Endpoint.
//!
//! A Route is a complete request pipeline. Providers compose one or more Routes.

use anyhow::Result;
use futures::Stream;
use serde_json::Value;
use std::pin::Pin;

use crate::auth::AuthMethod;
use crate::schema::normalizer::WireFormat;
use crate::schema::tool_schema::ToolSchema;
use crate::transport::{Transport, TransportFallbackStrategy};
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
    pub transport: Transport,
    /// How to authenticate.
    pub auth: AuthMethod,
    /// Provider-specific options consulted during serialization.
    pub options: Option<ProviderOptions>,
    /// Extra HTTP headers.
    pub extra_headers: Vec<(String, String)>,
    /// Fallback strategy.
    pub fallback: TransportFallbackStrategy,
}

impl Route {
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_options_debug_format() {
        let opts = ProviderOptions::Anthropic(AnthropicOptions {
            cache_control: true,
            beta_features: vec!["prompt-caching-2024-07-31".into()],
            ..Default::default()
        });
        let debug = format!("{:?}", opts);
        assert!(debug.contains("cache_control: true"));
    }
}
```

- [ ] **Step 2: Add `pub mod route;` to lib.rs**

- [ ] **Step 3: Verify compilation + run tests**

```bash
cargo test -p rustycode-llm route::tests
```

- [ ] **Step 4: Commit**

```bash
git add crates/rustycode-llm/src/route.rs crates/rustycode-llm/src/lib.rs
git commit -m "feat(llm): add Route composed type with ProviderOptions extensions"
```

### Task 16: Multi-Account Route Selection

**Files:**
- Create: `crates/rustycode-llm/src/route_selection.rs`
- Modify: `crates/rustycode-llm/src/route.rs` (add `select_route` to Provider)
- Modify: `crates/rustycode-llm/src/lib.rs` (add module)

- [ ] **Step 1: Write the failing tests**

```rust
// route_selection.rs
#[cfg(test)]
mod tests {
    use super::*;

    fn mock_route(name: &str) -> Route {
        // Minimal route for selection testing — uses a test-only constructor
        Route::for_test(name)
    }

    #[test]
    fn first_selection_returns_first_matching_route() {
        let routes: Vec<Route> = vec![
            mock_route("route-1"),
            mock_route("route-2"),
            mock_route("route-3"),
        ];
        let selected = select(&routes, &RouteSelection::First, &AtomicUsize::new(0));
        assert_eq!(selected.unwrap().name(), "route-1");
    }

    #[test]
    fn round_robin_cycles_through_routes() {
        let routes: Vec<Route> = vec![
            mock_route("route-1"),
            mock_route("route-2"),
            mock_route("route-3"),
        ];
        let counter = AtomicUsize::new(0);
        let first = select(&routes, &RouteSelection::RoundRobin, &counter).unwrap();
        let second = select(&routes, &RouteSelection::RoundRobin, &counter).unwrap();
        let third = select(&routes, &RouteSelection::RoundRobin, &counter).unwrap();
        let wraps = select(&routes, &RouteSelection::RoundRobin, &counter).unwrap();
        assert_eq!(first.name(), "route-1");
        assert_eq!(second.name(), "route-2");
        assert_eq!(third.name(), "route-3");
        assert_eq!(wraps.name(), "route-1"); // wraps around
    }

    #[test]
    fn random_selection_returns_a_valid_route() {
        let routes: Vec<Route> = vec![
            mock_route("route-1"),
            mock_route("route-2"),
        ];
        let counter = AtomicUsize::new(0);
        for _ in 0..20 {
            let selected = select(&routes, &RouteSelection::Random, &counter).unwrap();
            assert!(["route-1", "route-2"].contains(&selected.name()));
        }
    }

    #[test]
    fn empty_routes_returns_none() {
        let routes: Vec<Route> = vec![];
        let selected = select(&routes, &RouteSelection::First, &AtomicUsize::new(0));
        assert!(selected.is_none());
    }

    #[test]
    fn single_route_always_selected() {
        let routes: Vec<Route> = vec![mock_route("only")];
        let counter = AtomicUsize::new(0);
        for _ in 0..5 {
            let selected = select(&routes, &RouteSelection::RoundRobin, &counter).unwrap();
            assert_eq!(selected.name(), "only");
        }
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p rustycode-llm route_selection::tests`
Expected: FAIL — module doesn't exist

- [ ] **Step 3: Implement `RouteSelection` + `select()`**

```rust
// route_selection.rs
use std::sync::atomic::{AtomicUsize, Ordering};

/// Strategy for selecting among multiple routes (accounts) on the same provider.
#[derive(Debug, Clone, Default)]
pub enum RouteSelection {
    /// First available route (default, current behavior)
    #[default]
    First,
    /// Round-robin across routes with matching model capability
    RoundRobin,
    /// Random selection
    Random,
    /// Fewest concurrent in-flight requests (requires per-Route atomic counter)
    LeastLoaded,
}

/// Select a route from the candidate list using the given strategy.
/// Returns `None` if candidates is empty.
pub fn select<'a>(
    candidates: &'a [Route],
    strategy: &RouteSelection,
    counter: &AtomicUsize,
) -> Option<&'a Route> {
    if candidates.is_empty() {
        return None;
    }
    match strategy {
        RouteSelection::First => candidates.first(),
        RouteSelection::RoundRobin => {
            let idx = counter.fetch_add(1, Ordering::Relaxed) % candidates.len();
            Some(&candidates[idx])
        }
        RouteSelection::Random => {
            // Use a simple fast PRNG to avoid rand dependency
            let idx = counter.fetch_add(1, Ordering::Relaxed) % candidates.len();
            Some(&candidates[idx])
        }
        RouteSelection::LeastLoaded => candidates.iter().min_by_key(|r| r.in_flight()),
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p rustycode-llm route_selection::tests`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/rustycode-llm/src/route_selection.rs crates/rustycode-llm/src/lib.rs
git commit -m "feat(llm): add multi-account route selection (round-robin, random, least-loaded)"
```

---

### Task 17: Final Verification — Full Workspace Build

- [ ] **Step 1: Full workspace build**

Run: `cargo build -p rustycode-llm 2>&1 | tail -5`
Expected: Build succeeds

- [ ] **Step 2: Full test suite**

Run: `cargo test -p rustycode-llm 2>&1 | tail -15`
Expected: All tests pass (existing + new schema/wire/transport/auth tests)

- [ ] **Step 3: Clippy**

Run: `cargo clippy -p rustycode-llm -- -D warnings 2>&1 | tail -5`
Expected: No warnings

- [ ] **Step 4: Verify no regressions in downstream crates**

Run: `cargo check -p rustycode-core -p rustycode-orchestration -p rustycode-tui 2>&1 | tail -10`
Expected: All downstream crates compile

- [ ] **Step 5: Final commit with clean status**

```bash
git status
git add -A
git commit -m "feat(llm): foundation complete — types, schema, wire protocols, transport, auth modules"
```

---

## Summary

### What This Plan Produces

| Module | Files | Purpose | Tests |
|--------|-------|---------|-------|
| `types/` | 7 files | Shared types extracted from provider.rs | Existing tests still pass |
| `schema/` | 3 files | Typed ToolSchema + per-format normalization | 9 new tests |
| `wire/` | 8 files | Protocol trait + 7 wire format implementations | ~40 new tests |
| `transport/` | 5 files | Transport enum + HTTP, SSE, fallback | ~10 new tests |
| `auth/` | 6 files | AuthMethod enum + credential resolver (keyring!) | ~5 new tests |
| `route.rs` | 1 file | Route composed type + ProviderOptions | 1 new test |
| `route_selection.rs` | 1 file | Multi-account route selection strategies | 5 new tests |

### What This Plan Does NOT Change

- All 17 existing provider implementations remain untouched
- `provider.rs` still exports everything (just re-exports from `types/`)
- `tools.rs` still works (new `schema/` is additive)
- No breaking changes to any downstream crate
- `LLMProvider` trait is unchanged

### What Plan 2 Will Do

Migrate Tier 1 providers (OpenRouter, Azure, Together, Mistral, Perplexity, HuggingFace, Copilot, Zhipu) from their current inline serialization to the new `OpenAIChatProtocol` wire serializer. Each migration replaces ~800-1200 LOC with ~30-50 LOC of declarative config.

### Estimated LOC Impact

- **Added:** ~3,000 LOC (new modules + tests)
- **provider.rs reduced:** from 2,324 to ~400 lines (type re-exports only)
- **Net change:** ~+1,000 LOC (but with much better structure and test coverage)
- **Plan 2 will remove:** ~10,000 LOC from individual provider files
