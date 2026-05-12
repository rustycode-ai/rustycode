//! Tool Inspection Manager and core types
//!
//! Holds ToolInspectionManager. Extracted from inspector.rs to enable splitting
//! inspectors into separate files. Core types (ToolInspector, InspectionResult, etc)
//! are now in crates/rustycode-tools/src/executor/inspector/mod.rs.

pub use crate::executor::inspector::{
    InspectionAction, InspectionResult, ToolCallInfo, ToolInspector,
};
use crate::ToolContext;
/// Manages a pipeline of tool inspectors
pub struct ToolInspectionManager {
    inspectors: Vec<Box<dyn ToolInspector>>,
}

impl Default for ToolInspectionManager {
    fn default() -> Self {
        Self::new()
    }
}

impl ToolInspectionManager {
    pub fn new() -> Self {
        Self {
            inspectors: Vec::new(),
        }
    }

    /// Create a manager with default inspectors
    pub fn with_defaults(max_repetitions: u32) -> Self {
        let mut manager = Self::new();
        manager.add_inspector(Box::new(
            crate::executor::inspector::RepetitionInspector::new(Some(max_repetitions)),
        ));
        manager.add_inspector(Box::new(
            crate::executor::inspector::PermissionInspector::new(),
        ));
        manager
    }

    /// Create a manager with all inspectors including security scanning
    pub fn with_security(max_repetitions: u32) -> Self {
        let mut manager = Self::new();
        manager.add_inspector(Box::new(
            crate::executor::inspector::SecurityInspector::new(),
        ));
        manager.add_inspector(Box::new(crate::executor::inspector::EgressInspector::new()));
        manager.add_inspector(Box::new(crate::executor::inspector::OsvInspector::new()));
        manager.add_inspector(Box::new(
            crate::executor::inspector::RepetitionInspector::new(Some(max_repetitions)),
        ));
        manager.add_inspector(Box::new(
            crate::executor::inspector::PermissionInspector::new(),
        ));
        manager
    }

    /// Add an inspector to the pipeline
    pub fn add_inspector(&mut self, inspector: Box<dyn ToolInspector>) {
        self.inspectors.push(inspector);
    }

    /// Run all inspectors on a tool call
    ///
    /// Returns all results. If any inspector denies, the call should be blocked.
    /// The most restrictive action wins: Deny > `RequireApproval` > Allow.
    pub fn inspect(
        &self,
        call: &ToolCallInfo,
        history: &[ToolCallInfo],
        ctx: &ToolContext,
    ) -> Vec<InspectionResult> {
        let mut results = Vec::new();

        for inspector in &self.inspectors {
            if !inspector.is_enabled() {
                continue;
            }

            let result = inspector.inspect(call, history, ctx);
            tracing::debug!(
                "[{}] action={:?} reason={}",
                inspector.name(),
                result.action,
                result.reason
            );
            results.push(result);
        }

        results
    }

    /// Check if a tool call should be allowed
    ///
    /// Returns the most restrictive action from all inspectors.
    pub fn check(
        &self,
        call: &ToolCallInfo,
        history: &[ToolCallInfo],
        ctx: &ToolContext,
    ) -> InspectionAction {
        let results = self.inspect(call, history, ctx);

        let mut action = InspectionAction::Allow;
        for result in &results {
            match (&action, &result.action) {
                (_, InspectionAction::Deny) => {
                    return InspectionAction::Deny;
                }
                (InspectionAction::Allow, InspectionAction::RequireApproval(msg)) => {
                    action = InspectionAction::RequireApproval(msg.clone());
                }
                _ => {}
            }
        }
        action
    }

    /// Get the denial reason if any inspector denied the call
    pub fn denial_reason(
        &self,
        call: &ToolCallInfo,
        history: &[ToolCallInfo],
        ctx: &ToolContext,
    ) -> Option<String> {
        let results = self.inspect(call, history, ctx);
        results
            .iter()
            .find(|r| r.action == InspectionAction::Deny)
            .map(|r| r.reason.clone())
    }

