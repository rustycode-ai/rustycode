//! Workflow Registry
//!
//! This module provides workflow registration and lookup:
//! - Store workflow definitions
//! - Retrieve workflows by ID
//! - List available workflows
//! - Validation
//! - Built-in workflow templates

use crate::workflow::{Result, Workflow, WorkflowError};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

/// Workflow registry
pub struct WorkflowRegistry {
    workflows: HashMap<String, Arc<Workflow>>,
    workflow_templates: HashMap<String, Arc<Workflow>>,
}

impl WorkflowRegistry {
    pub fn new() -> Self {
        Self {
            workflows: HashMap::new(),
            workflow_templates: HashMap::new(),
        }
    }

    /// Register a workflow
    pub fn register(&mut self, workflow: Workflow) -> Result<()> {
        // Validate workflow before registering
        workflow.validate()?;

        let id = workflow.id.clone();
        if self.workflows.contains_key(&id) {
            return Err(WorkflowError::Validation(format!(
                "Workflow with id '{}' already exists",
                id
            )));
        }

        self.workflows.insert(id, Arc::new(workflow));
        Ok(())
    }

    /// Get a workflow by ID
    pub fn get(&self, id: &str) -> Option<Arc<Workflow>> {
        self.workflows
            .get(id)
            .cloned()
            .or_else(|| self.workflow_templates.get(id).cloned())
    }

    /// List all available workflow IDs
    pub fn list(&self) -> Vec<String> {
        let mut ids: Vec<String> = self.workflows.keys().cloned().collect();
        ids.extend(self.workflow_templates.keys().cloned());
        ids.sort();
        ids.dedup();
        ids
    }

    /// Register a workflow template
    pub fn register_template(&mut self, workflow: Workflow) -> Result<()> {
        workflow.validate()?;

        let id = workflow.id.clone();
        self.workflow_templates.insert(id, Arc::new(workflow));
        Ok(())
    }

    /// Load workflow from YAML file
    pub fn load_from_yaml_file<P: AsRef<Path>>(&mut self, path: P) -> Result<()> {
        let workflow = crate::workflow::parser::parse_yaml_file(path)?;
        self.register(workflow)
    }

    /// Load workflow from JSON file
    pub fn load_from_json_file<P: AsRef<Path>>(&mut self, path: P) -> Result<()> {
        let workflow = crate::workflow::parser::parse_json_file(path)?;
        self.register(workflow)
    }

    /// Unregister a workflow
    pub fn unregister(&mut self, id: &str) -> Result<()> {
        if self.workflows.remove(id).is_none() {
            return Err(WorkflowError::NotFound(id.to_string()));
        }
        Ok(())
    }

    /// Create built-in workflow templates
    pub fn with_builtin_templates(mut self) -> Self {
        // Register code review workflow template
        let code_review = Workflow::new(
            "code_review".to_string(),
            "Code Review".to_string(),
            "Comprehensive code review workflow".to_string(),
        );

        let _ = self.register_template(code_review);

        // Register refactor workflow template
        let refactor = Workflow::new(
            "refactor_component".to_string(),
            "Refactor Component".to_string(),
            "Safe component refactoring workflow".to_string(),
        );

        let _ = self.register_template(refactor);

        // Register debugging workflow template
        let debug = Workflow::new(
            "debug_issue".to_string(),
            "Debug Issue".to_string(),
            "Systematic debugging workflow".to_string(),
        );

        let _ = self.register_template(debug);

        self
    }

    /// Search workflows by name or description
    pub fn search(&self, query: &str) -> Vec<String> {
        let query_lower = query.to_lowercase();

        self.list()
            .into_iter()
            .filter(|id| {
                if let Some(workflow) = self.get(id) {
                    workflow.name.to_lowercase().contains(&query_lower)
                        || workflow.description.to_lowercase().contains(&query_lower)
                } else {
                    false
                }
            })
            .collect()
    }

