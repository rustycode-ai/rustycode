//! Git tool provider

use crate::providers::git::{GitCommitTool, GitDiffTool, GitLogTool, GitStatusTool};
use crate::registry_builder::ToolProvider;
use crate::ToolRegistry;
use anyhow::Result;

/// Provider for git version control tools
pub struct GitProvider;

impl ToolProvider for GitProvider {
    fn register_tools(&self, registry: &mut ToolRegistry) -> Result<()> {
        registry.register(GitStatusTool);
        registry.register(GitDiffTool);
        registry.register(GitLogTool);
        registry.register(GitCommitTool);
        Ok(())
    }
}
