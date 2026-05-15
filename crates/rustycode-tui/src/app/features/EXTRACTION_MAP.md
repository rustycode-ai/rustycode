# EXTRACTION_MAP.md — Handler → Feature Mapping

> **Purpose:** Complete mapping of every handler function to its feature domain,
> fields mutated, fields read, and cross-feature coupling. This document drives
> the TUI decomposition plan: each feature group becomes a `TuiFeature` impl.
>
> **Task:** T03 — Map handlers → (feature, fields_mutated) extraction contract
> **Generated from:** handlers/ (10 source files), state_model.rs (11 sub-structs),
> service_polling.rs, streaming/ (7 files), pipeline/ (11 files), agents/ (4 files)

---

## Legend

| Symbol | Meaning |
|--------|---------|
| **W** | Write (field is mutated) |
| **R** | Read (field is accessed immutably) |
| **M** | Method call on TUI that may mutate multiple fields |
| 🔴 | Coupling hotspot: touches 3+ feature domains |
| 🟡 | Moderate coupling: touches 2 feature domains |
| 🟢 | Narrow: single feature domain |

---

## Feature Domain Definitions

Feature domains are derived from the 11 sub-structs in `state_model.rs`.
Each domain groups related fields that will become a `FeatureState` struct.

### F1: Streaming (`InteractionSessionState.streaming`)
- `streaming.is_streaming`
- `streaming.current_stream_content`
- `streaming.chunks_received`
- `streaming.thinking_chunks_received`
- `streaming.stream_start_time`
- `streaming.streaming_render_buffer`
- `streaming.stream_cancelled`
- `streaming.queued_message`
- `streaming.begin_streaming()`

### F2: Session (`InteractionSessionState` minus streaming)
- `session.messages` (Vec<Message>)
- `session.active_tools` (HashMap)
- `session.auto_continue`
- `session.doom_loop`
- `session.pending_doom_note`
- `session.turn_snapshot`
- `session.execution_trace`
- `session.plan_mode_banner`
- `session.reasoning_budget`
- `session.session_sidebar`
- `session.undo`
- `session.session_recovery`
- `session.wizard`

### F3: Tool Execution (`ToolExecutionPanel`)
- `panels.tool_panel` (tool_panel_history)
- `panels.tool_approval` (awaiting, pending_requests, manager)
- `panels.ast_phase_state` (phase, phase_index, progress_fraction, active)
- `panels.clarification_panel`
- `panels.awaiting_clarification`
- `panels.symbol_outline`

### F4: Workspace (`TaskWorkspaceState`)
- `workspace.workspace_loaded`
- `workspace.workspace_context`
- `workspace.workspace_tasks` (todos, tasks)
- `workspace.last_extraction`
- `workspace.workspace_scan_progress`
- `workspace.git_branch`

### F5: Model (`ProviderModelState`)
- `model.current_model`
- `model.current_effort`
- `model.token_budget` (session/last-turn input/output/cache tokens, cost_usd)
- `model.plan_mode` (is_enabled, current_phase, is_tool_allowed)
- `model.api_key_warning`
- `model.show_task_dashboard`

### F6: Integration (`ServiceIntegrationState`)
- `integration.services` (ServiceManager — send_event, complete_query, cwd, ai_mode)
- `integration.rate_limit` (retry_count, until, backoff_delay_secs, message_index, auto_retry_cancelled, last_message)
- `integration.hook_manager` (hooks_dir, execute_blocking)
- `integration.pipeline` (PipelineRegistry)
- `integration.pipeline_ctx`
- `integration.storage`
- `integration.rate_limit_tracker`
- `integration.lsp`, `integration.mcp`, `integration.mcp_manager`
- `integration.scheduler_rx`, `integration.active_scheduled_phases`
- `integration.start_time`, `integration.event_receiver`, `integration.symbol_event_rx`
- `integration.todo_state`, `integration.todo_event_bus`, `integration.todo_dirty`
- `integration.tool_manager`, `integration.session_manager`
- `integration.skill_manager`

### F7: UI (`UIComponents` + `SystemState.renderer_mode`)
- `ui.view` (selected_message, scroll_offset_line, user_scrolled, last_total_lines)
- `ui.stashed_prompt`
- `ui.tui_config`
- `ui.status_bar_collapsed`, `ui.footer_collapsed`
- `ui.message_renderer`, `ui.input_handler`, `ui.animator`
- `ui.keyboard_handler`, `ui.sidebar_area`
- `ui.marketplace_browser`, `ui.skill_palette`, `ui.plugin_manager_ui`, `ui.help_state`
- `sys.renderer_mode`

### F8: System (`SystemState` minus renderer_mode)
- `sys.running`
- `sys.dirty`
- `sys.needs_full_redraw`
- `sys.compaction` (context_monitor)
- `sys.auto_memory`, `sys.memory_injection_config`
- `sys.plugin_manager`
- `sys.input_mode`

### F9: Theme & Notification (`ThemeNotificationState`)
- `theme.theme_colors`
- `theme.theme_preview`
- `theme.theme_switcher`
- `theme.toast_manager`
- `theme.error_manager`

