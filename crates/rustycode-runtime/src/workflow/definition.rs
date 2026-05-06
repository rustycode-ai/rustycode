//! Workflow definition types

use crate::workflow::{Result, WorkflowError};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Workflow definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Workflow {
    /// Unique workflow identifier
    pub id: String,

    /// Human-readable name
    pub name: String,

    /// Workflow description
    pub description: String,

    /// Workflow parameters
    pub parameters: Vec<Parameter>,

    /// Workflow steps
    pub steps: Vec<WorkflowStep>,

    /// Error handling strategy
    pub error_handling: ErrorStrategy,
}

/// Workflow step
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorkflowStep {
    /// Step identifier (unique within workflow)
    pub id: String,

    /// Step type
    #[serde(flatten)]
    pub step_type: StepType,

    /// Dependencies (step IDs that must complete first)
    #[serde(default)]
    pub depends_on: Vec<String>,

    /// What to do on failure
    #[serde(default)]
    pub on_failure: FailureAction,
}

/// Step type
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type")]
#[non_exhaustive]
pub enum StepType {
    /// Call a tool
    ToolCall {
        /// Tool name to call
        tool: String,
        /// Tool parameters (supports variable substitution)
        parameters: HashMap<String, String>,
    },

    /// Conditional execution
    Condition {
        /// Condition expression
        expression: Expression,
        /// Step to execute if condition is true
        then_step: Box<WorkflowStep>,
        /// Step to execute if condition is false (optional)
        else_step: Option<Box<WorkflowStep>>,
    },

    /// Loop construct
    Loop {
        /// Loop configuration
        loop_config: LoopConfig,
        /// Step to execute in loop
        step: Box<WorkflowStep>,
    },

    /// Parallel execution
    Parallel {
        /// Steps to execute in parallel
        steps: Vec<WorkflowStep>,
        /// Wait for all steps to complete
        wait_for_all: bool,
    },

    /// User input prompt
    UserInput {
        /// Prompt to display to user
        prompt: String,
        /// Variable name to store result
        variable_name: String,
        /// Default value (optional)
        default: Option<String>,
    },

    /// Data transformation
    Transform {
        /// Input data (can reference step results)
        input: String,
        /// Transformation type
        transform_type: TransformType,
    },
}

/// Expression for conditional execution
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[non_exhaustive]
pub enum Expression {
    /// Equals check
    Equals { variable: String, value: String },

    /// Not equals check
    NotEquals { variable: String, value: String },

    /// Greater than
    GreaterThan { variable: String, value: i64 },

    /// Less than
    LessThan { variable: String, value: i64 },

    /// Exists check
    Exists { variable: String },

    /// AND operation
    And {
        left: Box<Expression>,
        right: Box<Expression>,
    },

    /// OR operation
    Or {
        left: Box<Expression>,
        right: Box<Expression>,
    },

    /// NOT operation
    Not { expression: Box<Expression> },
}

/// Loop configuration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LoopConfig {
    pub loop_type: LoopType,

    /// Maximum iterations (None for unlimited)
    #[serde(default)]
    pub max_iterations: Option<usize>,

    /// Break condition (optional)
    #[serde(default)]
    pub break_condition: Option<Expression>,

    /// Continue condition (optional)
    #[serde(default)]
    pub continue_condition: Option<Expression>,
}

/// Loop type
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[non_exhaustive]
pub enum LoopType {
    /// For loop (iterate N times)
    For { iterations: usize },

    /// While loop (while condition is true)
    While,

    /// Until loop (until condition becomes true)
    Until,

    /// ForEach loop (iterate over collection)
    ForEach,
}

/// Data transformation type
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[non_exhaustive]
pub enum TransformType {
    /// Parse JSON
    JsonParse,

    /// Stringify JSON
    JsonStringify,

    /// Uppercase string
    Uppercase,

    /// Lowercase string
    Lowercase,

    /// Trim whitespace
    Trim,

    /// Extract field
    ExtractField { field: String },

    /// Replace text
    Replace {
        pattern: String,
        replacement: String,
    },

    /// Split string
    Split { delimiter: String },

    /// Join array
    Join { delimiter: String },

    /// Map/Transform
    Map { transform: String },

    /// Filter array
    Filter { condition: String },

    /// Base64 encode
    Base64Encode,

    /// Base64 decode
    Base64Decode,
}

/// Failure action
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[non_exhaustive]
pub enum FailureAction {
    /// Stop workflow execution
    #[default]
    Stop,

    /// Continue to next step
    Continue,

    /// Retry step
    Retry {
        /// Maximum retry attempts
        max_attempts: usize,
        /// Delay between retries in milliseconds
        retry_delay_ms: u64,
    },

    /// Execute fallback step
    Fallback { step_id: String },
}

/// Workflow parameter
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Parameter {
    /// Parameter name
    pub name: String,

    /// Parameter type
    pub param_type: ParameterType,

    /// Human-readable description
    pub description: String,

    /// Default value (optional)
    #[serde(default)]
    pub default: Option<String>,

    /// Whether parameter is required
    #[serde(default = "default_required")]
    pub required: bool,
}

fn default_required() -> bool {
    true
}

/// Parameter type
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[non_exhaustive]
pub enum ParameterType {
    /// String type
    String,

    /// Integer type
    Integer,

    /// Float type
    Float,

    /// Boolean type
    Boolean,

