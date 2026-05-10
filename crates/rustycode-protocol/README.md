# rustycode-protocol

Core protocol types and shared data structures for RustyCode.

## Purpose

Provides the foundation of RustyCode's type system: domain models for sessions, plans, events, messages, tool execution, and LLM communication. All crates depend on protocol for cross-module communication, ensuring type consistency across the system.

## Key Types

### ID System (Sortable, Human-Readable)

- `SessionId` — Session identifier (prefix: `sess_`)
- `PlanId` — Plan identifier (prefix: `plan_`)
- `EventId` — Event identifier (prefix: `evt_`)
- `MemoryId` — Memory entry identifier (prefix: `mem_`)
- `SkillId` — Skill identifier (prefix: `skl_`)
- `ToolId` — Tool identifier (prefix: `tool_`)
- `FileId` — File identifier (prefix: `file_`)
- `SortableId` — Generic sortable ID (custom prefix)

IDs are:
- **Time-sortable** — Can be sorted chronologically
- **Human-readable** — Prefixes indicate type
- **Compact** — 15-30 chars vs 36 for UUID
- **Collision-free** — Random component prevents duplicates

### Session & Execution

- `Session` — Session context, mode, status
- `SessionMode` — Execution mode (TUI, CLI, Headless, ACP)
- `SessionStatus` — State (Active, Paused, Completed, Failed)
- `Plan` — Step-by-step execution plan
- `PlanStep` — Individual step with status and result
- `PlanStatus` — Plan execution state

### Events & Messages

- `EventKind` — Event classification
- `SessionEvent` — Event in a session (with timestamp, data)
- `Message` — Conversation message (role, content, attachments)
- `MessageContent` — Multi-part message (text, tool calls, results)
- `Conversation` — Ordered message history
- `ToolCall` — Request to execute a tool
- `ToolResult` — Tool execution result (success/failure)

### Tool Execution

- `ToolCall` — Tool invocation with arguments
- `ToolResult` — Result (success with output, or error)
- `ToolMetadata` — Tool signature and capabilities
- `ToolSignature` — Parameter definitions

### Context & Prompts

- `ContextSectionKind` — Section type (code, memory, file, plan, etc.)
- `ContextSection` — Ranked context entry for LLM
- `CompletionRequest` — LLM request with context and messages
- `CompletionResponse` — LLM response with tokens and content

## Public API

```rust
use rustycode_protocol::{SessionId, Message, ToolCall, ToolResult};

// Create sortable IDs
let session_id = SessionId::new();
println!("Session: {}", session_id);  // sess_3w8qN5zX2yK9bF8pD3m

// Create a message
let msg = Message {
    role: "user".to_string(),
    content: MessageContent::Text("Write a function".to_string()),
    ..Default::default()
};

// Create a tool call and result
let call = ToolCall {
    tool_name: "Bash".to_string(),
    arguments: serde_json::json!({ "command": "ls" }),
};

let result = ToolResult {
    tool_call: call,
    success: true,
    output: "file1.rs\nfile2.rs".to_string(),
    error: None,
};
```

## Module Organization

- `session` — Session types and lifecycle
- `plan` — Plan execution types
- `event` — Event system types
- `context` — Context management
- `tool` — Tool execution types
- `message` — Conversation and message types
- `llm` — LLM provider abstractions
- `id` — Sortable ID system

## Design Principles

**No Circular Dependencies:** Protocol only imports std and serde. Every other crate can depend on protocol safely.

**Immutability:** Types immutable by default. Mutations via builder patterns or `with_*` methods.

**Serialization:** All types derive Serialize/Deserialize for persistence.

**Type Safety:** Domain types prevent invalid states.

## Dependencies

- `serde` — Serialization
- `chrono` — Timestamps
- `uuid` — ID generation
- `sha2` — Hashing
- `base64` — Encoding

No async runtime dependency. Protocol is the foundation.

## Testing

Tests verify ID generation, sorting, serialization, and type invariants.

## See Also

- All other crates (every crate depends on protocol)
- `rustycode-core` — Uses protocol for sessions
- `rustycode-llm` — Uses protocol for requests/responses
- `rustycode-tools` — Uses protocol for tool execution
