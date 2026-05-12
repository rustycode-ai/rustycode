//! Plan generation using LLM providers and template fallbacks.

use anyhow::{Context, Result};
use chrono::Utc;
use rustycode_protocol::{Plan, PlanId, PlanStatus, PlanStep, SessionId, StepStatus};

/// Render a plan as markdown for human review.
pub fn render_plan_markdown(plan: &Plan) -> String {
    let mut out = String::new();
    out.push_str(&format!("# Plan: {}\n\n", plan.task));
    out.push_str(&format!("**Session:** `{}`  \n", plan.session_id));
    out.push_str(&format!("**Plan ID:** `{}`  \n", plan.id));
    out.push_str(&format!("**Status:** `{:?}`\n\n", plan.status));
    out.push_str("## Summary\n\n");
    out.push_str(&format!("{}\n\n", plan.summary));
    out.push_str("## Approach\n\n");
    if plan.approach.trim().is_empty() {
        out.push_str("<!-- Describe your approach here -->\n\n");
    } else {
        out.push_str(&format!("{}\n\n", plan.approach));
    }
    out.push_str("## Steps\n\n");
    if plan.steps.is_empty() {
        out.push_str("No steps defined.\n\n");
    } else {
        for step in &plan.steps {
            out.push_str(&format!("### {}. {}\n\n", step.order, step.title));
            out.push_str(&format!("{}\n\n", step.description));
            out.push_str(&format!("**Tools:** {}\n\n", step.tools.join(", ")));
            out.push_str(&format!(
                "**Expected outcome:** {}\n\n",
                step.expected_outcome
            ));
            out.push_str(&format!("**Rollback:** {}\n\n", step.rollback_hint));
        }
    }
    out.push_str("## Files to Modify\n\n");
    if plan.files_to_modify.is_empty() {
        out.push_str("<!-- List files that will change, one per line -->\n\n");
    } else {
        for path in &plan.files_to_modify {
            out.push_str(&format!("- {}\n", path));
        }
        out.push('\n');
    }
    out.push_str("## Risks\n\n");
    if plan.risks.is_empty() {
        out.push_str("<!-- List potential issues or caveats -->\n\n");
    } else {
        for risk in &plan.risks {
            out.push_str(&format!("- {}\n", risk));
        }
        out.push('\n');
    }
    out.push_str("---\n");
    out.push_str("*Edit this file, then run `rustycode plan approve <session-id>` to execute.*\n");
    out
}

