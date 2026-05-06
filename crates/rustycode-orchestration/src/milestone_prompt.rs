//! Prompting and parsing helpers for milestone decomposition.

use anyhow::{Context, Result};
use chrono::Utc;
use rustycode_protocol::{
    Milestone, MilestoneId, MilestoneStatus, Plan, PlanDependency, PlanId, PlanStatus, SessionId,
};
use serde::{Deserialize, Serialize};

/// Output produced by the milestone prompt parser.
#[derive(Debug, Clone, PartialEq)]
pub struct MilestonePromptResult {
    pub milestone: Milestone,
    pub plans: Vec<Plan>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PromptPlanSpec {
    title: String,
    description: String,
    #[serde(default)]
    file_scope_estimate: Option<String>,
    #[serde(default)]
    depends_on: Vec<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PromptMilestoneResponse {
    title: String,
    description: String,
    #[serde(default)]
    success_criteria: Vec<String>,
    #[serde(default)]
    validation_command: Option<String>,
    plans: Vec<PromptPlanSpec>,
}

/// Build a decomposition prompt that asks for multiple plans inside one milestone.
pub fn build_milestone_prompt(task_description: &str, context: &str) -> String {
    let context_section = if context.trim().is_empty() {
        String::new()
    } else {
        format!("\n\nContext:\n{context}")
    };

    format!(
        "You are decomposing a large feature into a single milestone with multiple dependent plans.\n\
         Return only JSON with this shape:\n\
         {{\n\
         \"title\": \"short milestone title\",\n\
         \"description\": \"what this milestone accomplishes\",\n\
         \"success_criteria\": [\"criterion 1\", \"criterion 2\"],\n\
         \"validation_command\": \"optional shell command\",\n\
         \"plans\": [\n\
         {{\n\
         \"title\": \"plan title\",\n\
         \"description\": \"plan description\",\n\
         \"file_scope_estimate\": \"optional file scope estimate\",\n\
         \"depends_on\": [0]\n\
         }}\n\
         ]\n\
         }}\n\
         \n\
         Rules:\n\
         - Produce 3-6 plans for large features.\n\
         - Use dependency indices to express ordering.\n\
         - Include a file scope estimate in each plan when useful.\n\
         - Keep the milestone focused on a single feature slice.\n\
         \n\
         Task:\n\
         {task_description}{context_section}"
    )
}

/// Parse the model response into a milestone and empty plan shells.
pub fn parse_milestone_response(
    response: &str,
    session_id: SessionId,
) -> Result<MilestonePromptResult> {
    let payload = extract_json_payload(response);
    let parsed: PromptMilestoneResponse = serde_json::from_str(payload)
        .with_context(|| "failed to parse milestone response as JSON")?;

    let milestone_id = MilestoneId::new();
    let now = Utc::now();
    let mut plans = Vec::with_capacity(parsed.plans.len());
    let mut plan_ids = Vec::with_capacity(parsed.plans.len());

    for spec in &parsed.plans {
        let plan_id = PlanId::new();
        plan_ids.push(plan_id.clone());
        let summary = match spec.file_scope_estimate.as_deref() {
            Some(scope) => format!("{} ({scope})", spec.description),
            None => spec.description.clone(),
        };
        plans.push(Plan {
            id: plan_id,
            session_id: session_id.clone(),
            milestone_id: Some(milestone_id.clone()),
            task: format!("{}: {}", parsed.title, spec.title),
            created_at: now.clone(),
            status: PlanStatus::Draft,
            summary: summary.clone(),
            approach: summary,
            steps: vec![],
            files_to_modify: vec![],
            risks: vec![],
            current_step_index: None,
            execution_started_at: None,
            execution_completed_at: None,
            execution_error: None,
            task_profile: None,
        });
    }

    let plan_dependencies = parsed
        .plans
        .iter()
        .enumerate()
        .map(|(index, spec)| PlanDependency {
            plan_id: plan_ids[index].clone(),
            depends_on: spec
                .depends_on
                .iter()
                .filter_map(|dep_index| plan_ids.get(*dep_index).cloned())
                .collect(),
        })
        .collect();

    let milestone = Milestone {
        id: milestone_id,
        session_id,
        title: parsed.title,
        description: parsed.description,
        status: MilestoneStatus::Draft,
        plan_ids,
        plan_dependencies,
        success_criteria: parsed.success_criteria,
        validation_command: parsed.validation_command,
        created_at: now.clone(),
        updated_at: now,
        completed_at: None,
    };

    Ok(MilestonePromptResult { milestone, plans })
}

fn extract_json_payload(response: &str) -> &str {
    let trimmed = response.trim();
    if let Some(start) = trimmed.find("```json") {
        let after = &trimmed[start + "```json".len()..];
        if let Some(end) = after.find("```") {
            return after[..end].trim();
        }
    }
    if let Some(start) = trimmed.find("```") {
        let after = &trimmed[start + "```".len()..];
        if let Some(end) = after.find("```") {
            return after[..end].trim();
        }
    }
    let start = trimmed.find('{').unwrap_or(0);
    let end = trimmed
        .rfind('}')
        .map(|idx| idx + 1)
        .unwrap_or(trimmed.len());
    &trimmed[start..end]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_prompt_mentions_dependencies() {
        let prompt = build_milestone_prompt("Add auth", "Use existing middleware");
        assert!(prompt.contains("dependency indices"));
        assert!(prompt.contains("Add auth"));
        assert!(prompt.contains("Use existing middleware"));
    }

    #[test]
    fn parse_response_builds_shells() {
        let response = r#"{
            "title": "Auth milestone",
            "description": "Split auth work",
            "success_criteria": ["Login works"],
            "validation_command": "cargo test",
            "plans": [
                {
                    "title": "Research",
                    "description": "Look at patterns",
                    "file_scope_estimate": "2 files",
                    "depends_on": []
                },
                {
                    "title": "Implement",
                    "description": "Build module",
                    "depends_on": [0]
                }
            ]
        }"#;
        let parsed = parse_milestone_response(response, SessionId::new()).unwrap();
        assert_eq!(parsed.milestone.plan_ids.len(), 2);
        assert_eq!(parsed.plans.len(), 2);
        assert_eq!(parsed.plans[0].status, PlanStatus::Draft);
        assert!(parsed.plans[0].milestone_id.is_some());
        assert_eq!(parsed.milestone.success_criteria, vec!["Login works"]);
    }
}