    /// Array type
    Array {
        /// Array element type
        element_type: Box<ParameterType>,
    },

    /// Object type (JSON object)
    Object,

    /// Enum type
    Enum {
        /// Possible values
        values: Vec<String>,
    },
}

/// Error handling strategy
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ErrorStrategy {
    /// Overall strategy
    pub strategy: ErrorStrategyType,

    /// Retry configuration
    #[serde(default)]
    pub retry: Option<RetryConfig>,

    /// Whether to rollback on critical errors
    #[serde(default = "default_rollback")]
    pub rollback_on_critical: bool,
}

fn default_rollback() -> bool {
    false
}

/// Error strategy type
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[non_exhaustive]
pub enum ErrorStrategyType {
    /// Stop on any error
    Stop,

    /// Continue on error
    Continue,

    /// Retry failed steps
    Retry,
}

/// Retry configuration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RetryConfig {
    /// Maximum retry attempts
    pub max_attempts: usize,

    /// Delay between retries in milliseconds
    pub retry_delay_ms: u64,

    /// Exponential backoff multiplier
    #[serde(default = "default_backoff_multiplier")]
    pub backoff_multiplier: f64,
}

fn default_backoff_multiplier() -> f64 {
    2.0
}

/// Workflow state during execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowState {
    /// Current step index
    pub current_step: usize,

    /// Workflow variables
    pub variables: HashMap<String, serde_json::Value>,

    /// Step results (by step ID)
    pub step_results: HashMap<String, serde_json::Value>,

    /// Workflow status
    pub status: WorkflowStatus,

    /// Error information
    pub error: Option<String>,

    /// Execution start time
    pub start_time_ms: u64,
}

impl Default for WorkflowState {
    fn default() -> Self {
        Self {
            current_step: 0,
            variables: HashMap::new(),
            step_results: HashMap::new(),
            status: WorkflowStatus::Pending,
            error: None,
            start_time_ms: 0,
        }
    }
}

impl WorkflowState {
    /// Get a variable value by name
    pub fn variable(&self, name: &str) -> Option<&serde_json::Value> {
        self.variables.get(name)
    }

    /// Set a variable value
    pub fn set_variable(&mut self, name: String, value: serde_json::Value) {
        self.variables.insert(name, value);
    }

    /// Get a step result by step ID
    pub fn step_result(&self, step_id: &str) -> Option<&serde_json::Value> {
        self.step_results.get(step_id)
    }

    /// Set a step result
    pub fn set_step_result(&mut self, step_id: String, result: serde_json::Value) {
        self.step_results.insert(step_id, result);
    }
}

/// Workflow status
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[non_exhaustive]
pub enum WorkflowStatus {
    /// Workflow is pending execution
    Pending,

    /// Workflow is currently running
    Running,

    /// Workflow completed successfully
    Completed,

    /// Workflow failed
    Failed,

    /// Workflow was cancelled
    Cancelled,
}

impl Workflow {
    pub fn new(id: String, name: String, description: String) -> Self {
        Self {
            id,
            name,
            description,
            parameters: Vec::new(),
            steps: Vec::new(),
            error_handling: ErrorStrategy {
                strategy: ErrorStrategyType::Stop,
                retry: None,
                rollback_on_critical: false,
            },
        }
    }

    /// Add a parameter to the workflow
    pub fn add_parameter(mut self, param: Parameter) -> Self {
        self.parameters.push(param);
        self
    }

    /// Add a step to the workflow
    pub fn add_step(mut self, step: WorkflowStep) -> Self {
        self.steps.push(step);
        self
    }

    /// Validate the workflow
    pub fn validate(&self) -> Result<()> {
        // Check that step IDs are unique
        let mut ids = std::collections::HashSet::new();
        for step in &self.steps {
            if !ids.insert(&step.id) {
                return Err(WorkflowError::Validation(format!(
                    "Duplicate step ID: {}",
                    step.id
                )));
            }

            // Validate step
            step.validate(&self.parameters)?;
        }

        // Check that dependencies exist
        for step in &self.steps {
            for dep in &step.depends_on {
                if !ids.contains(dep) {
                    return Err(WorkflowError::Validation(format!(
                        "Step {} depends on non-existent step: {}",
                        step.id, dep
                    )));
                }
            }
        }

        Ok(())
    }

    /// Create initial workflow state
    pub fn create_state(
        &self,
        params: &HashMap<String, serde_json::Value>,
    ) -> Result<WorkflowState> {
        // Validate parameters
        for param in &self.parameters {
            if param.required && !params.contains_key(&param.name) {
                return Err(WorkflowError::InvalidParameters(format!(
                    "Required parameter '{}' not provided",
                    param.name
                )));
            }
        }

        // Create initial state with parameters
        let mut variables = HashMap::new();
        for param in &self.parameters {
            if let Some(value) = params.get(&param.name) {
                variables.insert(param.name.clone(), value.clone());
            } else if let Some(default_val) = &param.default {
                // Parse default value based on type
                let parsed = Self::parse_default_value(default_val, &param.param_type)?;
                variables.insert(param.name.clone(), parsed);
            }
        }

        Ok(WorkflowState {
            status: WorkflowStatus::Pending,
            variables,
            start_time_ms: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_err(|e| WorkflowError::Validation(format!("Time error: {}", e)))?
                .as_millis() as u64,
            ..Default::default()
        })
    }

