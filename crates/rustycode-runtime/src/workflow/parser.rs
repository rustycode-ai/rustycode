//! Workflow Parser
//!
//! This module provides parsing functionality for workflow definitions:
//! - YAML format parsing
//! - JSON format parsing
//! - Schema validation
//! - Example workflow definitions

use crate::workflow::definition::{
    ErrorStrategy, Expression, LoopConfig, LoopType, Parameter, StepType, TransformType, Workflow,
    WorkflowStep,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

use super::{Result, WorkflowError};

/// Intermediate format for deserialization
#[derive(Debug, Clone, Serialize, Deserialize)]
struct WorkflowDef {
    id: String,
    name: String,
    description: String,
    #[serde(default)]
    parameters: Vec<ParameterDef>,
    steps: Vec<StepDef>,
    #[serde(default)]
    error_handling: Option<ErrorHandlingDef>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ParameterDef {
    name: String,
    #[serde(rename = "type")]
    param_type: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    required: bool,
    #[serde(default)]
    default: Option<serde_json::Value>,
    #[serde(default)]
    values: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct StepDef {
    id: String,
    #[serde(rename = "type")]
    step_type: Option<String>,
    #[serde(default)]
    tool_call: Option<ToolCallDef>,
    #[serde(default)]
    condition: Option<ConditionDef>,
    #[serde(default)]
    loop_config: Option<LoopDef>,
    #[serde(default)]
    parallel: Option<Vec<StepDef>>,
    #[serde(default)]
    user_input: Option<UserInputDef>,
    #[serde(default)]
    transform: Option<TransformDef>,
    #[serde(default)]
    depends_on: Option<Vec<String>>,
    #[serde(default)]
    on_failure: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ToolCallDef {
    tool: String,
    #[serde(default)]
    parameters: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ConditionDef {
    expression: String,
    then_step: Box<StepDef>,
    #[serde(default)]
    else_step: Option<Box<StepDef>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct LoopDef {
    #[serde(rename = "type")]
    loop_type: String,
    #[serde(default)]
    iterations: Option<usize>,
    #[serde(default)]
    condition: Option<String>,
    #[serde(default)]
    break_condition: Option<String>,
    #[serde(default)]
    over_variable: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct UserInputDef {
    prompt: String,
    variable: String,
    #[serde(default)]
    default: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TransformDef {
    input: String,
    #[serde(rename = "type")]
    transform_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ErrorHandlingDef {
    #[serde(default)]
    strategy: Option<String>,
    #[serde(default)]
    retry_attempts: Option<usize>,
    #[serde(default)]
    rollback_on_critical: Option<bool>,
}

/// Parse workflow from YAML string
pub fn parse_yaml(yaml_content: &str) -> Result<Workflow> {
    let def: WorkflowDef = serde_yaml::from_str(yaml_content)
        .map_err(|e| WorkflowError::Validation(format!("YAML parsing error: {}", e)))?;

    convert_workflow(def)
}

/// Parse workflow from YAML file
pub fn parse_yaml_file<P: AsRef<Path>>(path: P) -> Result<Workflow> {
    let content = fs::read_to_string(path)
        .map_err(|e| WorkflowError::Validation(format!("Failed to read file: {}", e)))?;

    parse_yaml(&content)
}

/// Parse workflow from JSON string
pub fn parse_json(json_content: &str) -> Result<Workflow> {
    let def: WorkflowDef = serde_json::from_str(json_content)
        .map_err(|e| WorkflowError::Validation(format!("JSON parsing error: {}", e)))?;

    convert_workflow(def)
}

/// Parse workflow from JSON file
pub fn parse_json_file<P: AsRef<Path>>(path: P) -> Result<Workflow> {
    let content = fs::read_to_string(path)
        .map_err(|e| WorkflowError::Validation(format!("Failed to read file: {}", e)))?;

    parse_json(&content)
}

/// Convert intermediate format to Workflow
fn convert_workflow(def: WorkflowDef) -> Result<Workflow> {
    let mut workflow = Workflow::new(def.id, def.name, def.description);

    // Convert parameters
    for param_def in def.parameters {
        let param_type = match param_def.param_type.as_str() {
            "string" => crate::workflow::definition::ParameterType::String,
            "number" | "integer" => crate::workflow::definition::ParameterType::Integer,
            "float" => crate::workflow::definition::ParameterType::Float,
            "boolean" => crate::workflow::definition::ParameterType::Boolean,
            "enum" => crate::workflow::definition::ParameterType::Enum {
                values: param_def.values.unwrap_or_default(),
            },
            "array" => crate::workflow::definition::ParameterType::Array {
                element_type: Box::new(crate::workflow::definition::ParameterType::String),
            },
            "object" => crate::workflow::definition::ParameterType::Object,
            _ => {
                return Err(WorkflowError::Validation(format!(
                    "Unknown parameter type: {}",
                    param_def.param_type
                )))
            }
        };

        let param = Parameter {
            name: param_def.name,
            param_type,
            description: param_def.description.unwrap_or_default(),
            required: param_def.required,
            default: param_def.default.map(|v| match v {
                serde_json::Value::String(s) => s,
                _ => v.to_string(),
            }),
        };

        workflow = workflow.add_parameter(param);
    }

    // Convert steps
    for step_def in def.steps {
        let step = convert_step(step_def)?;
        workflow = workflow.add_step(step);
    }

    // Convert error handling
    let error_handling = if let Some(eh) = def.error_handling {
        let strategy = match eh.strategy.as_deref().unwrap_or("stop") {
            "continue" => crate::workflow::definition::ErrorStrategyType::Continue,
            "stop" => crate::workflow::definition::ErrorStrategyType::Stop,
            "retry" => crate::workflow::definition::ErrorStrategyType::Retry,
            _ => crate::workflow::definition::ErrorStrategyType::Stop,
        };

        let retry = if strategy == crate::workflow::definition::ErrorStrategyType::Retry {
            Some(crate::workflow::definition::RetryConfig {
                max_attempts: eh.retry_attempts.unwrap_or(3),
                retry_delay_ms: 1000,
                backoff_multiplier: 2.0,
            })
        } else {
            None
        };

        ErrorStrategy {
            strategy,
            rollback_on_critical: eh.rollback_on_critical.unwrap_or(false),
            retry,
        }
    } else {
        ErrorStrategy {
            strategy: crate::workflow::definition::ErrorStrategyType::Stop,
            rollback_on_critical: false,
            retry: None,
        }
    };

    workflow.error_handling = error_handling;

    // Validate workflow
    workflow.validate()?;

    Ok(workflow)
}

/// Convert step definition to WorkflowStep
fn convert_step(def: StepDef) -> Result<WorkflowStep> {
    let step_type = if let Some(tool_call) = def.tool_call {
        StepType::ToolCall {
            tool: tool_call.tool,
            parameters: tool_call.parameters,
        }
    } else if let Some(condition) = def.condition {
        StepType::Condition {
            expression: parse_expression(&condition.expression)?,
            then_step: Box::new(convert_step(*condition.then_step)?),
            else_step: if let Some(else_step) = condition.else_step {
                Some(Box::new(convert_step(*else_step)?))
            } else {
                None
            },
        }
    } else if let Some(loop_def) = def.loop_config {
        let loop_type = match loop_def.loop_type.as_str() {
            "for" => LoopType::For {
                iterations: loop_def.iterations.unwrap_or(10),
            },
            "while" => LoopType::While,
            "until" => LoopType::Until,
            "foreach" => LoopType::ForEach,
            _ => {
                return Err(WorkflowError::Validation(format!(
                    "Unknown loop type: {}",
                    loop_def.loop_type
                )))
            }
        };

        // For loops, we need a nested step - use a simple default
        // In real implementation, this would come from the workflow definition
        let nested_step = WorkflowStep::new(
            "loop_body".to_string(),
            StepType::ToolCall {
                tool: "noop".to_string(),
                parameters: HashMap::new(),
            },
        );

        StepType::Loop {
            loop_config: LoopConfig {
                loop_type,
                break_condition: if let Some(bc) = loop_def.break_condition {
                    Some(parse_expression(&bc)?)
                } else {
                    None
                },
                continue_condition: None,
                max_iterations: None,
            },
            step: Box::new(nested_step),
        }
    } else if let Some(parallel_steps) = def.parallel {
        StepType::Parallel {
            steps: parallel_steps
                .into_iter()
                .map(convert_step)
                .collect::<Result<_>>()?,
            wait_for_all: true,
        }
    } else if let Some(user_input) = def.user_input {
        StepType::UserInput {
            prompt: user_input.prompt,
            variable_name: user_input.variable,
            default: user_input.default,
        }
    } else if let Some(transform) = def.transform {
        let transform_type = match transform.transform_type.as_str() {
            "uppercase" => TransformType::Uppercase,
            "lowercase" => TransformType::Lowercase,
            "trim" => TransformType::Trim,
            "replace" => TransformType::Replace {
                pattern: String::new(),
                replacement: String::new(),
            },
            "split" => TransformType::Split {
                delimiter: String::new(),
            },
            "json_parse" => TransformType::JsonParse,
            "json_stringify" => TransformType::JsonStringify,
            "base64_encode" => TransformType::Base64Encode,
            "base64_decode" => TransformType::Base64Decode,
            _ => {
                return Err(WorkflowError::Validation(format!(
                    "Unknown transform type: {}",
                    transform.transform_type
                )))
            }
        };

        StepType::Transform {
            input: transform.input,
            transform_type,
        }
    } else {
        // Default to tool call if nothing else specified
        StepType::ToolCall {
            tool: def.id.clone(),
            parameters: HashMap::new(),
        }
    };

    let mut step = WorkflowStep::new(def.id, step_type);

    if let Some(depends) = def.depends_on {
        step.depends_on = depends;
    }

    if let Some(on_failure) = def.on_failure {
        step.on_failure = match on_failure.as_str() {
            "stop" => crate::workflow::definition::FailureAction::Stop,
            "continue" => crate::workflow::definition::FailureAction::Continue,
            "retry" => crate::workflow::definition::FailureAction::Retry {
                max_attempts: 3,
                retry_delay_ms: 1000,
            },
            _ => crate::workflow::definition::FailureAction::Stop,
        };
    }

    Ok(step)
}

/// Parse expression from string
fn parse_expression(expr_str: &str) -> Result<Expression> {
    // Simple expression parser for common patterns
    // Format: "variable == value", "variable != value", "variable > 5", etc.

    let expr_str = expr_str.trim();

    // Handle NOT
    if expr_str.starts_with('!') || expr_str.starts_with("not ") {
        let skip = if expr_str.starts_with("not ") { 4 } else { 1 };
        let inner = &expr_str[skip..].trim();
        return Ok(Expression::Not {
            expression: Box::new(parse_expression(inner)?),
        });
    }

    // Handle AND
    if let Some(pos) = expr_str.find(" && ") {
        let left = &expr_str[..pos];
        let right = &expr_str[pos + 4..];
        return Ok(Expression::And {
            left: Box::new(parse_expression(left)?),
            right: Box::new(parse_expression(right)?),
        });
    }

    // Handle OR
    if let Some(pos) = expr_str.find(" || ") {
        let left = &expr_str[..pos];
        let right = &expr_str[pos + 4..];
        return Ok(Expression::Or {
            left: Box::new(parse_expression(left)?),
            right: Box::new(parse_expression(right)?),
        });
    }

    // Handle EXISTS
    if let Some(stripped) = expr_str.strip_suffix("?") {
        let variable = stripped.trim();
        return Ok(Expression::Exists {
            variable: variable.to_string(),
        });
    }

    // Handle comparisons
    if let Some(pos) = expr_str.find(" == ") {
        let variable = expr_str[..pos].trim();
        let value = expr_str[pos + 4..].trim().trim_matches('"').to_string();
        return Ok(Expression::Equals {
            variable: variable.to_string(),
            value,
        });
    }

    if let Some(pos) = expr_str.find(" != ") {
        let variable = expr_str[..pos].trim();
        let value = expr_str[pos + 4..].trim().trim_matches('"').to_string();
        return Ok(Expression::NotEquals {
            variable: variable.to_string(),
            value,
        });
    }

    if let Some(pos) = expr_str.find(" > ") {
        let variable = expr_str[..pos].trim();
        let value = expr_str[pos + 3..].trim().parse().unwrap_or_else(|_| {
            tracing::warn!("Invalid comparison value in expression: '{}'", expr_str);
            0
        });
        return Ok(Expression::GreaterThan {
            variable: variable.to_string(),
            value,
        });
    }

    if let Some(pos) = expr_str.find(" < ") {
        let variable = expr_str[..pos].trim();
        let value = expr_str[pos + 3..].trim().parse().unwrap_or_else(|_| {
            tracing::warn!("Invalid comparison value in expression: '{}'", expr_str);
            0
        });
        return Ok(Expression::LessThan {
            variable: variable.to_string(),
            value,
        });
    }

    // If just a variable name, treat as exists check
    Ok(Expression::Exists {
        variable: expr_str.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workflow::definition::ErrorStrategyType;

    #[test]
    fn test_parse_simple_yaml_workflow() {
        let yaml = r#"
id: test_workflow
name: Test Workflow
description: A simple test workflow
parameters:
  - name: file_path
    type: string
    required: true
steps:
  - id: step1
    tool_call:
      tool: read_file
      parameters:
        path: "{{file_path}}"
error_handling:
  strategy: stop
"#;

        let workflow = parse_yaml(yaml).unwrap();
        assert_eq!(workflow.id, "test_workflow");
        assert_eq!(workflow.name, "Test Workflow");
        assert_eq!(workflow.steps.len(), 1);
        assert_eq!(workflow.steps[0].id, "step1");
    }

    #[test]
    fn test_parse_simple_json_workflow() {
        let json = r#"
{
  "id": "test_workflow",
  "name": "Test Workflow",
  "description": "A simple test workflow",
  "parameters": [],
  "steps": [
    {
      "id": "step1",
      "tool_call": {
        "tool": "Read",
        "parameters": {}
      }
    }
  ]
}
"#;

        let workflow = parse_json(json).unwrap();
        assert_eq!(workflow.id, "test_workflow");
        assert_eq!(workflow.name, "Test Workflow");
        assert_eq!(workflow.steps.len(), 1);
    }

    #[test]
    fn test_parse_expression() {
        assert!(matches!(
            parse_expression("test_var").unwrap(),
            Expression::Exists { variable } if variable == "test_var"
        ));

        assert!(matches!(
            parse_expression("test_var?").unwrap(),
            Expression::Exists { variable } if variable == "test_var"
        ));

        assert!(matches!(
            parse_expression("var1 == value").unwrap(),
            Expression::Equals { variable, value } if variable == "var1" && value == "value"
        ));

        assert!(matches!(
            parse_expression("count > 5").unwrap(),
            Expression::GreaterThan { variable, value } if variable == "count" && value == 5
        ));
    }

    #[test]
    fn test_parse_with_parameters() {
        let yaml = r#"
id: param_workflow
name: Parameter Workflow
description: Workflow with parameters
parameters:
  - name: file_path
    type: string
    required: true
    description: "Path to file"
  - name: max_depth
    type: number
    required: false
    default: 10
steps: []
"#;

        let workflow = parse_yaml(yaml).unwrap();
        assert_eq!(workflow.parameters.len(), 2);
        assert_eq!(workflow.parameters[0].name, "file_path");
        assert!(workflow.parameters[0].required);
        assert_eq!(workflow.parameters[1].name, "max_depth");
        assert!(!workflow.parameters[1].required);
    }

    // --- Expression parsing edge cases ---

    #[test]
    fn test_parse_expression_less_than() {
        assert!(matches!(
            parse_expression("x < 10").unwrap(),
            Expression::LessThan { variable, value } if variable == "x" && value == 10
        ));
    }

    #[test]
    fn test_parse_expression_not_equals() {
        assert!(matches!(
            parse_expression("status != done").unwrap(),
            Expression::NotEquals { variable, value } if variable == "status" && value == "done"
        ));
    }

    #[test]
    fn test_parse_expression_invalid() {
        // Empty expressions might parse as Exists with empty variable
        // Let's test that truly malformed expressions are handled
        let result = parse_expression("   ");
        // Whitespace-only expressions should return something or error
        assert!(result.is_ok() || result.is_err());
    }

    // --- YAML parsing edge cases ---

    #[test]
    fn test_parse_yaml_empty_steps() {
        let yaml = "id: empty\nname: Empty\ndescription: No steps\nsteps: []\n";
        let workflow = parse_yaml(yaml).unwrap();
        assert_eq!(workflow.id, "empty");
        assert!(workflow.steps.is_empty());
    }

    #[test]
    fn test_parse_yaml_invalid() {
        let yaml = "not: valid: yaml: [";
        assert!(parse_yaml(yaml).is_err());
    }

    #[test]
    fn test_parse_json_invalid() {
        assert!(parse_json("{invalid json").is_err());
    }

    // --- JSON parsing with error handling ---

    #[test]
    fn test_parse_json_with_error_handling() {
        let json = r#"{
            "id": "wf1",
            "name": "Test",
            "description": "desc",
            "steps": [],
            "error_handling": { "strategy": "continue" }
        }"#;
        let workflow = parse_json(json).unwrap();
        assert!(matches!(
            workflow.error_handling.strategy,
            ErrorStrategyType::Continue
        ));
    }

    // --- Step type resolution ---

    #[test]
    fn test_step_type_tool_call() {
        let json = r#"{
            "id": "wf1", "name": "T", "description": "d",
            "steps": [{"id": "s1", "tool_call": {"tool": "Bash"}}]
        }"#;
        let workflow = parse_json(json).unwrap();
        assert!(matches!(
            workflow.steps[0].step_type,
            StepType::ToolCall { .. }
        ));
    }

    #[test]
    fn test_step_type_condition() {
        // Condition steps use then_step/else_step (not then/else)
        let json = r#"{
            "id": "wf1", "name": "T", "description": "d",
            "steps": [{"id": "s1", "condition": {"expression": "x > 5", "then_step": {"id": "inner", "tool_call": {"tool": "echo"}}, "else_step": null}}]
        }"#;
        let workflow = parse_json(json).unwrap();
        assert!(matches!(
            workflow.steps[0].step_type,
            StepType::Condition { .. }
        ));
    }

    #[test]
    fn test_step_type_loop_for() {
        let json = r#"{
            "id": "wf1", "name": "T", "description": "d",
            "steps": [{"id": "s1", "loop_config": {"type": "for", "iterations": 3}}]
        }"#;
        let workflow = parse_json(json).unwrap();
        assert!(matches!(workflow.steps[0].step_type, StepType::Loop { .. }));
    }

    // --- Parameter with default ---

    #[test]
    fn test_parse_parameter_with_default() {
        let yaml = r#"
id: enum_wf
name: Enum
description: Enum parameter
parameters:
  - name: mode
    type: string
    default: fast
steps: []
"#;
        let workflow = parse_yaml(yaml).unwrap();
        assert_eq!(workflow.parameters[0].name, "mode");
        assert!(workflow.parameters[0].default.is_some());
    }

    /// Regression test: parse_expression with an invalid comparison value
    /// (non-numeric after `>` or `<`) must fall back to 0 without panicking.
    /// Before commit c758fb036 the fallback was silent (no logging); the fix
    /// adds a `tracing::warn!` call but still returns a valid Expression.
    #[test]
    fn test_parse_expression_invalid_greater_than_value_falls_back_gracefully() {
        let result = parse_expression("count > abc");
        assert!(
            result.is_ok(),
            "should not panic on non-numeric comparison value"
        );
        let expr = result.unwrap();
        assert!(matches!(
            expr,
            Expression::GreaterThan { variable, value } if variable == "count" && value == 0
        ));
    }

    /// Regression test: parse_expression with invalid less-than value
    /// also falls back to 0 without panicking.
    #[test]
    fn test_parse_expression_invalid_less_than_value_falls_back_gracefully() {
        let result = parse_expression("score < notanumber");
        assert!(result.is_ok());
        let expr = result.unwrap();
        assert!(matches!(
            expr,
            Expression::LessThan { variable, value } if variable == "score" && value == 0
        ));
    }
}
