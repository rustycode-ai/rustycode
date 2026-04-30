use serde::{Deserialize, Serialize};

/// Result of applying an instinct
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionResult {
    pub instinct_id: String,
    pub success: bool,
    pub changes: Vec<Change>,
    pub feedback: String,
}

/// A change made by an instinct
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Change {
    pub change_type: ChangeType,
    pub description: String,
    pub content: String,
    pub file_path: String,
}

/// Type of change
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[non_exhaustive]
pub enum ChangeType {
    Code,
    Documentation,
    Configuration,
    Test,
    Refactor,
    Other,
}

impl ActionResult {
    pub const fn new(instinct_id: String) -> Self {
        Self {
            instinct_id,
            success: true,
            changes: Vec::new(),
            feedback: String::new(),
        }
    }

    pub const fn with_success(mut self, success: bool) -> Self {
        self.success = success;
        self
    }

    pub fn with_feedback(mut self, feedback: String) -> Self {
        self.feedback = feedback;
        self
    }

    pub fn with_change(mut self, change: Change) -> Self {
        self.changes.push(change);
        self
    }

    pub const fn has_changes(&self) -> bool {
        !self.changes.is_empty()
    }
}

impl Change {
    pub const fn new(
        change_type: ChangeType,
        description: String,
        content: String,
        file_path: String,
    ) -> Self {
        Self {
            change_type,
            description,
            content,
            file_path,
        }
    }

    /// Create a code change
    pub const fn code(description: String, content: String, file_path: String) -> Self {
        Self::new(ChangeType::Code, description, content, file_path)
    }

    /// Create a documentation change
    pub const fn docs(description: String, content: String, file_path: String) -> Self {
        Self::new(ChangeType::Documentation, description, content, file_path)
    }

    /// Create a configuration change
    pub const fn config(description: String, content: String, file_path: String) -> Self {
        Self::new(ChangeType::Configuration, description, content, file_path)
    }

    /// Create a test change
    pub const fn test(description: String, content: String, file_path: String) -> Self {
        Self::new(ChangeType::Test, description, content, file_path)
    }

    /// Create a refactoring change
    pub const fn refactor(description: String, content: String, file_path: String) -> Self {
        Self::new(ChangeType::Refactor, description, content, file_path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_action_result() {
        let result = ActionResult::new("test-instinct".to_string())
            .with_success(true)
            .with_feedback("Applied successfully".to_string())
            .with_change(Change::code(
                "Fix error".to_string(),
                "fn test() {}".to_string(),
                "test.rs".to_string(),
            ));

        assert!(result.success);
        assert_eq!(result.feedback, "Applied successfully");
        assert_eq!(result.changes.len(), 1);
        assert!(result.has_changes());
    }

    #[test]
    fn test_change_creation() {
        let code_change = Change::code(
            "Add function".to_string(),
            "fn foo() {}".to_string(),
            "foo.rs".to_string(),
        );

        assert_eq!(code_change.change_type, ChangeType::Code);
        assert_eq!(code_change.description, "Add function");
        assert_eq!(code_change.file_path, "foo.rs");

        let docs_change = Change::docs(
            "Add docs".to_string(),
            "/// Documentation".to_string(),
            "foo.rs".to_string(),
        );

        assert_eq!(docs_change.change_type, ChangeType::Documentation);
    }

    #[test]
    fn test_action_result_default_success() {
        let result = ActionResult::new("id-1".to_string());
        assert!(result.success);
        assert!(result.changes.is_empty());
        assert!(result.feedback.is_empty());
        assert!(!result.has_changes());
    }

    #[test]
    fn test_action_result_with_failure() {
        let result = ActionResult::new("id-2".to_string()).with_success(false);
        assert!(!result.success);
    }

    #[test]
    fn test_action_result_multiple_changes() {
        let result = ActionResult::new("id-3".to_string())
            .with_change(Change::code("fix".into(), "code".into(), "a.rs".into()))
            .with_change(Change::docs(
                "docs".into(),
                "doc text".into(),
                "a.rs".into(),
            ))
            .with_change(Change::test(
                "test".into(),
                "assert!".into(),
                "a_test.rs".into(),
            ));
        assert_eq!(result.changes.len(), 3);
    }

    #[test]
    fn test_change_config() {
        let change = Change::config(
            "Update config".into(),
            "key = value".into(),
            "config.toml".into(),
        );
        assert_eq!(change.change_type, ChangeType::Configuration);
    }

    #[test]
    fn test_change_refactor() {
        let change = Change::refactor(
            "Extract method".into(),
            "fn helper()".into(),
            "lib.rs".into(),
        );
        assert_eq!(change.change_type, ChangeType::Refactor);
    }

    #[test]
    fn test_change_test() {
        let change = Change::test("Add unit test".into(), "#[test]".into(), "test.rs".into());
        assert_eq!(change.change_type, ChangeType::Test);
    }

    #[test]
    fn test_change_type_serde_roundtrip() {
        for ct in &[
            ChangeType::Code,
            ChangeType::Documentation,
            ChangeType::Configuration,
            ChangeType::Test,
            ChangeType::Refactor,
            ChangeType::Other,
        ] {
            let json = serde_json::to_string(ct).unwrap();
            let decoded: ChangeType = serde_json::from_str(&json).unwrap();
            assert_eq!(*ct, decoded);
        }
    }

    #[test]
    fn test_action_result_serialization() {
        let result = ActionResult::new("ser-1".into())
            .with_feedback("done".into())
            .with_change(Change::code("fix".into(), "code".into(), "f.rs".into()));
        let json = serde_json::to_string(&result).unwrap();
        let decoded: ActionResult = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.instinct_id, "ser-1");
        assert!(decoded.success);
        assert_eq!(decoded.changes.len(), 1);
    }

    #[test]
    fn test_change_serialization() {
        let change = Change::new(
            ChangeType::Other,
            "desc".into(),
            "content".into(),
            "path".into(),
        );
        let json = serde_json::to_string(&change).unwrap();
        let decoded: Change = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.change_type, ChangeType::Other);
        assert_eq!(decoded.description, "desc");
    }
}