    /// Parse a default value string into appropriate type
    fn parse_default_value(value: &str, param_type: &ParameterType) -> Result<serde_json::Value> {
        Ok(match param_type {
            ParameterType::String => serde_json::Value::String(value.to_string()),
            ParameterType::Integer => {
                let num = value.parse::<i64>().map_err(|_| {
                    WorkflowError::InvalidParameters(format!("Invalid integer default: {}", value))
                })?;
                serde_json::Value::Number(num.into())
            }
            ParameterType::Float => {
                let num = value.parse::<f64>().map_err(|_| {
                    WorkflowError::InvalidParameters(format!("Invalid float default: {}", value))
                })?;
                serde_json::Value::Number(serde_json::Number::from_f64(num).ok_or_else(|| {
                    WorkflowError::InvalidParameters(format!("Invalid float default: {}", value))
                })?)
            }
            ParameterType::Boolean => {
                let bool_val = value.parse::<bool>().map_err(|_| {
                    WorkflowError::InvalidParameters(format!("Invalid boolean default: {}", value))
                })?;
                serde_json::Value::Bool(bool_val)
            }
            ParameterType::Enum { values } => {
                if !values.contains(&value.to_string()) {
                    return Err(WorkflowError::InvalidParameters(format!(
                        "Invalid enum value '{}', expected one of: {:?}",
                        value, values
                    )));
                }
                serde_json::Value::String(value.to_string())
            }
            ParameterType::Array { .. } | ParameterType::Object => {
                return Err(WorkflowError::InvalidParameters(
                    "Complex types not supported as defaults".to_string(),
                ));
            }
        })
    }
}

impl WorkflowStep {
    pub fn new(id: String, step_type: StepType) -> Self {
        Self {
            id,
            step_type,
            depends_on: Vec::new(),
            on_failure: FailureAction::Stop,
        }
    }

    /// Validate the step
    pub fn validate(&self, workflow_params: &[Parameter]) -> Result<()> {
        match &self.step_type {
            StepType::ToolCall { tool, parameters } => {
                if tool.is_empty() {
                    return Err(WorkflowError::Validation(
                        "Tool name cannot be empty".to_string(),
                    ));
                }

                // Validate that parameter references exist
                for value_ref in parameters.values() {
                    if value_ref.starts_with("{{") && value_ref.ends_with("}}") {
                        let var_name = &value_ref[2..value_ref.len() - 2];
                        // Check if it's a workflow parameter
                        let is_workflow_param = workflow_params.iter().any(|p| p.name == var_name);

                        // Check if it's a step result reference
                        let is_step_ref = var_name.starts_with("step.");

                        if !is_workflow_param && !is_step_ref && !var_name.is_empty() {
                            return Err(WorkflowError::Validation(format!(
                                "Unknown variable reference: {}",
                                var_name
                            )));
                        }
                    }
                }
                Ok(())
            }
            StepType::Condition {
                expression,
                then_step,
                else_step,
            } => {
                expression.validate()?;
                then_step.validate(workflow_params)?;
                if let Some(else_step) = else_step {
                    else_step.validate(workflow_params)?;
                }
                Ok(())
            }
            StepType::Loop { loop_config, step } => {
                loop_config.validate()?;
                step.validate(workflow_params)
            }
            StepType::Parallel {
                steps,
                wait_for_all: _,
            } => {
                for step in steps {
                    step.validate(workflow_params)?;
                }
                Ok(())
            }
            StepType::UserInput {
                prompt,
                variable_name,
                ..
            } => {
                if prompt.is_empty() {
                    return Err(WorkflowError::Validation(
                        "User prompt cannot be empty".to_string(),
                    ));
                }
                if variable_name.is_empty() {
                    return Err(WorkflowError::Validation(
                        "Variable name cannot be empty".to_string(),
                    ));
                }
                Ok(())
            }
            StepType::Transform {
                input,
                transform_type: _,
            } => {
                if input.is_empty() {
                    return Err(WorkflowError::Validation(
                        "Transform input cannot be empty".to_string(),
                    ));
                }
                Ok(())
            }
        }
    }
}

impl Expression {
    /// Validate the expression
    pub fn validate(&self) -> Result<()> {
        match self {
            Expression::Equals { variable, .. } => {
                if variable.is_empty() {
                    return Err(WorkflowError::Validation(
                        "Variable name cannot be empty".to_string(),
                    ));
                }
                Ok(())
            }
            Expression::NotEquals { variable, .. } => {
                if variable.is_empty() {
                    return Err(WorkflowError::Validation(
                        "Variable name cannot be empty".to_string(),
                    ));
                }
                Ok(())
            }
            Expression::GreaterThan { variable, .. } | Expression::LessThan { variable, .. } => {
                if variable.is_empty() {
                    return Err(WorkflowError::Validation(
                        "Variable name cannot be empty".to_string(),
                    ));
                }
                Ok(())
            }
            Expression::Exists { variable } => {
                if variable.is_empty() {
                    return Err(WorkflowError::Validation(
                        "Variable name cannot be empty".to_string(),
                    ));
                }
                Ok(())
            }
            Expression::And { left, right } | Expression::Or { left, right } => {
                left.validate()?;
                right.validate()
            }
            Expression::Not { expression } => expression.validate(),
        }
    }

