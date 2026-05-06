//! Tool registration and MCP loading service
//!
//! Extracted from the TUI god object to encapsulate tool registration logic
//! (built-in tools, skill tools, agent/delegation tools) and MCP server
//! discovery/loading. Owns the session-long MCP proxy cache.
#![allow(dead_code)]

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::RwLock;

use rustycode_llm::tool_annotations::anthropic_annotations_for_tool_info;
use rustycode_tools::ToolRegistry;

/// Manages tool registration and MCP server integration for the TUI session.
///
/// Owns the session-long MCP proxy cache so that live MCP connections persist
/// for the full TUI lifecycle and can be shut down explicitly on exit.
#[allow(clippy::redundant_pub_crate)]
pub(crate) struct ToolManager {
    /// Session-long MCP proxy cache — owns live MCP connections for loaded tools.
    mcp_proxies: Option<Arc<RwLock<HashMap<String, rustycode_mcp::proxy::ToolProxy>>>>,
}

impl ToolManager {
    pub fn new() -> Self {
        Self { mcp_proxies: None }
    }

    /// Register all built-in tools for AI coding assistant functionality.
    ///
    /// Populates `tool_registry` with zero-config defaults, stateful tools
    /// (todo, semantic search), agent/delegation executors, team/cron tools,
    /// and skill-as-tool wrappers.
    #[allow(clippy::too_many_arguments)]
    pub fn register_builtin_tools(
        &self,
        tool_registry: &mut ToolRegistry,
        provider: &Arc<dyn rustycode_llm::LLMProvider>,
        current_model: &str,
        cwd: &Path,
        skill_manager: &Arc<std::sync::RwLock<crate::skills::manager::SkillStateManager>>,
        todo_state: &rustycode_tools::todo::TodoState,
    ) {
        use crate::skills::as_tool::{CreateCronTool, CreateTeamTool, SkillToolRegistry};
        use rustycode_tools::todo::{TodoUpdateTool, TodoWriteTool};
        use rustycode_tools::todo_read::TodoReadTool;
        #[cfg(feature = "vector-memory")]
        use rustycode_tools::SemanticSearchTool;

        // Register all zero-config built-in tools from default registry
        *tool_registry = rustycode_tools::default_registry();

        // Register stateful tools that require runtime state
        // Todo tools (shared state with TUI sidebar)
        tool_registry.register(TodoReadTool::new(todo_state.clone()));
        tool_registry.register(TodoWriteTool::new(todo_state.clone()));
        tool_registry.register(TodoUpdateTool::new(todo_state.clone()));

        // Semantic search tool (conditional feature)
        #[cfg(feature = "vector-memory")]
        tool_registry.register(SemanticSearchTool::new(
            &std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from(".")),
        ));

        // Agent tool - functional sub-agent backed by AgentSession.
        // Build full tool definitions (name + description + input_schema) for sub-agent.
        let tools_schema: Vec<serde_json::Value> = tool_registry
            .list()
            .iter()
            .map(|t| {
                let mut schema = serde_json::json!({
                    "name": t.name,
                    "description": t.description,
                    "input_schema": t.parameters_schema,
                });
                if let Some(annotations) = anthropic_annotations_for_tool_info(
                    &t.name,
                    matches!(t.permission, rustycode_tools::ToolPermission::Read),
                ) {
                    schema["annotations"] = annotations;
                }
                schema
            })
            .collect();
        let tools_schema_clone = tools_schema.clone();
        let agent_tool = crate::agents::agent_tool::AgentTool::new(
            Arc::clone(provider),
            current_model.to_string(),
            cwd.to_path_buf(),
            tools_schema,
        );
        tool_registry.register(agent_tool);

        // Delegation executor — real sub-agent execution for delegate_task tool.
        let delegation_executor = crate::agents::delegation_executor::DelegationExecutor::new(
            Arc::clone(provider),
            current_model.to_string(),
            cwd.to_path_buf(),
            tools_schema_clone,
        );
        tool_registry.register(delegation_executor);

        // Team management tool - allows LLM to create agent teams
        tool_registry.register(CreateTeamTool::new());

        // Cron scheduling tool - allows LLM to create scheduled tasks
        tool_registry.register(CreateCronTool::new());

        // Register skill-as-tool wrappers for active skills
        let skill_tool_registry = SkillToolRegistry::new(Arc::clone(skill_manager));
        let skill_tools = skill_tool_registry.build_tools();
        for skill_tool in skill_tools {
            tool_registry.register_boxed(skill_tool);
        }

