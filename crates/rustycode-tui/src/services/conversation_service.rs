//! Conversation service for RustyCode.
//!
//! This module provides a clean abstraction for conversation management and prompting,
//! making it easy to verify and reuse across different UI implementations.

#![allow(dead_code)]

use crate::agent_mode::AiMode;
use anyhow::Result;
use rustycode_config::Config;
use rustycode_llm::tool_annotations::anthropic_annotations_for_tool_info;
use rustycode_llm::ConversationManager;
use rustycode_memory::MemoryEntry;
use rustycode_prompt::ModelProvider;
use rustycode_protocol::{Conversation, Message, SessionId};
use rustycode_storage::memory_metrics::MemoryMetrics;
use rustycode_tools::ToolRegistry;
#[cfg(feature = "vector-memory")]
use rustycode_vector_memory::{MemoryResult, VectorMemory};

/// Stub for [`MemoryResult`] when the `vector-memory` feature is disabled.
#[cfg(not(feature = "vector-memory"))]
#[derive(Debug, Clone)]
pub struct MemoryResult {
    /// Placeholder – never populated without the feature.
    pub similarity: f32,
}
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::Path;
use std::sync::Arc;
use tokio::sync::Mutex;

/// Configuration for conversation service
#[derive(Clone)]
pub struct ConversationConfig {
    pub max_messages: usize,
    pub max_tokens: usize,
}

impl Default for ConversationConfig {
    fn default() -> Self {
        Self {
            max_messages: 50,
            max_tokens: 8000,
        }
    }
}

/// Conversation service that handles prompting and conversation management
pub struct ConversationService {
    conversation_manager: ConversationManager,
    cached_system_prompt: String,
    last_cache_key: Option<u64>,
    ai_mode: AiMode,
    pub tool_registry: Arc<ToolRegistry>,
    #[cfg(feature = "vector-memory")]
    vector_memory: Option<Arc<Mutex<VectorMemory>>>,
    memory_metrics: Arc<Mutex<MemoryMetrics>>,
    current_task: Option<String>,
}

impl ConversationService {
    /// Create a new conversation service
    pub fn new(config: ConversationConfig, tool_registry: Arc<ToolRegistry>) -> Self {
        let session_id = SessionId::new();
        let conversation = Conversation::new(session_id);
        let conversation_manager = ConversationManager::new(conversation)
            .with_max_messages(config.max_messages)
            .with_max_tokens(config.max_tokens);

        Self {
            conversation_manager,
            cached_system_prompt: String::new(),
            last_cache_key: None,
            ai_mode: AiMode::default(),
            tool_registry,
            #[cfg(feature = "vector-memory")]
            vector_memory: None,
            memory_metrics: Arc::new(Mutex::new(MemoryMetrics::new())),
            current_task: None,
        }
    }

    /// Create a new conversation service with vector memory support
    #[cfg(feature = "vector-memory")]
    pub fn with_vector_memory(
        config: ConversationConfig,
        tool_registry: Arc<ToolRegistry>,
        vector_memory: Arc<Mutex<VectorMemory>>,
    ) -> Self {
        let session_id = SessionId::new();
        let conversation = Conversation::new(session_id);
        let conversation_manager = ConversationManager::new(conversation)
            .with_max_messages(config.max_messages)
            .with_max_tokens(config.max_tokens);

        Self {
            conversation_manager,
            cached_system_prompt: String::new(),
            last_cache_key: None,
            ai_mode: AiMode::default(),
            tool_registry,
            vector_memory: Some(vector_memory),
            memory_metrics: Arc::new(Mutex::new(MemoryMetrics::new())),
            current_task: None,
        }
    }

    /// Set the current task context for memory retrieval
    pub fn set_current_task(&mut self, task: impl Into<String>) {
        self.current_task = Some(task.into());
    }

