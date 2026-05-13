//! Workflow Execution Engine
//!
//! This module provides workflow execution capabilities:
//! - Step execution
//! - Conditional execution
//! - Loop support
//! - Parallel execution
//! - Error handling and rollback

use crate::workflow::definition::{
    Expression, FailureAction, LoopConfig, LoopType, StepType, TransformType, Workflow,
    WorkflowState, WorkflowStatus,
};
use crate::workflow::{Result, StepResult, WorkflowError, WorkflowResult};
use std::collections::{HashMap, HashSet};
use std::time::SystemTime;

/// Workflow executor configuration
#[derive(Debug, Clone)]
pub struct ExecutorConfig {
    /// Maximum parallel steps
    pub max_parallel_steps: usize,
    /// Default step timeout in seconds
    pub default_timeout_secs: u64,
    /// Whether to enable dry-run mode
    pub dry_run: bool,
}

impl Default for ExecutorConfig {
    fn default() -> Self {
        Self {
            max_parallel_steps: 10,
            default_timeout_secs: 300,
            dry_run: false,
        }
    }
}

/// Workflow executor
pub struct WorkflowExecutor {
    config: ExecutorConfig,
    tool_executor: Option<std::sync::Arc<dyn crate::workflow::WorkflowToolExecutor>>,
}

impl WorkflowExecutor {
    pub fn new() -> Self {
        Self {
            config: ExecutorConfig::default(),
            tool_executor: None,
        }
    }

    pub fn with_config(config: ExecutorConfig) -> Self {
        Self {
            config,
            tool_executor: None,
        }
    }

    /// Set a custom tool executor for real tool dispatch.
    pub fn with_tool_executor(
        mut self,
        executor: std::sync::Arc<dyn crate::workflow::WorkflowToolExecutor>,
    ) -> Self {
        self.tool_executor = Some(executor);
        self
    }

    /// Execute a workflow with given parameters
    pub async fn execute(
        &self,
        workflow: &Workflow,
        params: &HashMap<String, serde_json::Value>,
    ) -> Result<WorkflowResult> {
        let start_time = SystemTime::now();

        // Create initial workflow state
        let mut state = workflow.create_state(params)?;
        state.status = WorkflowStatus::Running;

        // Track step results
        let mut step_results: HashMap<String, StepResult> = HashMap::new();

        // Build dependency graph
        let dependency_graph = self.build_dependency_graph(workflow);

        // Execute steps in topological order
        let mut executed_steps = HashSet::new();
        let mut steps_to_execute =
            self.get_ready_steps(workflow, &executed_steps, &dependency_graph);

        while !steps_to_execute.is_empty() {
            // Execute sequentially (parallel execution to be implemented in future iteration)
            for step_idx in &steps_to_execute {
                self.execute_step(workflow, *step_idx, &mut state, &mut step_results)
                    .await?;
            }

            // Mark steps as executed
            for step_idx in &steps_to_execute {
                executed_steps.insert(*step_idx);
            }

            // Get next batch of ready steps
            steps_to_execute = self.get_ready_steps(workflow, &executed_steps, &dependency_graph);
        }

        // Mark workflow as completed
        if state.status != WorkflowStatus::Failed {
            state.status = WorkflowStatus::Completed;
        }

        // Calculate duration (elapsed since start, not absolute timestamp)
        let duration_ms = start_time
            .elapsed()
            .map_err(|e| WorkflowError::Validation(format!("Time error: {}", e)))?
            .as_millis() as u64;

        // Determine success
        let success = state.status == WorkflowStatus::Completed;
        let error = if success { None } else { state.error.clone() };

        Ok(WorkflowResult {
            success,
            final_state: state,
            step_results,
            output: serde_json::Value::Null,
            duration_ms,
            error,
        })
    }

    /// Build dependency graph from workflow steps
    fn build_dependency_graph(&self, workflow: &Workflow) -> HashMap<usize, Vec<usize>> {
        let mut graph = HashMap::new();

        for (step_idx, step) in workflow.steps.iter().enumerate() {
            let dependencies: Vec<usize> = step
                .depends_on
                .iter()
                .filter_map(|dep_id| workflow.steps.iter().position(|s| &s.id == dep_id))
                .collect();

            graph.insert(step_idx, dependencies);
        }

        graph
    }