        tracing::info!("Registered {} built-in tools", tool_registry.list().len());
    }

    /// Load tools from configured MCP servers.
    ///
    /// Discovers MCP server configurations from standard locations, starts
    /// each server, and registers discovered tools into `tool_registry`.
    /// Tools that semantically overlap with built-in tools are skipped.
    /// Live proxy connections are cached in the internal proxy store.
    pub fn load_mcp_tools(&mut self, tool_registry: &mut ToolRegistry) {
        use rustycode_mcp::proxy::{ProxyConfig, ToolProxy};
        use rustycode_mcp::McpConfigFile;

        // Load MCP config from all standard locations
        let configs = McpConfigFile::load_from_standard_locations();

        if configs.is_empty() {
            tracing::debug!("No MCP server configs found in standard locations");
            return;
        }

        // Collect existing built-in tool names for overlap detection
        let builtin_names: std::collections::HashSet<String> = tool_registry
            .list()
            .iter()
            .map(|t| t.name.clone())
            .collect();

        // Known semantic equivalents: MCP tool name → built-in tool it duplicates.
        // These are common filesystem MCP server tools that overlap with built-in tools.
        let overlap_map: std::collections::HashMap<&str, &str> = [
            // @modelcontextprotocol/server-filesystem equivalents
            ("read_text_file", "read_file"),
            ("write_file", "write_file"),
            ("list_directory", "list_dir"),
            ("list_allowed_directories", "__skip__"), // no built-in equivalent, wastes turns
            ("search_files", "grep"),
            ("get_file_info", "__skip__"), // no useful equivalent
            ("create_directory", "bash"),  // mkdir via bash
            ("move_file", "bash"),         // mv via bash
            ("read_multiple_files", "read_file"), // can read files individually
            // Other common MCP servers
            ("directory_tree", "list_dir"),
            ("read_file", "read_file"), // exact overlap
        ]
        .into_iter()
        .collect();

        // Create a shared proxy cache for the session so MCP connections stay
        // alive for the full TUI lifecycle and can be shut down explicitly.
        let proxy_cache = Arc::new(tokio::sync::RwLock::new(std::collections::HashMap::<
            String,
            ToolProxy,
        >::new()));
        self.mcp_proxies = Some(proxy_cache.clone());

        // Use the shared persistent runtime so MCP child process handles
        // (ChildStdin/ChildStdout) remain valid for the entire session.
        // A short-lived local runtime would drop its I/O driver, killing
        // the MCP transport — see Bug #11.
        use rustycode_shared_runtime::SHARED_RUNTIME;

        // Load and start servers from all config files
        let mut started_count = 0;
        let mut tools_registered = 0;
        let mut tools_skipped = 0;
        for (config_path, config_file) in configs {
            tracing::info!("Loading MCP servers from {:?}", config_path);

            for (server_id, server_config) in config_file.servers {
                tracing::info!("Starting MCP server '{}'", server_id);

                // Create a tool proxy for this server (stdio only)
                let command = match server_config.command.clone() {
                    Some(cmd) => cmd,
                    None => {
                        tracing::debug!(
                            "Skipping MCP server '{}': no command (remote transport)",
                            server_id
                        );
                        continue;
                    }
                };
                let proxy_config = ProxyConfig {
                    server_name: server_id.clone(),
                    command,
                    args: server_config.args.clone(),
                    tool_prefix: None,
                    cache_tools: true,
                };

                match SHARED_RUNTIME.block_on(ToolProxy::with_discovery(proxy_config)) {
                    Ok(proxy) => {
                        tracing::info!("MCP server '{}' connected successfully", server_id);
                        started_count += 1;

                        // Keep the live proxy around for the rest of the session.
                        let proxy_for_cache = proxy.clone();
                        let proxy_cache_clone = proxy_cache.clone();
                        let server_id_for_cache = server_id.clone();
                        SHARED_RUNTIME.block_on(async move {
                            let mut cache = proxy_cache_clone.write().await;
                            cache.insert(server_id_for_cache, proxy_for_cache);
                        });

                        // Get all tools from the proxy and register them
                        let proxied_tools = SHARED_RUNTIME.block_on(proxy.tools());
                        for proxied_tool in proxied_tools {
                            let tool_name = proxied_tool.name.clone();

                            // Skip MCP tools that duplicate built-in functionality
                            if let Some(equivalent) = overlap_map.get(tool_name.as_str()) {
                                if *equivalent == "__skip__" {
                                    tracing::warn!(
                                        "Skipping MCP tool '{}' (no useful equivalent, wastes LLM turns)",
                                        tool_name
                                    );
                                    tools_skipped += 1;
                                    continue;
                                }
                                if builtin_names.contains(*equivalent) {
                                    tracing::warn!(
                                        "Skipping MCP tool '{}' — built-in '{}' already registered (semantic overlap)",
                                        tool_name,
                                        equivalent
                                    );
                                    tools_skipped += 1;
                                    continue;
                                }
                            }

                            // Also skip exact name collisions
                            if builtin_names.contains(&tool_name) {
                                tracing::warn!(
                                    "Skipping MCP tool '{}' — already registered as built-in",
                                    tool_name
                                );
                                tools_skipped += 1;
                                continue;
                            }

                            tool_registry.register(proxied_tool);
                            tracing::debug!("  Registered MCP tool: {}", tool_name);
                            tools_registered += 1;
                        }
                    }
                    Err(e) => {
                        tracing::error!("Failed to connect to MCP server '{}': {}", server_id, e);
                    }
                }
            }
        }

        if started_count > 0 {
            tracing::info!(
                "Started {} MCP server(s): {} tools registered, {} skipped (overlap with built-in tools)",
                started_count,
                tools_registered,
                tools_skipped
            );
        }
    }

    /// Get a reference to the MCP proxy cache.
    ///
    /// Used by `refresh_mcp_status` and other consumers that need to inspect
    /// or manage live MCP connections outside of tool loading.
    pub fn mcp_proxies(
        &self,
    ) -> &Option<Arc<RwLock<HashMap<String, rustycode_mcp::proxy::ToolProxy>>>> {
        &self.mcp_proxies
    }
}