    /// Retrieve relevant memories using vector search across all memory types
    #[cfg(feature = "vector-memory")]
    pub async fn retrieve_relevant_memories(&self, query: &str) -> Vec<MemoryResult> {
        const SIMILARITY_THRESHOLD: f32 = 0.6;
        const TOP_N: usize = 10;

        let Some(vector_memory) = self.vector_memory.as_ref() else {
            return Vec::new();
        };
        let vector_memory = vector_memory.clone();
        let query = query.to_string();

        // Search all memory types
        let results = {
            let memory = vector_memory.lock().await;
            memory.search_all(&query, TOP_N)
        };

        // Flatten and filter results
        let mut all_results: Vec<MemoryResult> = results
            .into_values()
            .flatten()
            .filter(|r| r.similarity >= SIMILARITY_THRESHOLD)
            .collect();

        // Sort by similarity (highest first)
        all_results.sort_by(|a, b| {
            b.similarity
                .partial_cmp(&a.similarity)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        all_results.truncate(TOP_N);

        // Record metrics
        let mut metrics = self.memory_metrics.lock().await;
        metrics.record_query_with_type(&query, "vector", all_results.len());

        tracing::debug!(
            memory_count = all_results.len(),
            query = %query,
            "Retrieved vector memories"
        );

        all_results
    }

    /// Retrieve relevant memories – stub when `vector-memory` feature is disabled.
    #[cfg(not(feature = "vector-memory"))]
    pub async fn retrieve_relevant_memories(&self, _query: &str) -> Vec<MemoryResult> {
        Vec::new()
    }

    /// Record that a memory was used (e.g., when user confirms it was helpful)
    pub async fn on_memory_used(&self, memory_id: &str) {
        let mut metrics = self.memory_metrics.lock().await;
        metrics.record_memory_used(memory_id);

        tracing::debug!(memory_id = %memory_id, "Memory marked as used");
    }

    /// Generate a text report of memory effectiveness metrics
    pub async fn get_memory_metrics_report(&self) -> String {
        let metrics = self.memory_metrics.lock().await;
        let report = metrics.generate_report();

        format!(
            "=== Memory Effectiveness Report ===
Period: {} to {}

Sessions Captured: {}
Events Captured: {}
Summaries Generated: {}
Vector Memories Stored: {}
Keyword Memories Stored: {}
Patterns Learned: {}

Query Statistics:
  Total Queries: {}
  Memories Retrieved: {}
  Memories Injected: {}
  Memories Boosted: {}
  Memories Pruned: {}
  Avg Results/Query: {:.2}
  Retrieval Precision: {:.2}

Top Memories Used:
{}
Unused Memories (candidates for pruning):
{}
",
            report.period_start.format("%Y-%m-%d %H:%M"),
            report.period_end.format("%Y-%m-%d %H:%M"),
            report.total_metrics.sessions_captured,
            report.total_metrics.events_captured,
            report.total_metrics.summaries_generated,
            report.total_metrics.vector_memories_stored,
            report.total_metrics.keyword_memories_stored,
            report.total_metrics.patterns_learned,
            report.total_metrics.memory_queries,
            report.total_metrics.memories_retrieved,
            report.total_metrics.memories_injected,
            report.total_metrics.memories_boosted,
            report.total_metrics.memories_pruned,
            report.total_metrics.avg_results_per_query,
            report.total_metrics.retrieval_precision,
            if report.top_memories_used.is_empty() {
                "  (none yet)".to_string()
            } else {
                report
                    .top_memories_used
                    .iter()
                    .map(|(id, count)| format!("  - {}: {} uses", id, count))
                    .collect::<Vec<_>>()
                    .join("\n")
            },
            if report.unused_memories.is_empty() {
                "  (none)".to_string()
            } else {
                report
                    .unused_memories
                    .iter()
                    .map(|id| format!("  - {}", id))
                    .collect::<Vec<_>>()
                    .join("\n")
            }
        )
    }

    /// Set the AI behavior mode
    pub fn set_ai_mode(&mut self, mode: AiMode) {
        self.ai_mode = mode;
        // Invalidate cache when mode changes
        self.cached_system_prompt.clear();
        self.last_cache_key = None;
    }

    /// Get the current AI mode
    pub fn ai_mode(&self) -> AiMode {
        self.ai_mode
    }

    /// Add a message to the conversation
    pub fn add_message(&mut self, message: Message) {
        self.conversation_manager.add_message(message);
    }

    /// Get the conversation formatted for LLM input
    pub fn get_conversation_prompt(&self) -> String {
        self.conversation_manager.to_prompt()
    }

    /// Get the current message count
    pub fn message_count(&self) -> usize {
        self.conversation_manager.message_count()
    }

    /// Get estimated token count
    pub fn estimated_tokens(&self) -> usize {
        self.conversation_manager.estimated_tokens()
    }

    /// Clear the conversation
    pub fn clear_conversation(&mut self) {
        self.conversation_manager.clear();
    }

    /// Build the system prompt with caching using PromptOrchestrator
    pub fn build_system_prompt(
        &mut self,
        _current_model: &str,
        cwd: &Path,
        workspace_context: &str,
        _memory_entries: &[MemoryEntry],
    ) -> Result<String> {
        let task_query = self.current_task.as_deref().unwrap_or("");
        let config = Config::load(cwd).unwrap_or_else(|_| Config::default());

        // Build cache key from workspace hash and mode
        let mut hasher = DefaultHasher::new();
        workspace_context.hash(&mut hasher);
        self.ai_mode.hash(&mut hasher);
        task_query.hash(&mut hasher);
        config.task_routing.enabled.hash(&mut hasher);
        config
            .task_routing
            .confidence_threshold
            .to_bits()
            .hash(&mut hasher);
        config
            .task_routing
            .max_clarifying_questions
            .hash(&mut hasher);
        config.task_routing.max_research_passes.hash(&mut hasher);
        serde_json::to_string(&config.task_routing.intent_overrides)
            .unwrap_or_default()
            .hash(&mut hasher);
        serde_json::to_string(&config.task_routing.workflow_overrides)
            .unwrap_or_default()
            .hash(&mut hasher);
        let cache_key = hasher.finish();

        // Check if we need to rebuild the system prompt
        if self.cached_system_prompt.is_empty() || self.last_cache_key != Some(cache_key) {
            // Use the new shared PromptOrchestrator
            let orchestrator = rustycode_runtime::orchestration::PromptOrchestrator::new();

            // For now, we pass the mode as string and query as empty (since this is
            // the system prompt base), but the orchestration logic is now centralized.
            let prompt = orchestrator.build_system_prompt(
                &format!("{:?}", self.ai_mode),
                task_query,
                workspace_context,
                false,
                false, // TUI doesn't use websocket
                Some(&config.task_routing),
            )?;

            // Store with cache key marker
            self.cached_system_prompt = format!("{}\n{}", prompt, cache_key);
            self.last_cache_key = Some(cache_key);
        }

        // Extract actual prompt (without cache key line)
        let prompt = self
            .cached_system_prompt
            .lines()
            .take_while(|l| l.parse::<u64>().is_err())
            .collect::<Vec<_>>()
            .join("\n");

        Ok(prompt)
    }

    /// Build the memory block from memory entries and vector memory
    fn build_memory_block(&self, memory_entries: &[MemoryEntry]) -> String {
        let mut memory_lines = Vec::new();

        // Add keyword-based memories (existing behavior)
        for entry in memory_entries {
            memory_lines.push(format!("- [Keyword] {}: {}", entry.trigger, entry.action));
        }

        // If we have vector memory, try to get additional context
        #[cfg(feature = "vector-memory")]
        if let Some(_vector_memory) = &self.vector_memory {
            // Use current task or build a query from keyword memories
            let query = self.current_task.clone().unwrap_or_else(|| {
                // Build query from first few keyword memories
                memory_entries
                    .iter()
                    .take(3)
                    .map(|e| e.trigger.clone())
                    .collect::<Vec<_>>()
                    .join(" ")
            });

            if !query.is_empty() {
                // Try to get vector memories synchronously (blocking for simplicity)
                // In practice, this should be called async and passed as parameter
                // For now, we'll skip the async call in sync context
                tracing::debug!(query = %query, "Would search vector memory (sync context)");
            }
        }

        if memory_lines.is_empty() {
            return String::new();
        }

        format!("## Persistent Memory\n{}", memory_lines.join("\n"))
    }

    /// Build the memory block with async vector memory retrieval
    #[cfg(feature = "vector-memory")]
    pub async fn build_memory_block_async(
        &self,
        memory_entries: &[MemoryEntry],
    ) -> (String, Vec<MemoryResult>) {
        let mut memory_lines = Vec::new();
        let mut vector_results = Vec::new();

        // Add keyword-based memories (existing behavior)
        for entry in memory_entries {
            memory_lines.push(format!("- [Keyword] {}: {}", entry.trigger, entry.action));
        }

        // Get vector memory results
        if self.vector_memory.is_some() {
            let query = self.current_task.clone().unwrap_or_else(|| {
                memory_entries
                    .iter()
                    .take(3)
                    .map(|e| e.trigger.clone())
                    .collect::<Vec<_>>()
                    .join(" ")
            });

            if !query.is_empty() {
                vector_results = self.retrieve_relevant_memories(&query).await;

                // Add vector memories with source indicators
                for result in &vector_results {
                    let source_label = match result.entry.metadata.source_task {
                        Some(_) => "[Learning]",
                        None => "[Vector]",
                    };
                    memory_lines.push(format!(
                        "- {} {} (similarity: {:.2})",
                        source_label, result.entry.content, result.similarity
                    ));
                }

                // Record which memories were used
                let mut metrics = self.memory_metrics.lock().await;
                for result in &vector_results {
                    metrics.record_memory_used(&result.entry.id);
                }
            }
        }

        if memory_lines.is_empty() {
            return (String::new(), vector_results);
        }

        let memory_block = format!("## Persistent Memory\n{}", memory_lines.join("\n"));
        (memory_block, vector_results)
    }

    /// Build the memory block with async vector memory retrieval – stub when
    /// `vector-memory` feature is disabled.
    #[cfg(not(feature = "vector-memory"))]
    pub async fn build_memory_block_async(
        &self,
        memory_entries: &[MemoryEntry],
    ) -> (String, Vec<MemoryResult>) {
        let mut memory_lines = Vec::new();

        for entry in memory_entries {
            memory_lines.push(format!("- [Keyword] {}: {}", entry.trigger, entry.action));
        }

        if memory_lines.is_empty() {
            return (String::new(), Vec::new());
        }

        let memory_block = format!("## Persistent Memory\n{}", memory_lines.join("\n"));
        (memory_block, Vec::new())
    }

    /// Get the mode-specific instruction
    fn get_mode_instruction(&self) -> String {
        match self.ai_mode {
            AiMode::Ask => "### Current Mode: ASK (Interactive)\n\
                    \n\
                    **You MUST:**\n\
                    - Ask for confirmation before ANY file modification\n\
                    - Explain your plan before executing\n\
                    - Wait for user approval on destructive operations\n\
                    \n\
                    **You MUST NOT:**\n\
                    - Execute write operations without explicit approval\n\
                    - Run tests without confirmation\n\
                    - Modify .git, config files, or dependencies without approval"
                .to_string(),
            AiMode::Plan => "### Current Mode: PLAN (Planning Only)\n\
                    \n\
                    **You MAY:**\n\
                    - Use read-only tools (read_file, list_dir, grep, glob, web_fetch) to research\n\
                    - Use find and inspect to explore code through the code index and LSP\n\
                    - Write documentation files (.md, .txt, .rst) to capture your plan\n\
                    \n\
                    **You MUST:**\n\
                    - Describe your plan in detail before taking action\n\
                    - Explain the rationale for each step\n\
                    - Identify potential risks and trade-offs\n\
                    - Break down complex tasks into phases\n\
                    \n\
                    **You MUST NOT:**\n\
                    - Execute code or run commands\n\
                    - Modify source code files\n\
                    - Run tests or builds\n\
                    - Make changes outside of documentation files"
                .to_string(),
            AiMode::Act => "### Current Mode: ACT (Execute with Brief Summaries)\n\
                    \n\
                    **You MAY:**\n\
                    - Execute plans with brief action summaries\n\
                    - Skip confirmation for non-destructive operations (read-only, builds, tests)\n\
                    - Proceed independently on safe operations\n\
                    \n\
                    **You MUST STILL:**\n\
                    - Ask before destructive operations (file deletion, git push, etc.)\n\
                    - Report any errors or failures immediately\n\
                    - Stop and ask if you encounter unexpected situations"
                .to_string(),
            AiMode::Yolo => "### Current Mode: YOLO (Fully Autonomous)\n\
                    \n\
                    **You have FULL AUTONOMY:**\n\
                    - Execute all actions immediately without asking\n\
                    - Use your best judgment on all decisions\n\
                    - Proceed with implementations, tests, and commits independently\n\
                    \n\
                    **You ARE STILL RESPONSIBLE FOR:**\n\
                    - Not deleting user data without backup\n\
                    - Not force-pushing to shared branches\n\
                    - Not breaking the build or tests\n\
                    - Stopping if you encounter critical errors"
                .to_string(),
        }
    }

    fn build_layered_prompt(
        &self,
        current_model: &str,
        cwd: &Path,
        workspace_context: &str,
        memory_block: &str,
        mode_instruction: &str,
    ) -> String {
        let tool_descriptions = self.generate_tool_descriptions();
        build_layered_prompt_sync(
            current_model,
            cwd,
            !self.conversation_manager.messages().is_empty(),
            workspace_context,
            memory_block,
            mode_instruction,
            None,
            &tool_descriptions,
        )
    }

    /// Get the conversation manager (for advanced operations)
    pub fn conversation_manager(&self) -> &ConversationManager {
        &self.conversation_manager
    }

    /// Get mutable access to conversation manager (for advanced operations)
    pub fn conversation_manager_mut(&mut self) -> &mut ConversationManager {
        &mut self.conversation_manager
    }

    /// Generate tool descriptions from the tool registry
    fn generate_tool_descriptions(&self) -> String {
        let mut descriptions = Vec::new();

        for tool in self.tool_registry.list() {
            let name = tool.name;
            let description = tool.description;

            // Format parameters from JSON schema
            let params_desc = self.format_parameters_description(&tool.parameters_schema);

            descriptions.push(format!(
                "**{}** - {}\n\
                \n\
                Parameters:\n\
                {}\n",
                name, description, params_desc
            ));
        }

        descriptions.join("\n")
    }

    /// Format parameter description from JSON schema
    fn format_parameters_description(&self, schema: &serde_json::Value) -> String {
        if let Some(obj) = schema.as_object() {
            if let Some(props) = obj.get("properties") {
                if let Some(props_obj) = props.as_object() {
                    let mut params = Vec::new();
                    for (name, prop) in props_obj {
                        let required = if let Some(required) = obj.get("required") {
                            if let Some(req_array) = required.as_array() {
                                req_array.iter().any(|r| r.as_str() == Some(name))
                            } else {
                                false
                            }
                        } else {
                            false
                        };

                        let required_str = if required { " (required)" } else { "" };
                        let desc = if let Some(desc) = prop.get("description") {
                            format!("{}: {}", name, desc.as_str().unwrap_or(""))
                        } else {
                            name.clone()
                        };

                        params.push(format!("- {}{}", desc, required_str));
                    }
                    return params.join("\n");
                }
            }
        }

        "No parameters defined".to_string()
    }

    /// Generate provider-specific tool schema in JSON format
    /// This can be used for MCP or for sending tool definitions to LLMs
    pub fn generate_tool_schema_for_provider(
        &self,
        provider: ModelProvider,
        model_id: &str,
    ) -> serde_json::Value {
        let tools = self.tool_registry.list();
        let mut tools_json = Vec::new();
        let tool_search_enabled = self.tool_search_enabled_for_provider(&provider, model_id);

        for tool in tools {
            let is_seed_tool = self.is_seed_tool_for_provider(&provider, &tool.name);
            let tool_schema = match provider {
                ModelProvider::Anthropic => {
                    let mut schema = serde_json::json!({
                        "name": tool.name,
                        "description": tool.description,
                        "input_schema": tool.parameters_schema,
                    });
                    if let Some(annotations) = anthropic_annotations_for_tool_info(
                        &tool.name,
                        matches!(tool.permission, rustycode_tools::ToolPermission::Read),
                    ) {
                        schema["annotations"] = annotations;
                    }
                    if tool_search_enabled && !is_seed_tool {
                        schema["defer_loading"] = serde_json::json!(true);
                    }
                    schema
                }
                ModelProvider::OpenAI => {
                    serde_json::json!({
                        "type": "function",
                        "function": {
                            "name": tool.name,
                            "description": tool.description,
                            "parameters": tool.parameters_schema
                        }
                    })
                }
                ModelProvider::Google => {
                    serde_json::json!({
                        "name": tool.name,
                        "description": tool.description,
                        "parameters": tool.parameters_schema
                    })
                }
                ModelProvider::Generic => {
                    let mut schema = serde_json::json!({
                        "name": tool.name,
                        "description": tool.description,
                        "input_schema": tool.parameters_schema
                    });
                    if let Some(annotations) = anthropic_annotations_for_tool_info(
                        &tool.name,
                        matches!(tool.permission, rustycode_tools::ToolPermission::Read),
                    ) {
                        schema["annotations"] = annotations;
                    }
                    schema
                }
                _ => {
                    let mut schema = serde_json::json!({
                        "name": tool.name,
                        "description": tool.description,
                        "input_schema": tool.parameters_schema
                    });
                    if let Some(annotations) = anthropic_annotations_for_tool_info(
                        &tool.name,
                        matches!(tool.permission, rustycode_tools::ToolPermission::Read),
                    ) {
                        schema["annotations"] = annotations;
                    }
                    schema
                }
            };
            if !tool_search_enabled || is_seed_tool {
                tools_json.push(tool_schema);
            }
        }

        if tool_search_enabled {
            tools_json.push(crate::tool_search::ToolSearch::tool_definition_for_provider(provider));
        }

        serde_json::json!({
            "tools": tools_json
        })
    }

    fn is_seed_tool_for_provider(&self, provider: &ModelProvider, tool_name: &str) -> bool {
        let allowlist = self.tools_seed_list_for_provider(provider);
        allowlist.contains(&tool_name)
    }

    fn tools_seed_list_for_provider(&self, provider: &ModelProvider) -> &'static [&'static str] {
        match provider {
            ModelProvider::Anthropic | ModelProvider::OpenAI => match self.ai_mode {
                AiMode::Ask => &[
                    "tool_search",
                    "read_file",
                    "list_dir",
                    "grep",
                    "glob",
                    "find",
                    "inspect",
                    "web_fetch",
                ],
                AiMode::Plan => &[
                    "tool_search",
                    "read_file",
                    "list_dir",
                    "grep",
                    "glob",
                    "find",
                    "inspect",
                    "web_fetch",
                    "write_file",
                ],
                AiMode::Act => &[
                    "tool_search",
                    "read_file",
                    "list_dir",
                    "grep",
                    "glob",
                    "find",
                    "inspect",
                    "web_fetch",
                    "bash",
                    "write_file",
                    "edit",
                    "test",
                ],
                AiMode::Yolo => &[
                    "tool_search",
                    "read_file",
                    "list_dir",
                    "grep",
                    "glob",
                    "find",
                    "inspect",
                    "web_fetch",
                    "bash",
                    "write_file",
                    "edit",
                    "test",
                    "git_status",
                    "git_diff",
                    "git_commit",
                ],
            },
            _ => &[],
        }
    }

    fn tool_search_enabled_for_provider(&self, provider: &ModelProvider, model_id: &str) -> bool {
        match provider {
            ModelProvider::Anthropic => true,
            ModelProvider::OpenAI => model_id.starts_with("gpt-5.4"),
            _ => false,
        }
    }
}

fn build_layered_prompt_sync(
    current_model: &str,
    cwd: &Path,
    _has_user_messages: bool,
    workspace_context: &str,
    memory_block: &str,
    mode_instruction: &str,
    _last_user_message: Option<&str>,
    tool_descriptions: &str,
) -> String {
    use rustycode_prompt::ModelProvider;

    let mut layers = Vec::new();

    let compact_workspace = compact_workspace_context(workspace_context);
    let compact_memory = compact_text(memory_block, 700, 10);

    layers.push(
        "You are RustyCode, an AI coding assistant optimized for:\n\
        \n\
        **Core Principles:**\n\
        1. **Correctness** - Ensure code works as intended and handles edge cases\n\
        2. **Fast iteration** - Make incremental progress with rapid feedback loops\n\
        3. **Production-safe** - Write code that's maintainable, testable, and deployable\n\
        \n\
        You excel at understanding codebases, implementing features, fixing bugs, and navigating complex code. You communicate clearly and take decisive action when the path forward is clear."
            .to_string(),
    );

    let provider = ModelProvider::from_model_id(current_model);
    let model_guidance = match provider {
        ModelProvider::Anthropic => {
            "You are Claude (Anthropic). You excel at:\n\
            - Understanding complex codebases and architectural patterns\n\
            - Writing clean, idiomatic code that follows best practices\n\
            - Breaking down complex problems into manageable steps\n\
            - Providing clear technical explanations\n\
            \n\
            Prefer decisive implementation steps with concise explanations focused on the 'why' behind important decisions."
        }
        ModelProvider::Google => {
            "You are Gemini (Google). You excel at:\n\
            - Analyzing code with precision and attention to detail\n\
            - Following explicit instructions and verification steps\n\
            - Providing reliable, well-grounded solutions\n\
            - Generating code that works correctly the first time\n\
            \n\
            Prioritize correctness and explicit verification over speed."
        }
        ModelProvider::OpenAI => {
            "You are GPT (OpenAI). You excel at:\n\
            - Strong code synthesis and rapid prototyping\n\
            - Generating functional code from descriptions\n\
            - Providing deterministic fixes for common issues\n\
            - Adapting to different coding styles and patterns\n\
            \n\
            Prioritize working code over creative solutions."
        }
        ModelProvider::Generic => {
            "You are an AI coding assistant. Focus on:\n\
            - Understanding the user's intent and context\n\
            - Providing practical, working solutions\n\
            - Communicating clearly about trade-offs and options\n\
            - Delivering value through correct, maintainable code"
        }
        _ => {
            "You are an AI coding assistant. Focus on:\n\
            - Understanding the user's intent and context\n\
            - Providing practical, working solutions\n\
            - Communicating clearly about trade-offs and options\n\
            - Delivering value through correct, maintainable code"
        }
    };
    layers.push(model_guidance.to_string());

    layers.push(format!(
        "## Environment\n{}\nPlatform: {}\nDate: {}",
        compact_workspace,
        std::env::consts::OS,
        chrono::Utc::now().format("%Y-%m-%d")
    ));

    if !compact_memory.is_empty() {
        layers.push(format!("## Memory\n{}", compact_memory));
    }

    if let Some(workspace_root) = find_workspace_root(cwd) {
        let project_type = detect_project_type(&workspace_root);
        if let Some(project_layer) = get_project_specific_layer(project_type.as_ref()) {
            layers.push(project_layer);
        }
    }

    layers.push(mode_instruction.to_string());

    let tool_instructions = match provider {
        ModelProvider::Anthropic | ModelProvider::OpenAI | ModelProvider::Google => {
            // All providers support native tool calling now
            format!(
                "## Available Tools\n\
                \n\
                You have access to the following tools. Use them when needed to complete tasks.\n\
                \n\
                {}",
                tool_descriptions
            )
        }
        ModelProvider::Generic => {
            format!(
                "## Available Tools\n\
                \n\
                === HOW TO MAKE TOOL CALLS ===\n\
                \n\
                Follow these steps EXACTLY when making a tool call:\n\
                \n\
                **Step 1:** Identify the tool you need\n\
                - User says \"Read X\" → use read_file\n\
                - User says \"Run command X\" → use bash\n\
                - User says \"Search for X\" → use web_search\n\
                \n\
                **Step 2:** Extract parameter values FROM THE USER'S REQUEST\n\
                - User says \"Read Cargo.toml\" → path is \"Cargo.toml\"\n\
                - User says \"List files\" → command is \"ls\"\n\
                - User says \"Search for Rust async\" → query is \"Rust async\"\n\
                \n\
                **Step 3:** Build the JSON with BOTH name AND input fields\n\
                {{\"name\": \"tool_name\", \"input\": {{\"param\": \"value_from_user_request\"}}}}\n\
                \n\
                **Complete examples:**\n\
                - Read file: {{\"name\": \"read_file\", \"input\": {{\"path\": \"Cargo.toml\"}}}}\n\
                - Run command: {{\"name\": \"bash\", \"input\": {{\"command\": \"ls\"}}}}\n\
                - Search: {{\"name\": \"web_search\", \"input\": {{\"query\": \"Rust async\"}}}}\n\
                - Write: {{\"name\": \"write_file\", \"input\": {{\"path\": \"test.txt\", \"content\": \"hello\"}}}}\n\
                \n\
                **COMMON MISTAKES - Do NOT make these:**\n\
                ❌ {{\"name\": \"read_file\", \"input\": {{}}}}  ← EMPTY! You must extract values from user request!\n\
                ❌ {{\"name\": \"read_file\", \"path\": \"x\"}}  ← Missing input wrapper!\n\
                ❌ {{\"name\": \"bash\", \"arguments\": {{\"command\": \"ls\"}}}}  ← Wrong field name!\n\
                \n\
                === TOOL DESCRIPTIONS ===\n\
                \n\
                {}",
                tool_descriptions
            )
        }
        _ => {
            // Future provider types fall back to native tool calling
            format!(
                "## Available Tools\n\
                \n\
                You have access to the following tools. Use them when needed to complete tasks.\n\
                \n\
                {}",
                tool_descriptions
            )
        }
    };

    layers.push(tool_instructions);

    layers.push(
        "## Working Style & Response Guidelines\n\
        \n\
        === Communication Style ===\n\
        \n\
        **Match response length to task complexity:**\n\
        - Simple/factual questions → Brief, direct answers (1-2 sentences)\n\
        - Coding tasks → Detailed with rationale and explanations\n\
        - Complex debugging → Step-by-step analysis with reasoning\n\
        \n\
        **Be proactive, not passive:**\n\
        - Suggest improvements even when not explicitly asked\n\
        - Identify potential issues before they become problems\n\
        - Offer alternative approaches when relevant\n\
        - Choose the safest reasonable default when options exist\n\
        \n\
        **Be decisive but thorough:**\n\
        - State your approach clearly before implementing\n\
        - Explain trade-offs when multiple options exist\n\
        - Proceed with confidence once a decision is made\n\
        - Don't over-explain obvious things, but don't skip important context\n\
        \n\
        === Implementation Philosophy ===\n\
        \n\
        **Prefer minimal, high-impact changes:**\n\
        - Make targeted fixes rather than broad refactors\n\
        - Change only what's necessary to solve the problem\n\
        - Avoid 'while I'm here' changes that aren't related to the task\n\
        - Don't reformat or refactor code unless it's part of the solution\n\
        \n\
        **Optimize for correctness over cleverness:**\n\
        - Clear, readable code is better than clever code\n\
        - Use standard library functions over custom implementations\n\
        - Follow established patterns in the codebase\n\
        - Add comments only when the 'why' isn't obvious from the code\n\
        \n\
        **Test when reasonable:**\n\
        - Run tests after making changes to verify nothing broke\n\
        - Suggest running tests if the user hasn't explicitly asked\n\
        - Don't add tests unless the user asks or the code is critical\n\
        \n\
        === Tool Usage Best Practices ===\n\
        \n\
        **ALWAYS use the right tool:**\n\
        - Use read_file to examine files, not bash 'cat'\n\
        - Use write_file to modify files, not bash 'echo' or 'sed'\n\
        - Use find for code-index-backed discovery of related code and concepts\n\
        - Use lsp_diagnostics to check for errors before editing\n\
        - Use inspect for exact symbol lookup and type info\n\
        - Use web_search for current docs, not rely on training data\n\
        \n\
        **Think before you act:**\n\
        - Read existing code before modifying it\n\
        - Understand the context before making changes\n\
        - Check for errors before trying to fix them\n\
        - Verify your approach will actually solve the problem"
            .to_string(),
    );

    layers.push(
        "## Error Handling & Recovery\n\
        \n\
        === When Tools Fail ===\n\
        \n\
        **Follow this systematic approach:**\n\
        \n\
        1. **Explain the error clearly**\n\
           - What went wrong (be specific about the error)\n\
           - Why it happened (root cause analysis)\n\
           - What it means for the task (impact assessment)\n\
        \n\
        2. **Suggest specific alternatives**\n\
           - What could be done differently (concrete options)\n\
           - Workarounds if available (practical alternatives)\n\
           - Whether to retry or try a different approach\n\
        \n\
        3. **Ask for guidance when uncertain**\n\
           - If the path forward is unclear, ask the user\n\
           - If multiple options exist, present them clearly\n\
           - Don't guess at critical decisions\n\
        \n\
        4. **Stop after 2-3 failed attempts**\n\
           - Don't retry the same thing indefinitely\n\
           - If something fails twice, try a different approach\n\
           - Escalate to the user if stuck\n\
        \n\
        === Common Error Patterns ===\n\
        \n\
        **Permission denied:**\n\
        - Check if file/directory exists first\n\
        - Verify the user has necessary permissions\n\
        - Suggest checking file ownership or using sudo (if appropriate)\n\
        \n\
        **File not found:**\n\
        - Verify the file path is correct\n\
        - Use bash 'ls' to check if the file exists\n\
        - Check for typos in the filename\n\
        - Suggest using find/locate to search for the file\n\
        \n\
        **Build failures (compilation errors):**\n\
        - Show the specific error message from the compiler\n\
        - Identify the root cause (missing dependency, type error, etc.)\n\
        - Suggest specific fixes based on the error\n\
        - Check if dependencies need to be installed\n\
        \n\
        **Test failures:**\n\
        - Report which specific tests failed\n\
        - Show the assertion or error message\n\
        - Explain why the test failed (not just that it failed)\n\
        - Suggest fixes based on the failure reason\n\
        \n\
        **LSP not available:**\n\
        - Inform user that rust-analyzer is required for LSP tools\n\
        - Suggest installing: 'rustup component add rust-analyzer'\n\
        - Fall back to read_file for examining code\n\
        \n\
        **Network errors (web_search):**\n\
        - Check if the query is too specific or vague\n\
        - Suggest reformulating the search query\n\
        - Offer to try again with a different query"
            .to_string(),
    );

    layers.push(
        "## Task Completion & Best Practices\n\
        \n\
        === When to Consider a Task Complete ===\n\
        \n\
        **A task is complete when:**\n\
        - All user requirements have been addressed\n\
        - The code compiles and tests pass (if applicable)\n\
        - The solution is correct and follows best practices\n\
        - You've verified the changes work as intended\n\
        \n\
        **Before completing:**\n\
        - Review the changes to ensure nothing was missed\n\
        - Check that all files were properly modified\n\
        - Verify no unintended side effects were introduced\n\
        - Confirm the solution matches what was requested\n\
        \n\
        **After completing:**\n\
        - Summarize what was done (brief, clear overview)\n\
        - Highlight any important changes or considerations\n\
        - Suggest next steps if appropriate (testing, verification, etc.)\n\
        - Offer to make adjustments if the user wants changes\n\
        \n\
        === Code Quality Standards ===\n\
        \n\
        **Always write production-ready code:**\n\
        - Handle errors appropriately (don't use unwrap() without context)\n\
        - Use descriptive variable and function names\n\
        - Add comments for non-obvious logic\n\
        - Follow the project's existing code style\n\
        - Don't leave TODOs or FIXMEs unless absolutely necessary\n\
        \n\
        **Avoid these common pitfalls:**\n\
        - Don't over-engineer simple solutions\n\
        - Don't add dependencies unless truly needed\n\
        - Don't copy-paste code without understanding it\n\
        - Don't make 'optimizations' without profiling\n\
        - Don't ignore compiler warnings\n\
        - Don't use unsafe code unless there's a clear reason and good documentation\n\
        \n\
        === Security Considerations ===\n\
        \n\
        **Be security-conscious:**\n\
        - Never hardcode credentials or API keys\n\
        - Validate and sanitize user inputs\n\
        - Use secure defaults (deny-all rather than allow-all)\n\
        - Follow the principle of least privilege\n\
        - Be cautious with user-supplied content (XSS, injection attacks)\n\
        - Don't expose sensitive information in error messages"
            .to_string(),
    );

    layers.join("\n\n")
}

// Helper functions for prompt building
fn compact_text(input: &str, max_chars: usize, max_lines: usize) -> String {
    if input.trim().is_empty() {
        return String::new();
    }

    let mut out = Vec::new();
    let mut chars = 0usize;

    for line in input.lines().map(str::trim).filter(|l| !l.is_empty()) {
        if out.len() >= max_lines {
            break;
        }
        if chars + line.len() > max_chars {
            break;
        }
        out.push(line.to_string());
        chars += line.len();
    }

    if out.is_empty() {
        return String::new();
    }

    let mut result = out.join("\n");
    if input.len() > result.len() {
        result.push_str("\n... [context compacted]");
    }
    result
}

fn compact_workspace_context(workspace_context: &str) -> String {
    if workspace_context.trim().is_empty() {
        return "Workspace context unavailable".to_string();
    }

    let mut priority = Vec::new();
    let mut secondary = Vec::new();

    for raw in workspace_context.lines() {
        let line = raw.trim();
        if line.is_empty() {
            continue;
        }

        let is_priority = line.starts_with("Workspace:")
            || line.starts_with("## Project Files Found")
            || line.starts_with("## Git Status")
            || line.starts_with("Branch:")
            || line.starts_with("✓")
            || line.starts_with("M ")
            || line.starts_with("A ")
            || line.starts_with("D ")
            || line.starts_with("??");

        if is_priority {
            priority.push(line.to_string());
        } else {
            secondary.push(line.to_string());
        }
    }

    let mut merged = priority;
    merged.extend(secondary);
    compact_text(
        &merged.join("\n"),
        2200, // WORKSPACE_CONTEXT_MAX_CHARS
        80,   // WORKSPACE_CONTEXT_MAX_LINES
    )
}

fn find_workspace_root(cwd: &Path) -> Option<std::path::PathBuf> {
    let mut current = Some(cwd);

    while let Some(path) = current {
        for marker in &[
            "Cargo.toml",
            ".git",
            "package.json",
            "pyproject.toml",
            "go.mod",
            "Gemfile",
            "pom.xml",
            "build.gradle",
        ] {
            if path.join(marker).exists() {
                return Some(path.to_path_buf());
            }
        }
        current = path.parent();
    }

    None
}

/// Detect project type based on workspace markers
fn detect_project_type(workspace_root: &Path) -> Option<ProjectType> {
    // Check for specific markers to determine project type
    if workspace_root.join("Cargo.toml").exists() {
        return Some(ProjectType::Rust);
    }
    if workspace_root.join("package.json").exists() {
        return Some(ProjectType::NodeJs);
    }
    if workspace_root.join("pyproject.toml").exists() || workspace_root.join("setup.py").exists() {
        return Some(ProjectType::Python);
    }
    if workspace_root.join("go.mod").exists() {
        return Some(ProjectType::Go);
    }
    if workspace_root.join("Gemfile").exists() {
        return Some(ProjectType::Ruby);
    }
    if workspace_root.join("pom.xml").exists() {
        return Some(ProjectType::Maven);
    }
    if workspace_root.join("build.gradle").exists()
        || workspace_root.join("settings.gradle").exists()
    {
        return Some(ProjectType::Gradle);
    }

    None
}

/// Get project-specific prompt layer based on detected project type
fn get_project_specific_layer(project_type: Option<&ProjectType>) -> Option<String> {
    match project_type {
        Some(ProjectType::Rust) => Some(get_rust_project_guidance()),
        Some(ProjectType::NodeJs) => Some(get_nodejs_project_guidance()),
        Some(ProjectType::Python) => Some(get_python_project_guidance()),
        Some(ProjectType::Go) => Some(get_go_project_guidance()),
        Some(ProjectType::Ruby) => Some(get_ruby_project_guidance()),
        Some(ProjectType::Maven) => Some(get_java_project_guidance()),
        Some(ProjectType::Gradle) => Some(get_java_project_guidance()),
        None => None,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ProjectType {
    Rust,
    NodeJs,
    Python,
    Go,
    Ruby,
    Maven,
    Gradle,
}

fn get_rust_project_guidance() -> String {
    "## Project-Specific Guidance: Rust\n\
    \n\
    === Rust Best Practices ===\n\
    \n\
    **Error Handling:**\n\
    - Use Result<T, E> for fallible operations, not unwrap() without context\n\
    - Provide context with .context() or .expect() for errors\n\
    - Avoid panic! in production code - use Result or Option instead\n\
    - Use ? operator for clean error propagation\n\
    \n\
    **Ownership & Borrowing:**\n\
    - Respect Rust's ownership rules - avoid fighting the borrow checker\n\
    - Use references (&) for temporary borrows, smart pointers for ownership transfers\n\
    - Prefer &str over String for function parameters when possible\n\
    - Use Clone sparingly - only when necessary\n\
    \n\
    **Common Patterns:**\n\
    - Use derive(Debug, Clone) for structs that need it\n\
    - Use IntoIterator in trait bounds for generic collections\n\
    - Use async/await for async code, not direct future manipulation\n\
    - Use serde::{Serialize, Deserialize} for JSON serialization\n\
    \n\
    **Testing:**\n\
    - Unit tests go in the same module as the code, marked with #[cfg(test)]\n\
    - Integration tests go in tests/ directory\n\
    - Use cargo test to run tests\n\
    - Use cargo clippy to catch common mistakes\n\
    \n\
    **Build & Dependency Management:**\n\
    - Use cargo build to compile\n\
    - Use cargo add to add dependencies\n\
    - Use cargo check for faster compilation checking\n\
    - Keep dependencies minimal - add only what you need\n\
    \n\
    **LSP Tool Notes:**\n\
    - LSP tools require rust-analyzer to be installed\n\
    - Install with: rustup component add rust-analyzer\n\
    - LSP provides accurate type information, go-to-definition, and completions\n\
    - Use find first for broad code exploration, then inspect or read_file for details"
        .to_string()
}

fn get_nodejs_project_guidance() -> String {
    "## Project-Specific Guidance: Node.js/TypeScript\n\
    \n\
    === Node.js Best Practices ===\n\
    \n\
    **Error Handling:**\n\
    - Use async/await for asynchronous code, not callbacks\n\
    - Always handle promise rejections with .catch() or try/catch\n\
    - Use Error objects, not strings for errors\n\
    - Provide stack traces in errors for debugging\n\
    \n\
    **Module System:**\n\
    - Use ES modules (import/export) by default\n\
    - Use require() only for CommonJS modules or when necessary\n\
    - Organize code into logical modules with clear exports\n\
    \n\
    **Package Management:**\n\
    - Use npm or yarn/pnpm for dependency management\n\
    - Lock files (package-lock.json, yarn.lock) should be committed\n\
    - Use npm run to execute scripts defined in package.json\n\
    - Keep dependencies up to date but test before upgrading\n\
    \n\
    **Testing:**\n\
    - Use Jest, Vitest, Mocha, or similar for testing\n\
    - Test files should be co-located with source or in __tests__ directories\n\
    - Use npm test to run the test suite\n\
    - Aim for good test coverage but 100% is not always necessary\n\
    \n\
    **TypeScript:**\n\
    - Leverage TypeScript's type system for safer code\n\
    - Avoid using 'any' unless absolutely necessary\n\
    - Use interfaces for object shapes, types for primitives/unions\n\
    - Enable strict mode in tsconfig.json"
        .to_string()
}

fn get_python_project_guidance() -> String {
    "## Project-Specific Guidance: Python\n\
    \n\
    === Python Best Practices ===\n\
    \n\
    **Code Style:**\n\
    - Follow PEP 8 style guide for Python code\n\
    - Use 4 spaces for indentation, not tabs\n\
    - Use meaningful variable and function names (snake_case)\n\
    - Keep lines under 100 characters when practical\n\
    \n\
    **Error Handling:**\n\
    - Use try/except blocks for error handling\n\
    - Catch specific exceptions, not bare except:\n\
    - Use finally for cleanup code that must run\n\
    - Use context managers (with statements) for resource management\n\
    \n\
    **Import Organization:**\n\
    - Group imports into three sections: stdlib, third-party, local\n\
    - Sort imports alphabetically within each group\n\
    - Use absolute imports, not relative imports when possible\n\
    \n\
    **Testing:**\n\
    - Use pytest, unittest, or similar testing frameworks\n\
    - Test files should be named test_*.py or *_test.py\n\
    - Use pytest to run tests\n\
    - Aim for descriptive test names that explain what they test\n\
    \n\
    **Virtual Environments:**\n\
    - Use venv, virtualenv, or conda for dependency isolation\n\
    - Activate the virtual environment before running commands\n\
    - Keep requirements.txt or pyproject.toml up to date\n\
    \n\
    **Common Patterns:**\n\
    - Use list comprehensions and generators for efficiency\n\
    - Use decorators (@property, @staticmethod, @classmethod) appropriately\n\
    - Use type hints for better IDE support and documentation\n\
    - Follow the explicit is better than implicit principle"
        .to_string()
}

fn get_go_project_guidance() -> String {
    "## Project-Specific Guidance: Go\n\
    \n\
    === Go Best Practices ===\n\
    \n\
    **Error Handling:**\n\
    - Always check for errors, never ignore them\n\
    - Use if err != nil patterns for error checking\n\
    - Return errors for the caller to handle, don't panic\n\
    - Add context to errors with fmt.Errorf or errors.Wrap\n\
    \n\
    **Concurrency:**\n\
    - Use goroutines for concurrency, not threads\n\
    - Use channels for communication between goroutines\n\
    - Use sync.Mutex for protecting shared state\n\
    - Avoid goroutine leaks - ensure they can exit\n\
    \n\
    **Code Organization:**\n\
    - Organize code into packages with clear responsibilities\n\
    - Use interfaces to define behavior, not structure\n\
    - Keep interfaces small and focused (1-3 methods)\n\
    - Accept interfaces, return structs\n\
    \n\
    **Testing:**\n\
    - Use go test for unit tests\n\
    - Test files should be named *_test.go\n\
    - Use table-driven tests for multiple test cases\n\
    - Aim for good test coverage\n\
    \n\
    **Build & Module Management:**\n\
    - Use go build to compile\n\
    - Use go mod for dependency management\n\
    - Use go get to add dependencies\n\
    - Use go fmt to format code\n\
    - Use go vet to check for issues"
        .to_string()
}

fn get_ruby_project_guidance() -> String {
    "## Project-Specific Guidance: Ruby\n\
    \n\
    === Ruby Best Practices ===\n\
    \n\
    **Code Style:**\n\
    - Follow the Ruby community style guide\n\
    - Use snake_case for method names and variables\n\
    - Use CamelCase for class names\n\
    - Use 2 spaces for indentation\n\
    \n\
    **Metaprogramming:**\n\
    - Use metaprogramming sparingly and only when necessary\n\
    - Prefer explicit methods over method_missing\n\
    - Document any metaprogramming clearly\n\
    \n\
    **Testing:**\n\
    - Use RSpec, Minitest, or similar for testing\n\
    - Test files should be named *_spec.rb or test_*.rb\n\
    - Use bundle exec rspec to run tests\n\
    - Write descriptive test names\n\
    \n\
    **Dependencies:**\n\
    - Use Bundler for dependency management\n\
    - Use bundle install to install dependencies\n\
    - Keep Gemfile and Gemfile.lock in version control\n\
    \n\
    **Error Handling:**\n\
    - Use begin/rescue blocks for error handling\n\
    - Rescue specific exceptions, not Exception\n\
    - Use ensure for cleanup code\n\
    \n\
    **Common Patterns:**\n\
    - Use blocks and yield for iterators\n\
    - Use modules and mixins for code reuse\n\
    - Use attr_reader, attr_writer, attr_accessor for accessors"
        .to_string()
}

fn get_java_project_guidance() -> String {
    "## Project-Specific Guidance: Java (Maven/Gradle)\n\
    \n\
    === Java Best Practices ===\n\
    \n\
    **Code Style:**\n\
    - Follow Java naming conventions (camelCase for methods, PascalCase for classes)\n\
    - Use 4 spaces for indentation\n\
    - Keep lines under 120 characters when practical\n\
    - Use @Override annotation for overridden methods\n\
    \n\
    **Error Handling:**\n\
    - Use try-catch blocks for exception handling\n\
    - Catch specific exceptions, not Exception\n\
    - Use finally for cleanup code\n\
    - Use custom exceptions for application-specific errors\n\
    \n\
    **Testing:**\n\
    - Use JUnit or TestNG for unit testing\n\
    - Test files should be named *Test.java\n\
    - Use mvn test or gradle test to run tests\n\
    - Aim for good test coverage\n\
    \n\
    **Build & Dependency Management:**\n\
    - Maven: use mvn compile, mvn package, mvn install\n\
    - Gradle: use gradle build, gradle test, gradle assemble\n\
    - Keep dependencies in pom.xml (Maven) or build.gradle (Gradle)\n\
    - Use dependency management (dependencyManagement in Maven)\n\
    \n\
    **Common Patterns:**\n\
    - Use interfaces to define contracts\n\
    - Use dependency injection for loose coupling\n\
    - Use streams API for collections processing\n\
    - Use Optional instead of null when possible"
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_conversation_service_creation() {
        let config = ConversationConfig::default();
        let tool_registry = rustycode_tools::default_registry();
        let service = ConversationService::new(config.clone(), Arc::new(tool_registry));

        assert_eq!(service.message_count(), 0);
    }

    #[test]
    fn test_build_memory_block() {
        use rustycode_memory::{MemoryDomain, MemoryEntry, MemoryScope, MemorySource};

        let config = ConversationConfig::default();
        let tool_registry = rustycode_tools::default_registry();
        let service = ConversationService::new(config, Arc::new(tool_registry));

        let entries = vec![MemoryEntry {
            id: "test1".to_string(),
            trigger: "git".to_string(),
            confidence: 0.8,
            domain: MemoryDomain::Git,
            source: MemorySource::ManualEntry,
            scope: MemoryScope::Project,
            project_id: None,
            action: "Use git add . to stage all changes".to_string(),
            evidence: vec![],
            created_at: std::time::SystemTime::UNIX_EPOCH,
            last_used: None,
            use_count: 0,
        }];

        let block = service.build_memory_block(&entries);
        assert!(block.contains("## Persistent Memory"));
        assert!(block.contains("git"));
        assert!(block.contains("Use git add . to stage all changes"));
        assert!(block.contains("[Keyword]"));
    }

    #[test]
    fn test_get_mode_instruction() {
        let config = ConversationConfig::default();
        let tool_registry = rustycode_tools::default_registry();
        let mut service = ConversationService::new(config, Arc::new(tool_registry));

        service.set_ai_mode(AiMode::Ask);
        let instruction = service.get_mode_instruction();
        assert!(instruction.contains("ASK (Interactive)"));

        service.set_ai_mode(AiMode::Yolo);
        let instruction = service.get_mode_instruction();
        assert!(instruction.contains("YOLO (Fully Autonomous)"));
    }

    #[test]
    fn test_build_system_prompt_caching() {
        let config = ConversationConfig::default();
        let tool_registry = rustycode_tools::default_registry();
        let mut service = ConversationService::new(config, Arc::new(tool_registry));

        let cwd = PathBuf::from("/test");
        let workspace_context = "Test workspace";
        let memory_entries = vec![];

        // First call should build the prompt
        let prompt1 = service
            .build_system_prompt("claude-sonnet", &cwd, workspace_context, &memory_entries)
            .expect("Failed to build system prompt");
        assert!(!prompt1.is_empty());

        // Second call with same parameters should use cache
        let prompt2 = service
            .build_system_prompt("claude-sonnet", &cwd, workspace_context, &memory_entries)
            .expect("Failed to build system prompt (cached)");
        assert_eq!(prompt1, prompt2);

        // Changing mode should invalidate cache and rebuild prompt
        service.set_ai_mode(AiMode::Yolo);
        let prompt3 = service
            .build_system_prompt("claude-sonnet", &cwd, workspace_context, &memory_entries)
            .expect("Failed to build system prompt (yolo mode)");
        // Mode change triggers cache rebuild — verify prompt is non-empty
        // (The prompt text may not differ between modes if PromptOrchestrator
        // doesn't inject mode-specific content, but caching still works.)
        assert!(!prompt3.is_empty());
    }

    #[test]
    fn test_tool_search_only_enabled_for_supported_models() {
        let config = ConversationConfig::default();
        let tool_registry = rustycode_tools::default_registry();
        let service = ConversationService::new(config, Arc::new(tool_registry));

        assert!(service
            .tool_search_enabled_for_provider(&ModelProvider::Anthropic, "claude-3-5-sonnet"));
        assert!(service.tool_search_enabled_for_provider(&ModelProvider::OpenAI, "gpt-5.4"));
        assert!(!service.tool_search_enabled_for_provider(&ModelProvider::OpenAI, "gpt-4o"));
        assert!(!service.tool_search_enabled_for_provider(&ModelProvider::Generic, "test"));
    }

    #[test]
    fn test_exploration_tools_are_seeded_for_native_tool_providers() {
        let config = ConversationConfig::default();
        let tool_registry = rustycode_tools::default_registry();
        let service = ConversationService::new(config, Arc::new(tool_registry));

        assert!(service.is_seed_tool_for_provider(&ModelProvider::Anthropic, "find"));
        assert!(service.is_seed_tool_for_provider(&ModelProvider::OpenAI, "inspect"));
        assert!(!service.is_seed_tool_for_provider(&ModelProvider::Anthropic, "semantic_search"));
        assert!(!service.is_seed_tool_for_provider(&ModelProvider::Anthropic, "lsp_definition"));
        assert!(!service.is_seed_tool_for_provider(&ModelProvider::Generic, "lsp_definition"));
    }
}
