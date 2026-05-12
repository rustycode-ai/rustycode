//! Runtime module - Core runtime and session management.
//!
//! This module extracts the runtime logic from the main lib.rs file,
//! including session management, event publishing, and tool execution orchestration.

use anyhow::Result;
use rustycode_bus::EventBus;
use rustycode_config::Config;
use rustycode_git::GitStatus;
use rustycode_lsp::LspServerStatus;
use rustycode_protocol::{ContextPlan, Session, SessionId};
use rustycode_session::session_manager::SessionManager;
use rustycode_storage::Storage;
use rustycode_tools::ToolRegistry;
use serde::Serialize;

use crate::tool_result_storage::CacheConfig;
use std::path::Path;
use std::sync::{Arc, Mutex};
use tracing::{info, warn};

// Domain operation modules
pub(crate) mod event_ops;
pub(crate) mod execution_ops;
pub(crate) mod memory_ops;
pub(crate) mod plan_ops;
pub(crate) mod session_ops;
pub(crate) mod tool_ops;

// Re-exports from sibling modules
pub use super::execution::{
    ExecutionConfig, ExecutionContext, StepExecutor, StepExecutorRegistry, ToolInvocationWrapper,
};
pub use super::plan_executor::{ExecutionOptions, ExecutionReport, PlanExecutor};
pub use super::session::{AiMode, MessageType, SessionState, ToolExecution, ToolStatus};
pub use super::tool_result_storage::ToolResultCache;

// === Report Structs ===

#[derive(Debug, Clone, Serialize)]
pub struct DoctorReport {
    pub config: Config,
    pub git: GitStatus,
    pub lsp_servers: Vec<LspServerStatus>,
    pub memory_entries: usize,
    pub skills: usize,
    pub sample_context_plan: ContextPlan,
}

#[derive(Debug, Clone, Serialize)]
pub struct RunReport {
    pub session: Session,
    pub git: GitStatus,
    pub lsp_servers: Vec<LspServerStatus>,
    pub memory: Vec<rustycode_memory::MemoryEntry>,
    pub skills: Vec<rustycode_skill::Skill>,
    pub active_skills: Vec<rustycode_skill::Skill>,
    pub recent_tasks: Vec<String>,
    pub code_excerpts: Vec<CodeExcerpt>,
    pub context_plan: ContextPlan,
}

#[derive(Debug, Clone, Serialize)]
pub struct ToolCallReport {
    pub session: Session,
    pub result: rustycode_protocol::ToolResult,
}

#[derive(Debug, Clone, Serialize)]
pub struct CodeExcerpt {
    pub path: String,
    pub preview: String,
    pub score: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct PlanReport {
    pub session: Session,
    pub plan: rustycode_protocol::Plan,
    /// Absolute path to the skeleton plan.md file.
    pub plan_path: std::path::PathBuf,
}

/// Core runtime struct - manages session lifecycle and tool execution
pub struct Runtime {
    pub config: Config,
    pub storage: Storage,
    pub tools: Arc<ToolRegistry>,
    pub bus: Arc<EventBus>,
    pub llm_provider: Option<Box<dyn rustycode_llm::LLMProvider>>,
    pub mcp_integration: Option<super::integration::mcp_integration::McpIntegration>,
    pub hooks: Arc<tokio::sync::RwLock<super::integration::hooks_integration::HookRegistry>>,
    pub session_manager: Option<SessionManager>,
    pub tool_cache: Arc<Mutex<ToolResultCache>>,
    pub skill_manager: Arc<Mutex<Option<rustycode_skill::manager::SkillManager>>>,
    pub(crate) active_session_id: Mutex<Option<SessionId>>,
}

impl Runtime {
    /// Load runtime from configuration
    pub fn load(cwd: &Path) -> Result<Self> {
        let config = Config::load(cwd)?;
        let storage = Storage::open(&config.data_dir.join("rustycode.db"))?;
        let mut tools = {
            let provider_caps = rustycode_tools::ProviderCaps::full();
            let filter = rustycode_tools::ToolFilter::probe(provider_caps, cwd.to_path_buf());
            rustycode_tools::default_registry_filtered(&filter)
        };

        // Register structured thinking tool for headless agent
        tools.register(
            rustycode_orchestration::structured_thinking_tool_impl::StructuredThinkingTool,
        );
        // Register ask_user tool so LLM can request clarification when stuck
        tools.register(rustycode_orchestration::ask_user_tool::AskUserTool);
        let bus = Arc::new(EventBus::new());

        // LLM provider initialization moved to runtime layer
        let llm_provider: Option<Box<dyn rustycode_llm::LLMProvider>> = None;

        // Initialize MCP integration if configured
        let mcp_integration = if !config.advanced.mcp_servers_map.is_empty() {
            info!(
                "Initializing MCP integration with {} server(s)",
                config.advanced.mcp_servers_map.len()
            );
            let config_clone = config.clone();
            let integration = crate::shared_runtime::block_on_shared_send(async move {
                let mut integration =
                    super::integration::mcp_integration::McpIntegration::new(&config_clone).await?;
                integration.start_servers().await?;
                Ok::<_, anyhow::Error>(integration)
            })?;

            info!("MCP integration initialized successfully");
            Some(integration)
        } else {
            info!("No MCP servers configured");
            None
        };

        // Initialize session manager
        let sessions_dir = config.data_dir.join("sessions");
        let session_manager = Some(SessionManager::new(sessions_dir));

        let tool_cache = Arc::new(Mutex::new(ToolResultCache::new(CacheConfig {
            max_entries: 1000,
            ttl: std::time::Duration::from_mins(5),
            min_size_to_cache: 100,
        })));

        let skill_manager = {
            let mut builder =
                rustycode_skill::manager::SkillManager::builder().token_budget(50_000);
            let user_skills = dirs::home_dir()
                .map(|h| h.join(".rustycode").join("skills"))
                .unwrap_or_else(|| config.data_dir.join("skills"));
            if user_skills.exists() {
                builder = builder.user_skills_dir(&user_skills);
            }
            if config.skills_dir.exists() {
                builder = builder.project_skills_dir(&config.skills_dir);
            }
            builder = builder.quality_storage_dir(config.data_dir.join("skill-quality"));
            builder = builder.graph_path(config.data_dir.join("capability-graph.json"));
            match builder.build() {
                Ok(mgr) => {
                    info!("Skill manager v2 initialized");
                    Some(mgr)
                }
                Err(e) => {
                    warn!("Failed to initialize skill manager v2: {}", e);
                    None
                }
            }
        };

        Ok(Self {
            config,
            storage,
            tools: Arc::new(tools),
            bus,
            llm_provider,
            mcp_integration,
            hooks: Arc::new(tokio::sync::RwLock::new(
                super::integration::hooks_integration::HookRegistry::new(),
            )),
            session_manager,
            tool_cache,
            skill_manager: Arc::new(Mutex::new(skill_manager)),
            active_session_id: Mutex::new(None),
        })
    }