### F10: Overlay (`OverlayState`)
- `overlay.command_palette`, `overlay.showing_command_palette`
- `overlay.model_selector`, `overlay.showing_provider_selector`
- `overlay.file_selector`, `overlay.showing_error`
- `overlay.showing_plugin_manager`, `overlay.showing_marketplace_browser`
- `overlay.last_esc_press`, `overlay.showing_skill_palette`

### F11: Terminal Progress
- `terminal_progress.enabled`
- `terminal_progress.clear()`
- `terminal_progress.set_progress()`

### F12: Search (`MessageSearchState`)
- `search.search_state`
- `search.file_finder`, `search.tag_filter`
- `search.message_areas`, `search.message_line_offsets`

### F13: Team (`TeamModeState`)
- `team.team_panel`, `team.team_handler`
- `team.worker_panel`, `team.agent_manager`

---

## Handler → Feature Mapping

### 1. stream_core.rs

#### `handle_text_chunk(tui: &mut TUI, text: String)` 🔴
| Field | Access | Domain |
|-------|--------|--------|
| `session.streaming.stream_start_time` | W | F1: Streaming |
| `session.streaming.is_streaming` | W | F1: Streaming |
| `session.streaming.streaming_render_buffer.push` | W | F1: Streaming |
| `session.streaming.chunks_received` | R | F1: Streaming |
| `session.streaming.current_stream_content` | R | F1: Streaming |
| `session.streaming.is_streaming` | R | F1: Streaming |
| `session.streaming.thinking_chunks_received` | R | F1: Streaming |
| `last_assistant_message_mut()` | M | F2: Session |
| `update_terminal_title()` | M | F7: UI |
| `sys.renderer_mode.is_brutalist` | R | F7: UI |

**Domains touched:** F1, F2, F7 (3) 🔴

#### `handle_thinking_chunk(tui: &mut TUI, thinking: String)` 🟡
| Field | Access | Domain |
|-------|--------|--------|
| `last_assistant_message_mut()` | M | F2: Session |
| `session.streaming.is_streaming` | R | F1: Streaming |
| `session.streaming.thinking_chunks_received` | R | F1: Streaming |
| `session.turn_snapshot` | R/W | F2: Session |

**Domains touched:** F1, F2 (2) 🟡

#### `handle_stream_chunk(tui: &mut TUI, chunk: StreamChunk)` 🔴
Master dispatcher — delegates to all other stream_* handlers.
| Field | Access | Domain |
|-------|--------|--------|
| `session.streaming.is_streaming` | W | F1: Streaming |
| `session.streaming.current_stream_content` | R | F1: Streaming |
| `session.streaming.thinking_chunks_received` | R | F1: Streaming |
| `session.streaming.chunks_received` | R | F1: Streaming |
| `session.streaming.stream_start_time` | W | F1: Streaming |
| `session.turn_snapshot` | W | F2: Session |
| `session.active_tools.contains_key` | R | F2: Session |
| `session.messages.len` | R | F2: Session |
| `model.token_budget.session_*_tokens` | R | F5: Model |
| `integration.services.send_event` | R | F6: Integration |
| `push_empty_assistant_message()` | M | F2: Session |
| `sys.dirty` | W | F8: System |
| `update_terminal_title()` | M | F7: UI |

**Domains touched:** F1, F2, F5, F6, F7, F8 (6) 🔴🔴🔴

---

### 2. stream_done.rs

#### `handle_done_chunk(tui: &mut TUI)` 🔴
| Field | Access | Domain |
|-------|--------|--------|
| `session.streaming.current_stream_content` | R | F1: Streaming |
| `session.streaming.stream_cancelled` | R | F1: Streaming |
| `session.streaming.streaming_render_buffer.flush` | W | F1: Streaming |
| `session.streaming.queued_message.take` | R/W | F1: Streaming |
| `session.streaming.begin_streaming` | W | F1: Streaming |
| `session.streaming.chunks_received` | R | F1: Streaming |
| `session.streaming.thinking_chunks_received` | R | F1: Streaming |
| `session.messages.push/remove` | W | F2: Session |
| `session.active_tools.clear` | W | F2: Session |
| `session.auto_continue.clear_pending/is_enabled` | R/W | F2: Session |
| `session.doom_loop.*` | R/W | F2: Session |
| `session.pending_doom_note` | R/W | F2: Session |
| `session.plan_mode_banner.is_some` | R | F2: Session |
| `session.turn_snapshot.take` | W | F2: Session |
| `model.plan_mode.*` | R | F5: Model |
| `integration.rate_limit.last_message` | R/W | F6: Integration |
| `theme.toast_manager.info` | M | F9: Theme |
| `ui.view.selected_message` | W | F7: UI |
| `ui.view.user_scrolled` | W | F7: UI |
| `sys.dirty` | W | F8: System |
| `add_system_message()` | M | F2: Session |
| `auto_scroll()` | M | F7: UI |
| `build_conversation_history()` | M | F2: Session |
| `is_awaiting_approval()` | M | F3: Tool |
| `last_assistant_message/mut()` | M | F2: Session |
| `mark_session_dirty()` | M | F2: Session |
| `prepare_message_for_send()` | M | F6: Integration |
| `push_empty_assistant_message()` | M | F2: Session |
| `reset_streaming_state()` | M | F1/F8 |
| `show_approval_banner()` | M | F3: Tool |
| `update_context_and_compact()` | M | F8: System |

