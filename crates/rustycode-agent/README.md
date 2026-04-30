# rustycode-agent

The shared thin LLM↔tool loop. No heuristics. No nudges. No behavioral injection.

## Architecture

```
Interface (TUI / CLI / Bench)
     │  implements AgentEvents
AgentSession::run()          ← this crate
     │  uses
LLMProvider + ToolRegistry
```

## Usage

```rust
use rustycode_agent::{AgentSession, AgentConfig, AgentEvents, AgentResult};

struct MyEvents;
impl AgentEvents for MyEvents {
    fn on_text_delta(&mut self, delta: &str) { print!("{delta}"); }
    fn on_tool_call(&mut self, id: &str, name: &str, input: &serde_json::Value) { /* ... */ }
    fn on_tool_result(&mut self, id: &str, name: &str, output: &str, is_error: bool) { /* ... */ }
    fn on_done(&mut self, result: &AgentResult) { println!("Done: {} turns", result.total_input_tokens); }
}

let session = AgentSession::new(AgentConfig::default(), cwd);
let result = session.run(&provider, "claude-sonnet-4-5", SYSTEM_PROMPT, messages, &tools_schema, &registry, &mut MyEvents).await?;
```

## What it does

- LLM call → stream → tool dispatch → append results → repeat
- Stops when: no tool calls, max turns, wall-clock timeout
- Context pruning (3-phase) to fit within budget
- Stream retry on transient errors (429, 503, etc.)
- Context-length recovery (aggressive trim + retry)

## What it does NOT do

- No urgency nudges
- No stagnation detection  
- No behavioral heuristics
- No 240-line system prompts
- No "CRITICAL RULES"

The model drives behavior. The loop enforces mechanical limits.

## Modules

- `session` — `AgentSession`, `AgentConfig`, `AgentResult`, `AgentEvents`, `ApprovalDecision`
- `context` — `prune_messages`, `clean_assistant_text` (lifted from headless/utils.rs)
- `tool_exec` — tool dispatch, truncation, error detection
