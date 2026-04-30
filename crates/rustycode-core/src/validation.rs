// ── Plan Validation System ─────────────────────────────────────────────────────

//! Plan validation system for pre-execution checks.
//!
//! This module provides comprehensive validation of plans before execution to prevent
//! failures due to circular dependencies, missing tools, invalid file paths, and
//! improperly ordered steps.

use anyhow::{Context, Result};
use rustycode_protocol::{Plan, PlanStep};
use rustycode_tools::ToolRegistry;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use thiserror::Error;

/// Detailed validation error with context.
#[derive(Debug, Clone, Error, PartialEq, Eq)]
#[non_exhaustive]
pub enum ValidationError {
    /// A circular dependency was detected in the plan steps.
    #[error("Circular dependency detected: {0}")]
    CircularDependency(String),

    /// A required tool is not registered in the tool registry.
    #[error("Tool not found: {tool_name} (required by step: {step_title})")]
    ToolNotFound {
        tool_name: String,
        step_title: String,
    },

    /// A file path is invalid or malformed.
    #[error("Invalid file path: {path} (reason: {reason})")]
    InvalidPath { path: String, reason: String },

    /// Steps are not properly ordered (order field is inconsistent).
    #[error("Steps are not properly ordered: {0}")]
    InvalidStepOrder(String),

    /// A plan has no steps to execute.
    #[error("Plan has no steps")]
    EmptyPlan,

    /// Multiple validation errors occurred.
    #[error("Multiple validation errors ({count}):\n{errors}")]
    MultipleErrors { count: usize, errors: String },

    /// A required field is missing or empty.
    #[error("Missing required field: {field} in step {step_index}")]
    MissingField { field: String, step_index: usize },

    /// Step references another step that doesn't exist.
    #[error("Step {step_index} references non-existent step {reference}")]
    InvalidStepReference { step_index: usize, reference: usize },
}

/// Result of plan validation.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValidationResult {
    /// Validation passed successfully.
    Valid,
    /// Validation failed with specific errors.
    Invalid(Vec<ValidationError>),
}

impl ValidationResult {
    /// Check if validation passed.
    pub fn is_valid(&self) -> bool {
        matches!(self, Self::Valid)
    }

    /// Get all validation errors if invalid.
    pub fn errors(&self) -> &[ValidationError] {
        match self {
            Self::Valid => &[],
            Self::Invalid(errors) => errors,
        }
    }

    /// Combine multiple validation results.
    pub fn combine(results: Vec<ValidationResult>) -> Self {
        let all_errors: Vec<ValidationError> = results
            .into_iter()
            .filter_map(|r| match r {
                ValidationResult::Invalid(errors) => Some(errors),
                ValidationResult::Valid => None,
            })
            .flatten()
            .collect();

        if all_errors.is_empty() {
            ValidationResult::Valid
        } else {
            ValidationResult::Invalid(all_errors)
        }
    }
}

/// Core trait for plan validation.
///
/// Implementers provide specific validation logic for different aspects of plans.
pub trait PlanValidator: Send + Sync {
    /// Validate a plan and return detailed results.
    ///
    /// # Arguments
    ///
    /// * `plan` - The plan to validate
    /// * `tool_registry` - Registry of available tools
    /// * `workspace_root` - Root path for resolving relative file paths
    ///
    /// # Returns
    ///
    /// `Ok(ValidationResult)` indicating validation success or failure with details.
    fn validate(
        &self,
        plan: &Plan,
        tool_registry: &ToolRegistry,
        workspace_root: &Path,
    ) -> Result<ValidationResult>;

    /// Get the name of this validator.
    fn name(&self) -> &str;
}

/// Comprehensive plan validator that runs all validation rules.
pub struct ComprehensivePlanValidator {
    /// Individual validators to run.
    validators: Vec<Box<dyn PlanValidator>>,
}

impl ComprehensivePlanValidator {
    /// Create a new comprehensive validator with all standard validators.
    pub fn new() -> Self {
        Self {
            validators: vec![
                Box::new(EmptyPlanValidator),
                Box::new(CircularDependencyValidator),
                Box::new(ToolExistenceValidator),
                Box::new(PathValidator),
                Box::new(StepOrderValidator),
                Box::new(FieldCompletenessValidator),
            ],
        }
    }

