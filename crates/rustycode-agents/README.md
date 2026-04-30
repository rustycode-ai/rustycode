# rustycode-agents

Agent implementations for autonomous development tasks in RustyCode.

## Purpose

Provides a collection of specialized agents for different development activities: code generation, code review, test generation, and debugging. Each agent is a specialized orchestrator that coordinates with the LLM provider, tool execution, and other subsystems to complete its task.

## Key Types

- `Agent` — Base trait for all agent implementations with lifecycle management
- `AgentConfig` — Configuration for agent behavior and constraints
- `AgentResult` — Structured result from agent execution with output and metadata
- `CodeAgent` — Generates and implements code based on requirements
- `ReviewAgent` — Analyzes code for correctness, style, and security issues
- `TestAgent` — Generates test cases and executes them
- `DebugAgent` — Diagnoses bugs and suggests fixes

## Public API

```rust
use rustycode_agents::{Agent, CodeAgent, AgentConfig};

// Create and configure an agent
let config = AgentConfig::default()
    .with_model("claude-opus-4-7")
    .with_max_iterations(10);

let agent = CodeAgent::new(config);

// Run the agent
let result = agent.execute(
    "Implement a binary search function",
    &tool_executor,
    &llm_provider
).await?;

println!("Output: {}", result.output);
println!("Success: {}", result.success);
```

## Agent Types

- **CodeAgent** — Writes code, implements features, follows TDD patterns
- **ReviewAgent** — Reviews code for bugs, style violations, security issues
- **TestAgent** — Generates unit/integration tests with high coverage
- **DebugAgent** — Analyzes error traces, suggests root causes and fixes

## Dependencies

- `rustycode-llm` — LLM provider interface
- `rustycode-tools` — Tool execution
- `rustycode-protocol` — Shared types
- `tokio` — Async runtime
- `anyhow` — Error handling

## Architecture Notes

Each agent follows a common pattern:
1. Accept a task specification and constraints (model, iterations, tools available)
2. Interact with the LLM provider using prompts tailored to the agent type
3. Parse LLM responses and call appropriate tools
4. Iteratively refine results based on feedback
5. Return structured result with success/failure status and artifacts

Agents are composed with tool executors and LLM providers at runtime — they have no direct dependency on implementation details of either.

## Testing

Unit tests verify agent behavior with mocked LLM and tool responses. Integration tests run agents against real tool implementations to verify end-to-end flows.

## See Also

- `rustycode-core` — Session lifecycle (agents run within sessions)
- `rustycode-llm` — LLM provider trait that agents use
- `rustycode-tools` — Tool execution framework
- `rustycode-tui-agents` — TUI integration for agent output display