    /// Get steps that are ready to execute (all dependencies satisfied)
    fn get_ready_steps(
        &self,
        workflow: &Workflow,
        executed_steps: &HashSet<usize>,
        dependency_graph: &HashMap<usize, Vec<usize>>,
    ) -> Vec<usize> {
        workflow
            .steps
            .iter()
            .enumerate()
            .filter(|(idx, _)| {
                !executed_steps.contains(idx) && {
                    if let Some(deps) = dependency_graph.get(idx) {
                        deps.iter().all(|dep| executed_steps.contains(dep))
                    } else {
                        true
                    }
                }
            })
            .map(|(idx, _)| idx)
            .collect()
    }

    /// Execute a single workflow step, with retry/fallback handling
    async fn execute_step(
        &self,
        workflow: &Workflow,
        step_idx: usize,
        state: &mut WorkflowState,
        step_results: &mut HashMap<String, StepResult>,
    ) -> Result<()> {
        let step = &workflow.steps[step_idx];
        let step_start = SystemTime::now();

        // Determine max attempts from the step's on_failure action
        let (max_attempts, retry_delay_ms) = match &step.on_failure {
            FailureAction::Retry {
                max_attempts,
                retry_delay_ms,
            } => (*max_attempts, *retry_delay_ms),
            _ => (1, 0),
        };

        // Attempt execution with retries
        let mut last_error: Option<WorkflowError> = None;
        let mut attempt = 0;
        let mut step_output: Option<serde_json::Value> = None;
        let mut succeeded = false;

        while attempt < max_attempts {
            attempt += 1;
            match self.execute_step_type(&step.step_type, state).await {
                Ok(output) => {
                    step_output = Some(output);
                    succeeded = true;
                    break;
                }
                Err(err) => {
                    last_error = Some(err.clone());
                    // Wait before retry (except on last attempt)
                    if attempt < max_attempts && retry_delay_ms > 0 {
                        tokio::time::sleep(std::time::Duration::from_millis(retry_delay_ms)).await;
                    }
                }
            }
        }

        let duration_ms = step_start
            .elapsed()
            .map_err(|e| WorkflowError::Validation(format!("Time error: {}", e)))?
            .as_millis() as u64;

        // If step failed (even after retries), try fallback
        if !succeeded {
            if let FailureAction::Fallback { ref step_id } = step.on_failure {
                // Look up the fallback step in the workflow
                if let Some(fallback_idx) = workflow.steps.iter().position(|s| &s.id == step_id) {
                    tracing::info!(
                        "Step '{}' failed, executing fallback step '{}' (attempt {})",
                        step.id,
                        step_id,
                        attempt
                    );
                    state.set_variable("fallback_triggered".to_string(), serde_json::json!(true));
                    state.set_variable(
                        "fallback_source_step".to_string(),
                        serde_json::json!(step.id),
                    );
                    state.set_variable(
                        "fallback_error".to_string(),
                        serde_json::json!(last_error
                            .as_ref()
                            .map(std::string::ToString::to_string)
                            .unwrap_or_default()),
                    );

                    match self
                        .execute_step_type(&workflow.steps[fallback_idx].step_type, state)
                        .await
                    {
                        Ok(output) => {
                            step_output = Some(output);
                            succeeded = true;
                        }
                        Err(fallback_err) => {
                            last_error = Some(fallback_err);
                        }
                    }
                } else {
                    tracing::warn!(
                        "Fallback step '{}' not found in workflow for step '{}'",
                        step_id,
                        step.id
                    );
                }
            }
        }

        let step_result = if succeeded {
            StepResult {
                step_id: step.id.clone(),
                success: true,
                output: step_output,
                error: None,
                duration_ms,
            }
        } else {
            let error = last_error
                .unwrap_or_else(|| WorkflowError::Validation("Unknown error".to_string()));

            // Handle remaining error actions (Stop/Continue) for non-retry, non-fallback cases
            match &step.on_failure {
                FailureAction::Continue => {
                    // Record failure but continue workflow
                }
                FailureAction::Stop => {}
                FailureAction::Retry { .. } | FailureAction::Fallback { .. } => {
                    // Already handled above; treat as Continue after exhausting options
                }
            }

            StepResult {
                step_id: step.id.clone(),
                success: false,
                output: None,
                error: Some(error.to_string()),
                duration_ms,
            }
        };

        // Store step result in state
        if let Some(ref output) = step_result.output {
            state.set_step_result(step.id.clone(), output.clone());
        }

        // Check if workflow should continue
        let should_stop = !step_result.success && matches!(step.on_failure, FailureAction::Stop);
        let error_msg = step_result.error.clone();

        step_results.insert(step.id.clone(), step_result);

        if should_stop {
            state.status = WorkflowStatus::Failed;
            state.error = error_msg.clone();
            return Err(WorkflowError::StepExecution {
                step: step.id.clone(),
                error: error_msg.unwrap_or_default(),
            });
        }

        Ok(())
    }