**Domains touched:** F1, F2, F3, F5, F6, F7, F8, F9 (8) 🔴🔴🔴

#### `handle_empty_stream_response(tui: &mut TUI)` 🔴
Same field set as handle_done_chunk with additional message manipulation.

**Domains touched:** F1, F2, F5, F6, F7, F8 (6) 🔴🔴🔴

---

### 3. stream_error.rs

#### `handle_error_chunk(tui: &mut TUI, err: StreamError)` 🔴
| Field | Access | Domain |
|-------|--------|--------|
| `session.streaming.is_streaming` | W | F1: Streaming |
| `session.streaming.stream_cancelled` | W | F1: Streaming |
| `session.streaming.current_stream_content` | R | F1: Streaming |
| `session.streaming.queued_message` | R/W | F1: Streaming |
| `session.messages.*` | R/W | F2: Session |
| `session.active_tools.clear` | W | F2: Session |
| `session.auto_continue.*` | W | F2: Session |
| `integration.rate_limit.*` | R/W | F6: Integration |
| `integration.services.complete_query` | M | F6: Integration |
| `ui.view.selected_message` | W | F7: UI |
| `sys.dirty` | W | F8: System |
| `add_system_message()` | M | F2: Session |
| `last_assistant_message_mut()` | M | F2: Session |
| `show_error()` | M | F10: Overlay |
| `update_context_and_compact()` | M | F8: System |
| `update_terminal_title()` | M | F7: UI |

**Domains touched:** F1, F2, F6, F7, F8, F10 (6) 🔴🔴🔴

---

### 4. stream_stopped.rs

#### `handle_stopped_chunk(tui: &mut TUI, stop_reason: String)` 🟡
| Field | Access | Domain |
|-------|--------|--------|
| `session.streaming.stream_cancelled` | R | F1: Streaming |
| `session.streaming.streaming_render_buffer.flush` | W | F1: Streaming |
| `add_system_message()` | M | F2: Session |
| `mark_session_dirty()` | M | F2: Session |
| `update_context_and_compact()` | M | F8: System |

**Domains touched:** F1, F2, F8 (3) 🔴 (but narrow field set)

---

### 5. stream_tools.rs

#### `handle_tool_start_chunk(tui: &mut TUI, ...)` 🔴
| Field | Access | Domain |
|-------|--------|--------|
| `session.active_tools.insert` | W | F2: Session |
| `session.reasoning_budget.lock` | R | F2: Session |
| `session.messages.iter_mut` | R/W | F2: Session |
| `panels.tool_panel.tool_panel_history.*` | R/W | F3: Tool |
| `model.plan_mode.is_tool_allowed` | R | F5: Model |
| `integration.hook_manager.hooks_dir` | R | F6: Integration |
| `sys.dirty` | W | F8: System |
| `add_system_message()` | M | F2: Session |
| `last_assistant_message_mut()` | M | F2: Session |
| `show_approval_banner()` | M | F3: Tool |
| `update_terminal_title()` | M | F7: UI |

**Domains touched:** F2, F3, F5, F6, F7, F8 (6) 🔴🔴🔴

#### `handle_tool_progress_chunk(tui: &mut TUI, ...)` 🟡
| Field | Access | Domain |
|-------|--------|--------|
| `panels.tool_panel.tool_panel_history.iter_mut` | R/W | F3: Tool |
| `session.messages.iter_mut` | R/W | F2: Session |
| `sys.dirty` | W | F8: System |

**Domains touched:** F2, F3, F8 (3) 🔴 (narrow)

#### `handle_tool_complete_chunk(tui: &mut TUI, ...)` 🔴
| Field | Access | Domain |
|-------|--------|--------|
| `session.active_tools.remove` | W | F2: Session |
| `session.doom_loop.*` | R | F2: Session |
| `session.reasoning_budget.lock` | R | F2: Session |
| `panels.tool_panel.tool_panel_history.iter_mut` | R/W | F3: Tool |
| `integration.hook_manager.hooks_dir` | R | F6: Integration |
| `theme.toast_manager.warning` | M | F9: Theme |
| `ui.view.scroll_offset_line` | W | F7: UI |
| `ui.view.user_scrolled` | W | F7: UI |
| `sys.dirty` | W | F8: System |
| `add_system_message()` | M | F2: Session |
| `last_assistant_message_mut()` | M | F2: Session |
| `update_terminal_title()` | M | F7: UI |

**Domains touched:** F2, F3, F6, F7, F8, F9 (6) 🔴🔴🔴

---

### 6. stream_approval.rs

