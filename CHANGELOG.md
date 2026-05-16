# Changelog

All notable changes to this project will be documented in this file.

## [0.5.7] - 2026-05-16

### Bug Fixes

- Remove dead code in llm utils and compaction

### Refactor

- Clean up protocol, bench, skill, storage, acp, mcp
- **tools**: Consolidate security patterns, clean up tool providers
- **tui**: DirtyFlags bitfield, handler cleanup, memory auto improvements

### Chores

- Remove rustycode-web crate (22k LOC)
- Remove rustycode-search and rustycode-semantic-search crates

## [0.5.6] - 2026-05-16

### Features

- **tui**: Improve message rendering with diff highlighting and expand state

### Bug Fixes

- **llm**: Handle Gemini thinking tokens in usage reporting

### Refactor

- **protocol**: Move RetryState and ErrorCategory from orchestration
- **tools**: Deduplicate code_context, explore, find_symbol implementations

### Chores

- Bump version to 0.5.6

## [0.5.5] - 2026-05-15

### Features

- **llm**: Emit CacheUsage stream events from Anthropic message_delta
- **bench**: Add import analysis and debugging prompts to SWE-bench

### Bug Fixes

- Correct doctest assertion, add missing Fatal category, hoist regexes to LazyLock statics

### Performance

- **tui**: Cache layout chunks to avoid recomputation on every frame

### Chores

- Bump version to 0.5.5

## [0.5.4] - 2026-05-15

### Features

- **bench**: Improve SWE-bench agent with early stop, TUI agent refinements
- **tui**: Add feature flag system for progressive capability rollout
- **scripts**: Add model catalog update script
- Created comprehensive EXTRACTION_MAP.md mapping all 29 handler fu…
- **tui**: Establish SessionStreamingFeature for Task 12 drain integration pattern
- **tui**: Add SessionFeature and ToolPanelFeature for Task 12
- **tui**: Add WorkspaceFeature for Task 12 integration
- **tui**: Complete drain loop with StreamChunk conversion to TuiEvent
- **tui**: Integrate drain loop into AppShell::poll_and_route()
- **tui**: Export HelpFeature from features module
- **bench**: Add source snippet pre-loading to SWE-bench experiment
- **runtime**: Add source context zone and nudge engine to compaction
- **bench**: Improve SWE-bench experiment with debugging prompts and import analysis

### Bug Fixes

- **tui**: Use display-width padding for scroll indicators
- **tui**: Batch of TUI bug fixes from Rounds 1-16
- **tui**: R17 bug fixes — session reset, approval queue, agent cleanup, file finder
- **tui**: Streaming system prompt refinement, height calculation, service polling
- **tui**: Use correct model ID in compaction context monitor test
- **tui**: Clippy fixes and refinements across feature modules
- **tui**: Replace unwrap with poison-recovery in search feature mutex locks
- **tui**: Harden 20+ panic/crash/race bugs across TUI runtime
- **tui**: Harden concurrency, clipboard, and error handling bugs
- **tui**: Honest timeout — join thread on budget exceeded, return actual result

### Refactor

- **llm**: Unify provider error handling, update registry and intent classification
- **providers**: Extract model catalog into shared module
- **tui**: Add AppShell shell alongside existing TUI — step 0.4
- **tui**: Extract Plugin Manager state into feature module — step 1.1
- **tui**: Add Plugin Manager integration tests to AppShell — step 1.2
- **tui**: Feature-based plugin manager, shell dispatch, legacy cleanup
- **tui**: Replace hardcoded tool name strings with constants

### Chores

- Unify all crate versions to 0.5.3
- Add SWE-bench experiment scripts, evaluation tools, and benchmark docs
- **tui**: Cargo fmt fixup
- Bump version to 0.5.4

## [0.5.3] - 2026-05-14

### Refactor

- **tui**: Extract system_prompt module, refactor streaming response handling

### Chores

- Bump version to 0.5.3

## [0.5.2] - 2026-05-14