    /// Add a custom validator to the comprehensive suite.
    pub fn add_validator(mut self, validator: Box<dyn PlanValidator>) -> Self {
        self.validators.push(validator);
        self
    }

    /// Validate a plan using all registered validators.
    ///
    /// This runs each validator in sequence and collects all errors.
    pub fn validate_all(
        &self,
        plan: &Plan,
        tool_registry: &ToolRegistry,
        workspace_root: &Path,
    ) -> Result<ValidationResult> {
        let results: Vec<ValidationResult> = self
            .validators
            .iter()
            .map(|v| v.validate(plan, tool_registry, workspace_root))
            .collect::<Result<Vec<_>>>()
            .context("Failed to run validators")?;

        Ok(ValidationResult::combine(results))
    }
}

impl Default for ComprehensivePlanValidator {
    fn default() -> Self {
        Self::new()
    }
}

// ── Individual Validators ──────────────────────────────────────────────────────

/// Validates that a plan has at least one step.
struct EmptyPlanValidator;

impl PlanValidator for EmptyPlanValidator {
    fn validate(
        &self,
        plan: &Plan,
        _tool_registry: &ToolRegistry,
        _workspace_root: &Path,
    ) -> Result<ValidationResult> {
        if plan.steps.is_empty() {
            Ok(ValidationResult::Invalid(vec![ValidationError::EmptyPlan]))
        } else {
            Ok(ValidationResult::Valid)
        }
    }

    fn name(&self) -> &str {
        "empty_plan_validator"
    }
}

/// Validates that there are no circular dependencies between steps.
///
/// This builds a dependency graph from tool dependencies and checks for cycles.
struct CircularDependencyValidator;

impl PlanValidator for CircularDependencyValidator {
    fn validate(
        &self,
        plan: &Plan,
        _tool_registry: &ToolRegistry,
        _workspace_root: &Path,
    ) -> Result<ValidationResult> {
        // Build adjacency list for dependency graph
        // We consider a step dependent on another if it references it in description/title
        let mut graph: HashMap<usize, Vec<usize>> = HashMap::new();
        let mut step_indices: HashSet<usize> = HashSet::new();

        // Collect all step indices
        for step in &plan.steps {
            step_indices.insert(step.order);
        }

        // Build dependency edges
        for step in &plan.steps {
            let dependencies = extract_step_dependencies(step, &plan.steps);
            graph.insert(step.order, dependencies);
        }

        // Detect cycles using DFS
        let mut visited = HashSet::new();
        let mut rec_stack = HashSet::new();

        for &node in &step_indices {
            if !visited.contains(&node) {
                if let Some(cycle) = detect_cycle(&graph, node, &mut visited, &mut rec_stack) {
                    return Ok(ValidationResult::Invalid(vec![
                        ValidationError::CircularDependency(format!(
                            "steps: {}",
                            cycle
                                .iter()
                                .map(|i| i.to_string())
                                .collect::<Vec<_>>()
                                .join(" -> ")
                        )),
                    ]));
                }
            }
        }

        Ok(ValidationResult::Valid)
    }

    fn name(&self) -> &str {
        "circular_dependency_validator"
    }
}

/// Extract step dependencies from a step's description and tool calls.
fn extract_step_dependencies(step: &PlanStep, all_steps: &[PlanStep]) -> Vec<usize> {
    let mut deps = Vec::new();

    // Look for step references in description (e.g., "after step 1", "depends on step 2")
    let text = format!("{} {}", step.title, step.description).to_lowercase();

    for other_step in all_steps {
        if other_step.order >= step.order {
            continue; // Only depend on earlier steps
        }

        // Check if this step references another step
        let reference_pattern = format!("step {}", other_step.order);
        let alt_pattern = format!("step #{}", other_step.order);
        let by_title = format!("\"{}\"", other_step.title.to_lowercase());

        if text.contains(&reference_pattern)
            || text.contains(&alt_pattern)
            || text.contains(&by_title)
        {
            deps.push(other_step.order);
        }
    }

    deps
}