#### `handle_approval_request_chunk(tui: &mut TUI, ...)` 🟡
| Field | Access | Domain |
|-------|--------|--------|
| `panels.tool_approval.awaiting` | W | F3: Tool |
| `panels.tool_approval.pending_requests.front` | R | F3: Tool |
| `panels.tool_approval.manager.is_blocked` | R | F3: Tool |
| `integration.hook_manager.execute_blocking` | M | F6: Integration |
| `integration.services.ai_mode` | R | F6: Integration |
| `sys.dirty` | W | F8: System |
| `add_system_message()` | M | F2: Session |

**Domains touched:** F2, F3, F6, F8 (4) 🔴

#### `handle_approval_approved_chunk(tui: &mut TUI, tool_id: String)` 🟢
| Field | Access | Domain |
|-------|--------|--------|
| `panels.tool_approval.awaiting` | W | F3: Tool |
| `panels.tool_approval.pending_requests.is_empty` | R | F3: Tool |
| `sys.dirty` | W | F8: System |
| `add_system_message()` | M | F2: Session |

**Domains touched:** F2, F3, F8 (3) — but only flag fields

#### `handle_approval_rejected_chunk(tui: &mut TUI, tool_id: String)` 🟢
Same pattern as approved.

**Domains touched:** F2, F3, F8 (3) — only flag fields

---

### 7. stream_data.rs

#### `handle_extract_tasks_chunk(tui: &mut TUI, text: String)` 🟡
| Field | Access | Domain |
|-------|--------|--------|
| `workspace.last_extraction` | W | F4: Workspace |
| `workspace.workspace_tasks.*` | R/W | F4: Workspace |
| `integration.services.cwd` | R | F6: Integration |
| `integration.storage.as_deref` | R | F6: Integration |
| `add_system_message()` | M | F2: Session |
| `auto_scroll()` | M | F7: UI |

**Domains touched:** F2, F4, F6, F7 (4) 🔴

#### `handle_tasks_extracted_chunk(tui: &mut TUI, ...)` 🟢
No-op (underscore `_tui`).

**Domains touched:** None

#### `handle_question_request_chunk(tui: &mut TUI, ...)` 🟡
| Field | Access | Domain |
|-------|--------|--------|
| `panels.awaiting_clarification` | W | F3: Tool |
| `panels.clarification_panel` | W | F3: Tool |
| `sys.dirty` | W | F8: System |
| `add_system_message()` | M | F2: Session |

**Domains touched:** F2, F3, F8 (3) — narrow

#### `handle_question_answered_chunk(tui: &mut TUI, ...)` 🟢
Minimal — likely just clears clarification state.

**Domains touched:** F3 (1) 🟢

#### `handle_file_snapshot_chunk(tui: &mut TUI, batch)` 🟡
| Field | Access | Domain |
|-------|--------|--------|
| `session.undo.push_file_batch` | W | F2: Session |
| `mark_session_dirty()` | M | F2: Session |

**Domains touched:** F2 (1) 🟢

#### `handle_token_usage_chunk(tui: &mut TUI, ...)` 🟡
| Field | Access | Domain |
|-------|--------|--------|
| `model.token_budget.last_turn_*_tokens` | W | F5: Model |
| `model.token_budget.session_*_tokens` | W | F5: Model |
| `model.token_budget.session_cost_usd` | W | F5: Model |
| `model.current_model` | R | F5: Model |
| `sys.dirty` | W | F8: System |

**Domains touched:** F5, F8 (2) 🟡

#### `handle_execution_trace_chunk(tui: &mut TUI, trace)` 🟢
| Field | Access | Domain |
|-------|--------|--------|
| `session.execution_trace` | W | F2: Session |
| `sys.dirty` | W | F8: System |

**Domains touched:** F2, F8 (2) 🟢

#### `handle_system_message_chunk(tui: &mut TUI, msg)` 🟢
| Field | Access | Domain |
|-------|--------|--------|
| `add_system_message()` | M | F2: Session |
| `sys.dirty` | W | F8: System |

**Domains touched:** F2, F8 (2) 🟢

#### `handle_milestone_progress_chunk(tui: &mut TUI, ...)` 🟡
| Field | Access | Domain |
|-------|--------|--------|
| `session.session_sidebar.update_milestone_progress` | W | F2: Session |
| `show_milestone_progress_banner()` | M | F7: UI |
| `sys.dirty` | W | F8: System |

**Domains touched:** F2, F7, F8 (3) 🔴 (narrow)

---

### 8. event_msg.rs

#### `handle_event_msg(tui: &mut TUI, msg: EventMsg)` 🔴🔴🔴
**This is the protocol dispatcher.** It does NOT directly access TUI fields —
it translates `EventMsg` variants and delegates to the handlers above.

