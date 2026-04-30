//! Plugin loader for dynamic tool loading

/// Plugin loader for dynamic tool registration
pub struct PluginLoader;

impl Default for PluginLoader {
    fn default() -> Self {
        Self::new()
    }
}

impl PluginLoader {
    /// Create a new plugin loader
    pub const fn new() -> Self {
        Self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plugin_loader_is_unit_struct() {
        let _: PluginLoader = PluginLoader;
    }
}
