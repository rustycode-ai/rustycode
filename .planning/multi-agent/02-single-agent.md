# 02 — Single Agent

The single agent is the fundamental execution unit. Everything above it (orchestration, teams,
ensembles) is coordination of multiple single agents.

## AgentSession

*`crates/rustycode-agent-runtime/src/session.rs`*

The live execution engine for a single agent turn sequence. Holds no persistent history —
messages are passed in at call time and returned in `AgentResult`.

```rust
pub struct AgentSession {
    pub config: AgentConfig,
    pub cwd: PathBuf,
    pub intelligence: Option<Box<dyn CodeIntelligence>>,
    pub activation: ToolActivationManager,
    pub hooks: ExpandedHookDispatcher,
    pub message_sender: Option<Arc<dyn MessageSender>>,
    event_tx: broadcast::Sender<EventMsg>,        // capacity 256
    op_rx: Option<mpsc::UnboundedReceiver<Op>>,   // inbound commands
    plugins: Vec<Box<dyn AgentPlugin>>,            // zero overhead when empty
}
```

### Key Design Decision: Injected Provider

Provider and model are **not stored on the struct**. They are injected at call time:

```rust
pub async fn run(
    &mut self,
    provider: &dyn LLMProvider,  // injected — enables per-call model switching
    model: &str,
    system: &str,
    messages: Vec<ChatMessage>,
    tools_schema: &[serde_json::Value],
    tool_registry: &ToolRegistry,
    events: &mut dyn AgentEvents,
) -> Result<AgentResult>
```

This is what enables polyglot orchestration: the same `AgentSession` can be driven with
different models on successive turns.

### Builder Pattern

```rust
let session = AgentSession::new(config, "/project/dir")
    .with_intelligence(repo_map)
    .with_hooks(hook_dispatcher)
    .with_tier(ToolTier::Standard)
    .with_message_sender(sender);
```

---

## AgentConfig

```rust
pub struct AgentConfig {
    pub max_turns: usize,              // default 25
    pub timeout_secs: u64,            // default 900
    pub max_tool_result_bytes: usize,  // default 8000
    pub temperature: f32,              // default 0.2
    pub effort: Option<EffortLevel>,
    pub max_output_tokens: u32,        // default 32768 (GLM-4/5: 65536)
}
```

---

## AgentResult

```rust
pub struct AgentResult {
    pub final_text: String,
    pub messages: Vec<ChatMessage>,
    pub stopped_reason: StoppedReason,
    pub total_input_tokens: u64,
    pub total_output_tokens: u64,
    pub total_cache_read_tokens: u64,
    pub total_cache_creation_tokens: u64,
}

pub enum StoppedReason {
    NoToolCalls,        // model stopped calling tools — task complete
    MaxTurnsReached,    // hard turn cap
    TimeoutExceeded,    // wall-clock timeout
    PluginStopped,      // a plugin requested early stop
}
```

---

## AgentPlugin System

*`crates/rustycode-agent-runtime/src/plugins/mod.rs`*

Plugins observe and optionally modify agent behavior at lifecycle hooks. Empty plugin list
means zero overhead.

```rust
pub trait AgentPlugin: Send + Sync {
    async fn on_start(&mut self, _ctx: &TurnContext) {}
    async fn on_tool_result(
        &mut self,
        _tool_name: &str,
        _tool_id: &str,
        _input: &Value,
        _output: &mut String,
    ) {}
    async fn should_stop(&mut self, _ctx: &TurnContext) -> bool { false }
    async fn on_done(&mut self, _ctx: &TurnContext) {}
}
```

### Built-in Plugins

| Plugin | Purpose |
|--------|---------|
| `EarlyStopPolicy` | Stops the agent when a configurable condition is met |
| `RepetitionDetector` | Detects repeated tool calls with identical inputs |
| `ConversationTrace` | Records full conversation for debugging/replay |

---

## Event System

### EventMsg (outbound)

`AgentSession` emits `EventMsg` on a broadcast channel (capacity 256). Subscribers that lag
receive a `Lagged` notification rather than blocking.

### Op (inbound)

An `mpsc::UnboundedReceiver<Op>` accepts commands to control the running agent (e.g., stop
stream, change behavior mid-turn).

### Lifecycle Hook Sequence

```
SessionStart
  └─ [turn loop]
       PreToolUse  →  tool executes  →  PostToolUse | ToolError
       plugin.on_tool_result() after each tool
       plugin.should_stop() check after each turn
  └─ [end]
SessionEnd
```

---

## Sub-Agents

A sub-agent is a scoped child `AgentSession` created by a parent agent or orchestrator:

- Inherits the parent's `cwd` and `SharedWorkspace`
- Gets a scoped `ToolActivationManager` (restricted tool set)
- Owns its own `ReasoningGraph`
- Returns an `AgentOutcome` (see [04-context-forwarding.md](04-context-forwarding.md)) to the parent

Sub-agents have no special type — they are regular `AgentSession` instances constructed with
restricted context by the parent.
