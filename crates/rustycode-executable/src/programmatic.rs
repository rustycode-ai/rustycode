//! Programmatic calling support for chaining executable units

use crate::{ExecutionContext, ExecutionInput, ExecutionOutput, ExecutableError};

/// Describes a chain of unit invocations
#[derive(Clone, Debug)]
pub struct CallChain {
    pub steps: Vec<ChainStep>,
}

/// A single step in a call chain
#[derive(Clone, Debug)]
pub struct ChainStep {
    pub unit_id: String,
    pub input_transform: Option<InputTransform>,
    pub output_transform: Option<OutputTransform>,
}

/// How to transform input for a step
#[derive(Clone, Debug)]
pub enum InputTransform {
    /// Use the output of the previous step as input
    PreviousOutput,
    /// Use a fixed value
    Fixed(serde_json::Value),
    /// Merge previous output with additional data
    Merge(serde_json::Value),
}

/// How to transform output from a step
#[derive(Clone, Debug)]
pub enum OutputTransform {
    /// Extract a field from the result
    ExtractField(String),
    /// Take only the data, drop metadata
    DataOnly,
    /// Keep the full output
    Full,
}

/// Result of executing a call chain
#[derive(Clone, Debug)]
pub struct ChainResult {
    pub outputs: Vec<ExecutionOutput>,
    pub final_output: ExecutionOutput,
    pub total_duration_ms: u64,
}

impl CallChain {
    /// Create an empty chain
    pub const fn new() -> Self {
        Self { steps: vec![] }
    }

    /// Add a step to the chain
    pub fn then(mut self, unit_id: impl Into<String>) -> Self {
        self.steps.push(ChainStep {
            unit_id: unit_id.into(),
            input_transform: None,
            output_transform: None,
        });
        self
    }

    /// Add a step with input from previous output
    pub fn then_with_prev(mut self, unit_id: impl Into<String>) -> Self {
        self.steps.push(ChainStep {
            unit_id: unit_id.into(),
            input_transform: Some(InputTransform::PreviousOutput),
            output_transform: None,
        });
        self
    }

    /// Execute the chain against a router
    pub async fn execute(
        &self,
        router: &crate::ExecutionRouter,
        initial_input: ExecutionInput,
    ) -> Result<ChainResult, ExecutableError> {
        let mut outputs = Vec::new();
        let mut current_input = initial_input;
        let mut total_duration_ms = 0u64;

        for (i, step) in self.steps.iter().enumerate() {
            let input = match &step.input_transform {
                Some(InputTransform::PreviousOutput) if i > 0 => {
                    let prev: &ExecutionOutput = &outputs[i - 1];
                    ExecutionInput {
                        data: prev.data.clone(),
                        caller_info: current_input.caller_info.clone(),
                        session_context: current_input.session_context.clone(),
                    }
                }
                Some(InputTransform::Fixed(val)) => ExecutionInput {
                    data: val.clone(),
                    caller_info: current_input.caller_info.clone(),
                    session_context: current_input.session_context.clone(),
                },
                Some(InputTransform::Merge(extra)) => {
                    let mut merged = current_input.data.clone();
                    if let (serde_json::Value::Object(ref mut map), serde_json::Value::Object(ref extra_map)) =
                        (&mut merged, extra)
                    {
                        for (k, v) in extra_map {
                            map.insert(k.clone(), v.clone());
                        }
                    }
                    ExecutionInput {
                        data: merged,
                        caller_info: current_input.caller_info.clone(),
                        session_context: current_input.session_context.clone(),
                    }
                }
                _ => current_input.clone(),
            };

            let context = ExecutionContext::ProgrammaticCall {
                chain_position: Some(i as u32),
                passthrough: i < self.steps.len() - 1,
            };

            let output = router.execute(&step.unit_id, input, context).await?;
            total_duration_ms += output.metadata.duration_ms;
            current_input = ExecutionInput {
                data: output.data.clone(),
                caller_info: None,
                session_context: None,
            };
            outputs.push(output);
        }

        let final_output = outputs.last().cloned().ok_or_else(|| {
            ExecutableError::ExecutionFailed("empty call chain".to_string())
        })?;

        Ok(ChainResult {
            outputs,
            final_output,
            total_duration_ms,
        })
    }
}

impl Default for CallChain {
    fn default() -> Self {
        Self::new()
    }
}
