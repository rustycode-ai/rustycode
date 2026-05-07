//! Two-phase architect→coder pipeline for cost-efficient code generation.
//!
//! Phase 1 (Design): Expensive model analyzes the task and produces a structured plan.
//! Phase 2 (Apply): Cheap model takes the plan and makes the actual code changes.
//!
//! This is the Aider "architect mode" pattern — splitting work into design and
//! execution phases so you only pay for expensive reasoning when planning, and use
//! a cheaper model for the mechanical edit operations.

use rustycode_llm::{ChatMessage, CompletionRequest, LLMProvider};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::{debug, info};

// Configuration

/// Configuration for the architect→coder model pair.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArchitectConfig {
    /// Model used for the design phase (e.g., "claude-opus-4-6").
    pub architect_model: String,
    /// Model used for the apply phase (e.g., "claude-sonnet-4-6").
    pub coder_model: String,
    /// Maximum tokens for the architect's plan.
    pub max_plan_tokens: u32,
    /// Maximum tokens for the coder's implementation.
    pub max_apply_tokens: u32,
}

impl Default for ArchitectConfig {
    fn default() -> Self {
        Self {
            architect_model: "claude-sonnet-4-6".to_string(),
            coder_model: "claude-haiku-4-5-20251001".to_string(),
            max_plan_tokens: 4096,
            max_apply_tokens: 8192,
        }
    }
}

// Plan types

/// A structured plan produced by the architect phase.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArchitectPlan {
    /// Brief summary of what needs to be done.
    pub task_summary: String,
    /// Individual file-level changes to make.
    pub file_plans: Vec<FilePlan>,
    /// Additional context or constraints the coder should know.
    pub notes: Vec<String>,
}

/// Planned changes for a single file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilePlan {
    /// Path relative to project root.
    pub path: String,
    /// What kind of change is needed.
    pub change_type: PlannedChange,
    /// Natural language description of the change.
    pub description: String,
}

/// The type of change planned for a file.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[non_exhaustive]
pub enum PlannedChange {
    /// Create a new file.
    Create,
    /// Modify an existing file.
    Modify,
    /// Delete a file.
    Delete,
    /// Rename or move a file.
    Rename,
}

// Result

/// Result of the full architect→coder pipeline.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArchitectResult {
    /// The plan produced by the architect phase.
    pub plan: ArchitectPlan,
    /// The raw output from the coder phase.
    pub coder_output: String,
    /// Model used for the architect phase.
    pub architect_model: String,
    /// Model used for the coder phase.
    pub coder_model: String,
    /// Token usage from both phases (architect, coder).
    pub usage: Option<(UsageStats, UsageStats)>,
}

/// Simplified token usage.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageStats {
    pub input_tokens: u32,
    pub output_tokens: u32,
}

impl From<&rustycode_llm::Usage> for UsageStats {
    fn from(u: &rustycode_llm::Usage) -> Self {
        Self {
            input_tokens: u.input_tokens,
            output_tokens: u.output_tokens,
        }
    }
}

// Executor

/// The architect mode executor.
///
/// Coordinates the two-phase design→apply pipeline using the configured
/// model pair.
pub struct ArchitectMode {
    config: ArchitectConfig,
    provider: Arc<dyn LLMProvider>,
}

impl ArchitectMode {
    pub fn new(config: ArchitectConfig, provider: Arc<dyn LLMProvider>) -> Self {
        Self { config, provider }
    }

    /// Create with default configuration.
    pub fn with_defaults(provider: Arc<dyn LLMProvider>) -> Self {
        Self {
            config: ArchitectConfig::default(),
            provider,
        }
    }

    // -------------------------------------------------------------------
    // Public API
    // -------------------------------------------------------------------

    /// Run the full two-phase pipeline: design then apply.
    pub async fn execute(&self, task: &str, context: &str) -> Result<ArchitectResult, String> {
        info!(
            "Architect mode: designing with {}, applying with {}",
            self.config.architect_model, self.config.coder_model
        );

        // Phase 1: Design
        let plan = self.design(task, context).await?;
        debug!(
            "Architect plan: {} file(s) to change",
            plan.file_plans.len()
        );

        // Phase 2: Apply
        let (coder_output, _coder_usage) = self.apply_raw(&plan).await?;
        debug!("Coder output: {} chars", coder_output.len());

        Ok(ArchitectResult {
            plan,
            coder_output,
            architect_model: self.config.architect_model.clone(),
            coder_model: self.config.coder_model.clone(),
            usage: None, // Will be populated when providers return usage
        })
    }