/// Detect cycle in directed graph using DFS.
fn detect_cycle(
    graph: &HashMap<usize, Vec<usize>>,
    node: usize,
    visited: &mut HashSet<usize>,
    rec_stack: &mut HashSet<usize>,
) -> Option<Vec<usize>> {
    visited.insert(node);
    rec_stack.insert(node);

    if let Some(neighbors) = graph.get(&node) {
        for &neighbor in neighbors {
            if !visited.contains(&neighbor) {
                if let Some(cycle) = detect_cycle(graph, neighbor, visited, rec_stack) {
                    let mut result = vec![node];
                    result.extend(cycle);
                    return Some(result);
                }
            } else if rec_stack.contains(&neighbor) {
                // Found a cycle
                return Some(vec![node, neighbor]);
            }
        }
    }

    rec_stack.remove(&node);
    None
}

/// Validates that all tools referenced in plan steps are registered.
struct ToolExistenceValidator;

impl PlanValidator for ToolExistenceValidator {
    fn validate(
        &self,
        plan: &Plan,
        tool_registry: &ToolRegistry,
        _workspace_root: &Path,
    ) -> Result<ValidationResult> {
        let mut errors = Vec::new();
        let available_tools: HashSet<String> =
            tool_registry.list().into_iter().map(|t| t.name).collect();

        for step in &plan.steps {
            for tool_name in &step.tools {
                if !available_tools.contains(tool_name) {
                    errors.push(ValidationError::ToolNotFound {
                        tool_name: tool_name.clone(),
                        step_title: step.title.clone(),
                    });
                }
            }
        }

        if errors.is_empty() {
            Ok(ValidationResult::Valid)
        } else {
            Ok(ValidationResult::Invalid(errors))
        }
    }

    fn name(&self) -> &str {
        "tool_existence_validator"
    }
}

/// Validates that file paths are valid and well-formed.
struct PathValidator;

impl PlanValidator for PathValidator {
    fn validate(
        &self,
        plan: &Plan,
        _tool_registry: &ToolRegistry,
        workspace_root: &Path,
    ) -> Result<ValidationResult> {
        let mut errors = Vec::new();

        // Validate files_to_modify
        for file_path in &plan.files_to_modify {
            // Check for obvious path traversal attempts
            if file_path.contains("..") {
                errors.push(ValidationError::InvalidPath {
                    path: file_path.clone(),
                    reason: "contains parent directory reference (..)".to_string(),
                });
                continue;
            }

            // Check if it's an absolute path (should be relative)
            if PathBuf::from(file_path).is_absolute() {
                errors.push(ValidationError::InvalidPath {
                    path: file_path.clone(),
                    reason: "must be relative to workspace root".to_string(),
                });
                continue;
            }

            // Check if path is valid (no invalid characters)
            if file_path.contains('\0') || file_path.contains('\n') || file_path.contains('\r') {
                errors.push(ValidationError::InvalidPath {
                    path: file_path.clone(),
                    reason: "contains invalid characters".to_string(),
                });
                continue;
            }

            // Resolve to full path and check if it's within workspace
            let full_path = workspace_root.join(file_path);
            match full_path.canonicalize() {
                Ok(canonical) => {
                    // Ensure canonical path starts with workspace root
                    if !canonical.starts_with(
                        workspace_root
                            .canonicalize()
                            .unwrap_or_else(|_| workspace_root.to_path_buf()),
                    ) {
                        errors.push(ValidationError::InvalidPath {
                            path: file_path.clone(),
                            reason: "resolves outside workspace directory".to_string(),
                        });
                    }
                }
                Err(_) => {
                    // File doesn't exist, but that's OK - we're validating the path format
                    // Just check that parent directories could potentially exist
                    if let Some(parent) = PathBuf::from(file_path).parent() {
                        if !parent.as_os_str().is_empty() {
                            let parent_full = workspace_root.join(parent);
                            if !parent_full.starts_with(workspace_root) {
                                errors.push(ValidationError::InvalidPath {
                                    path: file_path.clone(),
                                    reason: "parent directory outside workspace".to_string(),
                                });
                            }
                        }
                    }
                }
            }
        }

        if errors.is_empty() {
            Ok(ValidationResult::Valid)
        } else {
            Ok(ValidationResult::Invalid(errors))
        }
    }

