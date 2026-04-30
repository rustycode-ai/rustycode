// Render implementation for TUI
//
// This module contains the render methods for the TUI struct,
// split into sub-files for maintainability.
//
// Note: This file is included directly in event_loop.rs, so we use
// fully qualified paths to avoid import conflicts.

// Import shared helpers for all render sub-files
use crate::app::render::shared::{estimate_line_count, format_duration_ms, safe_truncate, shorten_path, tool_kind_icon};

/// Status for the status bar (local to render implementation)
enum RenderStatus {
    PlanMode {
        banner: crate::app::plan_mode_ops::PlanModeBanner,
    },
    Thinking {
        chunks_received: usize,
        thinking_chunks_received: usize,
    },
    RunningTools {
        count: usize,
        tool_names: Vec<String>,
        remaining: usize,
    },
    AstPhase {
        phase: String,
        phase_index: usize,
        milestones_completed: usize,
        milestones_total: usize,
        elapsed_ms: u64,
    },
    Idle,
}

// Each sub-file contains its own `impl TUI { ... }` block with related methods.
// This allows splitting without needing include!() inside an impl block.

// Chat message rendering with auto-scroll and search highlighting
include!("render/messages.rs");

// Tool panel and result detail overlay
include!("render/tools.rs");

// Input area rendering
include!("render/input.rs");

// Status bar rendering
include!("render/status.rs");

// Model/provider selector overlays
include!("render/selectors.rs");

// Search box rendering
include!("render/search.rs");

// BrutalistRenderer helper (single construction site)
include!("render/brutalist.rs");