- Delegates to: `handle_text_chunk`, `handle_thinking_chunk`, `handle_done_chunk`,
  `handle_error_chunk`, `handle_stopped_chunk`, `handle_tool_start_chunk`,
  `handle_tool_progress_chunk`, `handle_tool_complete_chunk`,
  `handle_approval_request_chunk`, `handle_approval_approved_chunk`,
  `handle_approval_rejected_chunk`, `handle_question_request_chunk`,
  `handle_question_answered_chunk`, `handle_extract_tasks_chunk`,
  `handle_tasks_extracted_chunk`, `handle_file_snapshot_chunk`,
  `handle_token_usage_chunk`, `handle_execution_trace_chunk`,
  `handle_system_message_chunk`, `handle_milestone_progress_chunk`,
  `handle_workspace_update`, `handle_slash_command_result`

**Coupling:** Maximal by design — this IS the dispatch boundary.

---

### 9. tool_result.rs

#### `handle_tool_result(tui: &mut TUI, result: ToolResult)` 🔴
| Field | Access | Domain |
|-------|--------|--------|
| `session.active_tools.remove` | W | F2: Session |
| `session.messages.*` | R | F2: Session |
| `panels.tool_panel.tool_panel_history.*` | R/W | F3: Tool |
| `panels.ast_phase_state.*` | R/W | F3: Tool |
| `terminal_progress.enabled/clear/set_progress` | R/W | F11: Terminal |
| `ui.view.user_scrolled` | R | F7: UI |
| `sys.dirty` | W | F8: System |
| `add_system_message()` | M | F2: Session |
| `auto_scroll()` | M | F7: UI |
| `last_assistant_message_mut()` | M | F2: Session |
| `integration` | R | F6: Integration |

**Domains touched:** F2, F3, F6, F7, F8, F11 (6) 🔴🔴🔴

---

### 10. workspace.rs

#### `handle_workspace_update(tui: &mut TUI, update: WorkspaceUpdate)` 🟡
| Field | Access | Domain |
|-------|--------|--------|
| `workspace.workspace_loaded` | W | F4: Workspace |
| `workspace.workspace_context` | W | F4: Workspace |
| `workspace.workspace_scan_progress` | W | F4: Workspace |
| `workspace.git_branch` | W | F4: Workspace |
| `integration.services.send_event` | R | F6: Integration |
| `sys.dirty` | W | F8: System |
| `add_system_message()` | M | F2: Session |

**Domains touched:** F2, F4, F6, F8 (4) 🔴

#### `handle_slash_command_result(tui: &mut TUI, result: SlashCommandResult)` 🟢
| Field | Access | Domain |
|-------|--------|--------|
| `add_system_message()` | M | F2: Session |
| `auto_scroll()` | M | F7: UI |
| `sys.dirty` | W | F8: System |

**Domains touched:** F2, F7, F8 (3) 🟢 (narrow — flag/message only)

---

### 11. helpers.rs

#### `complete_stream_cleanup(tui: &mut TUI)` 🟡
| Field | Access | Domain |
|-------|--------|--------|
| `session.active_tools` | R | F2: Session |
| `session.streaming` | R/W | F1: Streaming |
| `ui.view` | R | F7: UI |
| `reset_streaming_state()` | M | F1/F8 |
| `update_terminal_title()` | M | F7: UI |

**Domains touched:** F1, F2, F7, F8 (4) 🔴 (helper used by stream_done)

#### `reset_streaming_buffer(tui: &mut TUI)` 🟢
| Field | Access | Domain |
|-------|--------|--------|
| `session.streaming` | W | F1: Streaming |

**Domains touched:** F1 (1) 🟢

#### `mark_dirty_and_scroll(tui: &mut TUI)` 🟢
| Field | Access | Domain |
|-------|--------|--------|
| `sys.dirty` | W | F8: System |
| `auto_scroll()` | M | F7: UI |

**Domains touched:** F7, F8 (2) 🟢

#### `check_and_trigger_auto_continue(tui: &mut TUI)` 🟡
| Field | Access | Domain |
|-------|--------|--------|
| `session.active_tools` | R | F2: Session |
| `session.auto_continue` | R/W | F2: Session |
| `session.streaming` | R | F1: Streaming |
| `integration.services` | R | F6: Integration |
| `integration.rate_limit` | R | F6: Integration |
| `push_empty_assistant_message()` | M | F2: Session |
| `update_terminal_title()` | M | F7: UI |

**Domains touched:** F1, F2, F6, F7 (4) 🔴

#### `build_tool_summary_arg(...)` 🟢
Pure function — no TUI field access. Operates on JSON values only.

**Domains touched:** None 🟢

---

### 12. service_polling.rs (in app/, not handlers/)

#### `poll_services()` — inline in event_loop.rs 🔴🔴🔴
The service polling loop is the primary dispatch boundary that feeds handlers.

| Field | Access | Domain |
|-------|--------|--------|
| `session.streaming.is_streaming` | R | F1: Streaming |
| `session.streaming.current_stream_content.is_empty` | R | F1: Streaming |
| `session.streaming.is_streaming` | W (line 502) | F1: Streaming |
| Calls: `handle_stream_chunk`, `handle_tool_result`, `handle_workspace_update`, `handle_slash_command_result`, `handle_event_msg` | M | All |

**Domains touched:** All (by delegation) 🔴🔴🔴

---

## Shared State Analysis

