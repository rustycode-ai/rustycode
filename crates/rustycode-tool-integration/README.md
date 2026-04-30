# rustycode-tool-integration

Bridge between LLM providers and tool execution.

## Purpose

Provides integration layer between LLM tool calls and tool executor. Parses tool calls from LLM responses, validates arguments, executes tools, and formats results back into LLM format.

## Key Types

- `ToolIntegration` — Main integration coordinator
- `ToolCallParser` — Parses tool calls from LLM responses
- `ArgumentValidator` — Validates tool arguments against schema
- `ToolResultFormatter` — Formats tool results for LLM
- `ToolExecutionContext` — Execution context with permissions

## Workflow

```
LLM Response
    ↓
Parse Tool Calls
    ↓
Validate Arguments
    ↓
Execute Tools
    ↓
Format Results
    ↓
Return to LLM
```

## Public API

```rust
use rustycode_tool_integration::{ToolIntegration, ToolCallParser};

// Create integration with executor and registry
let integration = ToolIntegration::new(
    executor,
    registry,
    permissions
)?;

// Parse tool calls from LLM response
let tool_calls = integration.parse_llm_response(&llm_response)?;

// Execute each tool call
for call in tool_calls {
    let result = integration.execute_and_format(&call).await?;
    // Feed result back to LLM
    messages.push(result);
}
```

## Tool Call Formats Supported

- **Claude** — `<function_calls>` XML format
- **OpenAI** — `tool_calls` in response format
- **Gemini** — Function calls in response
- **Generic** — JSON tool call format

## Validation

Validates before execution:
- Tool exists in registry
- All required arguments provided
- Argument types match schema
- Arguments within constraints
- Permissions granted

Returns clear error messages for validation failures.

## Result Formatting

Formats tool results back into LLM format:
- Successful results as tool_result
- Errors as error messages
- Structured data as JSON
- Large results with truncation

## Dependencies

- `rustycode-tools` — Tool execution
- `rustycode-tools-registry` — Tool discovery
- `rustycode-tools-api` — Tool traits
- `rustycode-llm` — LLM response types
- `rustycode-protocol` — Core types
- `regex` — Parsing tool calls
- `serde_json` — JSON handling
- `anyhow` — Error handling

## Architecture Notes

Parsing is provider-agnostic via format adapters. Each provider has parser that converts to canonical ToolCall format.

Validation ensures safe execution: unknown tools rejected, dangerous arguments blocked.

Formatting preserves context for LLM: includes tool name, arguments, result for reasoning.

## Testing

Tests verify parsing for all providers, validation rules, safe execution, and formatting.

## See Also

- `rustycode-tools` — Tool execution
- `rustycode-llm` — LLM provider
- `rustycode-core` — Session using tool integration
