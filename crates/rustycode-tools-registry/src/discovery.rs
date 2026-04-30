//! Tool discovery utilities

/// Tool discovery for automatic tool loading
pub struct ToolDiscovery;

impl Default for ToolDiscovery {
    fn default() -> Self {
        Self::new()
    }
}

impl ToolDiscovery {
    /// Create a new tool discovery instance
    pub const fn new() -> Self {
        Self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discovery_is_unit_struct() {
        let _: ToolDiscovery = ToolDiscovery;
    }
}