### Features

- **bench**: Expand SWE-bench runner with prediction scoring and agent improvements
- **runtime**: Extend piggyback compaction with adaptive budget and quality metrics
- **tui**: Add compaction state, tool panel, input improvements, and toast notifications

### Documentation

- Add Configuration section with correct JSON format

### Chores

- Bump version to 0.5.2

## [0.5.1] - 2026-05-14

### Features

- **orchestration**: Add ReasoningGraph::summarize(), wire into handoff and agent_outcome
- **bench**: Add complex-mode thinking_guide, A/B testing flag, rework system prompt
- **agent-runtime**: Add TaskBrief delegated-agent contract with tool gating
- **agent-runtime**: Add SWE-bench lifecycle simulation tests, align TUI with orchestration
- **tools**: Add ToolCtx slim execution context, refactor glob/grep to use it
- **runtime**: Expand compaction piggyback with budget tracking and tier strategies

### Bug Fixes

- **llm**: Allow trivially_copy_pass_by_ref for serde serialize_temperature
- **tui**: Use display_width for unicode correctness, guard zero-area renders

### Refactor

- **tui**: Extract clear_on_cleanup() for terminal progress
- **tools**: Resolve all ToolExecutor/ToolInfo naming collisions across workspace
- **tui**: VecDeque for doom loop, error handling for tool approval, unicode display width

### Documentation

- Review and update ADR 004 and tool interface design
- Update ADR 004 and tool interface design with ToolCtx and plugin system

### Chores

- Add GSD and generated dirs to .gitignore
- Add planning docs, tool plugin design, code maps, reports, and test scripts
- **tui**: Apply cargo fmt formatting
- Bump version to 0.5.1

## [0.5.0] - 2026-05-14

### Features

- **protocol**: Add EventMsg unified event channel and ToolName namespace
- **agent-runtime**: Phase 1B dual emission — broadcast EventMsg alongside StreamEvent callbacks
- Expand EventMsg with session, orchestration, memory events and Op variants
- **tui**: Add shadow mode validation counters to TuiAgentBridge
- Enhance Op/EventMsg protocol, add Op receiver and StopStream to agent runtime
- Add rustycode-git, rustycode-search, and rustycode-semantic-search crates
- **tools**: Add Java/C++/Scala/Kotlin tree-sitter support, fix TypeScript query, harden parse_with_treesitter
- **tui**: Add symbol outline UI panel, update state model and workspace handlers
- **indexing**: Reclassify Function → Method inside impl/class/trait
- **tools**: Fix Function vs Method classification in tree-sitter extraction
- **agent-runtime**: Extract bench heuristics into AgentSession plugins
- **bench**: Add TUI-aligned bench agent, unify provider creation
- Thin CodeAgent, thinking_guide tool, SSE multi-event parsing, multi-agent planning

### Bug Fixes

- **ci**: Remove redundant build-release.yml, fix trigger-cicd dispatch
- **tui**: Fix broken /marketplace search and /memory search slash commands
- **tui**: Fix off-by-one in all marketplace subcommand handlers and config keybinding
- **tools-api**: Lowercase explore keywords to fix profile detection
- **tui**: Shared MCP manager, eliminate double-spawns, add timeouts, fix dispatch
- **agent-runtime**: Emit TurnCompleted when provider omits stop_reason
- Prevent stack overflows in serde_json and improve event/tui robustness
- 7 bug fixes across storage, tools, and apply_patch
- Replace expect/let-_ with proper error handling in gemini, harness, server, and search store
- Handle poisoned mutex in anthropic SSE stream parser
- 8 bug fixes across ws-server, team, and TUI crates (Round 7)
- **security**: Replace fastrand with rand for OAuth secrets, restrict token file perms, surface silent errors
- **connector**: Log AppleScript close-session failures instead of silently swallowing
- **tui,server**: Improve TUI event loop robustness and server-client integration
- **tools**: Adjust Kotlin tree-sitter API call, mark new languages as todo in query extractor
- **session,git**: 8 bug fixes across session and git crates
- **tools**: Namespace-aware tool resolution, simplify tree-sitter languages, harden query parsing
- 11 bugfixes across 7 crates (Round 12)
- Break TUI feedback loops, fix indexing queries, remove stack size bandaids
- Improve robustness across orchestration, tenacity, and symbol extraction
- Add impl_item query extraction, harden LRU cache and edit history
- Harden error handling across guard, mcp, runtime, team, ws-server, and cli
- Add PluginStopped variant to StoppedReason, handle in agent_executor

