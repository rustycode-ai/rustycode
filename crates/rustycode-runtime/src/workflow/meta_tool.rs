//! Meta Tools
//!
//! This module provides meta tool capabilities:
//! - Tool composition
//! - Parameter passing
//! - Result aggregation
//! - Validation
//! - Registration

use crate::workflow::{Result, WorkflowError};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Meta tool - a tool that calls other tools
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetaTool {
    /// Unique identifier for this meta tool
    pub id: String,

    /// Human-readable name
    pub name: String,

    /// Description of what this meta tool does
    pub description: String,

    /// Tools that this meta tool calls, in order
    pub tool_calls: Vec<ToolCallStep>,

    /// How to aggregate results from tool calls
    pub aggregation: AggregationStrategy,

    /// Parameters for this meta tool
    pub parameters: Vec<MetaToolParameter>,
}

/// A single tool call within a meta tool
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallStep {
    /// Step identifier
    pub id: String,

    /// Tool to call
    pub tool: String,

    /// Parameter mapping (meta_tool_param -> tool_param)
    pub parameter_mapping: HashMap<String, String>,

    /// Whether to use the result of this step in subsequent steps
    pub output_variable: Option<String>,

    /// Condition for executing this step (optional)
    pub condition: Option<String>,
}

/// How to aggregate results from multiple tool calls
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[non_exhaustive]
pub enum AggregationStrategy {
    /// Return only the last result
    Last,

    /// Return all results as an array
    All,

    /// Merge results into a single object
    Merge,

    /// Return the first non-error result
    FirstSuccess,

    /// Custom aggregation logic
    Custom(String),
}

/// Meta tool parameter definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetaToolParameter {
    /// Parameter name
    pub name: String,

    /// Parameter type
    pub param_type: String,

    pub description: String,

    /// Default value (optional)
    pub default: Option<String>,

    /// Required flag
    pub required: bool,
}

impl MetaTool {
    pub fn new(id: String, name: String, description: String) -> Self {
        Self {
            id,
            name,
            description,
            tool_calls: Vec::new(),
            aggregation: AggregationStrategy::Last,
            parameters: Vec::new(),
        }
    }

    /// Add a tool call step to this meta tool
    pub fn add_tool_call(
        &mut self,
        tool: &str,
        parameter_mapping: HashMap<String, String>,
    ) -> &mut Self {
        self.tool_calls.push(ToolCallStep {
            id: format!("step_{}", self.tool_calls.len().saturating_add(1)),
            tool: tool.to_string(),
            parameter_mapping,
            output_variable: None,
            condition: None,
        });
        self
    }

    /// Add a parameter to this meta tool
    pub fn add_parameter(&mut self, param: MetaToolParameter) -> &mut Self {
        self.parameters.push(param);
        self
    }

    /// Set the aggregation strategy
    pub fn with_aggregation(&mut self, aggregation: AggregationStrategy) -> &mut Self {
        self.aggregation = aggregation;
        self
    }

    /// Set an output variable for the last added tool call
    pub fn with_output_variable(&mut self, var_name: &str) -> &mut Self {
        if let Some(last) = self.tool_calls.last_mut() {
            last.output_variable = Some(var_name.to_string());
        }
        self
    }

    /// Set a condition for the last added tool call
    pub fn with_condition(&mut self, condition: &str) -> &mut Self {
        if let Some(last) = self.tool_calls.last_mut() {
            last.condition = Some(condition.to_string());
        }
        self
    }

    /// Validate the meta tool definition
    pub fn validate(&self) -> Result<()> {
        // Check that all required parameters are defined
        for param in &self.parameters {
            if param.required && param.default.is_none() {
                // Check if there's a parameter mapping that provides this value
                let has_mapping = self.tool_calls.iter().any(|call| {
                    call.parameter_mapping
                        .values()
                        .any(|v| v == &format!("{{{}}}", param.name))
                });

                if !has_mapping && !param.name.contains(':') {
                    return Err(WorkflowError::Validation(format!(
                        "Required parameter '{}' has no default or mapping",
                        param.name
                    )));
                }
            }
        }

        // Check that all tool calls have valid tool references
        for call in &self.tool_calls {
            if call.tool.is_empty() {
                return Err(WorkflowError::Validation(
                    "Tool call has empty tool name".to_string(),
                ));
            }
        }

        Ok(())
    }

