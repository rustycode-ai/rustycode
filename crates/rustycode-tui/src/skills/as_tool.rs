//! Skill-as-Tool wrapper
//!
//! This module provides a wrapper that exposes skills as LLM-callable tools,
//! enabling natural language invocation like Claude Code.
//!
//! ## Architecture
//!
//! When a skill is exposed as a tool:
//! 1. Skill metadata (name, description, parameters) is converted to JSON Schema
//! 2. LLM can call the skill like any built-in tool (e.g., read_file, bash)
//! 3. On execution, the skill's instructions are injected into the conversation
//! 4. The skill's commands are executed with provided parameters
//!
//! ## Example
//!
//! When user says "review this code for security issues":
//! - LLM recognizes this matches code-review skill description
//! - LLM calls tool: { name: "skill_code_review", input: { target: "src/main.rs" } }
//! - Tool execution injects skill instructions and runs the skill's commands
//! - Result is returned to LLM for continuation

use anyhow::Result;
use rustycode_tools::{Tool, ToolContext, ToolOutput};
use serde_json::{json, Value};
use std::sync::{Arc, RwLock};

use crate::skills::{Skill, SkillStateManager};

/// Wrapper that exposes a skill as an LLM-callable tool
pub struct SkillAsTool {
    skill: Skill,
    state_manager: Arc<RwLock<SkillStateManager>>,
    tool_name: String,
}

impl SkillAsTool {
    pub fn new(skill: Skill, state_manager: Arc<RwLock<SkillStateManager>>) -> Self {
        let tool_name = format!("skill_{}", skill.name.replace('-', "_"));
        Self {
            skill,
            state_manager,
            tool_name,
        }
    }

    fn build_parameters_schema(&self) -> Value {
        let mut properties = json!({});
        let mut required = Vec::new();

        for param in &self.skill.parameters {
            let param_schema = match param.param_type {
                crate::skills::loader::ParamType::Text => json!({
                    "type": "string",
                    "description": param.description
                }),
                crate::skills::loader::ParamType::File => json!({
                    "type": "string",
                    "description": format!("{} (file path)", param.description)
                }),
                crate::skills::loader::ParamType::Directory => json!({
                    "type": "string",
                    "description": format!("{} (directory path)", param.description)
                }),
                crate::skills::loader::ParamType::Number => json!({
                    "type": "number",
                    "description": param.description
                }),
            };

            if let Some(obj) = properties.as_object_mut() {
                obj.insert(param.name.clone(), param_schema);
            }

            if param.required {
                required.push(param.name.clone());
            }
        }

        // If no parameters defined, create a generic "input" parameter
        if self.skill.parameters.is_empty() {
            json!({
                "type": "object",
                "properties": {
                    "input": {
                        "type": "string",
                        "description": "Input or context for the skill"
                    }
                },
                "required": ["input"]
            })
        } else {
            json!({
                "type": "object",
                "properties": properties,
                "required": required
            })
        }
    }

    /// Execute the skill with given parameters
    fn execute_skill(&self, params: &Value) -> Result<String> {
        // Mark skill as running
        {
            let mut manager = self
                .state_manager
                .write()
                .unwrap_or_else(|e| e.into_inner());
            let _ = manager.mark_running(&self.skill.name);
        }

        // Build skill context with parameters
        let mut context_lines = Vec::new();
        context_lines.push(format!("Skill: {}", self.skill.name));
        context_lines.push(format!("Description: {}", self.skill.description));

        if !self.skill.instructions.is_empty() {
            context_lines.push("Instructions:".to_string());
            context_lines.push(self.skill.instructions.clone());
        }

        // Add parameters to context
        context_lines.push("Parameters:".to_string());
        for (key, value) in params.as_object().unwrap_or(&serde_json::Map::new()) {
            context_lines.push(format!("  {}: {}", key, value));
        }

        // Execute skill commands (if any defined)
        let mut output = String::new();
        output.push_str(&context_lines.join("\n"));
        output.push_str("\n\n");

        if !self.skill.commands.is_empty() {
            output.push_str("Commands to execute:\n");
            for cmd in &self.skill.commands {
                output.push_str(&format!("  - {}\n", cmd.invocation));
            }
        } else {
            output.push_str("Skill instructions ready for LLM context injection.\n");
        }

        // Mark skill as completed
        {
            let mut manager = self
                .state_manager
                .write()
                .unwrap_or_else(|e| e.into_inner());
            manager.mark_completed(&self.skill.name, true, None);
        }

        Ok(output)
    }
}