### Refactor

- **tui**: Complete god object state refactoring - Phase 2-4
- **tui**: Fix remaining state refactoring field access paths across handlers and renderers
- **tui**: Add submit_op method and migrate approval path to Op protocol
- **tui**: Migrate remaining approval call to submit_op
- **tui**: Add missing Op import to special_handlers
- **core**: Split impl Runtime into domain files and clean up lib.rs
- **rustycode-core**: Consolidate checkpoint files into recovery/ module (Phase 3)
- Rename misnamed event_loop files and consolidate context modules
- **tools**: Split god object files into focused submodules
- **skills**: Migrate CLI skills command from SkillRegistry to SkillManager
- Split monolithic modules into subdirectories, add server crates
- Extract symbol indexing into symbols module, add symbol tools and search providers
- **tools**: Improve tree-sitter queries for Rust/JS, refine symbol hashing and orchestrator
- **tools**: Update symbol tools registration and structural patch implementation
- **tui**: Rename misleading 'legacy streaming' to 'agent streaming'

### Documentation

- Add ADR-0004 unified code structure engine, guides, and drift-check script
- Add plan for unifying agent execution paths

### Testing

- **rustycode-core**: Migrate lib.rs unit tests to tests/lib_tests.rs
- Add regression tests for bug fixes across 5 crates

### Chores

- Fix async test, resolve activation findings, add architecture docs
- Add workspace dependency aliases for core crates
- Bump version to 0.5.0

## [0.4.0] - 2026-05-11

### Features

- **bench**: Add token usage tracking to TrialResult and BenchAgent trait
- **bench**: Add token/cost fields to BenchmarkResults and TaskResult
- **tools-api**: Add InputValidator, TokenAccountant, PrivilegeGate traits
- **bench**: Pre-validate required tool args before execution
- Add GA4 analytics module + tool schema/annotations refactor
- Bench multi-provider, task_context module, bus/team tools, production hardening status doc
- **llm**: Add schema, wire, transport, auth, route modules for provider abstraction
- **llm**: Add shared ModelCache with dynamic provider model discovery
- **llm**: Wire ModelCache into Gemini and OpenAI providers
- **tui**: Async service init with loading screen
- **tui**: Async service init with loading screen
- **tui**: Show loading screen during service init

### Bug Fixes