    /// Get names of all registered inspectors
    pub fn inspector_names(&self) -> Vec<&'static str> {
        self.inspectors.iter().map(|i| i.name()).collect()
    }

    /// Reset all inspectors
    pub fn reset_all(&self) {
        for inspector in &self.inspectors {
            inspector.reset();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn test_ctx() -> ToolContext {
        ToolContext::new(std::env::temp_dir())
    }

    fn make_call(name: &str, args: serde_json::Value) -> ToolCallInfo {
        ToolCallInfo::new("test-id", name, args)
    }

    #[test]
    fn test_inspection_manager_pipeline() {
        let mut manager = ToolInspectionManager::new();
        manager.add_inspector(Box::new(
            crate::executor::inspector::RepetitionInspector::new(Some(3)),
        ));
        manager.add_inspector(Box::new(
            crate::executor::inspector::PermissionInspector::new(),
        ));

        let ctx = test_ctx();
        let call = make_call("Read", json!({"path": "/tmp/test.txt"}));

        let results = manager.inspect(&call, &[], &ctx);
        assert!(!results.is_empty());
        assert!(results.iter().all(|r| r.action == InspectionAction::Allow));
    }

    #[test]
    fn test_inspection_manager_check() {
        let mut manager = ToolInspectionManager::new();
        manager.add_inspector(Box::new(
            crate::executor::inspector::PermissionInspector::new(),
        ));

        let ctx = test_ctx();

        // Read-only tool should be allowed
        let read_call = make_call("Read", json!({"path": "/tmp/test.txt"}));
        assert_eq!(
            manager.check(&read_call, &[], &ctx),
            InspectionAction::Allow
        );

        // Bash should require approval
        let bash_call = make_call("Bash", json!({"command": "ls"}));
        assert!(matches!(
            manager.check(&bash_call, &[], &ctx),
            InspectionAction::RequireApproval(_)
        ));
    }

    #[test]
    fn test_inspection_manager_deny_wins() {
        let mut manager = ToolInspectionManager::new();
        manager.add_inspector(Box::new(
            crate::executor::inspector::RepetitionInspector::new(Some(2)),
        ));
        manager.add_inspector(Box::new(
            crate::executor::inspector::PermissionInspector::new(),
        ));

        let ctx = test_ctx();
        let call = make_call("Bash", json!({"command": "ls"}));
        let history = vec![call.clone(), call.clone()];

        // Repetition inspector denies, permission inspector requires approval
        // Deny should win
        let action = manager.check(&call, &history, &ctx);
        assert_eq!(action, InspectionAction::Deny);
    }

    #[test]
    fn test_inspection_manager_denial_reason() {
        let mut manager = ToolInspectionManager::new();
        manager.add_inspector(Box::new(
            crate::executor::inspector::RepetitionInspector::new(Some(2)),
        ));

        let ctx = test_ctx();
        let call = make_call("Bash", json!({"command": "ls"}));
        let history = vec![call.clone(), call.clone()];

        let reason = manager.denial_reason(&call, &history, &ctx);
        assert!(reason.is_some());
        assert!(reason.unwrap().contains("repeated"));
    }

    #[test]
    fn test_inspection_manager_default_inspectors() {
        let manager = ToolInspectionManager::with_defaults(5);
        let names = manager.inspector_names();
        assert!(names.contains(&"repetition"));
        assert!(names.contains(&"permission"));
    }

    #[test]
    fn test_inspection_manager_with_security() {
        let manager = ToolInspectionManager::with_security(5);
        let names = manager.inspector_names();
        assert!(names.contains(&"security"));
        assert!(names.contains(&"repetition"));
        assert!(names.contains(&"permission"));
    }

    #[test]
    fn test_inspection_manager_reset() {
        let manager = ToolInspectionManager::with_defaults(3);
        manager.reset_all();
        // Should not panic
    }
}