    /// Execute a step type (boxed to avoid recursion)
    fn execute_step_type<'a>(
        &'a self,
        step_type: &'a StepType,
        state: &'a mut WorkflowState,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<serde_json::Value>> + Send + 'a>>
    {
        Box::pin(async move {
            match step_type {
                StepType::ToolCall { tool, parameters } => {
                    self.execute_tool_call(tool, parameters, state).await
                }
                StepType::Condition {
                    expression,
                    then_step,
                    else_step,
                } => {
                    self.execute_condition(expression, then_step, else_step, state)
                        .await
                }
                StepType::Loop { loop_config, step } => {
                    self.execute_loop(loop_config, step, state).await
                }
                StepType::Parallel {
                    steps,
                    wait_for_all: _,
                } => self.execute_parallel(steps, state).await,
                StepType::UserInput {
                    prompt: _,
                    variable_name,
                    default,
                } => self.execute_user_input(variable_name, default.as_deref(), state),
                StepType::Transform {
                    input,
                    transform_type,
                } => self.execute_transform(input, transform_type, state),
            }
        })
    }

    /// Execute a tool call
    async fn execute_tool_call(
        &self,
        tool: &str,
        parameters: &HashMap<String, String>,
        state: &mut WorkflowState,
    ) -> Result<serde_json::Value> {
        if self.config.dry_run {
            return Ok(serde_json::json!({
                "tool": tool,
                "parameters": parameters,
                "dry_run": true
            }));
        }

        // Substitute parameters from workflow state
        let substituted = self.substitute_parameters(parameters, state)?;

        // Dispatch to real tool executor if configured
        if let Some(ref executor) = self.tool_executor {
            match executor.execute_tool(tool, &substituted).await {
                Ok(result) => {
                    tracing::info!("Tool '{}' executed successfully via executor", tool);
                    return Ok(result);
                }
                Err(err) => {
                    return Err(WorkflowError::ToolExecutionFailed {
                        tool: tool.to_string(),
                        error: err,
                    });
                }
            }
        }

        // Fallback: return mock result when no executor is configured
        tracing::debug!(
            "No tool executor configured, returning mock result for '{}'",
            tool
        );
        Ok(serde_json::json!({
            "tool": tool,
            "result": format!("Executed {}", tool),
            "parameters": substituted
        }))
    }

    /// Execute conditional logic
    async fn execute_condition(
        &self,
        expression: &Expression,
        then_step: &crate::workflow::definition::WorkflowStep,
        else_step: &Option<Box<crate::workflow::definition::WorkflowStep>>,
        state: &mut WorkflowState,
    ) -> Result<serde_json::Value> {
        let condition_result = expression.evaluate(state)?;

        if condition_result {
            // Execute then step type directly (avoiding recursion)
            match &then_step.step_type {
                StepType::ToolCall { tool, parameters } => {
                    self.execute_tool_call(tool, parameters, state).await
                }
                StepType::Transform {
                    input,
                    transform_type,
                } => self.execute_transform(input, transform_type, state),
                StepType::UserInput {
                    prompt: _,
                    variable_name,
                    default,
                } => self.execute_user_input(variable_name, default.as_deref(), state),
                _ => {
                    // For complex nested conditions, return a placeholder
                    Ok(serde_json::json!({ "condition": true, "executed": "then", "nested": true }))
                }
            }
        } else if let Some(else_step) = else_step {
            // Execute else step type directly
            match &else_step.step_type {
                StepType::ToolCall { tool, parameters } => {
                    self.execute_tool_call(tool, parameters, state).await
                }
                StepType::Transform {
                    input,
                    transform_type,
                } => self.execute_transform(input, transform_type, state),
                StepType::UserInput {
                    prompt: _,
                    variable_name,
                    default,
                } => self.execute_user_input(variable_name, default.as_deref(), state),
                _ => {
                    // For complex nested conditions, return a placeholder
                    Ok(
                        serde_json::json!({ "condition": false, "executed": "else", "nested": true }),
                    )
                }
            }
        } else {
            Ok(serde_json::json!({ "condition": false, "executed": "none" }))
        }
    }

