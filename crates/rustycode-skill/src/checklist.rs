//! Execution checklist generation from workflows and pipelines.

use crate::types::{Pipeline, ProcedureKind};
use crate::workflows::Workflow;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChecklistItem {
    pub description: String,
    #[serde(default)]
    pub checked: bool,
}

impl ChecklistItem {
    pub const fn new(description: String) -> Self {
        Self {
            description,
            checked: false,
        }
    }

    pub const fn check(&mut self) {
        self.checked = true;
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Checklist {
    pub items: Vec<ChecklistItem>,
}

impl Checklist {
    pub const fn new(items: Vec<ChecklistItem>) -> Self {
        Self { items }
    }

    pub fn from_pipeline(pipeline: &Pipeline) -> Self {
        let items = pipeline
            .stages
            .iter()
            .map(|stage| {
                let description = if stage.description.is_empty() {
                    stage.name.clone()
                } else {
                    format!("{}: {}", stage.name, stage.description)
                };
                ChecklistItem::new(description)
            })
            .collect();
        Self { items }
    }

    pub fn from_workflow(workflow: &Workflow) -> Self {
        let items = workflow
            .phases
            .iter()
            .map(|phase| {
                let description = if phase.instructions.is_empty() {
                    phase.name.clone()
                } else {
                    format!("{}: {}", phase.name, phase.instructions)
                };
                ChecklistItem::new(description)
            })
            .collect();
        Self { items }
    }

    pub fn from_procedure(procedure: &ProcedureKind) -> Option<Self> {
        match procedure {
            ProcedureKind::Pipeline(pipeline) => Some(Self::from_pipeline(pipeline)),
            ProcedureKind::Instruction => None,
        }
    }

    pub fn check(&mut self, index: usize) -> Result<(), ChecklistError> {
        let len = self.items.len();
        let item = self
            .items
            .get_mut(index)
            .ok_or(ChecklistError::IndexOutOfRange { index, len })?;
        item.check();
        Ok(())
    }

    pub fn current_step(&self) -> Option<usize> {
        self.items.iter().position(|item| !item.checked)
    }

    pub fn progress(&self) -> (usize, usize) {
        let checked = self.items.iter().filter(|item| item.checked).count();
        (checked, self.items.len())
    }

    pub fn is_complete(&self) -> bool {
        self.items.iter().all(|item| item.checked)
    }

    pub fn format_markdown(&self) -> String {
        self.items
            .iter()
            .map(|item| {
                let marker = if item.checked { "[x]" } else { "[ ]" };
                format!("- {marker} {}", item.description)
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ChecklistError {
    #[error("checklist index {index} out of range (len={len})")]
    IndexOutOfRange { index: usize, len: usize },
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::PipelineStage;
    use crate::workflows::{FailureHandling, VerificationRule, WorkflowPhase};
    use rustycode_protocol::team::TeamRole;

    #[test]
    fn checklist_item_new_is_unchecked() {
        let item = ChecklistItem::new("Write failing test".to_string());
        assert!(!item.checked);
        assert_eq!(item.description, "Write failing test");
    }

    #[test]
    fn checklist_item_check_marks_checked() {
        let mut item = ChecklistItem::new("Write failing test".to_string());
        item.check();
        assert!(item.checked);
    }

    fn pipeline() -> Pipeline {
        Pipeline {
            stages: vec![
                PipelineStage {
                    name: "Analyze".to_string(),
                    description: "Read the code".to_string(),
                    required_tools: vec!["Read".to_string()],
                    parallel: false,
                },
                PipelineStage {
                    name: "Implement".to_string(),
                    description: String::new(),
                    required_tools: vec!["Write".to_string()],
                    parallel: false,
                },
            ],
        }
    }

    #[test]
    fn checklist_from_pipeline_uses_stage_names() {
        let checklist = Checklist::from_pipeline(&pipeline());
        assert_eq!(checklist.items.len(), 2);
        assert_eq!(checklist.items[0].description, "Analyze: Read the code");
        assert_eq!(checklist.items[1].description, "Implement");
    }

    #[test]
    fn checklist_check_updates_progress() {
        let mut checklist = Checklist::new(vec![
            ChecklistItem::new("Step 1".to_string()),
            ChecklistItem::new("Step 2".to_string()),
        ]);
        assert_eq!(checklist.current_step(), Some(0));
        checklist.check(0).unwrap();
        assert_eq!(checklist.current_step(), Some(1));
        assert_eq!(checklist.progress(), (1, 2));
    }

    #[test]
    fn checklist_check_out_of_range_errors() {
        let mut checklist = Checklist::new(vec![ChecklistItem::new("Step 1".to_string())]);
        let err = checklist.check(3).unwrap_err();
        assert!(matches!(err, ChecklistError::IndexOutOfRange { .. }));
    }

    #[test]
    fn checklist_from_workflow_uses_phase_instructions() {
        let workflow = Workflow {
            id: "demo".to_string(),
            name: "Demo".to_string(),
            description: "Demo workflow".to_string(),
            phases: vec![WorkflowPhase {
                name: "PLAN".to_string(),
                agent: TeamRole::Architect,
                instructions: "Create a plan".to_string(),
                verification: Some(VerificationRule {
                    check: "plan exists".to_string(),
                    retry_max: 1,
                    escalate_on_failure: false,
                }),
                on_failure: FailureHandling::Retry,
            }],
            triggers: vec![],
            enabled: true,
        };
        let checklist = Checklist::from_workflow(&workflow);
        assert_eq!(checklist.items[0].description, "PLAN: Create a plan");
    }

    #[test]
    fn checklist_from_procedure_pipeline_returns_some() {
        let procedure = ProcedureKind::Pipeline(pipeline());
        assert!(Checklist::from_procedure(&procedure).is_some());
    }

    #[test]
    fn checklist_from_procedure_instruction_returns_none() {
        assert!(Checklist::from_procedure(&ProcedureKind::Instruction).is_none());
    }

    #[test]
    fn checklist_format_markdown_renders_boxes() {
        let mut checklist = Checklist::new(vec![
            ChecklistItem::new("Step 1".to_string()),
            ChecklistItem::new("Step 2".to_string()),
        ]);
        checklist.check(0).unwrap();
        let md = checklist.format_markdown();
        assert!(md.contains("- [x] Step 1"));
        assert!(md.contains("- [ ] Step 2"));
    }

    #[test]
    fn checklist_is_complete_when_all_checked() {
        let mut checklist = Checklist::new(vec![
            ChecklistItem::new("Step 1".to_string()),
            ChecklistItem::new("Step 2".to_string()),
        ]);
        checklist.check(0).unwrap();
        checklist.check(1).unwrap();
        assert!(checklist.is_complete());
    }
}