    /// Phase 1: Send the task to the architect model and get a structured plan.
    pub async fn design(&self, task: &str, context: &str) -> Result<ArchitectPlan, String> {
        let system_prompt = r#"You are an architect. Analyze the task and produce a structured plan.
Output your plan in this exact JSON format:
{
  "task_summary": "Brief summary of what to do",
  "file_plans": [
    {
      "path": "src/path/to/file.rs",
      "change_type": "Modify",
      "description": "What to change in this file"
    }
  ],
  "notes": ["Any additional context"]
}

Change types: Create, Modify, Delete, Rename
Be specific about what each file change should accomplish."#;

        let user_message = if context.is_empty() {
            format!("Task: {}", task)
        } else {
            format!("Context:\n{}\n\nTask: {}", context, task)
        };

        let request = CompletionRequest::new(
            self.config.architect_model.clone(),
            vec![
                ChatMessage::system(system_prompt.to_string()),
                ChatMessage::user(user_message),
            ],
        )
        .with_max_tokens(self.config.max_plan_tokens);

        let response = self
            .provider
            .complete(request)
            .await
            .map_err(|e| format!("Architect phase failed: {}", e))?;

        parse_plan_from_response(&response.content)
    }

    /// Phase 2: Send the plan to the coder model and get implementation output.
    async fn apply_raw(
        &self,
        plan: &ArchitectPlan,
    ) -> Result<(String, Option<UsageStats>), String> {
        let plan_json = serde_json::to_string_pretty(plan)
            .map_err(|e| format!("Failed to serialize plan: {}", e))?;

        let system_prompt = r#"You are a coder. You receive a structured plan from an architect.
Implement the plan by writing the actual code changes.
For each file in the plan, provide the complete new content or the specific edits needed.
Use standard diff or search-replace format for modifications."#;

        let request = CompletionRequest::new(
            self.config.coder_model.clone(),
            vec![
                ChatMessage::system(system_prompt.to_string()),
                ChatMessage::user(format!("Plan:\n{}\n\nImplement these changes.", plan_json)),
            ],
        )
        .with_max_tokens(self.config.max_apply_tokens);

        let response = self
            .provider
            .complete(request)
            .await
            .map_err(|e| format!("Coder phase failed: {}", e))?;

        let usage = response.usage.as_ref().map(UsageStats::from);

        Ok((response.content, usage))
    }

    /// Get the current configuration.
    pub fn config(&self) -> &ArchitectConfig {
        &self.config
    }
}

// Plan parsing

/// Parse an ArchitectPlan from the LLM response text.
fn parse_plan_from_response(content: &str) -> Result<ArchitectPlan, String> {
    // Try to extract JSON from the response (may be wrapped in markdown)
    let json_str = extract_json(content)?;

    let preview_len = json_str.len().min(200);
    let preview_end = json_str.floor_char_boundary(preview_len);
    serde_json::from_str(&json_str).map_err(|e| {
        format!(
            "Failed to parse architect plan: {}\nJSON: {}",
            e,
            &json_str[..preview_end]
        )
    })
}

/// Extract JSON from a response that may contain markdown fences.
fn extract_json(content: &str) -> Result<String, String> {
    let trimmed = content.trim();

    // Case 1: Direct JSON
    if trimmed.starts_with('{') {
        return Ok(trimmed.to_string());
    }

    // Case 2: Inside markdown code fence
    if let Some(start) = trimmed.find("```json") {
        let json_start = start + 7;
        if let Some(end) = trimmed[json_start..].find("```") {
            return Ok(trimmed[json_start..json_start + end].trim().to_string());
        }
    }

    // Case 3: Inside generic code fence
    if let Some(start) = trimmed.find("```") {
        let json_start = start + 3;
        // Skip optional language tag
        let json_start = trimmed[json_start..]
            .find('\n')
            .map_or(json_start, |i| json_start + i + 1);
        if let Some(end) = trimmed[json_start..].find("```") {
            let extracted = trimmed[json_start..json_start + end].trim();
            if extracted.starts_with('{') {
                return Ok(extracted.to_string());
            }
        }
    }

    // Case 4: Find first { ... } block
    if let Some(start) = trimmed.find('{') {
        if let Some(end) = trimmed.rfind('}') {
            if end > start {
                return Ok(trimmed[start..=end].to_string());
            }
        }
    }

    Err("Could not extract JSON plan from architect response".to_string())
}