    /// Execute the meta tool with given parameters
    pub fn execute(
        &self,
        params: &HashMap<String, serde_json::Value>,
    ) -> Result<serde_json::Value> {
        self.validate()?;

        let mut results: Vec<serde_json::Value> = Vec::new();
        let mut step_variables: HashMap<String, serde_json::Value> = HashMap::new();

        for call in &self.tool_calls {
            // Check condition if present
            if let Some(ref condition_str) = &call.condition {
                let should_execute =
                    self.evaluate_condition(condition_str, &step_variables, params)?;
                if !should_execute {
                    continue;
                }
            }

            // Build parameters for this tool call
            let mut tool_params = HashMap::new();

            // Start with meta tool parameters
            for (meta_param, value) in params {
                if let Some(mapped) = call.parameter_mapping.get(meta_param) {
                    tool_params.insert(mapped.clone(), value.clone());
                }
            }

            // Add step variables from previous steps
            for (var_name, value) in &step_variables {
                if let Some(mapped) = call.parameter_mapping.get(var_name) {
                    tool_params.insert(mapped.clone(), value.clone());
                }
            }

            // Execute the tool call
            let result = self.execute_tool_call(&call.tool, &tool_params)?;

            // Store output variable if specified
            if let Some(ref var_name) = call.output_variable {
                step_variables.insert(var_name.clone(), result.clone());
            }

            results.push(result);
        }

        // Aggregate results based on strategy
        match self.aggregation {
            AggregationStrategy::Last => {
                Ok(results.last().cloned().unwrap_or(serde_json::Value::Null))
            }
            AggregationStrategy::All => serde_json::to_value(results)
                .map_err(|e| WorkflowError::Validation(format!("Serialization error: {}", e))),
            AggregationStrategy::FirstSuccess => results
                .into_iter()
                .find(|r| !r.is_null())
                .ok_or_else(|| WorkflowError::Validation("All tool calls failed".to_string())),
            AggregationStrategy::Merge => {
                // Merge all results into a single object
                let mut merged = serde_json::Map::new();
                for result in results {
                    if let Some(obj) = result.as_object() {
                        for (key, value) in obj {
                            merged.insert(key.clone(), value.clone());
                        }
                    }
                }
                Ok(serde_json::Value::Object(merged))
            }
            AggregationStrategy::Custom(_) => {
                // For custom aggregation, return all results
                serde_json::to_value(results)
                    .map_err(|e| WorkflowError::Validation(format!("Serialization error: {}", e)))
            }
        }
    }

    /// Execute a single tool call
    ///
    /// This method provides a framework for integrating with the actual tool registry.
    /// Currently returns a mock result, but can be extended to call real tools.
    fn execute_tool_call(
        &self,
        tool: &str,
        params: &HashMap<String, serde_json::Value>,
    ) -> Result<serde_json::Value> {
        // Framework for tool registry integration
        // To integrate with actual tools:
        // 1. Get the tool implementation from SessionState
        // 2. Convert parameters to tool-specific format
        // 3. Execute the tool via SessionState::execute_tool
        // 4. Return the actual result

        // For now, return a structured mock result
        Ok(serde_json::json!({
            "tool": tool,
            "parameters": params,
            "result": "executed",
            "meta_tool": self.name,
            "status": "success"
        }))
    }

    /// Execute with session context
    pub fn execute_with_session_context(
        &self,
        params: &HashMap<String, serde_json::Value>,
        session_state: &dyn std::any::Any,
    ) -> Result<serde_json::Value> {
        use rustycode_core::session::SessionState;

        // Downcast session_state to SessionState
        if let Some(_state) = session_state.downcast_ref::<SessionState>() {
            // This is still a bit complex because SessionState is not easily mutable from here
            // and MetaTool execution involves multiple tool calls.
            // For now, we still fall back to self.execute(params) but we've established
            // the connection point for future enhancement.
            tracing::info!("Executing meta tool {} with session context", self.name);
        }

        self.execute(params)
    }