impl Tool for SkillAsTool {
    fn name(&self) -> &str {
        &self.tool_name
    }

    fn description(&self) -> &str {
        &self.skill.description
    }

    fn permission(&self) -> rustycode_tools::ToolPermission {
        // Skills are read-only by default (they inject context/instructions)
        rustycode_tools::ToolPermission::Read
    }

    fn parameters_schema(&self) -> Value {
        self.build_parameters_schema()
    }

    fn execute(&self, params: Value, _ctx: &ToolContext) -> Result<ToolOutput> {
        let result = self.execute_skill(&params)?;
        Ok(ToolOutput::text(result))
    }
}

/// Registry for skill-as-tool wrappers
pub struct SkillToolRegistry {
    state_manager: Arc<RwLock<SkillStateManager>>,
}

impl SkillToolRegistry {
    pub fn new(state_manager: Arc<RwLock<SkillStateManager>>) -> Self {
        Self { state_manager }
    }

    /// Convert all active skills to tool wrappers
    pub fn build_tools(&self) -> Vec<Box<dyn Tool>> {
        let manager = self.state_manager.read().unwrap_or_else(|e| e.into_inner());
        let mut tools: Vec<Box<dyn Tool>> = Vec::new();

        for skill_state in &manager.skills {
            // Only expose skills that are active or have instructions
            if !skill_state.base.instructions.is_empty() {
                let tool =
                    SkillAsTool::new(skill_state.base.clone(), Arc::clone(&self.state_manager));
                tools.push(Box::new(tool));
            }
        }

        tools
    }

    /// Get tool schema for a specific skill (for provider-specific formatting)
    pub fn get_skill_schema(
        &self,
        skill: &Skill,
        provider: rustycode_prompt::ModelProvider,
    ) -> Value {
        let tool_name = format!("skill_{}", skill.name.replace('-', "_"));

        match provider {
            p if p.is_anthropic() => {
                json!({
                    "name": tool_name,
                    "description": skill.description,
                    "input_schema": Self::build_schema_for_skill(skill)
                })
            }
            p if p.is_openai() => {
                json!({
                    "type": "function",
                    "function": {
                        "name": tool_name,
                        "description": skill.description,
                        "parameters": Self::build_schema_for_skill(skill)
                    }
                })
            }
            p if p.is_google() => {
                json!({
                    "name": tool_name,
                    "description": skill.description,
                    "parameters": Self::build_schema_for_skill(skill)
                })
            }
            _ => {
                json!({
                    "name": tool_name,
                    "description": skill.description,
                    "input_schema": Self::build_schema_for_skill(skill)
                })
            }
        }
    }

    /// Build JSON Schema for a skill
    fn build_schema_for_skill(skill: &Skill) -> Value {
        let mut properties = json!({});
        let mut required = Vec::new();

        for param in &skill.parameters {
            let param_schema = match param.param_type {
                crate::skills::loader::ParamType::Text => json!({
                    "type": "string",
                    "description": param.description
                }),
                crate::skills::loader::ParamType::File => json!({
                    "type": "string",
                    "description": format!("{} (file path)", param.description)
                }),
                crate::skills::loader::ParamType::Directory => json!({
                    "type": "string",
                    "description": format!("{} (directory path)", param.description)
                }),
                crate::skills::loader::ParamType::Number => json!({
                    "type": "number",
                    "description": param.description
                }),
            };

            if let Some(obj) = properties.as_object_mut() {
                obj.insert(param.name.clone(), param_schema);
            }

            if param.required {
                required.push(param.name.clone());
            }
        }

        // If no parameters defined, create a generic "input" parameter
        if skill.parameters.is_empty() {
            json!({
                "type": "object",
                "properties": {
                    "input": {
                        "type": "string",
                        "description": "Input or context for the skill"
                    }
                },
                "required": ["input"]
            })
        } else {
            json!({
                "type": "object",
                "properties": properties,
                "required": required
            })
        }
    }
}

