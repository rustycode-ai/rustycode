//! File system tool provider

use crate::providers::fs::{ReadFileTool, WriteFileTool};
use crate::providers::list_dir::ListDirTool;
use crate::registry_builder::ToolProvider;
use crate::ToolRegistry;
use anyhow::Result;

/// Provider for essential file system tools
pub struct FileSystemProvider;

impl ToolProvider for FileSystemProvider {
    fn register_tools(&self, registry: &mut ToolRegistry) -> Result<()> {
        registry.register(ReadFileTool);
        registry.register(WriteFileTool);
        registry.register(ListDirTool);
        Ok(())
    }
}