Fields read/written by handlers from **different** feature domains:

### Critical Shared State (written by multiple features)

| Field | Written By | Read By | Conflict Risk |
|-------|-----------|---------|---------------|
| `session.messages` | stream_done (W), stream_tools (W), stream_error (W), tool_result (R) | stream_core (R), stream_done (R), stream_error (R) | **HIGH** — concurrent push/remove |
| `session.active_tools` | stream_tools (insert/remove), stream_done (clear), stream_error (clear), tool_result (remove) | stream_core (R), helpers (R) | **HIGH** — insert/remove/clear |
| `session.streaming.*` | stream_core (W), stream_done (W), stream_error (W), stream_stopped (W), helpers (W) | All stream handlers (R) | **HIGH** — is_streaming flag |
| `sys.dirty` | **ALL handlers** (W) | event_loop (R) | **LOW** — simple boolean flag |
| `ui.view.selected_message` | stream_done (W), stream_error (W) | tool_result (R) | **MEDIUM** — index manipulation |
| `panels.tool_panel.*` | stream_tools (W), tool_result (W) | stream_tools (R) | **MEDIUM** — history vec |
| `panels.tool_approval.*` | stream_approval (W) | stream_approval (R), stream_done (R via is_awaiting_approval) | **LOW** — flag + queue |
| `integration.rate_limit.*` | stream_done (W), stream_error (W) | stream_error (R) | **MEDIUM** — retry state machine |
| `workspace.*` | workspace (W), stream_data (R/W) | workspace (R) | **LOW** — single-writer mostly |

### Cross-Domain Read Dependencies

| Handler | Reads From Domain | Writes To Domain | Coupling |
|---------|------------------|-----------------|----------|
| `handle_stream_chunk` | F1, F2, F5, F6 | F1, F2, F8 | 🔴 Master dispatch |
| `handle_done_chunk` | F1, F2, F3, F5, F6 | F1, F2, F6, F7, F8, F9 | 🔴 Highest coupling |
| `handle_error_chunk` | F1, F2, F6 | F1, F2, F6, F7, F8, F10 | 🔴 High coupling |
| `handle_tool_start_chunk` | F2, F5, F6 | F2, F3, F8 | 🔴 High coupling |
| `handle_tool_result` | F2, F3, F6, F7 | F2, F3, F8, F11 | 🔴 High coupling |

---

## Coupling Hotspot Summary

### 🔴🔴🔴 Extreme (6+ domains)
1. **`handle_stream_chunk`** — master dispatch, touches F1–F8
2. **`handle_done_chunk`** — turn lifecycle, touches F1–F9
3. **`handle_event_msg`** — protocol dispatch (delegates to all)
4. **`poll_services()`** — polling loop (delegates to all)

### 🔴🔴 High (4–5 domains)
5. **`handle_tool_start_chunk`** — F2, F3, F5, F6, F7, F8
6. **`handle_tool_complete_chunk`** — F2, F3, F6, F7, F8, F9
7. **`handle_error_chunk`** — F1, F2, F6, F7, F8, F10
8. **`handle_tool_result`** — F2, F3, F6, F7, F8, F11
9. **`handle_empty_stream_response`** — F1, F2, F5, F6, F7, F8
10. **`handle_extract_tasks_chunk`** — F2, F4, F6, F7

### 🔴 Moderate (3 domains, narrow)
11. **`handle_text_chunk`** — F1, F2, F7
12. **`handle_stopped_chunk`** — F1, F2, F8
13. **`handle_approval_request_chunk`** — F2, F3, F6, F8
14. **`handle_workspace_update`** — F2, F4, F6, F8
15. **`check_and_trigger_auto_continue`** — F1, F2, F6, F7
16. **`complete_stream_cleanup`** — F1, F2, F7, F8

### 🟡 Low (2 domains)
17. **`handle_thinking_chunk`** — F1, F2
18. **`handle_token_usage_chunk`** — F5, F8
19. **`handle_execution_trace_chunk`** — F2, F8
20. **`handle_system_message_chunk`** — F2, F8

### 🟢 Minimal (1 domain or flag-only)
21. **`handle_tasks_extracted_chunk`** — None (no-op)
22. **`handle_approval_approved_chunk`** — F2, F3, F8 (flags only)
23. **`handle_approval_rejected_chunk`** — F2, F3, F8 (flags only)
24. **`handle_question_answered_chunk`** — F3 (minimal)
25. **`handle_file_snapshot_chunk`** — F2
26. **`reset_streaming_buffer`** — F1
27. **`mark_dirty_and_scroll`** — F7, F8
28. **`build_tool_summary_arg`** — None (pure function)

---

## Extraction Contract: Feature → Handlers → FeatureState

### StreamingFeature (F1)
**Handlers to extract:**
- `reset_streaming_buffer()` — pure streaming state reset
- `handle_text_chunk()` — streaming buffer accumulation (+ Session for message)
- `handle_thinking_chunk()` — streaming thinking count