    fn name(&self) -> &str {
        "path_validator"
    }
}

/// Validates that steps are properly ordered.
struct StepOrderValidator;

impl PlanValidator for StepOrderValidator {
    fn validate(
        &self,
        plan: &Plan,
        _tool_registry: &ToolRegistry,
        _workspace_root: &Path,
    ) -> Result<ValidationResult> {
        let mut errors = Vec::new();

        // Check that step orders are sequential starting from 0
        let mut orders: Vec<_> = plan.steps.iter().map(|s| s.order).collect();
        orders.sort();

        for (i, &order) in orders.iter().enumerate() {
            if order != i {
                errors.push(ValidationError::InvalidStepOrder(format!(
                    "step order {} should be {} (expected sequential ordering from 0)",
                    order, i
                )));
            }
        }

        // Check for duplicate orders
        let mut seen = HashSet::new();
        for step in &plan.steps {
            if !seen.insert(step.order) {
                errors.push(ValidationError::InvalidStepOrder(format!(
                    "duplicate step order: {} (in steps: {:?})",
                    step.order,
                    plan.steps
                        .iter()
                        .filter(|s| s.order == step.order)
                        .map(|s| &s.title)
                        .collect::<Vec<_>>()
                )));
            }
        }

        if errors.is_empty() {
            Ok(ValidationResult::Valid)
        } else {
            Ok(ValidationResult::Invalid(errors))
        }
    }

    fn name(&self) -> &str {
        "step_order_validator"
    }
}

/// Validates that all required fields are present and non-empty.
struct FieldCompletenessValidator;

impl PlanValidator for FieldCompletenessValidator {
    fn validate(
        &self,
        plan: &Plan,
        _tool_registry: &ToolRegistry,
        _workspace_root: &Path,
    ) -> Result<ValidationResult> {
        let mut errors = Vec::new();

        // Check plan-level fields
        if plan.task.trim().is_empty() {
            errors.push(ValidationError::MissingField {
                field: "task".to_string(),
                step_index: 0, // 0 indicates plan-level field
            });
        }

        if plan.summary.trim().is_empty() {
            errors.push(ValidationError::MissingField {
                field: "summary".to_string(),
                step_index: 0,
            });
        }

        // Check step-level fields
        for (idx, step) in plan.steps.iter().enumerate() {
            if step.title.trim().is_empty() {
                errors.push(ValidationError::MissingField {
                    field: "title".to_string(),
                    step_index: idx,
                });
            }

            if step.description.trim().is_empty() {
                errors.push(ValidationError::MissingField {
                    field: "description".to_string(),
                    step_index: idx,
                });
            }

            if step.expected_outcome.trim().is_empty() {
                errors.push(ValidationError::MissingField {
                    field: "expected_outcome".to_string(),
                    step_index: idx,
                });
            }
        }

        if errors.is_empty() {
            Ok(ValidationResult::Valid)
        } else {
            Ok(ValidationResult::Invalid(errors))
        }
    }

    fn name(&self) -> &str {
        "field_completeness_validator"
    }
}

// ── Utility Functions ─────────────────────────────────────────────────────────