- Resolve 22 bugs across 7 crates from systematic bug audit
- Resolve 12 bugs across orchestration, CLI, and TUI (wave 2)
- Resolve pre-existing clippy errors and test compilation issues
- Resolve 3 confirmed bugs from Oracle verification
- Resolve 30+ bugs across 10 crates (waves 5-6)
- Resolve 10 critical/high/medium bugs across 5 crates (wave 7)
- Replace silent error suppression with proper logging and remove debug artifacts
- **acp**: Resolve lock-across-await concurrency bugs in prompt handler and LLM integration
- Resolve 22 clippy warnings across 20 files (round 5)
- **acp**: Update test to match empty messages behavior
- Remove unused sha2 imports after crypto centralization
- Resolve 4 resource leaks — zombie processes, missing Drop, unenforced timeout
- Replace 15+ scattered len()/4 token heuristics with canonical estimate_tokens()
- **security**: Reject ALL tools on approval timeout, not just dangerous ones
- **security**: Harden input validation — LSP Content-Length cap + env injection allowlist
- **robustness**: Eliminate .expect() panics in signal handlers and runtime creation
- **concurrency**: Async correctness — cancellation, timeout abort, deadlock elimination
- **orchestration**: Tool error status, stream handling, phase transitions, input logic
- **llm**: Repair broken references after type extraction to types/
- **llm**: Real Sigv4 signing, random route hash, task contracts
- **leaks**: Eliminate zombie child processes and fire-and-forget spawns
- **concurrency**: Eliminate TOCTOU races and await_holding_lock
- **io**: Add file size guards to prevent OOM from unbounded reads
- **medium**: Preserve error context, use async I/O, remove unnecessary clone
- **tests**: Enable structured output in git test helper and fix edit assertion
- **tui**: Version display, Ctrl+W behavior, and tool name assertion
- **llm**: Guard Gemini toolConfig when no tools present, add startup perf instrumentation
- **llm**: Strip SSE data: prefix before protocol parsing
- **llm,tui**: Gemini provider reliability — safety stops, rate limit retry, and SSE tracing
- **agent-runtime**: Classify rate limit errors as Transient, not Fatal
- **llm**: Omit tools/toolConfig keys from Gemini JSON when absent

### Refactor

- Remove dead code from todo.rs
- **tui**: Remove dead code, disabled examples, and unused helpers
- **tools**: Remove dead code and unused plan template methods
- **core**: Remove dead code, unused variants, and retry prompts
- Remove unnecessary lint suppressions and fix pre-existing clippy issues
- **llm**: Replace map_or with idiomatic is_none_or/is_some_and
- Apply clippy-driven cleanups across 7 files
- Session-scoped worktree CWD + tracing instrumentation
- Remove CLI bench command, relocate headless utils, clean gitignore
- Consolidate duplicate test_ctx() helpers into shared module
- Replace verbose empty JSON object pattern with json!({})
- Deduplicate format_timestamp between CLI and storage
- Consolidate strip_ansi_escapes into rustycode-protocol
- Consolidate truncate, normalize_tool_name, and format_duration duplicates
- **llm**: Add empty types/ module skeleton for provider.rs extraction
- **llm**: Extract message types to types/message.rs
- **llm**: Migrate Cohere provider from inline serialization to Route+CohereProtocol
- **llm**: Migrate Ollama provider from inline serialization to Route abstraction
- **llm**: Migrate Gemini provider from inline serialization to Route+GeminiProtocol
- **llm**: Migrate Bedrock provider from inline serialization to Route+BedrockProtocol
- **llm**: Migrate OpenAI provider from inline serialization to Route+Protocol
- **runtime**: Migrate String errors to thiserror typed enums

### Documentation

- Add production hardening plan, status, and agent-bench CI workflow
- Add Plan 2 (Provider Migration) implementation plan
- Update production hardening status with audit results

### Chores

- **bench**: Add native CI benchmark task definitions
- Consolidate dependency versions to workspace references
- Upgrade bollard 0.17→0.21, jsonschema 0.26→0.46, consolidate more deps
- Add release profile optimizations (59MB → 35MB binary)
- Upgrade bollard 0.17→0.21 API, add sha2 to protocol, optimize hash formatting
- Remove unused sha2 dep from orchestration crate
- Fmt fix for text.rs test assertion
- Bump version to 0.4.0

## [0.3.0] - 2026-05-10

### Features

- **auth**: Add OAuth browser login, callback server, and provider modules
- Tmux CLI-only backend, MCP streamable HTTP, todo status overhaul
- **tools**: Add unified hook config, matcher, protocol, and middleware wiring
- **tui**: Add /feedback command to submit issues via browser
- **agents**: Add disallowed_tools, level, and AgentSource overlay support
- **agents**: Load default agents from embedded .md files instead of hardcoded definitions
- **routing**: Add LlmIntentClassifier with budget tracking (Phase B1-B4)
- **tools**: Add AskUserQuestion tool with structured multiple-choice schema
- **prompt**: Unify prompt layering with PromptResolver across orchestration
- **llm**: Add gemini provider routing, --provider CLI flag, and fix function calling
- Skill activation for headless runtime, config loader, compaction improvements

