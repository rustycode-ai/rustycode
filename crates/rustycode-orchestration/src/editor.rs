use crate::bus::BusHandle;
use crate::error::Result;
use crate::error_signal::ErrorSignal;
use crate::execution_trace::ExecutionTrace;
use crate::isolation::TierIsolation;
use crate::types::Step;
use std::sync::Arc;

pub struct Editor {
    bus: BusHandle,
    isolation: Arc<tokio::sync::RwLock<TierIsolation>>,
}

impl Editor {
    pub fn new(bus: BusHandle) -> Self {
        Self {
            bus,
            isolation: Arc::new(tokio::sync::RwLock::new(TierIsolation::with_defaults())),
        }
    }

    pub fn with_isolation(mut self, isolation: Arc<tokio::sync::RwLock<TierIsolation>>) -> Self {
        self.isolation = isolation;
        self
    }

    #[allow(clippy::unused_async)]
    pub async fn patch_score(
        &self,
        _trace: &ExecutionTrace,
        failed_step: &Step,
        error: &ErrorSignal,
    ) -> Result<Vec<Step>> {
        let reviewed_step = failed_step.clone();
        // Preserve original command - do not modify description

        tracing::info!(
            step_id = %failed_step.id,
            error = %error.message,
            "Editor reviewing step"
        );

        self.bus
            .publish(crate::bus::OrchestrationEvent::PartialResult {
                step_id: failed_step.id.clone(),
                content: format!("Editor reviewed: {}", error.message),
            });

        Ok(vec![reviewed_step])
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::error_signal::SignalCategory;

    fn sample_step() -> Step {
        Step {
            id: "step-1".into(),
            index: 0,
            description: "Run tests".into(),
            expected_output_type: crate::types::OutputType::Verification,
            suggested_tool: Some("bash".into()),
            retry_on_failure: true,
            required_resources: crate::guard::RequiredResources::new(),
        }
    }

    fn make_bus() -> BusHandle {
        BusHandle::new(16)
    }

    #[tokio::test]
    async fn test_editor_patches_step_description() {
        let editor = Editor::new(make_bus());
        let step = sample_step();
        let error = ErrorSignal::new(
            SignalCategory::LogicError,
            Some(1),
            "test failed".into(),
            "step-1".into(),
            "bash".into(),
        );
        let trace = ExecutionTrace::new("task-1".into());
        let result = editor.patch_score(&trace, &step, &error).await.unwrap();
        assert_eq!(result.len(), 1);
        // Editor reviews but does not modify the command
        assert_eq!(result[0].description, "Run tests");
    }

    #[tokio::test]
    async fn test_editor_preserves_step_id() {
        let editor = Editor::new(make_bus());
        let step = sample_step();
        let error = ErrorSignal::new(
            SignalCategory::LogicError,
            Some(1),
            "error".into(),
            "step-1".into(),
            "bash".into(),
        );
        let trace = ExecutionTrace::new("task-1".into());
        let result = editor.patch_score(&trace, &step, &error).await.unwrap();
        assert_eq!(result[0].id, "step-1");
    }

    #[tokio::test]
    async fn test_editor_publishes_bus_event() {
        let bus = make_bus();
        let mut rx = bus.subscribe();
        let editor = Editor::new(bus);
        let step = sample_step();
        let error = ErrorSignal::new(
            SignalCategory::SyntaxError,
            Some(2),
            "parse error".into(),
            "step-1".into(),
            "bash".into(),
        );
        let trace = ExecutionTrace::new("task-1".into());
        editor.patch_score(&trace, &step, &error).await.unwrap();

        let event = rx.try_recv().unwrap();
        match event {
            crate::bus::OrchestrationEvent::PartialResult { step_id, content } => {
                assert_eq!(step_id, "step-1");
                assert!(content.contains("Editor reviewed"));
            }
            other => panic!("Expected PartialResult, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_editor_with_various_error_categories() {
        let editor = Editor::new(make_bus());
        let step = sample_step();
        let trace = ExecutionTrace::new("task-1".into());

        for category in [
            SignalCategory::SyntaxError,
            SignalCategory::CompileError,
            SignalCategory::TypeError,
        ] {
            let error = ErrorSignal::new(
                category,
                Some(1),
                "error msg".into(),
                "step-1".into(),
                "bash".into(),
            );
            let result = editor.patch_score(&trace, &step, &error).await.unwrap();
            assert_eq!(result.len(), 1);
            assert_eq!(result[0].description, "Run tests");
        }
    }

    #[tokio::test]
    async fn test_editor_preserves_step_fields() {
        let editor = Editor::new(make_bus());
        let step = sample_step();
        let error = ErrorSignal::new(
            SignalCategory::LogicError,
            Some(1),
            "err".into(),
            "step-1".into(),
            "bash".into(),
        );
        let trace = ExecutionTrace::new("task-1".into());
        let result = editor.patch_score(&trace, &step, &error).await.unwrap();
        let patched = &result[0];
        assert_eq!(patched.index, step.index);
        assert_eq!(patched.expected_output_type, step.expected_output_type);
        assert_eq!(patched.suggested_tool, step.suggested_tool);
        assert_eq!(patched.retry_on_failure, step.retry_on_failure);
    }
}