/// Validate a plan before execution.
///
/// This is the main entry point for plan validation. It runs all validation rules
/// and returns detailed errors if validation fails.
///
/// # Arguments
///
/// * `plan` - The plan to validate
/// * `tool_registry` - Registry of available tools
/// * `workspace_root` - Root path for resolving relative file paths
///
/// # Returns
///
/// * `Ok(())` if validation passes
/// * `Err(anyhow::Error)` with detailed validation context if validation fails
///
/// # Example
///
/// ```ignore
/// use rustycode_core::validation::validate_plan;
/// use rustycode_protocol::{Plan, PlanStep, PlanId, PlanStatus, SessionId};
/// use rustycode_tools::ToolRegistry;
/// use std::path::Path;
/// use chrono::Utc;
///
/// # fn main() -> anyhow::Result<()> {
/// let plan = Plan {
///     id: PlanId::new(),
///     session_id: SessionId::new(),
///     task: "Example task".to_string(),
///     created_at: Utc::now(),
///     status: PlanStatus::Draft,
///     summary: "Example summary".to_string(),
///     approach: "Example approach".to_string(),
///     steps: vec![],
///     files_to_modify: vec![],
///     risks: vec![],
///     current_step_index: None,
///     execution_started_at: None,
///     execution_completed_at: None,
///     execution_error: None,
/// };
/// let tool_registry = ToolRegistry::new();
/// let workspace_root = Path::new("/my/project");
///
/// // This will fail with empty plan error
/// let result = validate_plan(&plan, &tool_registry, workspace_root);
/// assert!(result.is_err());
/// # Ok(())
/// # }
/// ```
pub fn validate_plan(
    plan: &Plan,
    tool_registry: &ToolRegistry,
    workspace_root: &Path,
) -> Result<()> {
    let validator = ComprehensivePlanValidator::new();
    let result = validator
        .validate_all(plan, tool_registry, workspace_root)
        .context("Failed to validate plan")?;

    match result {
        ValidationResult::Valid => Ok(()),
        ValidationResult::Invalid(errors) => {
            // Format all errors into a comprehensive message
            let error_messages: Vec<String> = errors.iter().map(|e| format!("  - {}", e)).collect();

            let msg = format!(
                "Plan validation failed with {} error(s):\n{}",
                errors.len(),
                error_messages.join("\n")
            );

            Err(anyhow::anyhow!(msg))
        }
    }
}