### Bug Fixes

- **bench**: Fetch specific base commit for shallow clones
- **llm**: Handle OpenAI API errors correctly per spec
- Add missing new_cwd field to all ToolResult initializers
- **prompt**: Strengthen anti-duplication, verification, and conciseness rules
- **routing**: Wire LlmIntentClassifier, fix non-exhaustive match, propagate async
- **tools**: Repair test regressions from PascalCase rename
- **tests**: PascalCase rename regressions, remove is_critical_tool, ignore browser tests
- **question**: Prevent stdin deadlock in TUI mode
- **tui**: Set RUSTYCODE_TUI env var to prevent stdin deadlock
- **web**: Prevent tool call drops and stuck pending state on disconnect
- **test**: Update symlink error assertion to match actual error message
- **async**: Prevent block_on deadlocks in sync-over-async contexts
- **casing**: Fix P0 runtime bugs where PascalCase literals never match lowered strings
- **tools**: Add missing web client module for async HTTP
- **tests**: Update tests for PascalCase tool name migration
- **tui**: Remove stale local runtime references in shared-runtime migration
- **runtime**: Drop reqwest blocking feature, fix shared-runtime test
- **cli**: Wire tool registry into run command, fix profile hints
- **core**: Use PascalCase tool names in headless system prompt
- **tui**: Use PascalCase tool names in user-facing error suggestions
- **llm**: Sanitize Gemini tool schemas to prevent API rejections
- **llm**: Map thinking blocks to reasoning_content field for OpenAI-compatible APIs
- **llm**: Fix Gemini streaming — add alt=sse param, strip SSE data: prefix, sanitize $defs/$ref/items:bool from tool schemas
- **skill**: Fix all 4 critical skill activation bugs
- **llm,tui**: Fix token double-counting, Gemini structured output, cache usage events, context meter accuracy
- **tui**: Wire CacheUsage events through to token budget for prompt cache tracking
- **llm**: Correct token total in streaming usage logs
- **llm**: Parse Gemini per-field token counts instead of total only

### Performance

- **bench**: Add early-stop heuristics for code agent
- **bench**: Tune code agent early-stop heuristics

### Refactor

- Replace TodoSync polling with event bus subscription
- **tools**: Split hooks module into directory structure
- **worktree**: Extract session state into tools-api to break circular dep
- **tools**: Convert provider tools to define_tool! macro
- **batch**: Convert BatchTool to zero-sized struct with session-keyed global registry
- **tools**: Convert remaining tools to define_tool! macro
- **tools**: Convert TaskTool to zero-sized struct with session-keyed state
- **semantic-search**: Phase 3 — convert SemanticSearchTool to zero-sized struct
- **tools**: Convert BashTool to define_tool! macro
- **tools**: Remove StructuredThinkingTool::new(None) constructor call
- **tools**: Reorganize providers into fs/ and web/ subdirectories
- **tools**: Define_tool! macro conversion, fs/web reorganization, agents embed
- **tools**: Activate LSP, WebSearch, and MultiEdit tools by default
- **tools**: Fix test assertions after PascalCase tool rename
- **llm,runtime**: Formatting cleanup and minor fixes
- **tools**: PascalCase tool names across codebase (WIP)
- **protocol**: Complete PascalCase tool name migration across all crates
- **tools**: Remove duplicate SmartApprove, disable Git tools, fix symlink resolution
- **tools**: Migrate codesearch to async HTTP, cap shared runtime workers
- Replace hardcoded tool names with tn::* constants, fix casing bugs

### Documentation

- Add MCP lifecycle architecture and streamable HTTP roadmap

### Testing

- Improve error message for tool schema validation

### Chores

- Add benchmark eval results and swebench evaluation script
- Remove incomplete refactoring artifacts (fs/, lsp/ dirs)
- Misc code quality improvements from session
- Bump version to 0.3.0
- Add release process docs to CLAUDE.md, generate v0.3.0 changelog

