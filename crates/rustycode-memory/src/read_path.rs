//! Read path: builds memory instructions for injection into the system prompt.
//!
//! Loads `memory_summary.md` (truncated to token limit) and wraps it with
//! usage instructions. Project-scoped: takes a memory directory path.

use std::path::Path;

use crate::memdir;

/// Build the developer instructions block injected into the system prompt.
///
/// Returns `None` if the memory system hasn't been initialized or the summary
/// is empty/default.
pub fn build_memory_instructions(mem_dir: &Path) -> Option<String> {
    if let Err(e) = memdir::ensure_layout(mem_dir) {
        tracing::warn!("failed to ensure memory layout: {e}");
    }

    let summary = match memdir::read_memory_summary(mem_dir) {
        Ok(s) => s,
        Err(e) => {
            tracing::debug!("failed to read memory summary: {e}");
            return None;
        }
    };

    // Skip if the summary is just the seed content.
    if summary.trim().is_empty() || summary.contains("No memories yet") {
        return None;
    }

    Some(render_instructions(&summary))
}

/// Render the full memory instructions block with the summary embedded.
fn render_instructions(summary: &str) -> String {
    format!(
        r"# Memory System

You have access to a persistent memory system that stores knowledge across sessions.

## Memory Layout

- **memory_summary.md** — compact summary of key memories (shown below)
- **MEMORY.md** — searchable registry with topic pointers and decisions
- **rollout_summaries/** — per-session detailed notes
- **extensions/ad_hoc/notes/** — user-requested memory updates

## Quick Memory Pass

When starting a task, do a quick memory pass:

1. **Skim** the summary below for relevant context
2. **Search** MEMORY.md for keywords related to the current task (max 4-6 lookups)
3. **Open** relevant rollout summaries only when the summary entry is insufficient
4. **Budget**: keep memory lookups to ≤6 steps to avoid excessive I/O

## Current Memory Summary

{summary}

## Rules

- Only update memories when the user explicitly asks
- Write user-requested updates to `extensions/ad_hoc/notes/` as dated markdown files
- Do NOT fabricate memory content — if unsure, say you don't have that information
",
        summary = summary.trim(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_instructions_contains_summary() {
        let instructions = render_instructions("Test summary content.\nLine 2.");
        assert!(instructions.contains("Test summary content"));
        assert!(instructions.contains("Quick Memory Pass"));
        assert!(instructions.contains("MEMORY.md"));
    }

    #[test]
    fn render_instructions_has_sections() {
        let instructions = render_instructions("Summary.");
        assert!(instructions.contains("## Memory Layout"));
        assert!(instructions.contains("## Quick Memory Pass"));
        assert!(instructions.contains("## Current Memory Summary"));
        assert!(instructions.contains("## Rules"));
    }
}