    /// Evaluate the expression
    pub fn evaluate(&self, state: &WorkflowState) -> Result<bool> {
        match self {
            Expression::Equals { variable, value } => {
                let var_value = state.variable(variable).ok_or_else(|| {
                    WorkflowError::Validation(format!("Variable not found: {}", variable))
                })?;

                Ok(evaluate_json_equals(var_value, value))
            }

            Expression::NotEquals { variable, value } => {
                let var_value = state.variable(variable).ok_or_else(|| {
                    WorkflowError::Validation(format!("Variable not found: {}", variable))
                })?;

                Ok(!evaluate_json_equals(var_value, value))
            }

            Expression::GreaterThan { variable, value } => {
                let var_value = state.variable(variable).ok_or_else(|| {
                    WorkflowError::Validation(format!("Variable not found: {}", variable))
                })?;

                Ok(evaluate_json_greater_than(var_value, *value)?)
            }

            Expression::LessThan { variable, value } => {
                let var_value = state.variable(variable).ok_or_else(|| {
                    WorkflowError::Validation(format!("Variable not found: {}", variable))
                })?;

                Ok(evaluate_json_less_than(var_value, *value)?)
            }

            Expression::Exists { variable } => Ok(state.variables.contains_key(variable)),

            Expression::And { left, right } => Ok(left.evaluate(state)? && right.evaluate(state)?),

            Expression::Or { left, right } => Ok(left.evaluate(state)? || right.evaluate(state)?),

            Expression::Not { expression } => Ok(!expression.evaluate(state)?),
        }
    }
}

impl LoopConfig {
    /// Validate the loop configuration
    pub fn validate(&self) -> Result<()> {
        match self.loop_type {
            LoopType::For { iterations } => {
                if iterations == 0 {
                    return Err(WorkflowError::Validation(
                        "Loop iterations must be greater than 0".to_string(),
                    ));
                }
                Ok(())
            }
            LoopType::While | LoopType::Until => {
                // Break/continue conditions validated during execution
                Ok(())
            }
            LoopType::ForEach => {
                // Validated during execution
                Ok(())
            }
        }
    }
}

/// Helper function to evaluate JSON equality
fn evaluate_json_equals(left: &serde_json::Value, right: &str) -> bool {
    // Try to parse as JSON first
    let right_value: serde_json::Value = match serde_json::from_str(right) {
        Ok(v) => v,
        Err(_) => {
            // If not valid JSON, treat as plain string
            serde_json::Value::String(right.to_string())
        }
    };

    left == &right_value
}

/// Helper function to evaluate JSON greater than
fn evaluate_json_greater_than(value: &serde_json::Value, threshold: i64) -> Result<bool> {
    match value {
        serde_json::Value::Number(n) => {
            if let Some(int_val) = n.as_i64() {
                Ok(int_val > threshold)
            } else if let Some(float_val) = n.as_f64() {
                Ok((float_val.floor() as i64) > threshold)
            } else {
                Ok(false)
            }
        }
        _ => Ok(false),
    }
}