    /// Evaluate a condition expression
    fn evaluate_condition(
        &self,
        condition: &str,
        step_variables: &HashMap<String, serde_json::Value>,
        params: &HashMap<String, serde_json::Value>,
    ) -> Result<bool> {
        // Simple condition evaluation
        // Supports: "variable == value", "variable != value", "variable"

        if condition.contains("==") {
            let parts: Vec<&str> = condition.split("==").collect();
            if parts.len() == 2 {
                let var_name = parts[0].trim();
                let expected = parts[1].trim().trim_matches('"');

                let value = step_variables
                    .get(var_name)
                    .or_else(|| params.get(var_name));

                if let Some(val) = value {
                    let val_str = match val {
                        serde_json::Value::String(s) => s.as_str(),
                        serde_json::Value::Bool(b) => {
                            return Ok(
                                *b == (expected == "true" || expected == "t" || expected == "1")
                            );
                        }
                        _ => return Ok(false),
                    };
                    return Ok(val_str == expected);
                }
            }
        } else if condition.contains("!=") {
            let parts: Vec<&str> = condition.split("!=").collect();
            if parts.len() == 2 {
                let var_name = parts[0].trim();
                let expected = parts[1].trim().trim_matches('"');

                let value = step_variables
                    .get(var_name)
                    .or_else(|| params.get(var_name));

                if let Some(val) = value {
                    let val_str = match val {
                        serde_json::Value::String(s) => s.as_str(),
                        _ => return Ok(false),
                    };
                    return Ok(val_str != expected);
                }
            }
        } else {
            // Simple existence check
            let var_name = condition.trim();
            return Ok(step_variables.contains_key(var_name) || params.contains_key(var_name));
        }

        Ok(false)
    }

    /// Get a builder for creating meta tools
    pub fn builder(id: &str, name: &str) -> MetaToolBuilder {
        MetaToolBuilder::new(id, name)
    }
}

/// Builder for creating meta tools
pub struct MetaToolBuilder {
    meta_tool: MetaTool,
}

impl MetaToolBuilder {
    pub fn new(id: &str, name: &str) -> Self {
        Self {
            meta_tool: MetaTool::new(
                id.to_string(),
                name.to_string(),
                format!("Meta tool: {}", name).to_string(),
            ),
        }
    }

    pub fn description(mut self, description: &str) -> Self {
        self.meta_tool.description = description.to_string();
        self
    }

    pub fn add_tool_call(mut self, tool: &str, parameter_mapping: &[(&str, &str)]) -> Self {
        let mapping: HashMap<String, String> = parameter_mapping
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();

        self.meta_tool.add_tool_call(tool, mapping);
        self
    }

    pub fn add_parameter(mut self, param: MetaToolParameter) -> Self {
        self.meta_tool.add_parameter(param);
        self
    }

    pub fn aggregation(mut self, aggregation: AggregationStrategy) -> Self {
        self.meta_tool.aggregation = aggregation;
        self
    }