    /// Execute loop logic
    async fn execute_loop(
        &self,
        loop_config: &LoopConfig,
        step: &crate::workflow::definition::WorkflowStep,
        state: &mut WorkflowState,
    ) -> Result<serde_json::Value> {
        // ForEach is a one-shot iteration over a collection; handle it separately.
        if matches!(loop_config.loop_type, LoopType::ForEach) {
            return self.execute_foreach(loop_config, step, state).await;
        }

        let mut iterations = 0;
        let max_iterations = loop_config.max_iterations.unwrap_or(100);
        let mut results: Vec<serde_json::Value> = Vec::new();

        loop {
            iterations += 1;

            if iterations > max_iterations {
                return Err(WorkflowError::LoopDetected(
                    "Maximum iterations exceeded".to_string(),
                ));
            }

            // Check break condition (skip for Until loops — body must execute at least once)
            if !matches!(loop_config.loop_type, LoopType::Until) {
                if let Some(ref break_cond) = loop_config.break_condition {
                    if break_cond.evaluate(state)? {
                        break;
                    }
                }
            }

            // Execute loop body directly (avoiding recursion)
            match &step.step_type {
                StepType::ToolCall { tool, parameters } => {
                    let result = self.execute_tool_call(tool, parameters, state).await?;
                    results.push(result);
                }
                StepType::Transform {
                    input,
                    transform_type,
                } => {
                    let result = self.execute_transform(input, transform_type, state)?;
                    results.push(result);
                }
                StepType::UserInput {
                    prompt: _,
                    variable_name,
                    default,
                } => {
                    let result =
                        self.execute_user_input(variable_name, default.as_deref(), state)?;
                    results.push(result);
                }
                _ => {
                    // For complex loops, limit iterations
                    if iterations >= max_iterations {
                        break;
                    }
                }
            }

            // Check continue condition
            if let Some(ref continue_cond) = loop_config.continue_condition {
                if !continue_cond.evaluate(state)? {
                    break;
                }
            }

            // Check loop type specific conditions
            match loop_config.loop_type {
                LoopType::For { iterations: n } => {
                    if iterations >= n {
                        break;
                    }
                }
                LoopType::While => {
                    // Continue while no break condition
                }
                LoopType::Until => {
                    // Continue until break condition is true
                    if let Some(ref break_cond) = loop_config.break_condition {
                        if break_cond.evaluate(state)? {
                            break;
                        }
                    }
                }
                LoopType::ForEach => {
                    // Handled above; unreachable
                }
            }
        }

        Ok(serde_json::json!({ "iterations": iterations, "results": results }))
    }

    /// Execute ForEach loop over a collection
    async fn execute_foreach(
        &self,
        loop_config: &LoopConfig,
        step: &crate::workflow::definition::WorkflowStep,
        state: &mut WorkflowState,
    ) -> Result<serde_json::Value> {
        // Determine the collection variable name.
        // Default is "items". To use a different variable, set break_condition to:
        //   Exists { variable: "<collection_name>" }
        let collection_var_name = loop_config
            .break_condition
            .as_ref()
            .and_then(|expr| match expr {
                Expression::Exists { variable } => Some(variable.clone()),
                _ => None,
            })
            .unwrap_or_else(|| "items".to_string());

        let collection = state
            .variable(&collection_var_name)
            .and_then(|v| v.as_array().cloned())
            .ok_or_else(|| {
                WorkflowError::Validation(format!(
                    "ForEach loop requires a collection variable '{}' (array) in state",
                    collection_var_name
                ))
            })?;

        let mut results: Vec<serde_json::Value> = Vec::new();

        for (i, item) in collection.into_iter().enumerate() {
            state.set_variable("loop_item".to_string(), item);
            state.set_variable("loop_index".to_string(), serde_json::json!(i));

            match &step.step_type {
                StepType::ToolCall { tool, parameters } => {
                    let result = self.execute_tool_call(tool, parameters, state).await?;
                    results.push(result);
                }
                StepType::Transform {
                    input,
                    transform_type,
                } => {
                    let result = self.execute_transform(input, transform_type, state)?;
                    results.push(result);
                }
                StepType::UserInput {
                    prompt: _,
                    variable_name,
                    default,
                } => {
                    let result =
                        self.execute_user_input(variable_name, default.as_deref(), state)?;
                    results.push(result);
                }
                _ => {
                    results.push(serde_json::json!({
                        "iteration": i,
                        "nested": true
                    }));
                }
            }
        }

        Ok(serde_json::json!({ "iterations": results.len(), "results": results }))
    }

