//! Context-aware tool filtering capabilities.
//!
//! Tools are filtered based on the task profile, harness requirements, and
//! session mode to prevent context-overflow and LLM hallucination.

use serde::{Deserialize, Serialize};

/// Criteria for filtering the tool registry before injection into the LLM context.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ToolFilterCriteria {
    /// Only allow tools with these specific tags.
    pub allowed_tags: Option<Vec<String>>,
    /// Exclude tools with these permissions (e.g., exclude `Network` for safety).
    pub excluded_permissions: Vec<String>,
    /// Minimum task relevance score (0.0 - 1.0) for a tool to be included.
    pub min_relevance: f32,
    /// Force inclusion of essential tools (e.g., system recovery).
    pub force_include: Vec<String>,
}

impl ToolFilterCriteria {
    pub fn new() -> Self {
        Self::default()
    }
}