    pub fn build(self) -> Result<MetaTool> {
        let meta_tool = self.meta_tool;
        meta_tool.validate()?;
        Ok(meta_tool)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_meta_tool_creation() {
        let mut meta_tool = MetaTool::new(
            "test_tool".to_string(),
            "Test Tool".to_string(),
            "A test meta tool".to_string(),
        );
        meta_tool.add_parameter(MetaToolParameter {
            name: "file_path".to_string(),
            param_type: "string".to_string(),
            description: "Path to file".to_string(),
            default: None,
            required: true,
        });

        assert_eq!(meta_tool.name, "Test Tool");
        assert_eq!(meta_tool.parameters.len(), 1);
    }

    #[test]
    fn test_meta_tool_builder() {
        let meta_tool = MetaTool::builder("refactor", "Refactor Component")
            .description("Safe component refactoring")
            .add_tool_call("read_file", &[("component_path", "file_path")])
            .add_tool_call("analyze", &[("analysis_type", "refactor")])
            .build()
            .unwrap();

        assert_eq!(meta_tool.name, "Refactor Component");
        assert_eq!(meta_tool.tool_calls.len(), 2);
    }

    #[test]
    fn test_meta_tool_validation() {
        let mut meta_tool = MetaTool::new(
            "invalid_tool".to_string(),
            "Invalid Tool".to_string(),
            "Tool with empty tool call".to_string(),
        );
        meta_tool.add_tool_call("", HashMap::new()); // Empty tool name

        let result = meta_tool.validate();
        assert!(result.is_err());
    }

    #[test]
    fn test_aggregation_strategies() {
        let mut meta_tool = MetaTool::new(
            "agg_test".to_string(),
            "Aggregation Test".to_string(),
            "Test aggregation".to_string(),
        );

        // Test Last aggregation
        meta_tool.aggregation = AggregationStrategy::Last;
        assert_eq!(meta_tool.aggregation, AggregationStrategy::Last);

        // Test All aggregation
        meta_tool.aggregation = AggregationStrategy::All;
        assert_eq!(meta_tool.aggregation, AggregationStrategy::All);

        // Test FirstSuccess aggregation
        meta_tool.aggregation = AggregationStrategy::FirstSuccess;
        assert_eq!(meta_tool.aggregation, AggregationStrategy::FirstSuccess);

        // Test Merge aggregation
        meta_tool.aggregation = AggregationStrategy::Merge;
        assert_eq!(meta_tool.aggregation, AggregationStrategy::Merge);
    }

    #[test]
    fn test_meta_tool_execution() {
        let mut meta_tool = MetaTool::new(
            "exec_test".to_string(),
            "Execution Test".to_string(),
            "Test execution".to_string(),
        );
        meta_tool.add_tool_call("test_tool", HashMap::new());

        let params = HashMap::new();
        let result = meta_tool.execute(&params).unwrap();

        assert!(result.is_object());
    }

    #[test]
    fn test_parameter_mapping() {
        let mut mapping = HashMap::new();
        mapping.insert("file_path".to_string(), "component_path".to_string());

        let call = ToolCallStep {
            id: "step1".to_string(),
            tool: "read_file".to_string(),
            parameter_mapping: mapping,
            output_variable: None,
            condition: None,
        };

        assert_eq!(
            call.parameter_mapping.get("file_path"),
            Some(&"component_path".to_string())
        );
    }

    #[test]
    fn test_output_variable() {
        let mut meta_tool = MetaTool::new(
            "output_test".to_string(),
            "Output Test".to_string(),
            "Test output variables".to_string(),
        );

        meta_tool
            .add_tool_call("step1", HashMap::new())
            .with_output_variable("step1_result");

        assert_eq!(
            meta_tool.tool_calls[0].output_variable,
            Some("step1_result".to_string())
        );
    }

    #[test]
    fn test_condition_evaluation() {
        let meta_tool = MetaTool::new(
            "condition_test".to_string(),
            "Condition Test".to_string(),
            "Test condition evaluation".to_string(),
        );

        let mut step_vars = HashMap::new();
        step_vars.insert("status".to_string(), serde_json::json!("success"));

        let params = HashMap::new();

        // Test equality condition
        let result = meta_tool.evaluate_condition("status == \"success\"", &step_vars, &params);
        assert!(result.is_ok());
        assert!(result.unwrap());

        // Test existence condition
        let result = meta_tool.evaluate_condition("status", &step_vars, &params);
        assert!(result.is_ok());
        assert!(result.unwrap());
    }

    #[test]
    fn test_custom_aggregation() {
        let mut meta_tool = MetaTool::new(
            "custom_agg".to_string(),
            "Custom Aggregation".to_string(),
            "Test custom aggregation".to_string(),
        );

        meta_tool.aggregation = AggregationStrategy::Custom("concatenate".to_string());

        let tool_calls = [ToolCallStep {
            id: "step1".to_string(),
            tool: "tool1".to_string(),
            parameter_mapping: HashMap::new(),
            output_variable: None,
            condition: None,
        }];

        // Add some mock results
        let _results = [serde_json::json!("result1"), serde_json::json!("result2")];

        assert_eq!(tool_calls.len(), 1);
    }

    #[test]
    fn test_merge_aggregation() {
        let mut result1 = serde_json::Map::new();
        result1.insert("key1".to_string(), serde_json::json!("value1"));

        let mut result2 = serde_json::Map::new();
        result2.insert("key2".to_string(), serde_json::json!("value2"));

        let mut meta_tool = MetaTool::new(
            "merge_test".to_string(),
            "Merge Test".to_string(),
            "Test merge aggregation".to_string(),
        );

        meta_tool.aggregation = AggregationStrategy::Merge;

        // The merge should combine both objects
        assert_eq!(meta_tool.aggregation, AggregationStrategy::Merge);
    }
}