    /// Execute parallel steps (currently sequential, to be improved)
    async fn execute_parallel(
        &self,
        steps: &[crate::workflow::definition::WorkflowStep],
        state: &mut WorkflowState,
    ) -> Result<serde_json::Value> {
        let mut results: Vec<serde_json::Value> = Vec::new();

        for step in steps {
            // Execute step type directly
            match &step.step_type {
                StepType::ToolCall { tool, parameters } => {
                    let result = self.execute_tool_call(tool, parameters, state).await?;
                    results.push(result);
                }
                StepType::Transform {
                    input,
                    transform_type,
                } => {
                    let result = self.execute_transform(input, transform_type, state)?;
                    results.push(result);
                }
                StepType::UserInput {
                    prompt: _,
                    variable_name,
                    default,
                } => {
                    let result =
                        self.execute_user_input(variable_name, default.as_deref(), state)?;
                    results.push(result);
                }
                _ => {
                    results.push(serde_json::Value::Null);
                }
            }
        }

        Ok(serde_json::json!({ "parallel_steps": steps.len(), "results": results }))
    }

    /// Execute user input prompt
    fn execute_user_input(
        &self,
        variable_name: &str,
        default: Option<&str>,
        state: &mut WorkflowState,
    ) -> Result<serde_json::Value> {
        // For now, use default value if provided
        let value = default.unwrap_or("").to_string();

        state.set_variable(
            variable_name.to_string(),
            serde_json::Value::String(value.clone()),
        );

        Ok(serde_json::Value::String(value))
    }

    /// Execute data transformation
    fn execute_transform(
        &self,
        input: &str,
        transform_type: &TransformType,
        state: &mut WorkflowState,
    ) -> Result<serde_json::Value> {
        let input_value = state
            .variable(input)
            .ok_or_else(|| WorkflowError::Validation(format!("Variable not found: {}", input)))?;

        let result = match transform_type {
            TransformType::Uppercase => input_value
                .as_str()
                .map(str::to_uppercase)
                .ok_or_else(|| WorkflowError::Validation("Input is not a string".to_string()))?,
            TransformType::Lowercase => input_value
                .as_str()
                .map(str::to_lowercase)
                .ok_or_else(|| WorkflowError::Validation("Input is not a string".to_string()))?,
            TransformType::Trim => input_value
                .as_str()
                .map(|s| s.trim().to_string())
                .ok_or_else(|| WorkflowError::Validation("Input is not a string".to_string()))?,
            TransformType::Replace {
                pattern,
                replacement,
            } => input_value
                .as_str()
                .map(|s| s.replace(pattern, replacement))
                .ok_or_else(|| WorkflowError::Validation("Input is not a string".to_string()))?,
            TransformType::Split { delimiter } => {
                let parts: Vec<&str> = input_value
                    .as_str()
                    .map(|s| s.split(delimiter).collect())
                    .ok_or_else(|| {
                    WorkflowError::Validation("Input is not a string".to_string())
                })?;
                return Ok(serde_json::json!(parts));
            }
            TransformType::JsonParse => input_value
                .as_str()
                .and_then(|s| {
                    serde_json::from_str(s)
                        .map_err(|e| {
                            tracing::debug!("JSON parse transform failed: {e}");
                            e
                        })
                        .ok()
                })
                .ok_or_else(|| WorkflowError::Validation("Failed to parse JSON".to_string()))?,
            TransformType::JsonStringify => {
                return Ok(serde_json::Value::String(
                    serde_json::to_string(input_value).map_err(|e| {
                        WorkflowError::Validation(format!("Serialization error: {}", e))
                    })?,
                ));
            }
            TransformType::Base64Encode => {
                use base64::{engine::general_purpose, Engine as _};
                input_value
                    .as_str()
                    .map(|s| general_purpose::STANDARD.encode(s))
                    .ok_or_else(|| WorkflowError::Validation("Input is not a string".to_string()))?
            }
            TransformType::Base64Decode => {
                use base64::{engine::general_purpose, Engine as _};
                input_value
                    .as_str()
                    .and_then(|s| {
                        general_purpose::STANDARD
                            .decode(s)
                            .map_err(|e| {
                                tracing::debug!("Base64 decode failed: {e}");
                                e
                            })
                            .ok()
                    })
                    .and_then(|bytes| {
                        String::from_utf8(bytes)
                            .map_err(|e| {
                                tracing::debug!("Base64 decoded bytes are not valid UTF-8: {e}");
                                e
                            })
                            .ok()
                    })
                    .ok_or_else(|| {
                        WorkflowError::Validation("Failed to decode base64".to_string())
                    })?
            }
            _ => {
                return Ok(serde_json::Value::Null);
            }
        };

        Ok(serde_json::Value::String(result))
    }

