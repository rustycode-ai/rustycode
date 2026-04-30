# tmux MCP Spec and Implementation Plan

## Reader

This document is for the engineer implementing tmux control for agent-driven workflows.

After reading it, they should be able to build an MCP layer that lets an LLM:

- create and manage tmux sessions
- inspect pane output, including ANSI/escape-sequence output when needed
- run commands and tests in a controlled workspace
- clean up sessions and child processes reliably

## Goal

Expose tmux as a stateful MCP-backed control plane so an LLM can operate a live terminal workflow across multiple calls.

The MCP layer should preserve state, not just return one-off command output. It should remember which tmux session, window, and pane belong to the current task and should make it safe to inspect or tear down that state later.

## Why MCP

tmux itself is already stateful. MCP adds a structured interface on top of that state so the model can:

- reuse stable IDs instead of guessing names
- read terminal output without scraping free-form chat history
- keep a live workflow open across separate tool calls
- enforce cleanup rules and execution limits

The static connector matrix remains useful as human documentation, but it is not enough for live control.

## Scope

The first version should support:

- tmux session creation and destruction
- pane splitting and selection
- keyboard input and command execution
- pane capture in plain text
- pane capture with escape sequences / ANSI fidelity
- waiting for output patterns
- workspace-level command execution for test runs
- session, pane, and process lifecycle cleanup

## Non-Goals

The first version does not need:

- a full remote tmux client replacement
- arbitrary tmux command passthrough
- background job scheduling outside the active workspace
- cross-host session federation
- persistent state across MCP server restarts beyond reconnection to still-live tmux sessions

## Core Model

The MCP server should manage three related concepts:

### 1. Session lease

A session lease is the logical handle the LLM uses to refer to a tmux session.

Each lease should track:

- `session_id`
- `session_name`
- `created_at`
- `last_used_at`
- `expires_at` or `ttl_secs`
- `owner`
- `workspace_root`
- `socket_name` when tmux is isolated on a custom socket

### 2. Pane handle

A pane handle identifies the interactive surface inside a session.

Each pane should track:

- `pane_id` or pane index
- `session_id`
- `window_id` if the implementation distinguishes windows
- `last_seen_at`
- `purpose` such as `app`, `tests`, or `shell`

### 3. Command handle

A command handle identifies a tracked execution, especially for long-running work.

Each command should track:

- `command_id`
- `session_id`
- `pane_id`
- `command`
- `started_at`
- `finished_at`
- `exit_code`
- `status`
- `captured_output_uri` or equivalent resource reference

## Tool Surface

The MCP surface should be small, explicit, and stable.

### Session tools

- `tmux.create_session`
- `tmux.list_sessions`
- `tmux.session_info`
- `tmux.close_session`

### Pane tools

- `tmux.split_pane`
- `tmux.select_pane`
- `tmux.kill_pane`

### Input and capture tools

- `tmux.send_keys`
- `tmux.capture_pane`
- `tmux.wait_for_output`

### Command tools

- `tmux.execute_command`
- `tmux.get_command_result`

### Workspace tools

- `workspace.exec`
- `workspace.run_tests`

The workspace tools are separate from tmux control. They are useful when the LLM should verify behavior without opening an interactive pane.

## API Sketch

The server should expose tools using standard MCP `tools/list` and `tools/call` handling.

Suggested implementation target:

- the MCP server layer owns tool registration, resources, and request routing
- the connector layer owns tmux-specific state and operations
- the tmux MCP server wraps `TerminalConnector` rather than duplicating tmux logic

### `tmux.create_session`

Creates a new leased tmux session.

Input:

```json
{
  "name": "api",
  "workspace_root": "/Users/nat/dev/rustycode",
  "socket_name": "optional-isolated-socket",
  "ttl_secs": 3600
}
```

Output:

```json
{
  "session_id": "rustycode-api-12345",
  "session_name": "rustycode-api-12345",
  "pane_id": "%0",
  "window_id": "0",
  "lease_expires_at": "2026-04-30T12:34:56Z"
}
```

