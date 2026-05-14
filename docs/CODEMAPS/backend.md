# Backend: LLM Providers, Tools, Orchestration

<!-- Generated: 2026-05-14 | Files scanned: 1601 | Token estimate: ~800 -->

## LLM Providers (rustycode-llm, 42K LOC)

| Provider | File | Notes |
|----------|------|-------|
| Anthropic | `anthropic.rs`, `anthropic_streaming.rs`, `anthropic_advanced_tools.rs` | Primary, streaming SSE, tool use |
| OpenAI-compatible | `openai_chat.rs` | Also covers Together, OpenRouter |
| Azure OpenAI | `azure.rs` | Enterprise endpoint |
| AWS Bedrock | `bedrock.rs` | Via AWS SDK |
| Google Gemini | `gemini.rs` | Native Gemini API |
| Ollama | `ollama.rs` | Local models |
| Cohere | `cohere.rs` | Command series |
| Mistral | `mistral.rs` | La plateforme |
| GitHub Copilot | `copilot.rs` | Copilot Chat API |
| HuggingFace | `huggingface.rs` | Inference endpoints |
| Perplexity | `perplexity.rs` | Sonar models |
| z.ai / GLM | `zhipu.rs` | Chinese provider |
| LiteRT | `litert_lm.rs` (in rustycode-litert) | On-device inference |
| Replay | `replay_provider.rs` | Replays recorded sessions |
| Mock | `mock.rs` | Testing |
| Fallback | `provider_fallback.rs` | Multi-provider failover |

**Key subsystems:**
- `client_pool.rs` — connection pooling, concurrent request multiplexing
- `circuit_breaker.rs` — per-provider fault tolerance
- `cost_tracker.rs` — token usage + cost accounting
- `caching.rs` — response caching layer
- `compaction.rs` — conversation context compression
- `registry.rs` — ProviderRegistry, ProviderMetadataRegistry
- `wire/` — protocol handlers (OpenAI chat, Anthropic SSE, streaming)
- `transport/` — HTTP/WS transport abstraction

## Tool Providers (rustycode-tools, 69K LOC)

### Core Tools
| Tool | File | Purpose |
|------|------|---------|
| bash | `providers/bash/` | Shell command execution with validation |
| read_file | `providers/read_file.rs` | File reading |
| write_file | `providers/write_file.rs` | File writing with diff output |
| edit_file | `providers/edit_file.rs` | 3-strategy matching (exact/normalized/trimmed) |
| glob | `providers/glob.rs` | File pattern search |
| grep | `providers/grep.rs` | Content search with type filters |
| web_fetch | `providers/web_fetch.rs` | HTTP fetch + content extraction |
| notebook | `providers/notebook.rs` | Jupyter cell editing |

### LSP Tools (`providers/lsp/`)
analyze_symbol, code_actions, completion, definition, diagnostics,
document_symbols, extract_symbol, formatting, hover, inline_symbol,
insert_after_symbol, insert_before_symbol, references, rename,
rename_symbol, replace_symbol_body, safe_delete_symbol, symbols_overview,
workspace_symbols

### Agent/Orchestration Tools
| Tool | File | Purpose |
|------|------|---------|
| ask_user_question | `providers/ask_user_question.rs` | Human-in-the-loop |
| delegation_tool | `providers/delegation_tool.rs` | Sub-agent dispatch |
| send_message | `providers/send_message.rs` | Inter-agent messaging |
| goal | `providers/goal.rs` | Task goal management |
| brief | `providers/brief.rs` | Context briefing |
| explore | `providers/explore.rs` | Codebase exploration |
| reasoning_types | `providers/reasoning_types.rs` | Structured reasoning |
| skill_discovery | `providers/skill_discovery.rs` | Skill lookup |
| tool_search | `providers/tool_search.rs` | Dynamic tool lookup |

### Infrastructure
- `executor/` — tool execution framework
- `security/` — path/command validation, filename blocking
- `indexing/` — CodeIndex, RepoMap, SemanticSearchTool
- `providers/bash/validation.rs` — command allowlist/blocklist
- `side_effects.rs` — SideEffectLedger for recovery

## Orchestration (rustycode-orchestration, 72K LOC)

### Execution Strategies
- `autonomous/` — autonomous dev loop
- `ensemble_strategy/` — multi-agent ensemble
- `fork_join/` — parallel task execution
- `plan_mode/` — plan-then-execute
- `conductor/` — multi-agent orchestration

### Reasoning & Quality
- `thinking/` — structured reasoning (DAG thought graph, confidence scoring)
- `ast/` — AST-based code analysis pipeline
- `quality_detector/` — output quality assessment
- `verification_gates/` — pre/post condition checks
- `skeptic/` — adversarial review agent
- `judge/` — quality judgment

### Planning
- `milestone_prompt/` — milestone-aware prompting
- `plan_refiner/` — plan iteration
- `task_decomposer/` — task breakdown
- `phase/`, `phase_lifecycle/` — execution phases

### Agent Management
- `agent_executor/` — single agent execution
- `agent_registry/`, `worker_registry/`, `team_registry/` — agent registration
- `cron_registry/` — scheduled tasks
- `handoff/` — agent handoff protocol
- `mailbox_router/`, `mailbox_sender/` — inter-agent messaging

### Infrastructure
- `context/`, `domain_context/` — context window management
- `cost_table/` — model cost data
- `routing/`, `routing_metrics/` — request routing
- `failure_store/` — failure pattern recording
- `recovery/` — checkpoint-based recovery
- `session/` — orchestration session state