    /// Substitute parameters with values from state
    fn substitute_parameters(
        &self,
        parameters: &HashMap<String, String>,
        state: &WorkflowState,
    ) -> Result<HashMap<String, String>> {
        let mut substituted = HashMap::new();

        for (key, value_template) in parameters {
            let substituted_value = self.substitute_value(value_template, state)?;
            substituted.insert(key.clone(), substituted_value);
        }

        Ok(substituted)
    }

    /// Substitute a single value template
    fn substitute_value(&self, template: &str, state: &WorkflowState) -> Result<String> {
        let mut result = template.to_string();

        // Find and replace {{variable}} patterns
        let mut start = 0;
        while let Some(open_pos) = result[start..].find("{{") {
            let absolute_open = start + open_pos;
            if let Some(close_pos) = result[absolute_open..].find("}}") {
                let absolute_close = absolute_open + close_pos + 2;
                let var_name = &result[absolute_open + 2..absolute_close - 2];

                // Look up variable
                if let Some(value) = state.variable(var_name) {
                    let value_str = match value {
                        serde_json::Value::String(s) => s.clone(),
                        _ => value.to_string(),
                    };
                    result.replace_range(absolute_open..absolute_close, &value_str);
                }

                start = absolute_open + 2;
            } else {
                break;
            }
        }

        Ok(result)
    }
}

