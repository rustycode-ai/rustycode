/// Well-known configuration templates
///
/// This module will provide pre-built configuration templates for common use cases.
pub struct WellKnownTemplates;

impl WellKnownTemplates {
    pub const fn new() -> Self {
        Self
    }
}

impl Default for WellKnownTemplates {
    fn default() -> Self {
        Self::new()
    }
}