/// Tool for spawning specialized agents
///
/// This tool allows the LLM to delegate tasks to specialized agents
/// like code-reviewer, planner, security-reviewer, etc.
///
/// ## Example Usage
///
/// When LLM needs to delegate a complex task:
/// ```json
/// {
///   "name": "spawn_agent",
///   "input": {
///     "role": "code-reviewer",
///     "task": "Review src/auth.rs for security vulnerabilities and code quality issues"
///   }
/// }
/// ```
pub struct SpawnAgentTool {
    _marker: std::marker::PhantomData<()>,
}

impl SpawnAgentTool {
    pub fn new() -> Self {
        Self {
            _marker: std::marker::PhantomData,
        }
    }
}

impl Default for SpawnAgentTool {
    fn default() -> Self {
        Self::new()
    }
}

impl Tool for SpawnAgentTool {
    fn name(&self) -> &str {
        "spawn_agent"
    }

    fn description(&self) -> &str {
        "Spawn a specialized agent to handle a complex task. Use when you need deep expertise in a specific area (code review, security analysis, planning, testing, etc.). Agents run asynchronously and return detailed results."
    }

    fn permission(&self) -> rustycode_tools::ToolPermission {
        rustycode_tools::ToolPermission::Execute
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "role": {
                    "type": "string",
                    "description": "Specialized agent role (e.g., 'code-reviewer', 'planner', 'security-reviewer', 'architect', 'tdd-guide', 'build-error-resolver')"
                },
                "task": {
                    "type": "string",
                    "description": "Detailed description of the task for the agent to complete"
                },
                "context": {
                    "type": "string",
                    "description": "Optional additional context or files to consider"
                }
            },
            "required": ["role", "task"]
        })
    }

    fn execute(&self, params: Value, ctx: &ToolContext) -> Result<ToolOutput> {
        let role = params
            .get("role")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: role"))?;

        let task = params
            .get("task")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: task"))?;

        let context = params.get("context").and_then(|v| v.as_str()).unwrap_or("");

        // Track agent spawn in WorkerRegistry
        let worker_registry = rustycode_protocol::worker_registry::global_worker_registry();
        let worker = worker_registry.spawn(&ctx.cwd.to_string_lossy());

        // Assign task to the worker
        let task_id = format!(
            "task_{}",
            worker.worker_id.split('_').next_back().unwrap_or("unknown")
        );
        let _ = worker_registry.assign_task(
            &worker.worker_id,
            &task_id,
            &format!("{}: {}", role, task),
        );

        // Log the agent spawn request
        tracing::info!(
            "Agent spawn requested: role={}, task={}, worker_id={}",
            role,
            task,
            worker.worker_id
        );

        // Return acknowledgment with worker tracking info
        let result = format!(
            "Agent spawn request queued.\n\
             Role: {}\n\
             Task: {}\n\
             Worker ID: {}\n\
             Status: {}\n\
             {}\n\
             Note: Agent execution is handled asynchronously by the TUI. \
             Use `/workers` to check status.",
            role,
            task,
            worker.worker_id,
            worker.status,
            if context.is_empty() {
                String::new()
            } else {
                format!("Context: {}\n", context)
            }
        );

        Ok(ToolOutput::text(result))
    }
}

/// Tool for creating agent teams
///
/// This tool allows the LLM to create teams for coordinated multi-agent work.
///
/// ## Example Usage
///
/// ```json
/// {
///   "name": "create_team",
///   "input": {
///     "name": "Security Review Team",
///     "tasks": ["task_001", "task_002"]
///   }
/// }
/// ```
pub struct CreateTeamTool {
    _marker: std::marker::PhantomData<()>,
}