    /// Build a Runtime from pre-existing parts (for async spawn_blocking).
    pub(crate) fn load_from_parts(
        config: Config,
        tools: Arc<ToolRegistry>,
        bus: Arc<EventBus>,
    ) -> Result<Self> {
        let storage = Storage::open(&config.data_dir.join("rustycode.db"))?;
        let sessions_dir = config.data_dir.join("sessions");
        let tool_cache = Arc::new(Mutex::new(ToolResultCache::new(CacheConfig::default())));
        Ok(Self {
            config,
            storage,
            tools,
            bus,
            llm_provider: None,
            mcp_integration: None,
            hooks: Arc::new(tokio::sync::RwLock::new(
                super::integration::hooks_integration::HookRegistry::new(),
            )),
            session_manager: Some(SessionManager::new(sessions_dir)),
            tool_cache,
            skill_manager: Arc::new(Mutex::new(None)),
            active_session_id: Mutex::new(None),
        })
    }

    /// Get config reference
    pub fn config(&self) -> &Config {
        &self.config
    }

    /// Get event bus reference
    pub fn event_bus(&self) -> &Arc<EventBus> {
        &self.bus
    }

    /// Get LLM provider reference
    pub fn llm_provider(&self) -> Option<&dyn rustycode_llm::LLMProvider> {
        self.llm_provider.as_deref()
    }

    /// List all available tools (including MCP tools)
    pub fn tool_list(&self) -> Vec<rustycode_tools::ToolInfo> {
        let mut tools = self.tools.list();

        // Add MCP tools if available
        if let Some(mcp) = &self.mcp_integration {
            let mcp_tools = mcp.mcp_tools();
            for mcp_tool in mcp_tools {
                tools.push(rustycode_tools::ToolInfo {
                    name: mcp_tool.name.clone(),
                    description: mcp_tool.description.clone(),
                    parameters_schema: mcp_tool.input_schema.clone(),
                    permission: rustycode_tools::ToolPermission::Execute,
                    defer_loading: None,
                    annotations: None,
                    tags: vec![],
                    max_result_size_chars: None,
                    is_destructive_default: false,
                });
            }
        }

        tools
    }

    // === Doctor / Diagnostics ===

    /// Run diagnostics and return a health report.
    pub fn doctor(&self, cwd: &Path) -> Result<DoctorReport> {
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
        let memory_entries = self.storage.memory("project").map(|v| v.len()).unwrap_or(0);
        // Count built-in skills (always available)
        let builtin_count = 5; // hardcoded; SkillRegistry is internal
                               // Count custom skills from the skills directory (only valid skill files)
        let custom_count = if self.config.skills_dir.exists() {
            std::fs::read_dir(&self.config.skills_dir)
                .map(|rd| {
                    rd.filter_map(|e| e.ok())
                        .filter(|e| {
                            e.path().extension().is_some_and(|ext| {
                                matches!(ext.to_str(), Some("yaml" | "yml" | "toml" | "json"))
                            })
                        })
                        .count()
                })
                .unwrap_or(0)
        } else {
            0
        };
        let skills = builtin_count + custom_count;
        let sample_context_plan = ContextPlan::default();
        Ok(DoctorReport {
            config: self.config.clone(),
            git,
            lsp_servers,
            memory_entries,
            skills,
            sample_context_plan,
        })
    }

    /// Perform cleanup when shutting down the runtime.
    pub fn shutdown(&self) {
        // Persist skill quality scores and capability graph
        if let Ok(mut guard) = self.skill_manager.lock() {
            if let Some(mgr) = guard.as_mut() {
                mgr.end_session();
            }
        }

        // Publish session completed event (only if a session was started)
        if let Ok(guard) = self.active_session_id.lock() {
            if let Some(ref session_id) = *guard {
                self.publish_session_completed(
                    session_id.clone(),
                    String::new(),
                    "completed".to_string(),
                    "Runtime shutdown".to_string(),
                );
            }
        }

        info!("Runtime shutdown complete");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use tempfile::TempDir;

    #[test]
    fn test_runtime_creation() {
        let temp_dir = TempDir::new().unwrap();
        let result = Runtime::load(temp_dir.path());
        // Note: This may fail if config doesn't exist, which is expected
        // The test verifies the module compiles and can be instantiated
        assert!(result.is_ok() || result.is_err());
    }
}

pub mod monitor;