### `tmux.capture_pane`

Captures the current visible content or scrollback from a pane.

Input:

```json
{
  "session_id": "rustycode-api-12345",
  "pane_id": "%0",
  "start": -200,
  "end": -1,
  "max_lines": 500,
  "include_escape_sequences": false,
  "join_wrapped_lines": true
}
```

Behavior:

- if `include_escape_sequences` is false, return plain text
- if `include_escape_sequences` is true, preserve ANSI / control sequences
- if `join_wrapped_lines` is true, return wrapped lines joined the way tmux does with `-J`

Output:

```json
{
  "session_id": "rustycode-api-12345",
  "pane_id": "%0",
  "captured_at": "2026-04-30T12:34:56Z",
  "content": "..."
}
```

### `tmux.execute_command`

Executes a tracked command and returns a command handle for polling.

Input:

```json
{
  "session_id": "rustycode-api-12345",
  "pane_id": "%0",
  "command": "cargo test",
  "timeout_secs": 300,
  "capture_output": true
}
```

Output:

```json
{
  "command_id": "cmd_01J...",
  "status": "running"
}
```

### `tmux.get_command_result`

Returns the final or current state of a tracked command.

Input:

```json
{
  "command_id": "cmd_01J..."
}
```

Output:

```json
{
  "command_id": "cmd_01J...",
  "status": "finished",
  "exit_code": 0,
  "stdout": "...",
  "stderr": "",
  "captured_output_uri": "tmux://command/cmd_01J.../result"
}
```

### `workspace.exec`

Runs a non-interactive command in the workspace root.

Input:

```json
{
  "workspace_root": "/Users/nat/dev/rustycode",
  "command": "cargo test",
  "timeout_secs": 300
}
```

### `workspace.run_tests`

Runs the standard test command for the workspace and returns parsed results.

Input:

```json
{
  "workspace_root": "/Users/nat/dev/rustycode",
  "filter": "session_sidebar"
}
```

## Resource Sketch

The tmux MCP server should expose read-only resources for inspection and recovery.

Suggested URIs:

- `tmux://server/info`
- `tmux://session/{sessionId}/tree`
- `tmux://session/{sessionId}/lease`
- `tmux://window/{windowId}/info`
- `tmux://pane/{paneId}`
- `tmux://pane/{paneId}/info`
- `tmux://pane/{paneId}/tail/{lines}`
- `tmux://pane/{paneId}/tail/{lines}/ansi`
- `tmux://command/{commandId}/result`

Resource rules:

- `.../tail/...` may be used for bounded capture or summaries
- `.../ansi` should preserve escape sequences
- resources must be read-only
- resource reads must not mutate lease state

## Suggested Module Split

The implementation should stay small and layered.

- `rustycode-connector` owns `TerminalConnector` and tmux backend behavior
- `rustycode-mcp` owns the MCP server, tool registration, resource routing, and lease store
- a thin tmux-specific adapter glues the two together

This keeps tmux logic testable without the MCP transport and keeps the MCP layer generic enough to host future connectors.

## Capture Rules

`tmux.capture_pane` should support two capture modes:

- plain text capture for normal summarization
- escape-sequence capture for fidelity-sensitive debugging

Recommended input shape:

```json
{
  "session_id": "$1",
  "pane_id": "%0",
  "start": -200,
  "max_lines": 500,
  "include_escape_sequences": false
}
```

If `include_escape_sequences` is true, the server should preserve ANSI / terminal control sequences in the output.

Capture should remain read-only and should not alter terminal state.

## Output Waiting

`tmux.wait_for_output` should poll a pane until:

- a pattern appears
- the timeout expires
- the pane exits or becomes unavailable

This is especially useful for:

- waiting for a dev server to boot
- waiting for tests to finish
- watching an interactive app reach a known state

## Execution Model

There are two execution paths:

### Interactive path

Use tmux panes when the model needs:

