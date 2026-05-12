//! Runtime core execution operations.

use anyhow::Result;
use rustycode_git::GitStatus;
use rustycode_lsp::LspServerStatus;
use rustycode_protocol::{ContextPlan, Session, SessionId};
use rustycode_skill::Skill;
use std::path::Path;

use super::{RunReport, Runtime};

impl Runtime {
    /// Run a task and return a full report.
    pub fn run(&self, cwd: &Path, task: &str) -> Result<RunReport> {
        let git = rustycode_git::inspect(cwd).unwrap_or(GitStatus {
            root: None,
            branch: None,
            worktree: false,
            dirty: None,
        });
        let lsp_servers: Vec<LspServerStatus> = self
            .config
            .lsp_servers
            .iter()
            .map(|name| LspServerStatus {
                name: name.clone(),
                installed: false,
                path: None,
            })
            .collect();
        let memory = Vec::new();

        // Activate skills for this task context
        if let Ok(mut guard) = self.skill_manager.lock() {
            if let Some(mgr) = guard.as_mut() {
                mgr.activate_for_context(task);
            }
        }

        let skills: Vec<Skill> = self
            .skill_manager
            .lock()
            .map(|guard| {
                guard
                    .as_ref()
                    .map(|mgr| {
                        mgr.all_definitions()
                            .iter()
                            .map(|def| Skill::from((*def).clone()))
                            .collect()
                    })
                    .unwrap_or_default()
            })
            .unwrap_or_default();

        // Collect active skills for the report
        let active_skills: Vec<Skill> = self
            .skill_manager
            .lock()
            .map(|guard| {
                guard
                    .as_ref()
                    .map(|mgr| {
                        mgr.active_definitions()
                            .iter()
                            .map(|def| Skill::from((*def).clone()))
                            .collect()
                    })
                    .unwrap_or_default()
            })
            .unwrap_or_default();

        let recent_tasks = Vec::new();
        let code_excerpts = Vec::new();
        let context_plan = ContextPlan::default();

        let session = Session::builder().task(task.to_string()).build();
        if let Ok(mut guard) = self.active_session_id.lock() {
            *guard = Some(session.id.clone());
        }
        self.storage.insert_session(&session)?;

        self.publish_session_started(
            session.id.clone(),
            session.task.clone(),
            format!("task={}", task),
        );

        Ok(RunReport {
            session,
            git,
            lsp_servers,
            memory,
            skills,
            active_skills,
            recent_tasks,
            code_excerpts,
            context_plan,
        })
    }

    /// Run an agent task synchronously.
    pub fn run_agent(&self, session_id: &SessionId, task: &str) -> Result<()> {
        let session = Session::builder().task(task.to_string()).build();
        self.storage.insert_session(&session)?;
        self.publish_session_started(
            session_id.clone(),
            task.to_string(),
            "mode=agent".to_string(),
        );
        Ok(())
    }

    /// Run a headless agent task with the shared tool registry.
    pub async fn run_headless_task_with_iteration(
        &self,
        provider: &dyn rustycode_llm::provider::LLMProvider,
        model: &str,
        task: &str,
        cwd: &Path,
        iteration: usize,
    ) -> Result<crate::headless::HeadlessTaskResult> {
        let tools_schema = rustycode_tools_api::build_canonical_tool_schemas(&self.tool_list());

        // Build system prompt augmented with auto-activated skills
        let system_prompt = self.build_skill_augmented_prompt(task, Some(cwd));

        let session_id = if let Ok(guard) = self.active_session_id.lock() {
            guard.clone()
        } else {
            None
        };

        let active_ops = self.active_ops.clone();
        let session_id_reg = session_id.clone();

        let result = crate::headless::run_headless_task_core(
            provider,
            model,
            &tools_schema,
            task,
            cwd,
            iteration,
            &self.tools,
            None,
            Some(&system_prompt),
            Some(Box::new(move |tx| {
                if let Some(id) = session_id_reg {
                    if let Ok(mut guard) = active_ops.lock() {
                        guard.insert(id, tx);
                    }
                }
            })),
        )
        .await;

        // Cleanup
        if let Some(id) = session_id {
            if let Ok(mut guard) = self.active_ops.lock() {
                guard.remove(&id);
            }
        }

        result
    }

    /// Run headless agent with prior conversation messages for retry continuation.
    pub async fn run_headless_with_prior_messages(
        &self,
        provider: &dyn rustycode_llm::provider::LLMProvider,
        model: &str,
        task: &str,
        cwd: &Path,
        iteration: usize,
        prior_messages: Option<Vec<rustycode_llm::provider::ChatMessage>>,
    ) -> Result<crate::headless::HeadlessTaskResult> {
        let tools_schema = rustycode_tools_api::build_canonical_tool_schemas(&self.tool_list());

        let session_id = if let Ok(guard) = self.active_session_id.lock() {
            guard.clone()
        } else {
            None
        };

        let active_ops = self.active_ops.clone();
        let session_id_reg = session_id.clone();

        let result = crate::headless::run_headless_task_core(
            provider,
            model,
            &tools_schema,
            task,
            cwd,
            iteration,
            &self.tools,
            prior_messages,
            None,
            Some(Box::new(move |tx| {
                if let Some(id) = session_id_reg {
                    if let Ok(mut guard) = active_ops.lock() {
                        guard.insert(id, tx);
                    }
                }
            })),
        )
        .await;

        // Cleanup
        if let Some(id) = session_id {
            if let Ok(mut guard) = self.active_ops.lock() {
                guard.remove(&id);
            }
        }

        result
    }
}