impl CreateTeamTool {
    pub fn new() -> Self {
        Self {
            _marker: std::marker::PhantomData,
        }
    }
}

impl Default for CreateTeamTool {
    fn default() -> Self {
        Self::new()
    }
}

impl Tool for CreateTeamTool {
    fn name(&self) -> &str {
        "create_team"
    }

    fn description(&self) -> &str {
        "Create a team of agents to work on related tasks. Use when you need multiple agents coordinating on a complex project."
    }

    fn permission(&self) -> rustycode_tools::ToolPermission {
        rustycode_tools::ToolPermission::Execute
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "name": {
                    "type": "string",
                    "description": "Human-readable name for the team"
                },
                "tasks": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "List of task IDs to assign to this team"
                }
            },
            "required": ["name"]
        })
    }

    fn execute(&self, params: Value, _ctx: &ToolContext) -> Result<ToolOutput> {
        use rustycode_protocol::team_registry::global_team_registry;

        let name = params
            .get("name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: name"))?;

        let task_ids: Vec<String> = params
            .get("tasks")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();

        let registry = global_team_registry();
        let team = registry.create(name, task_ids.clone());

        let result = format!(
            "✅ Team created successfully.\n\n\
             **Team ID:** {}\n\
             **Name:** {}\n\
             **Status:** {}\n\
             **Tasks assigned:** {}\n\
             \n\
             Use `/team` to manage team execution.",
            team.team_id,
            team.name,
            team.status,
            if task_ids.is_empty() {
                "None (add tasks later)".to_string()
            } else {
                task_ids.join(", ")
            }
        );

        Ok(ToolOutput::text(result))
    }
}

/// Tool for creating scheduled cron tasks
///
/// This tool allows the LLM to set up autonomous scheduled operations.
///
/// ## Example Usage
///
/// ```json
/// {
///   "name": "create_cron",
///   "input": {
///     "schedule": "0 9 * * *",
///     "prompt": "Run morning test suite and report results",
///     "description": "Daily morning tests"
///   }
/// }
/// ```
pub struct CreateCronTool {
    _marker: std::marker::PhantomData<()>,
}

impl CreateCronTool {
    pub fn new() -> Self {
        Self {
            _marker: std::marker::PhantomData,
        }
    }
}

impl Default for CreateCronTool {
    fn default() -> Self {
        Self::new()
    }
}

impl Tool for CreateCronTool {
    fn name(&self) -> &str {
        "create_cron"
    }

    fn description(&self) -> &str {
        "Create a scheduled autonomous task. The task will run automatically on the specified cron schedule. Use for recurring operations like daily tests, periodic cleanup, or scheduled reports."
    }

