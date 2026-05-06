//! Bridge between rustycode-vector-memory and rustycode-memory.
//!
//! Converts vector-memory entries into the summary format used by
//! `memdir::generate_summary()`, then writes to `memory_summary.md`
//! for system prompt injection.

use std::path::Path;

#[cfg(feature = "vector-memory")]
use rustycode_vector_memory::{MemoryType, VectorMemory};

/// Refresh `memory_summary.md` from vector-memory entries.
///
/// Loads all active entries across all memory types, converts to the
/// summary tuple format, generates markdown, and writes atomically.
#[cfg(feature = "vector-memory")]
pub fn refresh_summary_from_vector_memory(
    vector_mem: &VectorMemory,
    mem_dir: &Path,
) -> anyhow::Result<usize> {
    let entries = collect_active_entries(vector_mem);
    let count = entries.len();

    if entries.is_empty() {
        rustycode_memory::memdir::write_memory_summary(
            mem_dir,
            "# Memory Summary\n\nNo memories yet.\n",
        )?;
        return Ok(0);
    }

    let summary = rustycode_memory::memdir::generate_summary(&entries);
    rustycode_memory::memdir::write_memory_summary(mem_dir, &summary)?;
    Ok(count)
}

/// Collect all active vector-memory entries into the summary tuple format.
#[cfg(feature = "vector-memory")]
fn collect_active_entries(vector_mem: &VectorMemory) -> Vec<(String, String, f32)> {
    let types = [
        MemoryType::Learnings,
        MemoryType::CodePatterns,
        MemoryType::TaskTraces,
        MemoryType::ToolUsage,
    ];

    let mut entries = Vec::new();
    for mt in &types {
        let type_name = format!("{mt:?}").to_ascii_lowercase();
        for entry in vector_mem.active(*mt) {
            entries.push((
                entry.content.clone(),
                type_name.clone(),
                entry.metadata.confidence,
            ));
        }
    }
    entries
}

/// Refresh summary without vector-memory backend (seed content only).
#[cfg(not(feature = "vector-memory"))]
pub fn refresh_summary_from_vector_memory(_no_data: &(), mem_dir: &Path) -> anyhow::Result<usize> {
    rustycode_memory::memdir::ensure_layout(mem_dir)?;
    Ok(0)
}