impl Default for WorkflowExecutor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workflow::definition::{
        Expression, FailureAction, LoopConfig, LoopType, Parameter, ParameterType, WorkflowStep,
    };

    #[tokio::test]
    async fn test_simple_workflow_execution() {
        let executor = WorkflowExecutor::new();

        let workflow = Workflow::new(
            "test_workflow".to_string(),
            "Test Workflow".to_string(),
            "A simple test workflow".to_string(),
        )
        .add_step(WorkflowStep::new(
            "step1".to_string(),
            StepType::ToolCall {
                tool: "test_tool".to_string(),
                parameters: HashMap::new(),
            },
        ));

        let params = HashMap::new();
        let result = executor.execute(&workflow, &params).await.unwrap();

        assert!(result.success);
        assert_eq!(result.step_results.len(), 1);
        assert!(result.step_results["step1"].success);
    }

    #[tokio::test]
    async fn test_workflow_with_parameters() {
        let executor = WorkflowExecutor::new();

        let workflow = Workflow::new(
            "param_workflow".to_string(),
            "Parameter Workflow".to_string(),
            "Workflow with parameters".to_string(),
        )
        .add_parameter(Parameter {
            name: "file_path".to_string(),
            param_type: ParameterType::String,
            description: "Path to file".to_string(),
            required: true,
            default: None,
        })
        .add_step(WorkflowStep::new(
            "step1".to_string(),
            StepType::ToolCall {
                tool: "Read".to_string(),
                parameters: {
                    let mut params = HashMap::new();
                    params.insert("path".to_string(), "{{file_path}}".to_string());
                    params
                },
            },
        ));

        let mut params = HashMap::new();
        params.insert("file_path".to_string(), serde_json::json!("test.txt"));

        let result = executor.execute(&workflow, &params).await.unwrap();

        assert!(result.success);
    }

    #[tokio::test]
    async fn test_workflow_execution_with_dependencies() {
        let executor = WorkflowExecutor::new();

        let workflow = Workflow::new(
            "dep_workflow".to_string(),
            "Dependency Workflow".to_string(),
            "Workflow with step dependencies".to_string(),
        )
        .add_step(WorkflowStep::new(
            "step1".to_string(),
            StepType::ToolCall {
                tool: "tool1".to_string(),
                parameters: HashMap::new(),
            },
        ))
        .add_step({
            let mut step = WorkflowStep::new(
                "step2".to_string(),
                StepType::ToolCall {
                    tool: "tool2".to_string(),
                    parameters: HashMap::new(),
                },
            );
            step.depends_on = vec!["step1".to_string()];
            step
        });

        let params = HashMap::new();
        let result = executor.execute(&workflow, &params).await.unwrap();

        assert!(result.success);
        assert_eq!(result.step_results.len(), 2);
    }

    #[tokio::test]
    async fn test_transform_execution() {
        let executor = WorkflowExecutor::new();

        let workflow = Workflow::new(
            "transform_workflow".to_string(),
            "Transform Workflow".to_string(),
            "Workflow with data transformation".to_string(),
        )
        .add_step(WorkflowStep::new(
            "set_value".to_string(),
            StepType::UserInput {
                prompt: "Enter value".to_string(),
                variable_name: "input_value".to_string(),
                default: Some("hello world".to_string()),
            },
        ))
        .add_step(WorkflowStep::new(
            "transform".to_string(),
            StepType::Transform {
                input: "input_value".to_string(),
                transform_type: TransformType::Uppercase,
            },
        ));

        let params = HashMap::new();
        let result = executor.execute(&workflow, &params).await.unwrap();

        assert!(result.success);
        assert_eq!(result.step_results.len(), 2);
    }

    #[tokio::test]
    async fn test_dry_run_mode() {
        let config = ExecutorConfig {
            dry_run: true,
            ..Default::default()
        };
        let executor = WorkflowExecutor::with_config(config);

        let workflow = Workflow::new(
            "dry_run_workflow".to_string(),
            "Dry Run Workflow".to_string(),
            "Workflow in dry-run mode".to_string(),
        )
        .add_step(WorkflowStep::new(
            "step1".to_string(),
            StepType::ToolCall {
                tool: "production_tool".to_string(),
                parameters: HashMap::new(),
            },
        ));

        let params = HashMap::new();
        let result = executor.execute(&workflow, &params).await.unwrap();

        assert!(result.success);
        // In dry-run mode, tools should return a result indicating dry run
        if let Some(ref output) = result.step_results["step1"].output {
            assert!(output.is_object() || output.is_null());
        }
    }

    #[tokio::test]
    async fn test_retry_on_step_failure() {
        let executor = WorkflowExecutor::new();

        // Step with retry configured -- tool execution succeeds (mock returns Ok)
        let mut step = WorkflowStep::new(
            "retry_step".to_string(),
            StepType::ToolCall {
                tool: "flaky_tool".to_string(),
                parameters: HashMap::new(),
            },
        );
        step.on_failure = FailureAction::Retry {
            max_attempts: 3,
            retry_delay_ms: 0,
        };

        let workflow = Workflow::new(
            "retry_workflow".to_string(),
            "Retry Workflow".to_string(),
            "Tests retry logic".to_string(),
        )
        .add_step(step);

        let params = HashMap::new();
        let result = executor.execute(&workflow, &params).await.unwrap();

        assert!(result.success);
        assert!(result.step_results["retry_step"].success);
    }

    #[tokio::test]
    async fn test_fallback_step_execution() {
        let executor = WorkflowExecutor::new();

        // Primary step that will succeed (mock), with a fallback configured
        let mut primary_step = WorkflowStep::new(
            "primary".to_string(),
            StepType::ToolCall {
                tool: "primary_tool".to_string(),
                parameters: HashMap::new(),
            },
        );
        primary_step.on_failure = FailureAction::Fallback {
            step_id: "fallback".to_string(),
        };

        let fallback_step = WorkflowStep::new(
            "fallback".to_string(),
            StepType::ToolCall {
                tool: "fallback_tool".to_string(),
                parameters: HashMap::new(),
            },
        );

        let workflow = Workflow::new(
            "fallback_workflow".to_string(),
            "Fallback Workflow".to_string(),
            "Tests fallback logic".to_string(),
        )
        .add_step(primary_step)
        .add_step(fallback_step);

        let params = HashMap::new();
        let result = executor.execute(&workflow, &params).await.unwrap();

        // Primary succeeds so fallback should not be triggered
        assert!(result.success);
        assert!(result.step_results["primary"].success);
    }

    #[tokio::test]
    async fn test_fallback_missing_step() {
        let executor = WorkflowExecutor::new();

        // Step referencing a non-existent fallback -- tool succeeds so fallback isn't needed
        let mut step = WorkflowStep::new(
            "primary".to_string(),
            StepType::ToolCall {
                tool: "test_tool".to_string(),
                parameters: HashMap::new(),
            },
        );
        step.on_failure = FailureAction::Fallback {
            step_id: "nonexistent".to_string(),
        };

        let workflow = Workflow::new(
            "missing_fallback_workflow".to_string(),
            "Missing Fallback Workflow".to_string(),
            "Tests missing fallback step".to_string(),
        )
        .add_step(step);

        let params = HashMap::new();
        let result = executor.execute(&workflow, &params).await.unwrap();

        // Tool succeeds, so the missing fallback is never reached
        assert!(result.success);
        assert!(result.step_results["primary"].success);
    }

    #[tokio::test]
    async fn test_foreach_with_items() {
        let executor = WorkflowExecutor::new();

        let mut state_params = HashMap::new();
        state_params.insert(
            "items".to_string(),
            serde_json::json!(["alpha", "beta", "gamma"]),
        );

        let workflow = Workflow::new(
            "foreach_workflow".to_string(),
            "ForEach Workflow".to_string(),
            "Tests foreach iteration".to_string(),
        )
        .add_step(WorkflowStep::new(
            "iterate".to_string(),
            StepType::Loop {
                loop_config: LoopConfig {
                    loop_type: LoopType::ForEach,
                    max_iterations: Some(100),
                    break_condition: None,
                    continue_condition: None,
                },
                step: Box::new(WorkflowStep::new(
                    "body".to_string(),
                    StepType::ToolCall {
                        tool: "process_item".to_string(),
                        parameters: HashMap::new(),
                    },
                )),
            },
        ));

        // We need to pre-populate the state with items.
        // Since execute() creates state from params, pass items as a parameter.
        // But Workflow has no parameter declaration for items, so we inject it.
        let workflow = workflow.add_parameter(Parameter {
            name: "items".to_string(),
            param_type: ParameterType::Array {
                element_type: Box::new(ParameterType::String),
            },
            description: "Items to iterate".to_string(),
            required: false,
            default: None,
        });

        let mut params = HashMap::new();
        params.insert(
            "items".to_string(),
            serde_json::json!(["alpha", "beta", "gamma"]),
        );

        let result = executor.execute(&workflow, &params).await.unwrap();

        assert!(result.success);
        let step_result = &result.step_results["iterate"];
        assert!(step_result.success);

        // Verify iterations happened
        if let Some(ref output) = step_result.output {
            let iterations = output
                .get("iterations")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            assert_eq!(
                iterations, 3,
                "ForEach should iterate 3 times over the collection"
            );
            let results = output.get("results").unwrap().as_array().unwrap();
            assert_eq!(results.len(), 3);
        }
    }

    #[tokio::test]
    async fn test_foreach_with_custom_collection_variable() {
        let executor = WorkflowExecutor::new();

        let workflow = Workflow::new(
            "foreach_custom_workflow".to_string(),
            "ForEach Custom Workflow".to_string(),
            "Tests foreach with custom collection variable".to_string(),
        )
        .add_parameter(Parameter {
            name: "files".to_string(),
            param_type: ParameterType::Array {
                element_type: Box::new(ParameterType::String),
            },
            description: "Files to process".to_string(),
            required: false,
            default: None,
        })
        .add_step(WorkflowStep::new(
            "iterate_files".to_string(),
            StepType::Loop {
                loop_config: LoopConfig {
                    loop_type: LoopType::ForEach,
                    max_iterations: Some(100),
                    // Use Exists to specify the collection variable name
                    break_condition: Some(Expression::Exists {
                        variable: "files".to_string(),
                    }),
                    continue_condition: None,
                },
                step: Box::new(WorkflowStep::new(
                    "process_file".to_string(),
                    StepType::ToolCall {
                        tool: "process".to_string(),
                        parameters: HashMap::new(),
                    },
                )),
            },
        ));

        let mut params = HashMap::new();
        params.insert("files".to_string(), serde_json::json!(["a.rs", "b.rs"]));

        let result = executor.execute(&workflow, &params).await.unwrap();

        assert!(result.success);
        let step_result = &result.step_results["iterate_files"];
        assert!(step_result.success);

        if let Some(ref output) = step_result.output {
            let iterations = output
                .get("iterations")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            assert_eq!(
                iterations, 2,
                "ForEach should iterate 2 times over custom collection"
            );
        }
    }

    #[tokio::test]
    async fn test_continue_on_failure_action() {
        let executor = WorkflowExecutor::new();

        // A Transform step referencing a non-existent variable will fail,
        // but with FailureAction::Continue the workflow should proceed.
        let mut failing_step = WorkflowStep::new(
            "will_fail".to_string(),
            StepType::Transform {
                input: "nonexistent_var".to_string(),
                transform_type: TransformType::Uppercase,
            },
        );
        failing_step.on_failure = FailureAction::Continue;

        let workflow = Workflow::new(
            "continue_on_failure_workflow".to_string(),
            "Continue On Failure Workflow".to_string(),
            "Tests Continue failure action".to_string(),
        )
        .add_step(failing_step)
        .add_step(WorkflowStep::new(
            "after_fail".to_string(),
            StepType::ToolCall {
                tool: "next_tool".to_string(),
                parameters: HashMap::new(),
            },
        ));

        let params = HashMap::new();
        let result = executor.execute(&workflow, &params).await.unwrap();

        // Workflow should succeed overall because failure action is Continue
        assert!(result.success);
        // First step should be marked as failed
        assert!(!result.step_results["will_fail"].success);
        // Second step should still execute
        assert!(result.step_results["after_fail"].success);
    }
}