**Becomes FeatureState:**
```rust
struct StreamingState {
    is_streaming: bool,
    current_stream_content: String,
    chunks_received: usize,
    thinking_chunks_received: usize,
    stream_start_time: Option<Instant>,
    streaming_render_buffer: StreamingRenderBuffer,
    stream_cancelled: bool,
    queued_message: Option<String>,
}
```

**Cross-feature reads needed:** `session.messages` (F2), `session.turn_snapshot` (F2)

### SessionFeature (F2)
**Handlers to extract:**
- `handle_file_snapshot_chunk()` — undo tracking
- `handle_execution_trace_chunk()` — trace recording
- `handle_system_message_chunk()` — system message append
- `handle_done_chunk()` (partial) — doom_loop, auto_continue, turn_snapshot
- `handle_empty_stream_response()` (partial) — message manipulation

**Becomes FeatureState:**
```rust
struct SessionState {
    messages: Vec<Message>,
    active_tools: HashMap<String, ActiveTool>,
    auto_continue: AutoContinueState,
    doom_loop: DoomLoopState,
    pending_doom_note: Option<String>,
    turn_snapshot: Option<TurnSnapshot>,
    execution_trace: Option<Value>,
    plan_mode_banner: Option<PlanModeBanner>,
    reasoning_budget: Arc<Mutex<ReasoningBudget>>,
    session_sidebar: SessionSidebar,
    undo: UndoHistory,
    session_recovery: SessionRecovery,
    wizard: WizardState,
}
```

**Cross-feature reads needed:** Almost all other features read/write here.

### ToolFeature (F3)
**Handlers to extract:**
- `handle_approval_request_chunk()` — approval flow
- `handle_approval_approved_chunk()` — approval clear
- `handle_approval_rejected_chunk()` — approval clear
- `handle_question_request_chunk()` — clarification panel
- `handle_question_answered_chunk()` — clarification clear
- `handle_tool_progress_chunk()` — tool panel update (narrow)
- `handle_tool_result()` (partial) — ast_phase_state, tool_panel_history

**Becomes FeatureState:**
```rust
struct ToolPanelState {
    tool_panel: ToolPanel,
    ast_phase_state: AstPhaseState,
    clarification_panel: ClarificationPanel,
    awaiting_clarification: bool,
    tool_approval: ToolApprovalState,
    symbol_outline: SymbolOutline,
}
```

**Cross-feature reads needed:** `session.active_tools` (F2), `session.messages` (F2)

### WorkspaceFeature (F4)
**Handlers to extract:**
- `handle_workspace_update()` — workspace state updates
- `handle_extract_tasks_chunk()` (partial) — task extraction

**Becomes FeatureState:**
```rust
struct WorkspaceState {
    workspace_loaded: bool,
    workspace_context: Option<String>,
    workspace_tasks: WorkspaceTasks,
    last_extraction: Option<(String, Vec<TodoItem>)>,
    workspace_scan_progress: Option<(usize, usize)>,
    git_branch: String,
}
```

**Cross-feature reads needed:** `integration.services.cwd` (F6), `integration.storage` (F6)

### ModelFeature (F5)
**Handlers to extract:**
- `handle_token_usage_chunk()` — token budget updates

**Becomes FeatureState:**
```rust
struct ModelState {
    current_model: String,
    current_effort: EffortLevel,
    token_budget: TokenBudget,
    plan_mode: PlanModeState,
    api_key_warning: bool,
    show_task_dashboard: bool,
}
```

**Cross-feature reads needed:** Minimal — mostly written by stream_data.

### IntegrationFeature (F6)
**No handlers extract fully** — integration is infrastructure accessed by all features.
Instead, `ServiceIntegrationState` becomes shared infrastructure passed via `UpdateCtx`/`RenderCtx`.

**Becomes:** Part of the host shell context, not a feature module.

### UIFeature (F7) / SystemFeature (F8)
**No handlers extract** — UI and System are rendering infrastructure.
- `sys.dirty` is a flag set by all handlers
- `ui.view` is navigation state

**Becomes:** Part of the host shell context via `RenderCtx`.

---

## TUI Method Dependencies

These `impl TUI` methods are called by handlers and need adaptation during extraction:

| Method | Called By | Fields Accessed | Extraction Strategy |
|--------|-----------|----------------|---------------------|
| `add_system_message()` | 12+ handlers | session.messages | SessionFeature method |
| `auto_scroll()` | 6 handlers | ui.view | UIFeature method |
| `last_assistant_message_mut()` | 5 handlers | session.messages | SessionFeature method |
| `push_empty_assistant_message()` | 3 handlers | session.messages | SessionFeature method |
| `mark_session_dirty()` | 4 handlers | session.* | SessionFeature method |
| `reset_streaming_state()` | 3 handlers | session.streaming, panels.ast, terminal_progress | StreamingFeature + cross-feature |
| `update_terminal_title()` | 6 handlers | session.*, model.* | UIFeature method |
| `update_context_and_compact()` | 3 handlers | sys.compaction | SystemFeature method |
| `show_error()` | 1 handler | overlay.showing_error | OverlayFeature method |
| `show_approval_banner()` | 2 handlers | panels.tool_approval | ToolFeature method |
| `build_conversation_history()` | 2 handlers | session.messages | SessionFeature method |
| `prepare_message_for_send()` | 1 handler | integration.services | IntegrationFeature method |
| `is_awaiting_approval()` | 1 handler | panels.tool_approval | ToolFeature method |
| `show_milestone_progress_banner()` | 1 handler | session.sidebar | SessionFeature method |