/// Async implementation: generate a plan using an LLM provider
pub async fn generate_plan_with_llm_async(
    provider: &dyn rustycode_llm::provider::LLMProvider,
    task: &str,
    available_tools: &[&str],
) -> Result<Plan> {
    use rustycode_llm::provider::{ChatMessage, CompletionRequest};

    let tools_str = available_tools.join(", ");
    let prompt = format!(
        r#"You are a coding assistant. Generate a plan to accomplish the following task:

Task: {}

Available tools: {}

Respond in JSON format with the following structure:
{{
    "summary": "Brief summary of the plan",
    "approach": "High-level approach description",
    "steps": [
        {{
            "title": "Step title",
            "description": "What this step does",
            "tools": ["tool1", "tool2"],
            "expected_outcome": "What this step achieves",
            "rollback_hint": "How to undo (or N/A)"
        }}
    ],
    "files_to_modify": ["file1.rs", "file2.rs"],
    "risks": ["risk1", "risk2"]
}}

Generate a practical, actionable plan with 2-5 steps. Each step should use appropriate tools from the available list."#,
        task, tools_str
    );

    // Convert the prompt to a CompletionRequest
    let request =
        CompletionRequest::new("default-model".to_string(), vec![ChatMessage::user(prompt)]);

    // Async retry loop using tokio sleep
    #[allow(unused_assignments)]
    let mut last_err: Option<anyhow::Error> = None;
    let mut attempt = 0usize;
    let max_attempts = 3usize;
    let mut backoff = std::time::Duration::from_millis(200);

    let response = loop {
        attempt += 1;
        match provider.complete(request.clone()).await {
            Ok(resp) => break resp,
            Err(e) => {
                last_err = Some(anyhow::anyhow!("{}", e));
                if attempt >= max_attempts {
                    let err = last_err.unwrap_or_else(|| {
                        anyhow::anyhow!("LLM provider failed after retries (no error captured)")
                    });
                    return Err(err.context("LLM provider failed after retries"));
                }
                crate::sleep::hybrid_sleep(backoff).await;
                backoff = std::cmp::min(backoff * 2, std::time::Duration::from_secs(5));
                continue;
            }
        }
    };

    let content = response.content;

    // Try to parse JSON from response
    let json: serde_json::Value = serde_json::from_str(&content)
        .or_else(|_| {
            // Try extracting JSON from markdown code block
            if let Some(start) = content.find("```json") {
                if let Some(end) = content[start + 7..].find("```") {
                    let json_str = &content[start + 7..start + 7 + end];
                    serde_json::from_str(json_str)
                } else {
                    serde_json::from_str(&content)
                }
            } else {
                serde_json::from_str(&content)
            }
        })
        .context("Failed to parse LLM response as JSON")?;

    // Extract plan from JSON
    let summary = json
        .get("summary")
        .and_then(|v| v.as_str())
        .unwrap_or(task)
        .to_string();

    let approach = json
        .get("approach")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let files_to_modify: Vec<String> = json
        .get("files_to_modify")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();

    let risks: Vec<String> = json
        .get("risks")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();

    let steps: Vec<PlanStep> = json
        .get("steps")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .enumerate()
                .map(|(i, step)| PlanStep {
                    order: i,
                    title: step
                        .get("title")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                    description: step
                        .get("description")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                    tools: step
                        .get("tools")
                        .and_then(|v| v.as_array())
                        .map(|arr| {
                            arr.iter()
                                .filter_map(|v| v.as_str().map(String::from))
                                .collect()
                        })
                        .unwrap_or_default(),
                    expected_outcome: step
                        .get("expected_outcome")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                    rollback_hint: step
                        .get("rollback_hint")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                    execution_status: StepStatus::default(),
                    tool_calls: vec![],
                    tool_executions: vec![],
                    results: vec![],
                    errors: vec![],
                    started_at: None,
                    completed_at: None,
                })
                .collect()
        })
        .unwrap_or_else(|| {
            vec![PlanStep {
                order: 0,
                title: "Explore codebase".to_string(),
                description: "Use available tools to understand the codebase.".to_string(),
                tools: vec![
                    "Read".to_string(),
                    "Grep".to_string(),
                    "ListDir".to_string(),
                ],
                expected_outcome: "Understand the codebase structure.".to_string(),
                rollback_hint: "N/A — read-only step.".to_string(),
                execution_status: StepStatus::default(),
                tool_calls: vec![],
                tool_executions: vec![],
                results: vec![],
                errors: vec![],
                started_at: None,
                completed_at: None,
            }]
        });

    Ok(Plan {
        id: PlanId::new(),
        session_id: SessionId::new(),
        task: task.to_string(),
        created_at: Utc::now(),
        status: PlanStatus::Draft,
        summary,
        approach,
        steps,
        files_to_modify,
        risks,
        current_step_index: None,
        execution_started_at: None,
        execution_completed_at: None,
        execution_error: None,
        task_profile: None,
        milestone_id: None,
    })
}

/// Synchronous wrapper for plan generation with LLM.
pub fn generate_plan_with_llm(
    provider: &dyn rustycode_llm::provider::LLMProvider,
    task: &str,
    available_tools: &[&str],
) -> Result<Plan> {
    crate::shared_runtime::block_on_shared(generate_plan_with_llm_async(
        provider,
        task,
        available_tools,
    ))
}

/// Generate a plan from user task, optionally using an LLM provider.
/// Falls back to template if LLM is unavailable or fails.
pub fn generate_smart_plan(
    task: &str,
    available_tools: &[&str],
    provider: Option<&dyn rustycode_llm::provider::LLMProvider>,
) -> Plan {
    crate::shared_runtime::block_on_shared(generate_smart_plan_async(
        task,
        available_tools,
        provider,
    ))
}

pub async fn generate_smart_plan_async(
    task: &str,
    available_tools: &[&str],
    provider: Option<&dyn rustycode_llm::provider::LLMProvider>,
) -> Plan {
    // Try to generate plan with LLM if available
    if let Some(p) = provider {
        match generate_plan_with_llm_async(p, task, available_tools).await {
            Ok(plan) => return plan,
            Err(e) => tracing::warn!("LLM plan generation failed: {}", e),
        }
    }

    // Fall back to template-based plan
    let steps = vec![PlanStep {
        order: 0,
        title: "Explore codebase".to_string(),
        description: "Use read_file, grep, and list_dir to understand the relevant code."
            .to_string(),
        tools: vec![
            "Read".to_string(),
            "Grep".to_string(),
            "ListDir".to_string(),
        ],
        expected_outcome: "Understand the files that need to change.".to_string(),
        rollback_hint: "N/A — read-only step.".to_string(),
        execution_status: StepStatus::default(),
        tool_calls: vec![],
        tool_executions: vec![],
        results: vec![],
        errors: vec![],
        started_at: None,
        completed_at: None,
    }];

    Plan {
        id: PlanId::new(),
        session_id: SessionId::new(),
        task: task.to_string(),
        created_at: Utc::now(),
        status: PlanStatus::Draft,
        summary: format!("Plan for: {}", task),
        approach: String::new(),
        steps,
        files_to_modify: vec![],
        risks: vec![],
        current_step_index: None,
        execution_started_at: None,
        execution_completed_at: None,
        execution_error: None,
        task_profile: None,
        milestone_id: None,
    }
}