## [0.1.2] - 2026-05-08

### Features

- **bench**: Add SWE-bench runner module and expand orchestration support

### Bug Fixes

- **team**: Use default_registry in test helper to match production constructors
- **connector**: Improve iTerm2 integration and tmux error handling
- Improve error handling in storage crate
- Error handling and cleanup across runtime, team, tools, and git

### Refactor

- **runtime**: Remove dead code from orchestrator and negotiation modules
- **tools**: Remove dead semantic search code, fix watcher warnings
- Move SWE-bench from orchestration to bench crate
- Clean up LLM provider implementations
- Remove dead code from orchestration modules
- Remove swebench CLI command, clean up bench and memory commands

### Chores

- Update CLAUDE.md, lock file, and agent intelligence module
- Add temp files to .gitignore
- Update docs, config, and minor TUI/core cleanup
- Update CHANGELOG for v0.1.2

## [0.1.1] - 2026-05-08

### Features

- Add git-cliff changelog generation and release notes script

### Bug Fixes

- Security hardening, silent error logging, dead code cleanup

### Chores

- Update CHANGELOG.md for v0.1.1

## [0.1.0] - 2026-05-07

### Features

- Complete Phase 6 - add benchmarks and accuracy validation tests
- Add orchestration optimizations for token/time efficiency
- Start tool tier at Extended for immediate LSP access
- Update prompt templates to reference LSP and Extended tier tools
- Natural multi-line input — Enter always submits, Shift+Enter for newline
- **agent**: Enforce senior engineering habits
- **llm**: Add xhigh effort level, adaptive thinking support, and model-aware UI wiring
- Add tool deferral system, OpenAI Responses API, and OpenRouter streaming
- **tools**: Wire registry into tool_search, add WS transport, centralize deferred stubs
- Deferred tool loading tests, Responses API fallback, LSP readiness
- **llm**: Add ThinkingBlock display field, reasoning_details for MiniMax M2.7
- **tools-api**: Add tool safety and context traits
- **tools**: Add PowerShell provider and refactor command execution
- **tools**: Add workspace boundary control and improve validation
- **cli**: Add workspace override flag and PowerShell support
- **tui**: Integrate workspace boundary control in streaming
- **tools**: Add file staleness detection, device blocking, and cmd provider
- **tui,llm**: Wire image paste end-to-end pipeline
- **prompt**: Add model-specific prompt routing for 15 model variants
- Add session recording, workspace memory, cost tracking, sandbox hardening, and permission classifier
- Add quality detector and orchestration integration wiring
- **tui**: Sync LLM todo state into workspace tasks
- **protocol**: Add MessageRole enum for type-safe message roles
- Add Codex-style memory pipeline + semver hardening
- Add per-agent rate limiting to MailboxRouter
- Unified agent architecture integration tests + misc fixes
- Add `rustycode update` self-update command
- Unify StepExecutor trait across crates (core/execution)

### Bug Fixes