/// Validate a plan and return detailed result without converting to error.
///
/// Use this when you want to handle validation results programmatically
/// rather than just failing on error.
///
/// # Arguments
///
/// * `plan` - The plan to validate
/// * `tool_registry` - Registry of available tools
/// * `workspace_root` - Root path for resolving relative file paths
///
/// # Returns
///
/// `ValidationResult` indicating success or detailed failures
pub fn validate_plan_detailed(
    plan: &Plan,
    tool_registry: &ToolRegistry,
    workspace_root: &Path,
) -> Result<ValidationResult> {
    let validator = ComprehensivePlanValidator::new();
    validator
        .validate_all(plan, tool_registry, workspace_root)
        .context("Failed to validate plan")
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use rustycode_protocol::{PlanId, PlanStatus, SessionId, StepStatus};

    fn create_test_plan(steps: Vec<PlanStep>) -> Plan {
        Plan {
            id: PlanId::new(),
            session_id: SessionId::new(),
            task: "Test task".to_string(),
            created_at: Utc::now(),
            status: PlanStatus::Draft,
            summary: "Test summary".to_string(),
            approach: "Test approach".to_string(),
            steps,
            files_to_modify: vec![],
            risks: vec![],
            current_step_index: None,
            execution_started_at: None,
            execution_completed_at: None,
            execution_error: None,
            task_profile: None,
        }
    }

    fn create_test_step(order: usize, tools: Vec<&str>) -> PlanStep {
        PlanStep {
            order,
            title: format!("Step {}", order),
            description: format!("Description for step {}", order),
            tools: tools.into_iter().map(String::from).collect(),
            expected_outcome: format!("Outcome for step {}", order),
            rollback_hint: format!("Rollback step {}", order),
            execution_status: StepStatus::Pending,
            tool_calls: vec![],
            tool_executions: vec![],
            results: vec![],
            errors: vec![],
            started_at: None,
            completed_at: None,
        }
    }

    #[test]
    fn test_validate_empty_plan() {
        let plan = create_test_plan(vec![]);
        let tool_registry = ToolRegistry::new();
        let workspace = Path::new("/tmp/test");

        let result = validate_plan_detailed(&plan, &tool_registry, workspace).unwrap();
        assert!(!result.is_valid());
        assert!(matches!(result.errors()[0], ValidationError::EmptyPlan));
    }

    #[test]
    fn test_validate_missing_tool() {
        let plan = create_test_plan(vec![create_test_step(0, vec!["nonexistent_tool"])]);
        let tool_registry = ToolRegistry::new();
        let workspace = Path::new("/tmp/test");

        let result = validate_plan_detailed(&plan, &tool_registry, workspace).unwrap();
        assert!(!result.is_valid());
        match &result.errors()[0] {
            ValidationError::ToolNotFound { tool_name, .. } => {
                assert_eq!(tool_name, "nonexistent_tool");
            }
            _ => panic!("Expected ToolNotFound error"),
        }
    }

    #[test]
    fn test_validate_invalid_path() {
        let mut plan = create_test_plan(vec![create_test_step(0, vec![])]);
        plan.files_to_modify = vec!["../../../etc/passwd".to_string()];

        let tool_registry = ToolRegistry::new();
        let workspace = Path::new("/tmp/test");

        let result = validate_plan_detailed(&plan, &tool_registry, workspace).unwrap();
        assert!(!result.is_valid());
        match &result.errors()[0] {
            ValidationError::InvalidPath { path, .. } => {
                assert!(path.contains(".."));
            }
            _ => panic!("Expected InvalidPath error"),
        }
    }

    #[test]
    fn test_validate_invalid_step_order() {
        let plan = create_test_plan(vec![
            create_test_step(0, vec![]),
            create_test_step(2, vec![]), // Skip 1 - should fail
            create_test_step(3, vec![]), // Not sequential
        ]);

        let tool_registry = ToolRegistry::new();
        let workspace = Path::new("/tmp/test");

        let result = validate_plan_detailed(&plan, &tool_registry, workspace).unwrap();
        assert!(!result.is_valid());
        assert!(result
            .errors()
            .iter()
            .any(|e| matches!(e, ValidationError::InvalidStepOrder(_))));
    }

    #[test]
    fn test_validate_missing_fields() {
        let mut step = create_test_step(0, vec![]);
        step.title = "".to_string(); // Missing title

        let plan = create_test_plan(vec![step]);

        let tool_registry = ToolRegistry::new();
        let workspace = Path::new("/tmp/test");

        let result = validate_plan_detailed(&plan, &tool_registry, workspace).unwrap();
        assert!(!result.is_valid());
        assert!(result
            .errors()
            .iter()
            .any(|e| matches!(e, ValidationError::MissingField { .. })));
    }

    #[test]
    fn test_validate_valid_plan() {
        let plan = create_test_plan(vec![
            create_test_step(0, vec![]),
            create_test_step(1, vec![]),
            create_test_step(2, vec![]),
        ]);

        let tool_registry = ToolRegistry::new();
        let workspace = Path::new("/tmp/test");

        let result = validate_plan_detailed(&plan, &tool_registry, workspace).unwrap();
        assert!(result.is_valid());
    }

    #[test]
    fn test_validate_duplicate_step_order() {
        let plan = create_test_plan(vec![
            create_test_step(0, vec![]),
            create_test_step(1, vec![]),
            create_test_step(1, vec![]), // Duplicate order
        ]);

        let tool_registry = ToolRegistry::new();
        let workspace = Path::new("/tmp/test");

        let result = validate_plan_detailed(&plan, &tool_registry, workspace).unwrap();
        assert!(!result.is_valid());
        assert!(result
            .errors()
            .iter()
            .any(|e| matches!(e, ValidationError::InvalidStepOrder(_))));
    }

    #[test]
    fn test_validate_path_with_absolute_path() {
        let mut plan = create_test_plan(vec![create_test_step(0, vec![])]);
        plan.files_to_modify = vec!["/etc/passwd".to_string()];

        let tool_registry = ToolRegistry::new();
        let workspace = Path::new("/tmp/test");

        let result = validate_plan_detailed(&plan, &tool_registry, workspace).unwrap();
        assert!(!result.is_valid());
        match &result.errors()[0] {
            ValidationError::InvalidPath { reason, .. } => {
                assert!(reason.contains("relative"));
            }
            _ => panic!("Expected InvalidPath error for absolute path"),
        }
    }
}