    fn permission(&self) -> rustycode_tools::ToolPermission {
        rustycode_tools::ToolPermission::Execute
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "schedule": {
                    "type": "string",
                    "description": "Cron expression (e.g., '0 9 * * *' for daily at 9am, '*/5 * * * *' for every 5 minutes)"
                },
                "prompt": {
                    "type": "string",
                    "description": "The task/prompt to execute on schedule"
                },
                "description": {
                    "type": "string",
                    "description": "Optional human-readable description"
                }
            },
            "required": ["schedule", "prompt"]
        })
    }

    fn execute(&self, params: Value, _ctx: &ToolContext) -> Result<ToolOutput> {
        use rustycode_protocol::cron_registry::global_cron_registry;

        let schedule = params
            .get("schedule")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: schedule"))?;

        let prompt = params
            .get("prompt")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: prompt"))?;

        let description = params.get("description").and_then(|v| v.as_str());

        let registry = global_cron_registry();
        let entry = registry.create(schedule, prompt, description, None);

        let result = format!(
            "⏰ Cron task scheduled successfully.\n\n\
             **Cron ID:** {}\n\
             **Schedule:** `{}`\n\
             **Task:** {}\n\
             {}\n\
             Use `/cron` to list, enable, or disable scheduled tasks.",
            entry.cron_id,
            entry.schedule,
            entry.prompt,
            description
                .map(|d| format!("**Description:** {}\n", d))
                .unwrap_or_default()
        );

        Ok(ToolOutput::text(result))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::skills::loader::{SkillCategory, SkillCommand, SkillParameter};
    use crate::skills::manager::SkillState;
    use std::path::PathBuf;

    fn create_test_skill() -> Skill {
        Skill {
            name: "test-skill".to_string(),
            description: "A test skill for unit tests".to_string(),
            category: SkillCategory::Tools,
            parameters: vec![SkillParameter {
                name: "target".to_string(),
                description: "Target file to process".to_string(),
                required: true,
                param_type: crate::skills::loader::ParamType::File,
            }],
            commands: vec![SkillCommand {
                name: "test".to_string(),
                invocation: "/test".to_string(),
                description: "Run test".to_string(),
            }],
            instructions: "This is a test instruction for the skill.".to_string(),
            path: PathBuf::from("/test"),
        }
    }

    #[test]
    fn test_skill_as_tool_creation() {
        let state_manager = Arc::new(RwLock::new(SkillStateManager::new()));
        let skill = create_test_skill();
        let tool = SkillAsTool::new(skill, Arc::clone(&state_manager));

        assert!(tool.name().starts_with("skill_"));
        assert!(!tool.description().is_empty());
    }

    #[test]
    fn test_parameters_schema_generation() {
        let state_manager = Arc::new(RwLock::new(SkillStateManager::new()));
        let skill = create_test_skill();
        let tool = SkillAsTool::new(skill, state_manager);

        let schema = tool.build_parameters_schema();
        assert!(schema.is_object());

        let obj = schema.as_object().unwrap();
        assert!(obj.contains_key("properties"));
        assert!(obj.contains_key("required"));
    }

    #[test]
    fn test_skill_tool_registry() {
        let mut manager = SkillStateManager::new();

        // Add a test skill
        let skill = create_test_skill();
        manager.skills.push(SkillState::from_base(skill));

        let state_manager = Arc::new(RwLock::new(manager));
        let registry = SkillToolRegistry::new(state_manager);

        let tools = registry.build_tools();
        assert_eq!(tools.len(), 1);
    }

    #[test]
    fn test_spawn_agent_tool_creation() {
        let tool = SpawnAgentTool::new();
        assert_eq!(tool.name(), "spawn_agent");
        assert!(tool.description().contains("specialized agent"));
    }

    #[test]
    fn test_spawn_agent_tool_schema() {
        let tool = SpawnAgentTool::new();
        let schema = tool.parameters_schema();

        assert!(schema.is_object());
        let obj = schema.as_object().unwrap();

        // Check required fields
        let required = obj.get("required").unwrap().as_array().unwrap();
        assert!(required.iter().any(|v| v.as_str() == Some("role")));
        assert!(required.iter().any(|v| v.as_str() == Some("task")));

        // Check properties
        let props = obj.get("properties").unwrap().as_object().unwrap();
        assert!(props.contains_key("role"));
        assert!(props.contains_key("task"));
        assert!(props.contains_key("context"));
    }

    #[test]
    fn test_spawn_agent_tool_execute_success() {
        use rustycode_tools::ToolContext;
        use serde_json::json;

        let tool = SpawnAgentTool::new();
        let ctx = ToolContext::new(std::env::temp_dir());

        let params = json!({
            "role": "code-reviewer",
            "task": "Review the authentication module"
        });

        let result = tool.execute(params, &ctx);
        assert!(result.is_ok());
        let output = result.unwrap();
        assert!(output.text.contains("Agent spawn request queued"));
        assert!(output.text.contains("code-reviewer"));
    }

    #[test]
    fn test_spawn_agent_tool_execute_missing_role() {
        use rustycode_tools::ToolContext;
        use serde_json::json;

        let tool = SpawnAgentTool::new();
        let ctx = ToolContext::new(std::env::temp_dir());

        let params = json!({
            "task": "Do something"
            // Missing "role"
        });

        let result = tool.execute(params, &ctx);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("role"));
    }

    #[test]
    fn test_spawn_agent_tool_execute_missing_task() {
        use rustycode_tools::ToolContext;
        use serde_json::json;

        let tool = SpawnAgentTool::new();
        let ctx = ToolContext::new(std::env::temp_dir());

        let params = json!({
            "role": "planner"
            // Missing "task"
        });

        let result = tool.execute(params, &ctx);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("task"));
    }

    #[test]
    fn test_create_team_tool_creation() {
        let tool = CreateTeamTool::new();
        assert_eq!(tool.name(), "create_team");
        assert!(!tool.description().is_empty());
    }

    #[test]
    fn test_create_team_tool_schema() {
        let tool = CreateTeamTool::new();
        let schema = tool.parameters_schema();

        let obj = schema.as_object().unwrap();
        assert_eq!(obj.get("type").unwrap().as_str(), Some("object"));

        let props = obj.get("properties").unwrap().as_object().unwrap();
        assert!(props.contains_key("name"));
        assert!(props.contains_key("tasks"));
    }

    #[test]
    fn test_create_team_tool_execute_success() {
        use rustycode_tools::ToolContext;
        use serde_json::json;

        let tool = CreateTeamTool::new();
        let ctx = ToolContext::new(std::env::temp_dir());

        let params = json!({
            "name": "Test Team",
            "tasks": ["task_1", "task_2"]
        });

        let result = tool.execute(params, &ctx);
        assert!(result.is_ok());
        let output = result.unwrap();
        assert!(output.text.contains("Team created successfully"));
        assert!(output.text.contains("Test Team"));
    }

    #[test]
    fn test_create_team_tool_execute_missing_name() {
        use rustycode_tools::ToolContext;
        use serde_json::json;

        let tool = CreateTeamTool::new();
        let ctx = ToolContext::new(std::env::temp_dir());

        let params = json!({
            "tasks": ["task_1"]
            // Missing "name"
        });

        let result = tool.execute(params, &ctx);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("name"));
    }

    #[test]
    fn test_create_cron_tool_creation() {
        let tool = CreateCronTool::new();
        assert_eq!(tool.name(), "create_cron");
        assert!(!tool.description().is_empty());
    }

    #[test]
    fn test_create_cron_tool_schema() {
        let tool = CreateCronTool::new();
        let schema = tool.parameters_schema();

        let obj = schema.as_object().unwrap();
        assert_eq!(obj.get("type").unwrap().as_str(), Some("object"));

        let props = obj.get("properties").unwrap().as_object().unwrap();
        assert!(props.contains_key("schedule"));
        assert!(props.contains_key("prompt"));
        assert!(props.contains_key("description"));
    }

    #[test]
    fn test_create_cron_tool_execute_success() {
        use rustycode_tools::ToolContext;
        use serde_json::json;

        let tool = CreateCronTool::new();
        let ctx = ToolContext::new(std::env::temp_dir());

        let params = json!({
            "schedule": "0 9 * * *",
            "prompt": "Run morning tests",
            "description": "Daily test run"
        });

        let result = tool.execute(params, &ctx);
        assert!(result.is_ok());
        let output = result.unwrap();
        assert!(output.text.contains("Cron task scheduled successfully"));
        assert!(output.text.contains("0 9 * * *"));
    }

    #[test]
    fn test_create_cron_tool_execute_missing_schedule() {
        use rustycode_tools::ToolContext;
        use serde_json::json;

        let tool = CreateCronTool::new();
        let ctx = ToolContext::new(std::env::temp_dir());

        let params = json!({
            "prompt": "Run tests"
            // Missing "schedule"
        });

        let result = tool.execute(params, &ctx);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("schedule"));
    }
}