- visible live state
- prompts or interactive menus
- incremental output capture
- manual user handoff into the same session

### Non-interactive path

Use workspace execution when the model needs:

- `cargo test`
- `cargo check`
- one-shot verification
- deterministic command results

Non-interactive execution should still be tracked as a command handle so the model can poll results later.

## Lifecycle Rules

### Session creation

When a new session is created, the MCP server should:

- choose or receive a workspace root
- create or reuse a tmux socket scope
- create the session and first pane
- register the lease and initial pane
- return stable IDs to the caller

### Session renewal

Any operation against a session should refresh `last_used_at`.

### Session expiry

An idle reaper should clean up sessions that exceed their TTL.

The reaper should be conservative:

- never reap an actively used session
- never kill a session that has recent command activity
- record why the cleanup happened

### Explicit close

`tmux.close_session` should:

- stop tracked commands if needed
- terminate the pane process group when appropriate
- close the tmux session
- remove the lease and associated state

### Crash recovery

If the MCP server restarts:

- it should enumerate still-live tmux sessions when possible
- it should invalidate leases for sessions it can no longer confirm
- it should not pretend a stale handle is still live

## Process Cleanup

Session cleanup is not enough. The server must also manage process lifetime.

Rules:

- every command launched interactively should be associated with a pane
- every long-running exec should be associated with a command handle
- closing a pane or session should terminate the child process tree
- orphaned processes should be reaped if their owning lease disappears

Recommended strategy:

- track process group IDs when the platform exposes them
- use tmux state as the authoritative terminal registry
- use a best-effort local registry for process handles and exit status

## Security And Policy

Because tmux control can become powerful quickly, the MCP layer should enforce:

- workspace scoping
- allowed command patterns for direct exec
- clear separation between interactive and non-interactive tools
- optional approval for destructive actions such as pane or session deletion
- bounded output capture sizes

## Resource Model

Expose read-only MCP resources for state inspection.

Useful resources include:

- session tree snapshots
- pane content snapshots
- tracked command results
- tmux server summary information

Resources should be stable, enumerable, and safe to read without side effects.

## Implementation Plan

### Phase 1: Define the MCP contract

- finalize tool names and argument shapes
- choose the capture API shape, including escape-sequence support
- define session, pane, and command IDs
- define cleanup and timeout defaults

Exit criteria:

- the tool list is frozen
- capture behavior is specified
- lifecycle fields are agreed

### Phase 2: Build the tmux-backed server

- implement session creation and listing
- implement pane control and capture
- implement command tracking and polling
- implement workspace execution tools

Exit criteria:

- an agent can create a session, run a command, and read output
- pane capture works in both plain and escape-sequence modes

### Phase 3: Add lifecycle management

- add idle TTL handling
- add explicit close and kill semantics
- add process-group cleanup
- add stale lease invalidation

Exit criteria:

- unused sessions are reclaimed
- closing a session also removes tracked child processes

### Phase 4: Add resources and observability

- expose session and pane snapshots as resources
- expose command result resources
- log lifecycle events and cleanup reasons

Exit criteria:

- the model can inspect current state without opening a new tool call for every question
- operators can diagnose cleanup or capture issues

### Phase 5: Wire into the product

- register the MCP server in the runtime configuration
- make it discoverable from the terminal workflow entry points
- document the supported workflow for test runs and app control

Exit criteria:

- a user can enable tmux MCP and drive a live workspace through the model

## Suggested Defaults

- session TTL: 60 minutes of idle time
- command timeout: 5 minutes for ordinary exec, configurable for tests
- capture limit: bounded by default, with explicit override for large scrollback
- plain capture as default, escape-sequence capture opt-in

## Acceptance Criteria

The implementation is complete when all of the following are true:

- the model can create a tmux session and retain its ID across calls
- the model can split panes, send input, and capture output
- the model can request capture with and without escape sequences
- the model can run tests without leaving orphaned processes behind
- closing a session reliably tears down its tracked children
- stale sessions are reaped or invalidated predictably