    /// Get workflow statistics
    pub fn stats(&self) -> WorkflowStats {
        WorkflowStats {
            total_workflows: self.workflows.len() + self.workflow_templates.len(),
            custom_workflows: self.workflows.len(),
            template_workflows: self.workflow_templates.len(),
        }
    }
}

impl Default for WorkflowRegistry {
    fn default() -> Self {
        Self::new().with_builtin_templates()
    }
}

/// Workflow statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowStats {
    pub total_workflows: usize,
    pub custom_workflows: usize,
    pub template_workflows: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_registry_creation() {
        let registry = WorkflowRegistry::new().with_builtin_templates();
        let stats = registry.stats();

        assert_eq!(stats.total_workflows, 3); // 3 built-in templates
        assert_eq!(stats.custom_workflows, 0);
        assert_eq!(stats.template_workflows, 3);
    }

    #[test]
    fn test_workflow_registration() {
        let mut registry = WorkflowRegistry::new();

        let workflow = Workflow::new(
            "test_workflow".to_string(),
            "Test Workflow".to_string(),
            "A test workflow".to_string(),
        );

        let result = registry.register(workflow);
        assert!(result.is_ok());

        let retrieved = registry.get("test_workflow");
        assert!(retrieved.is_some());

        let list = registry.list();
        assert!(list.contains(&"test_workflow".to_string()));
    }

    #[test]
    fn test_duplicate_registration() {
        let mut registry = WorkflowRegistry::new();

        let workflow = Workflow::new(
            "duplicate".to_string(),
            "Duplicate".to_string(),
            "Test duplicate".to_string(),
        );

        registry.register(workflow.clone()).unwrap();

        let result = registry.register(workflow);
        assert!(result.is_err());
    }

    #[test]
    fn test_workflow_unregistration() {
        let mut registry = WorkflowRegistry::new();

        let workflow = Workflow::new(
            "temp_workflow".to_string(),
            "Temp Workflow".to_string(),
            "Temporary workflow".to_string(),
        );

        registry.register(workflow).unwrap();
        assert!(registry.get("temp_workflow").is_some());

        let result = registry.unregister("temp_workflow");
        assert!(result.is_ok());
        assert!(registry.get("temp_workflow").is_none());
    }

    #[test]
    fn test_workflow_search() {
        let registry = WorkflowRegistry::new().with_builtin_templates();

        // Search for "review"
        let results = registry.search("review");
        assert!(results.contains(&"code_review".to_string()));

        // Search for "debug"
        let results = registry.search("debug");
        assert!(results.contains(&"debug_issue".to_string()));
    }

    #[test]
    fn test_template_workflow_lookup() {
        let registry = WorkflowRegistry::new().with_builtin_templates();

        // Built-in templates should be available
        let code_review = registry.get("code_review");
        assert!(code_review.is_some());

        let refactor = registry.get("refactor_component");
        assert!(refactor.is_some());

        let debug = registry.get("debug_issue");
        assert!(debug.is_some());
    }

    #[test]
    fn test_workflow_stats() {
        let mut registry = WorkflowRegistry::new().with_builtin_templates();

        let stats = registry.stats();
        assert_eq!(stats.total_workflows, 3); // Built-in templates

        let workflow = Workflow::new(
            "custom".to_string(),
            "Custom".to_string(),
            "Custom workflow".to_string(),
        );

        registry.register(workflow).unwrap();

        let stats = registry.stats();
        assert_eq!(stats.total_workflows, 4);
        assert_eq!(stats.custom_workflows, 1);
        assert_eq!(stats.template_workflows, 3);
    }

    #[test]
    fn test_builtin_templates_validity() {
        let registry = WorkflowRegistry::new().with_builtin_templates();

        // All built-in templates should be valid
        for id in registry.list() {
            if let Some(workflow) = registry.get(&id) {
                let validation = workflow.validate();
                assert!(
                    validation.is_ok(),
                    "Template {} should be valid: {:?}",
                    id,
                    validation
                );
            }
        }
    }
}
