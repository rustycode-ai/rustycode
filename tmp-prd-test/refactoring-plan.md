# Refactoring Plan: `render/` Modules (3806 LOC across 8 files)

## Duplication #1: `shared.rs` helpers copied inline (25 sites)
`estimate_line_count`, `format_byte_size`, `safe_truncate` exist in `shared.rs` but are reimplemented inline in `messages.rs` and `status.rs`.
**Before**: `let lines = content.lines().count() + content.lines().filter(|l| l.len() > w).count();`
**After**: `let lines = shared::estimate_line_count_wrapped(&content, max_width);`

## Duplication #2: Status icon `Span::styled` patterns (48 occurrences)
`"●"` green, `"✗"` red repeated identically across `messages.rs`, `tools.rs`, `status.rs`.
**Before**: `Span::styled("● ".into(), Style::default().fg(Color::Green))` (×48)
**After**: `shared::status_icon(success: bool) -> Span<'static>` (1 call each)

## Duplication #3: Tool summary formatting (2 files, ~40 LOC each)
Both `tools.rs` and `messages.rs` format `name + duration + byte_size`.
**Before**: `format!("{} {} {}", tool.name, format_duration_ms(d), format_byte_size(s))`
**After**: `shared::format_tool_summary(name, duration_ms, output_len)`

## Duplication #4: Chunked scroll-slice rendering (3 files, ~15 LOC each)
`messages.rs`, `tools.rs`, `selectors.rs` all compute visible slice from scroll offset.
**Before**: `let start = scroll.min(items.len()); let vis = &items[start..]; for i in vis.take(h) {}`
**After**: `shared::visible_slice(items, scroll, height) -> &[T]`

## File Changes & Savings

| File | Action | LOC Δ |
|------|--------|-------|
| `shared.rs` | Add `status_icon`, `format_tool_summary`, `visible_slice` | +40 |
| `messages.rs` | Replace inline patterns | −60 |
| `tools.rs` | Replace inline patterns | −35 |
| `status.rs` | Replace inline patterns | −25 |
| `selectors.rs` | Replace chunked rendering | −15 |

**Net: −95 LOC** (2.5%). Zero behavior change.

## Execution Order
1. Add 3 functions + tests to `shared.rs`
2. Migrate `tools.rs` → `status.rs` → `messages.rs` → `selectors.rs`
3. `cargo clippy -p rustycode-tui -- -D warnings && cargo test -p rustycode-tui`