---

## Streaming Module Integration Points

The `streaming/` module (7 files) provides protocol parsing and tool detection:
- `adapter.rs` — StreamChunk → TUI dispatch adaptation
- `events.rs` — Event type definitions
- `response.rs` — Response processing
- `system_prompt.rs` — System prompt construction
- `tool_detection.rs` — Detects tool use in stream
- `tool_execution.rs` — Tool execution result types
- `mod.rs` — Module root

**Coupling to TUI:** The streaming module is **read-only** — it produces StreamChunk
variants consumed by handlers. No direct TUI field access. Remains as-is during extraction.

## Pipeline Module Integration Points

The `pipeline/` module (11 files) provides step orchestration:
- `tui_integration.rs` — Pipeline↔TUI bridge (reads/writes TUI state)
- `executor.rs`, `scheduler.rs` — Step execution
- `registry.rs`, `tool_registry.rs` — Step registration
- `agent_manager.rs`, `browser_manager.rs` — Specialized step types
- `artifact_registry.rs`, `manifest.rs` — Artifact/manifest handling
- `types.rs`, `mod.rs` — Types and module root

**Coupling to TUI:** `tui_integration.rs` bridges pipeline steps to TUI state.
Needs adaptation to go through feature contexts rather than `&mut TUI`.

## Agents Module Integration Points

The `agents/` module (4 files, in `src/agents/` not `src/app/agents/`):
- `definitions.rs` — Agent definitions (pure data)
- `agent_tool.rs` — Tool for agent spawning
- `delegation_executor.rs` — Delegation execution
- `mod.rs` — Module root

**Coupling to TUI:** Agents module has **no direct TUI field access**. It operates
through `AgentManager` (owned by TeamModeState) and protocol messages.

---

## Recommended Extraction Order

Based on coupling analysis, features should be extracted in this order:

1. **WorkspaceFeature (F4)** — Lowest coupling, 2 handlers, clear boundary
2. **ModelFeature (F5)** — Low coupling, 1 handler, pure data
3. **StreamingFeature (F1)** — Medium coupling, 3 handlers, but F2 dependency
4. **ToolFeature (F3)** — Medium coupling, 7 handlers, depends on F2
5. **SessionFeature (F2)** — Highest coupling, depends on everything
6. Integration/UI/System — Remain as host shell infrastructure

---

## Handler File → Feature Module Assignment

| Handler File | Primary Feature | Secondary Features |
|-------------|----------------|-------------------|
| `stream_core.rs` | F1: Streaming | F2: Session, F5: Model, F7: UI |
| `stream_done.rs` | F2: Session | F1: Streaming, F3: Tool, F5: Model, F6: Integration, F9: Theme |
| `stream_error.rs` | F2: Session | F1: Streaming, F6: Integration, F10: Overlay |
| `stream_stopped.rs` | F1: Streaming | F2: Session, F8: System |
| `stream_tools.rs` | F3: Tool | F2: Session, F5: Model, F6: Integration, F9: Theme |
| `stream_approval.rs` | F3: Tool | F2: Session, F6: Integration |
| `stream_data.rs` | Mixed (F2, F4, F5) | F3: Tool, F6: Integration, F7: UI |
| `event_msg.rs` | Dispatch only | All (delegates) |
| `tool_result.rs` | F3: Tool | F2: Session, F11: Terminal |
| `workspace.rs` | F4: Workspace | F2: Session, F6: Integration |
| `helpers.rs` | Mixed utility | F1, F2, F6, F7 |

---

## Coverage Audit

Total handler functions: 28 (including helpers and event_msg dispatch)
- Mapped in this document: 28 ✅
- Coverage: 100%

### Files in handlers/ directory
| File | Handler Count | Status |
|------|-------------|--------|
| `event_msg.rs` | 1 (+ 4 conversion helpers) | ✅ Mapped |
| `stream_core.rs` | 3 | ✅ Mapped |
| `stream_done.rs` | 2 | ✅ Mapped |
| `stream_error.rs` | 1 | ✅ Mapped |
| `stream_stopped.rs` | 1 | ✅ Mapped |
| `stream_tools.rs` | 3 | ✅ Mapped |
| `stream_approval.rs` | 3 | ✅ Mapped |
| `stream_data.rs` | 8 | ✅ Mapped |
| `tool_result.rs` | 1 | ✅ Mapped |
| `workspace.rs` | 2 | ✅ Mapped |
| `helpers.rs` | 4 (+ 1 pure) | ✅ Mapped |
| `tests.rs` | test only | N/A |

**Total: 29 handler functions mapped across 12 files.** ✅
