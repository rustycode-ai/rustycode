//! Plan mode tool allowlist and validation.
//!
//! This module enforces that plan mode only permits inspection tools,
//! preventing accidental destructive operations during the planning phase.

use anyhow::{anyhow, Result};

/// Tools allowed in plan mode (inspection only, no destructive ops)
const INSPECTION_TOOLS: &[&str] = &[
    // File reading
    "read",
    // Code search
    "grep",
    "search_for_pattern",
    // Symbol inspection
    "find_symbol",
    "get_symbols_overview",
    "find_referencing_symbols",
    // LSP queries
    "hover",
    "goToDefinition",
    "findReferences",
    "documentSymbol",
    // Listing
    "list_dir",
    "find_file",
    "glob",
];

/// Tools explicitly forbidden in plan mode
const DESTRUCTIVE_TOOLS: &[&str] = &[
    "bash",   // Shell commands
    "write",  // File writes
    "edit",   // File edits
    "delete", // File deletion
];

/// Represents a tool call step with its name and parameters.
#[derive(Debug, Clone)]
pub struct ExecutionStep {
    /// Name of the tool being invoked
    pub tool: String,
    /// Tool parameters (can be empty)
    pub params: std::collections::HashMap<String, String>,
}

/// Validator for plan mode execution steps.
pub struct PlanValidator;

impl PlanValidator {
    /// Validate that a step is allowed in plan mode.
    ///
    /// Returns `Ok(())` if the tool is in the inspection allowlist.
    /// Returns error if tool is destructive or not in allowlist.
    pub fn validate_step(step: &ExecutionStep) -> Result<()> {
        let tool_name = &step.tool;

        // Check if tool is explicitly forbidden
        if DESTRUCTIVE_TOOLS.contains(&tool_name.as_str()) {
            return Err(anyhow!(
                "Plan mode: destructive tool '{}' not allowed. Only inspection tools permitted.",
                tool_name
            ));
        }

        // Check if tool is in allowlist
        if !INSPECTION_TOOLS.contains(&tool_name.as_str()) {
            return Err(anyhow!(
                "Plan mode: tool '{}' not in allowlist. Only inspection tools permitted: {:?}",
                tool_name,
                INSPECTION_TOOLS
            ));
        }

        Ok(())
    }

    /// Validate entire plan before execution.
    ///
    /// Checks all steps and returns error on first violation,
    /// with step number included in error message.
    pub fn validate_plan(steps: &[ExecutionStep]) -> Result<()> {
        for (i, step) in steps.iter().enumerate() {
            Self::validate_step(step).map_err(|e| anyhow!("Step {}: {}", i + 1, e))?;
        }
        Ok(())
    }

    /// Get list of allowed tools.
    pub fn allowed_tools() -> Vec<&'static str> {
        INSPECTION_TOOLS.to_vec()
    }

    /// Check if tool is allowed in plan mode.
    pub fn is_tool_allowed(tool_name: &str) -> bool {
        INSPECTION_TOOLS.contains(&tool_name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_inspection_tools_allowed() {
        let step = ExecutionStep {
            tool: "read".to_string(),
            params: Default::default(),
        };
        assert!(PlanValidator::validate_step(&step).is_ok());
    }

    #[test]
    fn test_destructive_tools_forbidden() {
        let step = ExecutionStep {
            tool: "bash".to_string(),
            params: Default::default(),
        };
        assert!(PlanValidator::validate_step(&step).is_err());
    }

    #[test]
    fn test_unknown_tools_forbidden() {
        let step = ExecutionStep {
            tool: "unknown_tool".to_string(),
            params: Default::default(),
        };
        let result = PlanValidator::validate_step(&step);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not in allowlist"));
    }

    #[test]
    fn test_validate_multi_step_plan() {
        let steps = vec![
            ExecutionStep {
                tool: "read".to_string(),
                params: Default::default(),
            },
            ExecutionStep {
                tool: "grep".to_string(),
                params: Default::default(),
            },
            ExecutionStep {
                tool: "find_symbol".to_string(),
                params: Default::default(),
            },
        ];

        assert!(PlanValidator::validate_plan(&steps).is_ok());
    }

    #[test]
    fn test_validate_plan_with_forbidden_step() {
        let steps = vec![
            ExecutionStep {
                tool: "read".to_string(),
                params: Default::default(),
            },
            ExecutionStep {
                tool: "write".to_string(), // FORBIDDEN
                params: Default::default(),
            },
        ];

        let result = PlanValidator::validate_plan(&steps);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Step 2"));
    }

    #[test]
    fn test_allowed_tools_list() {
        let allowed = PlanValidator::allowed_tools();
        assert!(allowed.contains(&"read"));
        assert!(allowed.contains(&"grep"));
        assert!(!allowed.contains(&"write"));
        assert!(!allowed.contains(&"bash"));
    }

    #[test]
    fn test_is_tool_allowed() {
        assert!(PlanValidator::is_tool_allowed("read"));
        assert!(PlanValidator::is_tool_allowed("find_symbol"));
        assert!(!PlanValidator::is_tool_allowed("write"));
        assert!(!PlanValidator::is_tool_allowed("bash"));
    }
}
