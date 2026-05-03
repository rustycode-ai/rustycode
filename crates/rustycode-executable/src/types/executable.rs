use crate::types::{AdvancedToolMetadata, ToolSchema, UnitCapabilities};
use crate::Callable;
use std::sync::Arc;

/// A callable entity that can behave as tool, skill, or agent based on context
#[derive(Clone)]
pub struct ExecutableUnit {
    /// Unique identifier (e.g., "bash", `edit_file`, `code_reviewer`)
    pub id: String,

    /// Human-readable name
    pub name: String,

    /// Description of what the unit does
    pub description: String,

    /// What execution contexts this unit supports
    pub capabilities: UnitCapabilities,

    /// Advanced tool use metadata
    pub advanced_metadata: AdvancedToolMetadata,

    /// The execution implementation
    pub handler: Arc<dyn Callable>,

    /// Where this unit came from
    pub source: UnitSource,

    /// Optional: structured input/output schema
    pub schema: Option<ToolSchema>,

    /// Optional: tags for discovery
    pub tags: Vec<String>,

    /// Optional: version for evolution tracking
    pub version: Option<String>,
}

/// Where the unit originated
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type")]
pub enum UnitSource {
    /// From rustycode-tools
    NativeTool { path: String },

    /// From Claude Code ~/.claude/skills
    InstalledSkill {
        path: String,
        version: Option<String>,
    },

    /// From `RustyCode` agents
    BundledAgent { path: String },
}