- Deduplicate clipboard feedback, add /provider list, fix flaky streaming test
- Use descriptive label in clipboard toast instead of generic message
- Stop stream continuing after user presses Esc/Ctrl+C between turns
- Correct tool height calculation and remove unused wildcard import
- Replace production unwrap() with safe error handling
- **orchestration**: Silent error swallowing, state leaks, path traversal
- Resolve all clippy warnings in rustycode-ws-server
- .gitignore and restore package.json
- Remove redundant clone in validation test
- Consolidate tool registry and remove duplication
- Consolidate tool registry and remove duplication
- Add rustycode-tools dependency to resolve default_registry compilation errors
- Add comprehensive cross-platform support for Windows, Linux, macOS
- Repo_map uses WalkBuilder for .gitignore-aware file collection
- **tools**: Resolve LSP runtime shutdown panic and improve tools
- **web**: Session highlighting, input history, and add E2E tests
- Fixup! refactor(llm): extract shared SSE parsing and reduce provider duplication
- **llm**: Address review findings for effort level implementation
- **llm**: API correctness fixes for Anthropic, OpenAI, and protocol layer
- Resolve clippy warnings in test files and remove unused import
- Suppress dead_code warning for PowerShell restart
- **tools**: Add flexible matching to edit_file (claude_text_editor)
- **tui**: Add missing ToolInfo fields in test fixtures
- Address code review findings — message eviction, token overflow, mutex recovery
- **tui**: Downgrade TodoSync logging to debug level
- **tui**: Prevent panic in /skill commands when no Tokio runtime available
- Frontmatter validation, test fixes, metadata and security improvements
- Improve mutex and todo sync safety in tasks.rs
- Add missing OrchestrationClient import, remove unused ToolStatus import
- Allow new Rust 1.95 clippy lints + cargo fmt
- Allow Rust 1.95 lints for CI compatibility
- Allow more Rust 1.95 clippy lints + declare landlock feature
- Correct WebFetchTool import path and regenerate Cargo.lock
- Vendor openssl-sys for cross-compilation + cargo fmt
- Replace tokio Command::get_envs with simpler PATH guard
- Replace redundant closure with method reference
- Suppress unexpected_cfgs false positive and format linux.rs for CI
- Add secrecy dev-dependency for TUI e2e tests
- Linux.rs formatting and markdown.rs expect_used clippy error
- Clippy errors and format issues for Rust 1.95
- Clippy needless_raw_string_hashes and private module visibility for CI
- Redundant closure clippy lint and unused config import
- Unused import, non_exhaustive constructors, and test type mismatch
- Format ToolResult::new constructor for Linux rustfmt
- Needless_collect in memory test and remove unreachable BehaviorConfig test
- Clippy filter_next, export DiffRenderer/SyntaxHighlighter for tests
- Resolve Rust 1.95 clippy lints across workspace
- Increase LSP active verification test timeout for CI
- Make 4 tests CI-compatible (branch name, env vars, WSL paths)
- Fmt markdown.rs + make security artifact upload non-blocking
- Ignore evaluate_harness test missing external CSV, remove stale cost_integration comment
- Resolve CI test failures across TUI, LSP, and macOS
- Add reason to #[ignore] attribute for clippy ignore_without_reason
- Sanitize script filename in docker exec_script to prevent shell injection
- Shell-quote script path in native exec_script to prevent injection
- Shorten doc comment to satisfy clippy too_long_first_doc_paragraph
- Cap middle_out_indices removal count to input length
- Fmt, unused dep, and command function rename
- Parameterize SQL query in event_store to prevent injection
- Ignore PowerShell session tests on CI, make coverage non-blocking
- Isolate checkpoint tests with unique dirs, fix WSL path assertion
- Tolerate parallel test races, make Home/End always scroll messages
- Replace bash read with cat in hook test scripts for CI compatibility
- Native_cat size limit, VecDeque for tracker, created_at preservation, silent error logging
- WSL path test assertions for Linux CI
- Cargo fmt for CI quality check
- Compare pending_count against post-register snapshot to fix race
- Update release workflow with correct binary name and deps
- Lsp doc test uses Uri::parse instead of nonexistent from_file_path
- Gate slow integration tests behind feature flag, guard plan_mode
- Saturating arithmetic for exponential backoff, log silent failures
- Cargo fmt for cfg_attr line length and graceful_degradation
- Log silent failures and eliminate O(n²) redundant searches
- Log mutex poison recovery and cwd fallback instead of silent handling
- Gate real_model_bench behind slow-tests feature flag
- Log silent failures and improve mutex recovery formatting
- Log silent failures in task dependency parsing and benchmark duration
- Log schema registration failures and remove redundant lints override
- Log silent failures in storage crate cleanup operations
- Allow unsafe_code in rustycode-tools for Linux subprocess management
- Replace ToolOutput::success with ToolOutput::text for Windows build
- Use SHELL_INFO for cross-platform shell invocation
- **tui**: Prevent double-Done killing queued message streams
- **tui**: Auto-scroll after slash command while user scrolled up
- Session switch during streaming, scroll bugs, mutex poison recovery
- **tui**: Sidebar session load stream_cancelled, state reset, doom loop false positives
- **tui**: Guard against loading empty session that would wipe conversation
- **tui**: Search Ctrl+U inserts chars instead of clearing, sidebar unreachable in tmux
- **tui**: Theme list emoji rendering causes garbled color codes in tmux
- **tui**: Input history navigation, message rendering, and status bar improvements
- Use floor_char_boundary for UTF-8 safe string truncation