/// Helper function to evaluate JSON less than
fn evaluate_json_less_than(value: &serde_json::Value, threshold: i64) -> Result<bool> {
    match value {
        serde_json::Value::Number(n) => {
            if let Some(int_val) = n.as_i64() {
                Ok(int_val < threshold)
            } else if let Some(float_val) = n.as_f64() {
                Ok((float_val.ceil() as i64) < threshold)
            } else {
                Ok(false)
            }
        }
        _ => Ok(false),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_workflow_creation() {
        let workflow = Workflow::new(
            "test_workflow".to_string(),
            "Test Workflow".to_string(),
            "A test workflow".to_string(),
        )
        .add_parameter(Parameter {
            name: "file_path".to_string(),
            param_type: ParameterType::String,
            description: "File to process".to_string(),
            default: Some("default.txt".to_string()),
            required: true,
        })
        .add_step(WorkflowStep {
            id: "step1".to_string(),
            step_type: StepType::ToolCall {
                tool: "read_file".to_string(),
                parameters: [("path".to_string(), "{{file_path}}".to_string())]
                    .iter()
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect(),
            },
            depends_on: vec![],
            on_failure: FailureAction::Stop,
        });

        assert_eq!(workflow.id, "test_workflow");
        assert_eq!(workflow.name, "Test Workflow");
        assert_eq!(workflow.parameters.len(), 1);
        assert_eq!(workflow.steps.len(), 1);
    }

    #[test]
    fn test_workflow_validation() {
        let workflow = Workflow::new("test".to_string(), "Test".to_string(), "Test".to_string())
            .add_step(WorkflowStep {
                id: "step1".to_string(),
                step_type: StepType::ToolCall {
                    tool: "test".to_string(),
                    parameters: HashMap::new(),
                },
                depends_on: vec![],
                on_failure: FailureAction::Stop,
            });

        assert!(workflow.validate().is_ok());
    }

    #[test]
    fn test_duplicate_step_ids() {
        let workflow = Workflow::new("test".to_string(), "Test".to_string(), "Test".to_string())
            .add_step(WorkflowStep {
                id: "step1".to_string(),
                step_type: StepType::ToolCall {
                    tool: "test".to_string(),
                    parameters: HashMap::new(),
                },
                depends_on: vec![],
                on_failure: FailureAction::Stop,
            })
            .add_step(WorkflowStep {
                id: "step1".to_string(), // Duplicate!
                step_type: StepType::ToolCall {
                    tool: "test".to_string(),
                    parameters: HashMap::new(),
                },
                depends_on: vec![],
                on_failure: FailureAction::Stop,
            });

        assert!(workflow.validate().is_err());
    }

    #[test]
    fn test_dependency_validation() {
        let workflow = Workflow::new("test".to_string(), "Test".to_string(), "Test".to_string())
            .add_step(WorkflowStep {
                id: "step1".to_string(),
                step_type: StepType::ToolCall {
                    tool: "test".to_string(),
                    parameters: HashMap::new(),
                },
                depends_on: vec![],
                on_failure: FailureAction::Stop,
            })
            .add_step(WorkflowStep {
                id: "step2".to_string(),
                step_type: StepType::ToolCall {
                    tool: "test".to_string(),
                    parameters: HashMap::new(),
                },
                depends_on: vec!["nonexistent".to_string()], // Invalid dependency
                on_failure: FailureAction::Stop,
            });

        assert!(workflow.validate().is_err());
    }

    #[test]
    fn test_expression_evaluation() {
        let mut state = WorkflowState::default();
        state
            .variables
            .insert("test_var".to_string(), serde_json::json!("test"));

        // Test equals
        let expr = Expression::Equals {
            variable: "test_var".to_string(),
            value: "test".to_string(),
        };
        assert!(expr.evaluate(&state).unwrap());

        // Test exists
        let expr = Expression::Exists {
            variable: "test_var".to_string(),
        };
        assert!(expr.evaluate(&state).unwrap());

        // Test not exists
        let expr = Expression::Exists {
            variable: "nonexistent".to_string(),
        };
        assert!(!expr.evaluate(&state).unwrap());
    }

    // --- Serde roundtrips ---

    #[test]
    fn workflow_serde_roundtrip() {
        let wf = Workflow::new("wf1".into(), "Name".into(), "Desc".into())
            .add_parameter(Parameter {
                name: "p1".into(),
                param_type: ParameterType::String,
                description: "A param".into(),
                default: Some("val".into()),
                required: true,
            })
            .add_step(WorkflowStep::new(
                "s1".into(),
                StepType::ToolCall {
                    tool: "bash".into(),
                    parameters: HashMap::new(),
                },
            ));
        let json = serde_json::to_string(&wf).unwrap();
        let decoded: Workflow = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.id, "wf1");
        assert_eq!(decoded.parameters.len(), 1);
        assert_eq!(decoded.steps.len(), 1);
    }

    #[test]
    fn step_type_serde_variants() {
        // ToolCall
        let st = StepType::ToolCall {
            tool: "read".into(),
            parameters: HashMap::new(),
        };
        let json = serde_json::to_string(&st).unwrap();
        let back: StepType = serde_json::from_str(&json).unwrap();
        assert_eq!(st, back);

        // UserInput
        let st = StepType::UserInput {
            prompt: "Name?".into(),
            variable_name: "name".into(),
            default: Some("anon".into()),
        };
        let json = serde_json::to_string(&st).unwrap();
        let back: StepType = serde_json::from_str(&json).unwrap();
        assert_eq!(st, back);

        // Transform
        let st = StepType::Transform {
            input: "{{data}}".into(),
            transform_type: TransformType::Uppercase,
        };
        let json = serde_json::to_string(&st).unwrap();
        let back: StepType = serde_json::from_str(&json).unwrap();
        assert_eq!(st, back);

        // Parallel
        let st = StepType::Parallel {
            steps: vec![],
            wait_for_all: true,
        };
        let json = serde_json::to_string(&st).unwrap();
        let back: StepType = serde_json::from_str(&json).unwrap();
        assert_eq!(st, back);
    }

    #[test]
    fn expression_serde_variants() {
        let cases: Vec<Expression> = vec![
            Expression::Equals {
                variable: "x".into(),
                value: "1".into(),
            },
            Expression::NotEquals {
                variable: "y".into(),
                value: "2".into(),
            },
            Expression::GreaterThan {
                variable: "a".into(),
                value: 10,
            },
            Expression::LessThan {
                variable: "b".into(),
                value: 5,
            },
            Expression::Exists {
                variable: "z".into(),
            },
            Expression::And {
                left: Box::new(Expression::Equals {
                    variable: "x".into(),
                    value: "1".into(),
                }),
                right: Box::new(Expression::Exists {
                    variable: "y".into(),
                }),
            },
            Expression::Or {
                left: Box::new(Expression::Exists {
                    variable: "a".into(),
                }),
                right: Box::new(Expression::Exists {
                    variable: "b".into(),
                }),
            },
            Expression::Not {
                expression: Box::new(Expression::Exists {
                    variable: "c".into(),
                }),
            },
        ];
        for expr in &cases {
            let json = serde_json::to_string(expr).unwrap();
            let back: Expression = serde_json::from_str(&json).unwrap();
            assert_eq!(expr, &back);
        }
    }

    #[test]
    fn loop_type_serde_variants() {
        for lt in &[
            LoopType::For { iterations: 5 },
            LoopType::While,
            LoopType::Until,
            LoopType::ForEach,
        ] {
            let json = serde_json::to_string(lt).unwrap();
            let back: LoopType = serde_json::from_str(&json).unwrap();
            assert_eq!(lt, &back);
        }
    }

    #[test]
    fn transform_type_serde_variants() {
        let cases: Vec<TransformType> = vec![
            TransformType::Uppercase,
            TransformType::Lowercase,
            TransformType::Trim,
            TransformType::JsonParse,
            TransformType::JsonStringify,
            TransformType::Base64Encode,
            TransformType::Base64Decode,
            TransformType::ExtractField {
                field: "name".into(),
            },
            TransformType::Replace {
                pattern: "old".into(),
                replacement: "new".into(),
            },
            TransformType::Split {
                delimiter: ",".into(),
            },
            TransformType::Join {
                delimiter: ";".into(),
            },
            TransformType::Map {
                transform: "x * 2".into(),
            },
            TransformType::Filter {
                condition: "x > 0".into(),
            },
        ];
        for tt in &cases {
            let json = serde_json::to_string(tt).unwrap();
            let back: TransformType = serde_json::from_str(&json).unwrap();
            assert_eq!(tt, &back);
        }
    }

    #[test]
    fn failure_action_serde_variants() {
        let cases: Vec<FailureAction> = vec![
            FailureAction::Stop,
            FailureAction::Continue,
            FailureAction::Retry {
                max_attempts: 3,
                retry_delay_ms: 1000,
            },
            FailureAction::Fallback {
                step_id: "s1".into(),
            },
        ];
        for fa in &cases {
            let json = serde_json::to_string(fa).unwrap();
            let back: FailureAction = serde_json::from_str(&json).unwrap();
            assert_eq!(fa, &back);
        }
    }

    #[test]
    fn failure_action_default_is_stop() {
        assert_eq!(FailureAction::default(), FailureAction::Stop);
    }

    #[test]
    fn parameter_type_serde_variants() {
        let cases: Vec<ParameterType> = vec![
            ParameterType::String,
            ParameterType::Integer,
            ParameterType::Float,
            ParameterType::Boolean,
            ParameterType::Object,
            ParameterType::Array {
                element_type: Box::new(ParameterType::String),
            },
            ParameterType::Enum {
                values: vec!["a".into(), "b".into()],
            },
        ];
        for pt in &cases {
            let json = serde_json::to_string(pt).unwrap();
            let back: ParameterType = serde_json::from_str(&json).unwrap();
            assert_eq!(pt, &back);
        }
    }

    #[test]
    fn error_strategy_serde() {
        let es = ErrorStrategy {
            strategy: ErrorStrategyType::Retry,
            retry: Some(RetryConfig {
                max_attempts: 5,
                retry_delay_ms: 2000,
                backoff_multiplier: 2.0,
            }),
            rollback_on_critical: true,
        };
        let json = serde_json::to_string(&es).unwrap();
        let back: ErrorStrategy = serde_json::from_str(&json).unwrap();
        assert_eq!(es, back);
    }

    #[test]
    fn error_strategy_type_serde() {
        for est in &[
            ErrorStrategyType::Stop,
            ErrorStrategyType::Continue,
            ErrorStrategyType::Retry,
        ] {
            let json = serde_json::to_string(est).unwrap();
            let back: ErrorStrategyType = serde_json::from_str(&json).unwrap();
            assert_eq!(est, &back);
        }
    }

    #[test]
    fn workflow_status_serde() {
        for ws in &[
            WorkflowStatus::Pending,
            WorkflowStatus::Running,
            WorkflowStatus::Completed,
            WorkflowStatus::Failed,
            WorkflowStatus::Cancelled,
        ] {
            let json = serde_json::to_string(ws).unwrap();
            let back: WorkflowStatus = serde_json::from_str(&json).unwrap();
            assert_eq!(ws, &back);
        }
    }

    #[test]
    fn workflow_state_default() {
        let state = WorkflowState::default();
        assert_eq!(state.current_step, 0);
        assert!(state.variables.is_empty());
        assert!(state.step_results.is_empty());
        assert_eq!(state.status, WorkflowStatus::Pending);
        assert!(state.error.is_none());
        assert_eq!(state.start_time_ms, 0);
    }

    // --- Expression validation ---

    #[test]
    fn expression_validate_empty_variable_fails() {
        let expr = Expression::Equals {
            variable: "".into(),
            value: "x".into(),
        };
        assert!(expr.validate().is_err());

        let expr = Expression::NotEquals {
            variable: "".into(),
            value: "x".into(),
        };
        assert!(expr.validate().is_err());

        let expr = Expression::GreaterThan {
            variable: "".into(),
            value: 1,
        };
        assert!(expr.validate().is_err());

        let expr = Expression::LessThan {
            variable: "".into(),
            value: 1,
        };
        assert!(expr.validate().is_err());

        let expr = Expression::Exists {
            variable: "".into(),
        };
        assert!(expr.validate().is_err());
    }

    #[test]
    fn expression_validate_valid_passes() {
        assert!(Expression::Equals {
            variable: "x".into(),
            value: "1".into()
        }
        .validate()
        .is_ok());
        assert!(Expression::NotEquals {
            variable: "y".into(),
            value: "2".into()
        }
        .validate()
        .is_ok());
        assert!(Expression::GreaterThan {
            variable: "a".into(),
            value: 10
        }
        .validate()
        .is_ok());
        assert!(Expression::LessThan {
            variable: "b".into(),
            value: 5
        }
        .validate()
        .is_ok());
        assert!(Expression::Exists {
            variable: "z".into()
        }
        .validate()
        .is_ok());
    }

    // --- Expression evaluation ---

    #[test]
    fn expression_not_equals_eval() {
        let mut state = WorkflowState::default();
        state
            .variables
            .insert("status".into(), serde_json::json!("done"));
        let expr = Expression::NotEquals {
            variable: "status".into(),
            value: "pending".into(),
        };
        assert!(expr.evaluate(&state).unwrap());
    }

    #[test]
    fn expression_greater_than_eval() {
        let mut state = WorkflowState::default();
        state
            .variables
            .insert("count".into(), serde_json::json!(10));
        let expr = Expression::GreaterThan {
            variable: "count".into(),
            value: 5,
        };
        assert!(expr.evaluate(&state).unwrap());

        let expr = Expression::GreaterThan {
            variable: "count".into(),
            value: 10,
        };
        assert!(!expr.evaluate(&state).unwrap());
    }

    #[test]
    fn expression_less_than_eval() {
        let mut state = WorkflowState::default();
        state.variables.insert("count".into(), serde_json::json!(3));
        let expr = Expression::LessThan {
            variable: "count".into(),
            value: 5,
        };
        assert!(expr.evaluate(&state).unwrap());
    }

    #[test]
    fn expression_and_or_not_eval() {
        let mut state = WorkflowState::default();
        state.variables.insert("a".into(), serde_json::json!(1));
        state.variables.insert("b".into(), serde_json::json!(2));

        let and_expr = Expression::And {
            left: Box::new(Expression::Exists {
                variable: "a".into(),
            }),
            right: Box::new(Expression::Exists {
                variable: "b".into(),
            }),
        };
        assert!(and_expr.evaluate(&state).unwrap());

        let or_expr = Expression::Or {
            left: Box::new(Expression::Exists {
                variable: "missing".into(),
            }),
            right: Box::new(Expression::Exists {
                variable: "a".into(),
            }),
        };
        assert!(or_expr.evaluate(&state).unwrap());

        let not_expr = Expression::Not {
            expression: Box::new(Expression::Exists {
                variable: "missing".into(),
            }),
        };
        assert!(not_expr.evaluate(&state).unwrap());
    }

    #[test]
    fn expression_missing_variable_errors() {
        let state = WorkflowState::default();
        let expr = Expression::Equals {
            variable: "missing".into(),
            value: "x".into(),
        };
        assert!(expr.evaluate(&state).is_err());
    }

    // --- Step validation ---

    #[test]
    fn step_validate_empty_tool_name_fails() {
        let step = WorkflowStep::new(
            "s1".into(),
            StepType::ToolCall {
                tool: "".into(),
                parameters: HashMap::new(),
            },
        );
        assert!(step.validate(&[]).is_err());
    }

    #[test]
    fn step_validate_empty_prompt_fails() {
        let step = WorkflowStep::new(
            "s1".into(),
            StepType::UserInput {
                prompt: "".into(),
                variable_name: "v".into(),
                default: None,
            },
        );
        assert!(step.validate(&[]).is_err());
    }

    #[test]
    fn step_validate_empty_variable_name_fails() {
        let step = WorkflowStep::new(
            "s1".into(),
            StepType::UserInput {
                prompt: "Name?".into(),
                variable_name: "".into(),
                default: None,
            },
        );
        assert!(step.validate(&[]).is_err());
    }

    #[test]
    fn step_validate_empty_transform_input_fails() {
        let step = WorkflowStep::new(
            "s1".into(),
            StepType::Transform {
                input: "".into(),
                transform_type: TransformType::Uppercase,
            },
        );
        assert!(step.validate(&[]).is_err());
    }

    #[test]
    fn step_validate_unknown_variable_ref_fails() {
        let step = WorkflowStep::new(
            "s1".into(),
            StepType::ToolCall {
                tool: "bash".into(),
                parameters: [("ref".into(), "{{unknown_var}}".into())].into(),
            },
        );
        assert!(step.validate(&[]).is_err());
    }

    #[test]
    fn step_validate_step_ref_allowed() {
        let step = WorkflowStep::new(
            "s1".into(),
            StepType::ToolCall {
                tool: "bash".into(),
                parameters: [("ref".into(), "{{step.result}}".into())].into(),
            },
        );
        assert!(step.validate(&[]).is_ok());
    }

    // --- LoopConfig validation ---

    #[test]
    fn loop_config_for_zero_iterations_fails() {
        let lc = LoopConfig {
            loop_type: LoopType::For { iterations: 0 },
            max_iterations: None,
            break_condition: None,
            continue_condition: None,
        };
        assert!(lc.validate().is_err());
    }

    #[test]
    fn loop_config_for_valid_passes() {
        let lc = LoopConfig {
            loop_type: LoopType::For { iterations: 5 },
            max_iterations: None,
            break_condition: None,
            continue_condition: None,
        };
        assert!(lc.validate().is_ok());
    }

    #[test]
    fn loop_config_while_validates() {
        let lc = LoopConfig {
            loop_type: LoopType::While,
            max_iterations: Some(100),
            break_condition: None,
            continue_condition: None,
        };
        assert!(lc.validate().is_ok());
    }

    // --- WorkflowState operations ---

    #[test]
    fn workflow_state_get_set_variable() {
        let mut state = WorkflowState::default();
        assert!(state.variable("x").is_none());
        state.set_variable("x".into(), serde_json::json!(42));
        assert_eq!(state.variable("x").unwrap(), &serde_json::json!(42));
    }

    #[test]
    fn workflow_state_get_set_step_result() {
        let mut state = WorkflowState::default();
        assert!(state.step_result("s1").is_none());
        state.set_step_result("s1".into(), serde_json::json!("ok"));
        assert_eq!(state.step_result("s1").unwrap(), &serde_json::json!("ok"));
    }

    // --- Workflow::create_state ---

    #[test]
    fn create_state_missing_required_param_fails() {
        let wf = Workflow::new("wf".into(), "W".into(), "D".into()).add_parameter(Parameter {
            name: "required_p".into(),
            param_type: ParameterType::String,
            description: "required".into(),
            default: None,
            required: true,
        });
        let result = wf.create_state(&HashMap::new());
        assert!(result.is_err());
    }

    #[test]
    fn create_state_uses_defaults() {
        let wf = Workflow::new("wf".into(), "W".into(), "D".into()).add_parameter(Parameter {
            name: "p".into(),
            param_type: ParameterType::String,
            description: "has default".into(),
            default: Some("fallback".into()),
            required: false,
        });
        let state = wf.create_state(&HashMap::new()).unwrap();
        assert_eq!(state.variable("p").unwrap(), &serde_json::json!("fallback"));
    }

    #[test]
    fn create_state_with_provided_params() {
        let wf = Workflow::new("wf".into(), "W".into(), "D".into()).add_parameter(Parameter {
            name: "p".into(),
            param_type: ParameterType::Integer,
            description: "num".into(),
            default: None,
            required: false,
        });
        let mut params = HashMap::new();
        params.insert("p".into(), serde_json::json!(42));
        let state = wf.create_state(&params).unwrap();
        assert_eq!(state.variable("p").unwrap(), &serde_json::json!(42));
    }

    // --- parse_default_value ---

    #[test]
    fn parse_default_string() {
        let val = Workflow::parse_default_value("hello", &ParameterType::String).unwrap();
        assert_eq!(val, serde_json::json!("hello"));
    }

    #[test]
    fn parse_default_integer() {
        let val = Workflow::parse_default_value("42", &ParameterType::Integer).unwrap();
        assert_eq!(val, serde_json::json!(42));
    }

    #[test]
    #[allow(clippy::approx_constant)] // 3.14 is a test float value, not PI
    fn parse_default_float() {
        let val = Workflow::parse_default_value("3.14", &ParameterType::Float).unwrap();
        assert_eq!(val, serde_json::json!(3.14));
    }

    #[test]
    fn parse_default_boolean() {
        let val = Workflow::parse_default_value("true", &ParameterType::Boolean).unwrap();
        assert_eq!(val, serde_json::json!(true));
    }

    #[test]
    fn parse_default_invalid_integer_fails() {
        assert!(Workflow::parse_default_value("not_a_number", &ParameterType::Integer).is_err());
    }

    #[test]
    fn parse_default_enum_valid() {
        let val = Workflow::parse_default_value(
            "a",
            &ParameterType::Enum {
                values: vec!["a".into(), "b".into()],
            },
        )
        .unwrap();
        assert_eq!(val, serde_json::json!("a"));
    }

    #[test]
    fn parse_default_enum_invalid_fails() {
        assert!(Workflow::parse_default_value(
            "c",
            &ParameterType::Enum {
                values: vec!["a".into(), "b".into()]
            }
        )
        .is_err());
    }

    #[test]
    fn parse_default_array_unsupported() {
        assert!(Workflow::parse_default_value(
            "[]",
            &ParameterType::Array {
                element_type: Box::new(ParameterType::String)
            }
        )
        .is_err());
    }

    // --- evaluate_json helpers ---

    #[test]
    fn evaluate_json_equals_string() {
        assert!(evaluate_json_equals(&serde_json::json!("hello"), "hello"));
        assert!(!evaluate_json_equals(&serde_json::json!("hello"), "world"));
    }

    #[test]
    fn evaluate_json_equals_number() {
        assert!(evaluate_json_equals(&serde_json::json!(42), "42"));
        assert!(!evaluate_json_equals(&serde_json::json!(42), "43"));
    }

    #[test]
    fn evaluate_json_greater_than_integer() {
        assert!(evaluate_json_greater_than(&serde_json::json!(10), 5).unwrap());
        assert!(!evaluate_json_greater_than(&serde_json::json!(5), 5).unwrap());
        assert!(!evaluate_json_greater_than(&serde_json::json!(3), 5).unwrap());
    }

    #[test]
    fn evaluate_json_less_than_integer() {
        assert!(evaluate_json_less_than(&serde_json::json!(3), 5).unwrap());
        assert!(!evaluate_json_less_than(&serde_json::json!(5), 5).unwrap());
        assert!(!evaluate_json_less_than(&serde_json::json!(10), 5).unwrap());
    }

    #[test]
    fn evaluate_json_comparisons_non_number() {
        assert!(!evaluate_json_greater_than(&serde_json::json!("string"), 5).unwrap());
        assert!(!evaluate_json_less_than(&serde_json::json!(true), 5).unwrap());
    }

    // --- RetryConfig ---

    #[test]
    fn retry_config_default_backoff() {
        assert_eq!(default_backoff_multiplier(), 2.0);
    }
}
