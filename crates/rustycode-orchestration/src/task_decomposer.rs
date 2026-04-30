use crate::error::Result;
use crate::types::{Difficulty, OutputType, Step};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DecomposedTask {
    pub original_task: String,
    pub task_category: String,
    pub steps: Vec<Step>,
    pub estimated_difficulty: Difficulty,
}

#[allow(async_fn_in_trait)]
pub trait TaskDecomposer: Send + Sync {
    async fn decompose(&self, task: &str, category: &str) -> Result<DecomposedTask>;
}

pub struct Decomposer {}

impl Decomposer {
    pub const fn new() -> Self {
        Self {}
    }
}

impl Default for Decomposer {
    fn default() -> Self {
        Self::new()
    }
}

impl TaskDecomposer for Decomposer {
    async fn decompose(&self, task: &str, category: &str) -> Result<DecomposedTask> {
        Ok(DecomposedTask {
            original_task: task.to_string(),
            task_category: category.to_string(),
            steps: vec![Step {
                id: "step-1".into(),
                index: 0,
                description: format!("Setup for: {task}"),
                expected_output_type: OutputType::Verification,
                suggested_tool: Some("bash".into()),
                retry_on_failure: true,
                required_resources: crate::guard::RequiredResources::default(),
            }],
            estimated_difficulty: Difficulty::Easy,
        })
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_decompose_returns_original_task() {
        let decomposer = Decomposer::new();
        let result = decomposer
            .decompose("Build a web server", "code")
            .await
            .unwrap();
        assert_eq!(result.original_task, "Build a web server");
        assert_eq!(result.task_category, "code");
    }

    #[tokio::test]
    async fn test_decompose_returns_steps() {
        let decomposer = Decomposer::new();
        let result = decomposer
            .decompose("Implement auth", "code")
            .await
            .unwrap();
        assert!(!result.steps.is_empty());
        assert_eq!(result.steps[0].id, "step-1");
        assert_eq!(result.steps[0].index, 0);
    }

    #[tokio::test]
    async fn test_decompose_step_description_contains_task() {
        let decomposer = Decomposer::new();
        let result = decomposer.decompose("Fix the bug", "debug").await.unwrap();
        assert!(result.steps[0].description.contains("Fix the bug"));
    }

    #[tokio::test]
    async fn test_decompose_defaults_to_easy_difficulty() {
        let decomposer = Decomposer::new();
        let result = decomposer.decompose("Simple task", "code").await.unwrap();
        assert_eq!(result.estimated_difficulty, Difficulty::Easy);
    }

    #[tokio::test]
    async fn test_decompose_step_uses_bash_tool() {
        let decomposer = Decomposer::new();
        let result = decomposer.decompose("Run tests", "test").await.unwrap();
        assert_eq!(result.steps[0].suggested_tool, Some("bash".into()));
        assert!(result.steps[0].retry_on_failure);
    }

    #[tokio::test]
    async fn test_decompose_serialization_roundtrip() {
        let decomposer = Decomposer::new();
        let result = decomposer.decompose("Build feature", "code").await.unwrap();
        let json = serde_json::to_string(&result).unwrap();
        let back: DecomposedTask = serde_json::from_str(&json).unwrap();
        assert_eq!(result.original_task, back.original_task);
        assert_eq!(result.task_category, back.task_category);
        assert_eq!(result.steps.len(), back.steps.len());
    }

    #[test]
    fn test_decomposed_task_fields() {
        let task = DecomposedTask {
            original_task: "test".into(),
            task_category: "code".into(),
            steps: vec![],
            estimated_difficulty: Difficulty::Medium,
        };
        assert_eq!(task.original_task, "test");
        assert_eq!(task.task_category, "code");
        assert!(task.steps.is_empty());
        assert_eq!(task.estimated_difficulty, Difficulty::Medium);
    }
}