// Tests

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_architect_config_default() {
        let config = ArchitectConfig::default();
        assert!(!config.architect_model.is_empty());
        assert!(!config.coder_model.is_empty());
        assert!(config.max_plan_tokens > 0);
        assert!(config.max_apply_tokens > 0);
    }

    #[test]
    fn test_planned_change_variants() {
        assert_ne!(PlannedChange::Create, PlannedChange::Modify);
        assert_ne!(PlannedChange::Modify, PlannedChange::Delete);
        assert_ne!(PlannedChange::Delete, PlannedChange::Rename);
    }

    #[test]
    fn test_file_plan_serialization() {
        let plan = FilePlan {
            path: "src/main.rs".to_string(),
            change_type: PlannedChange::Modify,
            description: "Add error handling".to_string(),
        };
        let json = serde_json::to_string(&plan).unwrap();
        let back: FilePlan = serde_json::from_str(&json).unwrap();
        assert_eq!(plan.path, back.path);
        assert_eq!(plan.change_type, back.change_type);
    }

    #[test]
    fn test_architect_plan_serialization() {
        let plan = ArchitectPlan {
            task_summary: "Fix the login bug".to_string(),
            file_plans: vec![
                FilePlan {
                    path: "src/auth.rs".to_string(),
                    change_type: PlannedChange::Modify,
                    description: "Fix null check".to_string(),
                },
                FilePlan {
                    path: "src/auth_test.rs".to_string(),
                    change_type: PlannedChange::Create,
                    description: "Add regression test".to_string(),
                },
            ],
            notes: vec!["Keep backward compatibility".to_string()],
        };

        let json = serde_json::to_string_pretty(&plan).unwrap();
        let back: ArchitectPlan = serde_json::from_str(&json).unwrap();
        assert_eq!(back.file_plans.len(), 2);
        assert_eq!(back.notes.len(), 1);
    }

    #[test]
    fn test_extract_json_direct() {
        let content = r#"{"task_summary": "test", "file_plans": [], "notes": []}"#;
        let json = extract_json(content).unwrap();
        assert!(json.starts_with('{'));
    }

    #[test]
    fn test_extract_json_markdown_fenced() {
        let content = "Here's my plan:\n```json\n{\"task_summary\": \"test\", \"file_plans\": [], \"notes\": []}\n```\nDone.";
        let json = extract_json(content).unwrap();
        let plan: ArchitectPlan = serde_json::from_str(&json).unwrap();
        assert_eq!(plan.task_summary, "test");
    }

    #[test]
    fn test_extract_json_generic_fence() {
        let content = "```\n{\"task_summary\": \"hello\", \"file_plans\": [], \"notes\": []}\n```";
        let json = extract_json(content).unwrap();
        let plan: ArchitectPlan = serde_json::from_str(&json).unwrap();
        assert_eq!(plan.task_summary, "hello");
    }

    #[test]
    fn test_extract_json_embedded() {
        let content = "I think we should do this:\n{\"task_summary\": \"fix\", \"file_plans\": [], \"notes\": []}\nThat should work.";
        let json = extract_json(content).unwrap();
        assert!(json.contains("fix"));
    }

    #[test]
    fn test_extract_json_failure() {
        let content = "No JSON here, just plain text.";
        assert!(extract_json(content).is_err());
    }

    #[test]
    fn test_parse_plan_from_response_valid() {
        let content = r#"```json
{
    "task_summary": "Add caching layer",
    "file_plans": [
        {
            "path": "src/cache.rs",
            "change_type": "Create",
            "description": "New cache module with LRU eviction"
        }
    ],
    "notes": ["Use tokio::sync::RwLock"]
}
```"#;
        let plan = parse_plan_from_response(content).unwrap();
        assert_eq!(plan.task_summary, "Add caching layer");
        assert_eq!(plan.file_plans.len(), 1);
        assert_eq!(plan.file_plans[0].path, "src/cache.rs");
        assert_eq!(plan.file_plans[0].change_type, PlannedChange::Create);
    }

    #[test]
    fn test_usage_stats_from_llm_usage() {
        let usage = rustycode_llm::Usage {
            input_tokens: 100,
            output_tokens: 200,
            total_tokens: 300,
            cache_creation_input_tokens: 0,
            cache_read_input_tokens: 0,
            reasoning_tokens: None,
        };
        let stats = UsageStats::from(&usage);
        assert_eq!(stats.input_tokens, 100);
        assert_eq!(stats.output_tokens, 200);
    }

    #[test]
    fn test_architect_result_construction() {
        let result = ArchitectResult {
            plan: ArchitectPlan {
                task_summary: "test".to_string(),
                file_plans: vec![],
                notes: vec![],
            },
            coder_output: "done".to_string(),
            architect_model: "claude-opus".to_string(),
            coder_model: "claude-haiku".to_string(),
            usage: None,
        };
        assert_eq!(result.architect_model, "claude-opus");
        assert!(result.usage.is_none());
    }

    #[test]
    fn test_architect_config_serialization() {
        let config = ArchitectConfig::default();
        let json = serde_json::to_string(&config).unwrap();
        let back: ArchitectConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(config.architect_model, back.architect_model);
        assert_eq!(config.coder_model, back.coder_model);
    }

    // =========================================================================
    // Terminal-bench: 15 additional tests for architect_mode
    // =========================================================================

    // 1. ArchitectConfig custom values serde roundtrip
    #[test]
    fn architect_config_custom_serde_roundtrip() {
        let config = ArchitectConfig {
            architect_model: "gpt-4".into(),
            coder_model: "gpt-3.5-turbo".into(),
            max_plan_tokens: 2048,
            max_apply_tokens: 4096,
        };
        let json = serde_json::to_string(&config).unwrap();
        let decoded: ArchitectConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.architect_model, "gpt-4");
        assert_eq!(decoded.coder_model, "gpt-3.5-turbo");
        assert_eq!(decoded.max_plan_tokens, 2048);
        assert_eq!(decoded.max_apply_tokens, 4096);
    }

    // 2. ArchitectPlan with empty file_plans serde roundtrip
    #[test]
    fn architect_plan_empty_file_plans_serde() {
        let plan = ArchitectPlan {
            task_summary: "Empty plan".into(),
            file_plans: vec![],
            notes: vec!["No changes needed".into()],
        };
        let json = serde_json::to_string(&plan).unwrap();
        let decoded: ArchitectPlan = serde_json::from_str(&json).unwrap();
        assert!(decoded.file_plans.is_empty());
        assert_eq!(decoded.notes.len(), 1);
    }

    // 3. FilePlan with all PlannedChange variants serde
    #[test]
    fn file_plan_all_change_types_serde() {
        let change_types = [
            PlannedChange::Create,
            PlannedChange::Modify,
            PlannedChange::Delete,
            PlannedChange::Rename,
        ];
        for ct in &change_types {
            let plan = FilePlan {
                path: "src/test.rs".into(),
                change_type: *ct,
                description: "Test change".into(),
            };
            let json = serde_json::to_string(&plan).unwrap();
            let decoded: FilePlan = serde_json::from_str(&json).unwrap();
            assert_eq!(decoded.change_type, *ct);
        }
    }

    // 4. ArchitectResult with full usage serde roundtrip
    #[test]
    fn architect_result_with_usage_serde_roundtrip() {
        let result = ArchitectResult {
            plan: ArchitectPlan {
                task_summary: "Refactor module".into(),
                file_plans: vec![FilePlan {
                    path: "src/lib.rs".into(),
                    change_type: PlannedChange::Modify,
                    description: "Split into submodules".into(),
                }],
                notes: vec![],
            },
            coder_output: "Applied changes successfully".into(),
            architect_model: "claude-opus-4-6".into(),
            coder_model: "claude-haiku-4-5-20251001".into(),
            usage: Some((
                UsageStats {
                    input_tokens: 500,
                    output_tokens: 200,
                },
                UsageStats {
                    input_tokens: 300,
                    output_tokens: 800,
                },
            )),
        };
        let json = serde_json::to_string(&result).unwrap();
        let decoded: ArchitectResult = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.plan.file_plans.len(), 1);
        assert!(decoded.usage.is_some());
        let (arch, coder) = decoded.usage.unwrap();
        assert_eq!(arch.input_tokens, 500);
        assert_eq!(coder.output_tokens, 800);
    }

    // 5. UsageStats serde roundtrip
    #[test]
    fn usage_stats_serde_roundtrip() {
        let stats = UsageStats {
            input_tokens: 1000,
            output_tokens: 500,
        };
        let json = serde_json::to_string(&stats).unwrap();
        let decoded: UsageStats = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.input_tokens, 1000);
        assert_eq!(decoded.output_tokens, 500);
    }

    // 6. parse_plan_from_response with direct JSON
    #[test]
    fn parse_plan_direct_json() {
        let content = r#"{"task_summary":"Direct","file_plans":[{"path":"a.rs","change_type":"Create","description":"New file"}],"notes":[]}"#;
        let plan = parse_plan_from_response(content).unwrap();
        assert_eq!(plan.task_summary, "Direct");
        assert_eq!(plan.file_plans[0].change_type, PlannedChange::Create);
    }

    // 7. parse_plan_from_response with invalid JSON returns error
    #[test]
    fn parse_plan_invalid_json_returns_error() {
        let content = r#"{"task_summary": "good", "file_plans": [invalid]}"#;
        assert!(parse_plan_from_response(content).is_err());
    }

    // 8. extract_json with nested curly braces picks outermost braces
    #[test]
    fn extract_json_nested_braces() {
        let content = r#"Here's the plan: {"task_summary": "test", "file_plans": [{"path": "a.rs", "change_type": "Create", "description": "New"}], "notes": []} done"#;
        let json = extract_json(content).unwrap();
        let plan: ArchitectPlan = serde_json::from_str(&json).unwrap();
        assert_eq!(plan.task_summary, "test");
    }

    // 9. ArchitectConfig default values are sensible
    #[test]
    fn architect_config_default_values() {
        let config = ArchitectConfig::default();
        assert_eq!(config.max_plan_tokens, 4096);
        assert_eq!(config.max_apply_tokens, 8192);
        assert!(!config.architect_model.is_empty());
        assert!(!config.coder_model.is_empty());
    }

    // 10. PlannedChange serde roundtrip for all variants
    #[test]
    fn planned_change_serde_roundtrip() {
        let variants = [
            PlannedChange::Create,
            PlannedChange::Modify,
            PlannedChange::Delete,
            PlannedChange::Rename,
        ];
        for v in &variants {
            let json = serde_json::to_string(v).unwrap();
            let decoded: PlannedChange = serde_json::from_str(&json).unwrap();
            assert_eq!(*v, decoded);
        }
    }

    // 11. ArchitectPlan with multiple notes serde roundtrip
    #[test]
    fn architect_plan_with_multiple_notes_serde() {
        let plan = ArchitectPlan {
            task_summary: "Big refactor".into(),
            file_plans: vec![
                FilePlan {
                    path: "src/a.rs".into(),
                    change_type: PlannedChange::Modify,
                    description: "Update a".into(),
                },
                FilePlan {
                    path: "src/b.rs".into(),
                    change_type: PlannedChange::Delete,
                    description: "Remove b".into(),
                },
                FilePlan {
                    path: "src/c.rs".into(),
                    change_type: PlannedChange::Create,
                    description: "Add c".into(),
                },
            ],
            notes: vec![
                "Keep backward compat".into(),
                "Update tests".into(),
                "Run CI before merge".into(),
            ],
        };
        let json = serde_json::to_string(&plan).unwrap();
        let decoded: ArchitectPlan = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.file_plans.len(), 3);
        assert_eq!(decoded.notes.len(), 3);
    }

    // 12. extract_json with no opening brace returns error
    #[test]
    fn extract_json_no_brace_returns_error() {
        let content = "just some text without any json";
        assert!(extract_json(content).is_err());
    }

    // 13. ArchitectResult with None usage serde roundtrip
    #[test]
    fn architect_result_no_usage_serde() {
        let result = ArchitectResult {
            plan: ArchitectPlan {
                task_summary: "Simple".into(),
                file_plans: vec![],
                notes: vec![],
            },
            coder_output: "done".into(),
            architect_model: "model-a".into(),
            coder_model: "model-b".into(),
            usage: None,
        };
        let json = serde_json::to_string(&result).unwrap();
        let decoded: ArchitectResult = serde_json::from_str(&json).unwrap();
        assert!(decoded.usage.is_none());
        assert_eq!(decoded.coder_output, "done");
    }

    // 14. FilePlan with Rename change_type serde roundtrip
    #[test]
    fn file_plan_rename_serde() {
        let plan = FilePlan {
            path: "src/new_name.rs".into(),
            change_type: PlannedChange::Rename,
            description: "Rename from old_name.rs".into(),
        };
        let json = serde_json::to_string(&plan).unwrap();
        let decoded: FilePlan = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.change_type, PlannedChange::Rename);
        assert_eq!(decoded.path, "src/new_name.rs");
    }

    // 15. extract_json with only whitespace returns error
    #[test]
    fn extract_json_whitespace_returns_error() {
        assert!(extract_json("   \n\t  ").is_err());
    }
}