### Refactor

- Adaptive workspace scanning with ignore crate
- **tui**: Replace toast with system message for clipboard feedback
- **llm**: Extract shared SSE parsing and reduce provider duplication
- **tui**: Extract service modules from god object
- Rename rustycode-agent to rustycode-agent-runtime, add rustycode-agents
- **tui**: Improve tmux detection, input handling, and clipboard
- **core**: Extract checkpoint, edit, and code panel state from SessionState
- **core**: Extract token budget and streaming state - Phase 2
- **core**: Extract PerformanceMetrics and ProviderConfig from SessionState - Phase 3
- **core**: Extract SessionModeState and MemoryState from SessionState - Phase 4
- **core**: Extract ToolRuntimeState from SessionState - Phase 5
- **core**: Extract SessionContext from SessionState - Phase 6
- **core**: Complete SessionState sub-struct extraction - Phases 7-9
- Remove dead code, trim verbose docs, add state unit tests
- Remove dead code, clean allow(dead_code), wire runtime modules
- Minor import cleanup, getter rename follow-up, doc trim
- TUI getter renames, dead code removal, tool_approval cleanup
- Separate mutable retry state from immutable delegation token
- Dead code removal, silent error logging, and lint fixes
- Remove unused select_agent function from routing
- Remove 5.4K lines of dead code from tools/security
- **bus**: Split monolithic events.rs into events/ module directory
- **llm**: Split monolithic openai.rs into openai/ and anthropic/ module directories
- **git**: Split monolithic lib.rs into focused module files
- **storage**: Split monolithic lib.rs into focused module files
- Rename binary to rustycode, update docs, fix anthropic module split

### Documentation

- Add unified callable abstraction design specification
- Add detailed implementation plan for unified callable abstraction
- Mark unified callable abstraction implementation complete
- Add comprehensive verification report
- Update architecture review, CRATES.md, README; remove stale legacy docs
- Add Ralph capability test results and TUI extraction plan
- Context compression pipeline design spec
- Comprehensive cleanup and new project documentation
- Update crate count, remove ratzilla refs, add orchestration client
- Update CLAUDE.md and CRATES.md for milestone grouping layer

### Testing

- **tui**: Add 56 unit tests for text selection and mouse drag
- **protocol**: Add MessageRole serialization roundtrip safety tests

### CI

- Trigger fresh CI run

### Chores

- Remove node_modules from git tracking, add .gitignore
- Clippy fixes, web e2e setup, gitignore cleanup
- LSP tool tags, remove debug logging, DRY web e2e
- Fix .gitignore entries for web e2e artifacts
- **registry**: Remove unused clippy allow attribute
- Remove dead artifacts, disabled files, and ratzilla-wasm
- Allow missing_const_for_fn lint, update Cargo.lock
- Remove doomgeneric, tmp-prd-test, mcp-test-server, 1argo from git
- Remove 65 irrelevant files from git tracking
- Commit Cargo.lock for --locked CI reproducibility
- Regenerate Cargo.lock after adding secrecy dev-dep
- Clean up unused imports across crates
- **storage**: Remove redundant ALTER TABLE milestone_id migration

### Ws-server

- Allow clippy::unwrap_used and clippy::expect_used in tests; refactor session update to use method reference

<!-- generated by git-cliff -->
