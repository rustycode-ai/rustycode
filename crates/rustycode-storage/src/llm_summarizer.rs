//! LLM-Powered Session Summarization
//!
//! This module provides rich, AI-generated session summaries by leveraging
//! LLM capabilities to analyze session events, extract insights, and generate
//! narrative summaries that go beyond basic metrics.
//!
//! ## Features
//!
//! - **Narrative Summaries**: Natural language descriptions of session activities
//! - **Technical Details**: Extraction of code changes, architectural decisions
//! - **Learning Extraction**: Identification of lessons learned and patterns
//! - **Decision Tracking**: Log of key decisions made during the session
//! - **Pattern Recognition**: Identification of reusable solution patterns
//!
//! ## Example
//!
//! ```rust,no_run
//! use rustycode_storage::llm_summarizer::{
//!     SummarizerConfig, SessionSummarizer, SummaryRequest, RichSummary
//! };
//! use rustycode_storage::session_capture::{InteractionEvent, SessionMetrics};
//!
//! # fn main() -> anyhow::Result<()> {
//! // Configure the summarizer
//! let config = SummarizerConfig {
//!     model: "claude-sonnet".to_string(),
//!     max_summary_tokens: 2000,
//!     temperature: 0.7,
//!     extract_learnings: true,
//!     extract_patterns: true,
//! };
//!
//! // Create summarizer
//! let summarizer = SessionSummarizer::new(config);
//!
//! // Prepare request with session data
//! let request = SummaryRequest {
//!     session_events: vec![],
//!     session_metrics: SessionMetrics::default(),
//!     existing_learnings: vec![],
//! };
//!
//! // Generate rich summary
//! let summary = summarizer.summarize_session(request)?;
//! println!("Narrative: {}", summary.narrative_summary);
//! # Ok(())
//! # }
//! ```

use crate::session_capture::{InteractionEvent, SessionMetrics, SessionOutcome, SessionSummary};
use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Configuration for the LLM-based summarizer
///
/// Controls model selection, token limits, temperature, and
/// which extraction features are enabled.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SummarizerConfig {
    /// The LLM model to use (e.g., "claude-sonnet", "gpt-4")
    pub model: String,
    /// Maximum tokens for generated summaries
    pub max_summary_tokens: usize,
    /// Temperature for generation (0.0 - 1.0)
    pub temperature: f32,
    /// Whether to extract learnings from the session
    pub extract_learnings: bool,
    /// Whether to identify reusable patterns
    pub extract_patterns: bool,
}

impl Default for SummarizerConfig {
    fn default() -> Self {
        Self {
            model: "claude-sonnet".to_string(),
            max_summary_tokens: 2000,
            temperature: 0.7,
            extract_learnings: true,
            extract_patterns: true,
        }
    }
}

impl SummarizerConfig {
    /// Create a new config with the specified model
    pub fn with_model(model: impl Into<String>) -> Self {
        Self {
            model: model.into(),
            ..Default::default()
        }
    }

    /// Set the maximum summary tokens
    pub const fn with_max_tokens(mut self, tokens: usize) -> Self {
        self.max_summary_tokens = tokens;
        self
    }

    /// Set the temperature
    pub const fn with_temperature(mut self, temp: f32) -> Self {
        self.temperature = temp.clamp(0.0, 1.0);
        self
    }
}

/// Request for generating a session summary
///
/// Contains all the session data needed for LLM analysis
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SummaryRequest {
    /// All interaction events from the session
    pub session_events: Vec<InteractionEvent>,
    /// Aggregated session metrics
    pub session_metrics: SessionMetrics,
    /// Previously extracted learnings to build upon
    pub existing_learnings: Vec<String>,
}

impl SummaryRequest {
    /// Create a new summary request
    pub const fn new(events: Vec<InteractionEvent>, metrics: SessionMetrics) -> Self {
        Self {
            session_events: events,
            session_metrics: metrics,
            existing_learnings: Vec::new(),
        }
    }

    /// Add existing learnings to the request
    pub fn with_learnings(mut self, learnings: Vec<String>) -> Self {
        self.existing_learnings = learnings;
        self
    }
}

/// Represents a code change identified during the session
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CodeChange {
    /// File path that was modified
    pub file_path: String,
    /// Type of change (create, modify, delete)
    pub change_type: ChangeType,
    /// Brief description of what changed
    pub description: String,
    /// Programming language (if detectable)
    pub language: Option<String>,
    /// Approximate lines changed (if available)
    pub lines_changed: Option<usize>,
    /// Timestamp of the change
    pub timestamp: Option<DateTime<Utc>>,
}

/// Type of code change
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum ChangeType {
    /// New file created
    Created,
    /// Existing file modified
    Modified,
    /// File deleted
    Deleted,
    /// File renamed
    Renamed,
    /// File read but not modified
    Read,
}

impl std::fmt::Display for ChangeType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Created => write!(f, "created"),
            Self::Modified => write!(f, "modified"),
            Self::Deleted => write!(f, "deleted"),
            Self::Renamed => write!(f, "renamed"),
            Self::Read => write!(f, "read"),
        }
    }
}

/// Represents a decision made during the session
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Decision {
    /// Brief description of the decision
    pub description: String,
    /// Rationale or context for the decision
    pub rationale: String,
    /// When the decision was made
    pub timestamp: Option<DateTime<Utc>>,
    /// Whether this was a technical or architectural decision
    pub is_architectural: bool,
    /// Alternatives considered (if any)
    pub alternatives: Vec<String>,
}

/// Enhanced session summary with LLM-generated content
///
/// Extends the basic `SessionSummary` with rich narrative content,
/// technical analysis, and structured insights extracted by an LLM.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RichSummary {
    // === Base SessionSummary fields ===
    /// Unique identifier for the session
    pub session_id: String,
    /// The task or goal of the session
    pub task: String,
    /// Duration of the session in milliseconds
    pub duration_ms: u64,
    /// Key points or milestones from the session
    pub key_points: Vec<String>,
    /// Files that were touched during the session
    pub files_touched: Vec<String>,
    /// Errors that were encountered
    pub errors_encountered: Vec<String>,
    /// Tools that were used
    pub tools_used: Vec<String>,
    /// Outcome of the session
    pub outcome: SessionOutcome,
    /// Learnings or insights from the session
    pub learnings: Vec<String>,
    /// Recommended next steps
    pub next_steps: Vec<String>,
    /// When the session started
    pub started_at: DateTime<Utc>,
    /// When the session ended
    pub ended_at: DateTime<Utc>,

    // === LLM-Enhanced fields ===
    /// Natural language narrative summary of the session
    pub narrative_summary: String,
    /// Technical details and observations
    pub technical_details: Vec<String>,
    /// Code changes extracted from the session
    pub code_changes: Vec<CodeChange>,
    /// Log of key decisions made
    pub decision_log: Vec<Decision>,
    /// Root causes of errors or issues
    pub root_causes: Vec<String>,
    /// Reusable solution patterns identified
    pub solution_patterns: Vec<String>,
}

impl RichSummary {
    /// Convert to a base `SessionSummary` (lossy - drops rich fields)
    pub fn to_base_summary(&self) -> SessionSummary {
        // Note: This requires SessionId parsing. In practice, you'd want to
        // store the original SessionId. For now, we use a placeholder.
        SessionSummary {
            session_id: rustycode_protocol::SessionId::new(),
            task: self.task.clone(),
            duration_ms: self.duration_ms,
            key_points: self.key_points.clone(),
            files_touched: self.files_touched.clone(),
            errors_encountered: self.errors_encountered.clone(),
            tools_used: self.tools_used.clone(),
            outcome: self.outcome,
            learnings: self.learnings.clone(),
            next_steps: self.next_steps.clone(),
            started_at: self.started_at,
            ended_at: self.ended_at,
        }
    }

    /// Get a formatted summary for display
    pub fn format_summary(&self) -> String {
        let mut output = String::new();

        output.push_str(&format!("# Session Summary: {}\n\n", self.task));
        output.push_str(&format!("**Outcome:** {}\n", self.outcome));
        output.push_str(&format!("**Duration:** {}ms\n\n", self.duration_ms));

        if !self.narrative_summary.is_empty() {
            output.push_str("## Narrative\n\n");
            output.push_str(&self.narrative_summary);
            output.push_str("\n\n");
        }

        if !self.code_changes.is_empty() {
            output.push_str(&format!(
                "## Code Changes ({} files)\n\n",
                self.code_changes.len()
            ));
            for change in &self.code_changes {
                output.push_str(&format!(
                    "- `{}`: {}\n",
                    change.file_path, change.change_type
                ));
            }
            output.push('\n');
        }

        if !self.decision_log.is_empty() {
            output.push_str(&format!(
                "## Decisions ({} made)\n\n",
                self.decision_log.len()
            ));
            for decision in &self.decision_log {
                output.push_str(&format!("- {}\n", decision.description));
            }
            output.push('\n');
        }

        if !self.solution_patterns.is_empty() {
            output.push_str("## Patterns Identified\n\n");
            for pattern in &self.solution_patterns {
                output.push_str(&format!("- {pattern}\n"));
            }
        }

        output
    }
}

/// LLM-powered session summarizer
///
/// Uses LLM capabilities to generate rich, narrative summaries
/// of coding sessions with detailed technical analysis.
pub struct SessionSummarizer {
    config: SummarizerConfig,
    // TODO: Add LLM client reference when integrated
    // llm_client: Option<Box<dyn LLMProvider>>,
}

impl SessionSummarizer {
    /// Create a new session summarizer with the given configuration
    ///
    /// # Arguments
    ///
    /// * `config` - Configuration for model selection and extraction options
    ///
    /// # Example
    ///
    /// ```
    /// use rustycode_storage::llm_summarizer::{SessionSummarizer, SummarizerConfig};
    ///
    /// let config = SummarizerConfig::default();
    /// let summarizer = SessionSummarizer::new(config);
    /// ```
    pub const fn new(config: SummarizerConfig) -> Self {
        Self {
            config,
            // llm_client: None,
        }
    }

    /// Generate a rich summary from session data
    ///
    /// This method analyzes the session events using LLM capabilities
    /// to produce a comprehensive `RichSummary` with narrative content,
    /// technical details, and extracted insights.
    ///
    /// # Arguments
    ///
    /// * `request` - The summary request containing session events and metrics
    ///
    /// # Returns
    ///
    /// A `RichSummary` containing the enhanced session analysis
    ///
    /// # Errors
    ///
    /// Returns an error if LLM generation fails or if the response cannot be parsed
    ///
    /// # Example
    ///
    /// ```
    /// use rustycode_storage::llm_summarizer::{
    ///     SessionSummarizer, SummarizerConfig, SummaryRequest
    /// };
    /// use rustycode_storage::session_capture::SessionMetrics;
    ///
    /// let summarizer = SessionSummarizer::new(SummarizerConfig::default());
    /// let request = SummaryRequest::new(vec![], SessionMetrics::default());
    ///
    /// // Note: This uses placeholder LLM responses in current implementation
    /// let summary = summarizer.summarize_session(request).unwrap();
    /// ```
    pub fn summarize_session(&self, request: SummaryRequest) -> Result<RichSummary> {
        // Extract base information from events
        let files_touched = self.extract_files_touched(&request.session_events);
        let tools_used = self.extract_tools_used(&request.session_events);
        let errors_encountered = self.extract_errors(&request.session_events);

        // Generate narrative summary via LLM
        let narrative_summary = self.generate_narrative(&request.session_events);

        // Extract technical details
        let code_changes = self.extract_code_changes(&request.session_events);
        let decision_log = self.extract_decisions(&request.session_events);

        // Generate/learn additional insights
        let technical_details = self.generate_technical_details(&request.session_events);
        let root_causes = if self.config.extract_learnings {
            self.extract_root_causes(&request.session_events)
        } else {
            Vec::new()
        };
        let solution_patterns = if self.config.extract_patterns {
            self.identify_patterns(&request.session_events)
        } else {
            Vec::new()
        };

        // Build key points from events
        let key_points = self.generate_key_points(&request.session_events);

        // Generate next steps
        let next_steps = self.generate_next_steps(&request.session_events, &errors_encountered);

        // Build learnings (combine existing with new)
        let mut learnings = request.existing_learnings.clone();
        learnings.extend(self.extract_session_learnings(&request.session_events));

        // Determine outcome
        let outcome = self.determine_outcome(&request.session_events, &errors_encountered);

        let now = Utc::now();
        let started_at = request
            .session_events
            .first()
            .and_then(|e| match e {
                InteractionEvent::UserMessage { timestamp, .. } => Some(*timestamp),
                InteractionEvent::AssistantMessage { timestamp, .. } => Some(*timestamp),
                _ => None,
            })
            .unwrap_or(now);

        Ok(RichSummary {
            session_id: rustycode_protocol::SessionId::new().to_string(),
            task: self.infer_task(&request.session_events),
            duration_ms: request.session_metrics.session_duration_ms,
            key_points,
            files_touched,
            errors_encountered,
            tools_used,
            outcome,
            learnings,
            next_steps,
            started_at,
            ended_at: now,
            narrative_summary,
            technical_details,
            code_changes,
            decision_log,
            root_causes,
            solution_patterns,
        })
    }

    /// Generate a natural language narrative summary of the session
    ///
    /// Uses LLM to create a coherent story of what happened during the session,
    /// including the problem being solved, approach taken, and outcomes.
    ///
    /// # Arguments
    ///
    /// * `events` - The session events to analyze
    ///
    /// # Returns
    ///
    /// A narrative string describing the session
    pub fn generate_narrative(&self, events: &[InteractionEvent]) -> String {
        if events.is_empty() {
            return "No session events recorded.".to_string();
        }

        // TODO: Replace with actual LLM call using SUMMARY_PROMPT
        // let prompt = SUMMARY_PROMPT
        //     .replace("{{events}}", &self.format_events_for_prompt(events));
        // let response = self.llm_client.generate(&prompt, &self.config)?;

        // Placeholder: Generate a simple narrative from events
        let mut narrative = String::new();

        // Count event types
        let user_msgs = events
            .iter()
            .filter(|e| matches!(e, InteractionEvent::UserMessage { .. }))
            .count();
        let tool_calls = events
            .iter()
            .filter(|e| matches!(e, InteractionEvent::ToolCall { .. }))
            .count();
        let errors = events
            .iter()
            .filter(|e| matches!(e, InteractionEvent::Error { .. }))
            .count();
        let file_ops = events
            .iter()
            .filter(|e| matches!(e, InteractionEvent::FileOperation { .. }))
            .count();

        narrative.push_str(&format!(
            "This session involved {user_msgs} user interactions, {tool_calls} tool executions, "
        ));
        narrative.push_str(&format!(
            "{file_ops} file operations, and {errors} errors. "
        ));

        // Extract first user message as context
        if let Some(InteractionEvent::UserMessage { content, .. }) = events
            .iter()
            .find(|e| matches!(e, InteractionEvent::UserMessage { .. }))
        {
            let preview = truncate_to_char_boundary(content, 100);
            narrative.push_str(&format!(
                "The session began with the user request: '{preview}'"
            ));
            if content.chars().count() > 100 {
                narrative.push_str("...");
            }
            narrative.push('.');
        }

        // Summarize errors if any
        if errors > 0 {
            narrative.push_str(&format!(" {errors} error(s) were encountered and handled."));
        }

        // Note file modifications
        if file_ops > 0 {
            narrative.push_str(&format!(" {file_ops} file operation(s) were performed."));
        }

        narrative.push_str(
            "\n\n[LLM-generated narrative will be inserted here when LLM integration is complete]",
        );

        narrative
    }

    /// Extract code changes from session events
    ///
    /// Analyzes file operations and tool calls to identify
    /// what code changes were made during the session.
    ///
    /// # Arguments
    ///
    /// * `events` - The session events to analyze
    ///
    /// # Returns
    ///
    /// A vector of `CodeChange` records
    pub fn extract_code_changes(&self, events: &[InteractionEvent]) -> Vec<CodeChange> {
        use crate::session_capture::FileOperationType;

        let mut changes: HashMap<String, CodeChange> = HashMap::new();

        for event in events {
            if let InteractionEvent::FileOperation {
                path,
                operation,
                content_hash: _,
            } = event
            {
                let change_type = match operation {
                    FileOperationType::Created => ChangeType::Created,
                    FileOperationType::Modified => ChangeType::Modified,
                    FileOperationType::Deleted => ChangeType::Deleted,
                    FileOperationType::Renamed => ChangeType::Renamed,
                    FileOperationType::Read => ChangeType::Read,
                };

                // Detect language from file extension
                let language = path
                    .rsplit('.')
                    .next()
                    .map(|ext| match ext {
                        "rs" => "Rust",
                        "py" => "Python",
                        "js" => "JavaScript",
                        "ts" => "TypeScript",
                        "go" => "Go",
                        "java" => "Java",
                        "cpp" | "cc" | "cxx" => "C++",
                        "c" => "C",
                        "h" | "hpp" => "C/C++ Header",
                        "toml" => "TOML",
                        "json" => "JSON",
                        "yaml" | "yml" => "YAML",
                        "md" => "Markdown",
                        _ => ext,
                    })
                    .map(ToString::to_string);

                // Update or create change record
                changes
                    .entry(path.clone())
                    .and_modify(|change| {
                        // Upgrade read -> modify/create -> delete
                        // Created + Modified = Modified (file was created then modified)
                        change.change_type = match (change.change_type, change_type) {
                            (_, ChangeType::Deleted) => ChangeType::Deleted,
                            (ChangeType::Read, ChangeType::Modified) => ChangeType::Modified,
                            (ChangeType::Read, ChangeType::Created) => ChangeType::Created,
                            (ChangeType::Created, ChangeType::Modified) => ChangeType::Modified,
                            (existing, _) => existing,
                        };
                    })
                    .or_insert(CodeChange {
                        file_path: path.clone(),
                        change_type,
                        description: format!("File {change_type}"),
                        language,
                        lines_changed: None,
                        timestamp: Some(Utc::now()),
                    });
            }
        }

        changes.into_values().collect()
    }

    /// Extract decisions made during the session
    ///
    /// Analyzes user messages and assistant responses to identify
    /// key decisions, particularly architectural or design choices.
    ///
    /// # Arguments
    ///
    /// * `events` - The session events to analyze
    ///
    /// # Returns
    ///
    /// A vector of `Decision` records
    pub fn extract_decisions(&self, events: &[InteractionEvent]) -> Vec<Decision> {
        let mut decisions = Vec::new();

        // TODO: Replace with LLM-based extraction using PATTERN_PROMPT
        // For now, use heuristic extraction

        for event in events {
            match event {
                InteractionEvent::UserMessage { content, timestamp } => {
                    // Look for decision keywords
                    let decision_keywords = [
                        "decided to",
                        "choose to",
                        "going with",
                        "let's use",
                        "let's go with",
                        "i think we should",
                        "we should",
                        "opt for",
                    ];

                    let lower = content.to_lowercase();
                    for keyword in &decision_keywords {
                        if lower.contains(keyword) {
                            decisions.push(Decision {
                                description: content.clone(),
                                rationale: "User-initiated decision".to_string(),
                                timestamp: Some(*timestamp),
                                is_architectural: lower.contains("architecture")
                                    || lower.contains("design")
                                    || lower.contains("pattern"),
                                alternatives: Vec::new(),
                            });
                            break;
                        }
                    }
                }
                InteractionEvent::ModeChange { from, to } => {
                    // Mode changes often represent strategic decisions
                    decisions.push(Decision {
                        description: format!("Switched from {from} mode to {to} mode"),
                        rationale: "Workflow state transition".to_string(),
                        timestamp: None,
                        is_architectural: false,
                        alternatives: vec![format!("Continue in {} mode", from)],
                    });
                }
                _ => {}
            }
        }

        decisions
    }

    // === Private helper methods ===

    /// Extract list of files touched during the session
    fn extract_files_touched(&self, events: &[InteractionEvent]) -> Vec<String> {
        let mut files: std::collections::HashSet<String> = std::collections::HashSet::new();

        for event in events {
            if let InteractionEvent::FileOperation { path, .. } = event {
                files.insert(path.clone());
            }
        }

        files.into_iter().collect()
    }

    /// Extract list of tools used during the session
    fn extract_tools_used(&self, events: &[InteractionEvent]) -> Vec<String> {
        let mut tools: std::collections::HashSet<String> = std::collections::HashSet::new();

        for event in events {
            if let InteractionEvent::ToolCall { tool_name, .. } = event {
                tools.insert(tool_name.clone());
            }
        }

        tools.into_iter().collect()
    }

    /// Extract errors encountered during the session
    fn extract_errors(&self, events: &[InteractionEvent]) -> Vec<String> {
        events
            .iter()
            .filter_map(|e| match e {
                InteractionEvent::Error {
                    error_type,
                    message,
                    ..
                } => Some(format!("{error_type}: {message}")),
                _ => None,
            })
            .collect()
    }

    /// Generate key points from events
    fn generate_key_points(&self, events: &[InteractionEvent]) -> Vec<String> {
        let mut points = Vec::new();

        for event in events {
            match event {
                InteractionEvent::ToolCall {
                    tool_name, success, ..
                } if *success => {
                    points.push(format!("Executed {tool_name}"));
                }
                InteractionEvent::FileOperation {
                    path, operation, ..
                } => {
                    use crate::session_capture::FileOperationType;
                    let op_str = match operation {
                        FileOperationType::Created => "Created",
                        FileOperationType::Modified => "Modified",
                        FileOperationType::Deleted => "Deleted",
                        FileOperationType::Renamed => "Renamed",
                        FileOperationType::Read => "Read",
                    };
                    points.push(format!("{op_str} {path}"));
                }
                _ => {}
            }
        }

        // Deduplicate while preserving order
        let mut seen = std::collections::HashSet::new();
        points.retain(|p| seen.insert(p.clone()));

        points
    }

    /// Generate technical details from events
    fn generate_technical_details(&self, events: &[InteractionEvent]) -> Vec<String> {
        let mut details = Vec::new();

        // Count operations
        let file_ops = events
            .iter()
            .filter(|e| matches!(e, InteractionEvent::FileOperation { .. }))
            .count();
        let tool_calls = events
            .iter()
            .filter(|e| matches!(e, InteractionEvent::ToolCall { .. }))
            .count();

        if file_ops > 0 {
            details.push(format!("{file_ops} file operations performed"));
        }
        if tool_calls > 0 {
            details.push(format!("{tool_calls} tool calls executed"));
        }

        // Identify languages from file extensions
        let languages: std::collections::HashSet<String> = events
            .iter()
            .filter_map(|e| match e {
                InteractionEvent::FileOperation { path, .. } => {
                    path.rsplit('.').next().map(str::to_lowercase)
                }
                _ => None,
            })
            .collect();

        if !languages.is_empty() {
            let lang_list: Vec<_> = languages.into_iter().collect();
            details.push(format!("Languages/extensions: {}", lang_list.join(", ")));
        }

        details
    }

    /// Extract root causes from error events
    fn extract_root_causes(&self, events: &[InteractionEvent]) -> Vec<String> {
        events
            .iter()
            .filter_map(|e| match e {
                InteractionEvent::Error {
                    error_type,
                    message,
                    resolution,
                    ..
                } => {
                    let mut cause = format!("{error_type}: {message}");
                    if let Some(res) = resolution {
                        cause.push_str(&format!(" (Resolved: {res})"));
                    }
                    Some(cause)
                }
                _ => None,
            })
            .collect()
    }

    /// Identify reusable patterns from the session
    fn identify_patterns(&self, events: &[InteractionEvent]) -> Vec<String> {
        // TODO: Replace with LLM-based pattern identification using PATTERN_PROMPT
        let mut patterns = Vec::new();

        // Look for common patterns in tool usage
        let has_file_ops = events
            .iter()
            .any(|e| matches!(e, InteractionEvent::FileOperation { .. }));
        let has_error_handling = events
            .iter()
            .any(|e| matches!(e, InteractionEvent::Error { .. }));

        if has_file_ops && has_error_handling {
            patterns.push("Error-aware file operations".to_string());
        }

        // Check for iterative refinement pattern
        let user_msgs = events
            .iter()
            .filter(|e| matches!(e, InteractionEvent::UserMessage { .. }))
            .count();
        if user_msgs > 3 {
            patterns.push("Iterative refinement workflow".to_string());
        }

        patterns
    }

    /// Extract learnings from the session
    fn extract_session_learnings(&self, events: &[InteractionEvent]) -> Vec<String> {
        let mut learnings = Vec::new();

        let error_count = events
            .iter()
            .filter(|e| matches!(e, InteractionEvent::Error { .. }))
            .count();

        if error_count > 0 {
            learnings.push(format!("Encountered {error_count} error(s)"));
        }

        let tool_count = events
            .iter()
            .filter(|e| matches!(e, InteractionEvent::ToolCall { .. }))
            .count();

        if tool_count > 0 {
            learnings.push(format!("Used {tool_count} tool call(s)"));
        }

        learnings
    }

    /// Generate next steps based on session analysis
    fn generate_next_steps(&self, events: &[InteractionEvent], errors: &[String]) -> Vec<String> {
        let mut steps = Vec::new();

        if !errors.is_empty() {
            steps.push("Review and address errors from the session".to_string());
        }

        let has_file_ops = events
            .iter()
            .any(|e| matches!(e, InteractionEvent::FileOperation { .. }));
        if has_file_ops {
            steps.push("Verify file changes are correct".to_string());
        }

        let has_bash = events.iter().any(|e| match e {
            InteractionEvent::ToolCall { tool_name, .. } => tool_name == "bash",
            _ => false,
        });
        if has_bash {
            steps.push("Review shell commands executed".to_string());
        }

        steps
    }

    /// Determine the session outcome based on events and errors
    fn determine_outcome(&self, events: &[InteractionEvent], errors: &[String]) -> SessionOutcome {
        if errors.is_empty() {
            return SessionOutcome::Success;
        }

        // Check if all errors were resolved
        let unresolved_errors = events
            .iter()
            .filter(|e| {
                matches!(
                    e,
                    InteractionEvent::Error {
                        resolution: None,
                        ..
                    }
                )
            })
            .count();

        if (unresolved_errors == 0 && !errors.is_empty()) || events.len() > errors.len() * 2 {
            SessionOutcome::Success // All errors resolved, or more successes than errors
        } else {
            SessionOutcome::Failed
        }
    }

    /// Infer the session task from events
    fn infer_task(&self, events: &[InteractionEvent]) -> String {
        // Try to find first user message as task description
        events
            .iter()
            .find_map(|e| match e {
                InteractionEvent::UserMessage { content, .. } => {
                    // Truncate if too long
                    let task = if content.chars().count() > 100 {
                        format!("{}...", truncate_to_char_boundary(content, 97))
                    } else {
                        content.clone()
                    };
                    Some(task)
                }
                _ => None,
            })
            .unwrap_or_else(|| "Unnamed session".to_string())
    }

    /// Format events for LLM prompt
    #[allow(dead_code)] // Used when LLM integration is added
    fn format_events_for_prompt(&self, events: &[InteractionEvent]) -> String {
        events
            .iter()
            .map(|e| format!("{e:?}"))
            .collect::<Vec<_>>()
            .join("\n")
    }
}

fn truncate_to_char_boundary(input: &str, max_chars: usize) -> &str {
    if input.chars().count() <= max_chars {
        return input;
    }

    match input.char_indices().nth(max_chars) {
        Some((idx, _)) => &input[..idx],
        None => input,
    }
}

// =============================================================================
// Prompt Templates
// =============================================================================

/// Prompt template for generating narrative summaries
///
/// Use this template when calling the LLM to generate a natural language
/// summary of session activities.
pub const SUMMARY_PROMPT: &str = r"You are analyzing a coding session to create a comprehensive narrative summary.

Session Events:
{{events}}

Session Metrics:
- Total Interactions: {{interaction_count}}
- Tool Calls: {{tool_call_count}}
- Errors: {{error_count}}
- Files Modified: {{files_modified_count}}
- Duration: {{duration_ms}}ms

Please provide a narrative summary (2-4 paragraphs) that:
1. Describes the overall goal or task being worked on
2. Explains the approach taken and key steps
3. Highlights any challenges encountered and how they were addressed
4. Summarizes the outcome and any deliverables

Write in a clear, professional style suitable for session documentation.";

/// Prompt template for extracting learnings
///
/// Use this template when calling the LLM to extract lessons learned
/// from a session.
pub const LEARNING_PROMPT: &str = r"Analyze this coding session and extract key learnings, insights, and takeaways.

Session Events:
{{events}}

Errors Encountered:
{{errors}}

Tools Used:
{{tools}}

Files Modified:
{{files}}

Existing Learnings:
{{existing_learnings}}

Please identify:
1. Technical lessons learned (new patterns, techniques, APIs discovered)
2. Process insights (workflow improvements, debugging strategies)
3. Mistakes to avoid in the future
4. Useful resources or documentation discovered

Return your response as a JSON array of learning strings.";

/// Prompt template for identifying reusable patterns
///
/// Use this template when calling the LLM to identify patterns and
/// solutions that could be reused in future sessions.
pub const PATTERN_PROMPT: &str = r"Analyze this coding session to identify reusable patterns, solutions, and approaches.

Session Events:
{{events}}

Code Changes:
{{code_changes}}

Decisions Made:
{{decisions}}

Please identify:
1. Design patterns used or discovered
2. Solution approaches that could be templated
3. Code organization patterns
4. Debugging or troubleshooting strategies
5. Tool combinations that worked well together

For each pattern, provide:
- Name/description of the pattern
- When it applies (context/conditions)
- How to apply it (steps or example)

Return your response as structured data that can be used to build a pattern library.";

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session_capture::{FileOperationType, InteractionEvent};
    use chrono::Utc;

    fn create_test_events() -> Vec<InteractionEvent> {
        vec![
            InteractionEvent::UserMessage {
                content: "Create a new module for LLM summarization".to_string(),
                timestamp: Utc::now(),
            },
            InteractionEvent::AssistantMessage {
                content: "I'll help you create the module".to_string(),
                reasoning: Some("This is a reasonable request".to_string()),
                timestamp: Utc::now(),
            },
            InteractionEvent::ToolCall {
                tool_name: "write_file".to_string(),
                input: serde_json::json!({"path": "/tmp/test.rs"}),
                output: None,
                success: true,
                duration_ms: 100,
            },
            InteractionEvent::FileOperation {
                path: "/tmp/test.rs".to_string(),
                operation: FileOperationType::Created,
                content_hash: None,
            },
            InteractionEvent::FileOperation {
                path: "/tmp/test.rs".to_string(),
                operation: FileOperationType::Modified,
                content_hash: None,
            },
            InteractionEvent::Error {
                error_type: "ParseError".to_string(),
                message: "Invalid syntax".to_string(),
                resolution: Some("Fixed the syntax".to_string()),
            },
        ]
    }

    #[test]
    fn test_summarizer_config_default() {
        let config = SummarizerConfig::default();
        assert_eq!(config.model, "claude-sonnet");
        assert_eq!(config.max_summary_tokens, 2000);
        assert_eq!(config.temperature, 0.7);
        assert!(config.extract_learnings);
        assert!(config.extract_patterns);
    }

    #[test]
    fn test_summarizer_config_builder() {
        let config = SummarizerConfig::with_model("gpt-4")
            .with_max_tokens(4000)
            .with_temperature(0.5);

        assert_eq!(config.model, "gpt-4");
        assert_eq!(config.max_summary_tokens, 4000);
        assert_eq!(config.temperature, 0.5);
    }

    #[test]
    fn test_summarizer_config_temperature_clamping() {
        let config = SummarizerConfig::default().with_temperature(1.5);
        assert_eq!(config.temperature, 1.0);

        let config = SummarizerConfig::default().with_temperature(-0.5);
        assert_eq!(config.temperature, 0.0);
    }

    #[test]
    fn test_summary_request_builder() {
        let events = vec![];
        let metrics = SessionMetrics::default();

        let request = SummaryRequest::new(events.clone(), metrics)
            .with_learnings(vec!["test learning".to_string()]);

        assert!(request.session_events.is_empty());
        assert_eq!(request.existing_learnings.len(), 1);
    }

    #[test]
    fn test_code_change_detection() {
        let summarizer = SessionSummarizer::new(SummarizerConfig::default());
        let events = create_test_events();

        let changes = summarizer.extract_code_changes(&events);

        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].file_path, "/tmp/test.rs");
        assert_eq!(changes[0].change_type, ChangeType::Modified); // Upgraded from Created
        assert_eq!(changes[0].language, Some("Rust".to_string()));
    }

    #[test]
    fn test_extract_decisions() {
        let summarizer = SessionSummarizer::new(SummarizerConfig::default());
        let mut events = create_test_events();

        // Add a mode change event
        events.push(InteractionEvent::ModeChange {
            from: "planning".to_string(),
            to: "executing".to_string(),
        });

        let decisions = summarizer.extract_decisions(&events);

        // Should detect the mode change at minimum
        assert!(!decisions.is_empty());
    }

    #[test]
    fn test_extract_decisions_from_user_messages() {
        let summarizer = SessionSummarizer::new(SummarizerConfig::default());
        let events = vec![
            InteractionEvent::UserMessage {
                content: "I decided to use a struct instead of a tuple".to_string(),
                timestamp: Utc::now(),
            },
            InteractionEvent::UserMessage {
                content: "Let's go with the async approach".to_string(),
                timestamp: Utc::now(),
            },
        ];

        let decisions = summarizer.extract_decisions(&events);
        assert_eq!(decisions.len(), 2);
        assert!(decisions[0].description.contains("decided"));
    }

    #[test]
    fn test_narrative_generation() {
        let summarizer = SessionSummarizer::new(SummarizerConfig::default());
        let events = create_test_events();

        let narrative = summarizer.generate_narrative(&events);

        assert!(!narrative.is_empty());
        assert!(narrative.contains("user interactions"));
        assert!(narrative.contains("tool executions"));
    }

    #[test]
    fn test_narrative_generation_empty_events() {
        let summarizer = SessionSummarizer::new(SummarizerConfig::default());
        let narrative = summarizer.generate_narrative(&[]);

        assert_eq!(narrative, "No session events recorded.");
    }

    #[test]
    fn test_narrative_generation_handles_multibyte_content() {
        let summarizer = SessionSummarizer::new(SummarizerConfig::default());
        let long_message = "😀".repeat(120);
        let events = vec![InteractionEvent::UserMessage {
            content: long_message,
            timestamp: Utc::now(),
        }];

        let narrative = summarizer.generate_narrative(&events);

        assert!(narrative.contains("user interactions"));
        assert!(narrative.contains("😀"));
    }

    #[test]
    fn test_infer_task_handles_multibyte_content() {
        let summarizer = SessionSummarizer::new(SummarizerConfig::default());
        let events = vec![InteractionEvent::UserMessage {
            content: "こんにちは".repeat(30),
            timestamp: Utc::now(),
        }];

        let task = summarizer.infer_task(&events);

        assert!(task.starts_with("こんにちは"));
        assert!(task.ends_with("..."));
    }

    #[test]
    fn test_summarize_session() {
        let summarizer = SessionSummarizer::new(SummarizerConfig::default());
        let events = create_test_events();

        let request = SummaryRequest::new(events, SessionMetrics::default());
        let summary = summarizer.summarize_session(request).unwrap();

        assert!(!summary.task.is_empty());
        assert!(!summary.narrative_summary.is_empty());
        assert!(!summary.code_changes.is_empty());
        assert_eq!(summary.files_touched.len(), 1);
    }

    #[test]
    fn test_rich_summary_formatting() {
        let summary = RichSummary {
            session_id: "test-123".to_string(),
            task: "Test task".to_string(),
            duration_ms: 5000,
            key_points: vec!["Point 1".to_string()],
            files_touched: vec!["/tmp/test.rs".to_string()],
            errors_encountered: vec![],
            tools_used: vec!["write_file".to_string()],
            outcome: SessionOutcome::Success,
            learnings: vec!["Learning 1".to_string()],
            next_steps: vec!["Step 1".to_string()],
            started_at: Utc::now(),
            ended_at: Utc::now(),
            narrative_summary: "Test narrative".to_string(),
            technical_details: vec!["Detail 1".to_string()],
            code_changes: vec![CodeChange {
                file_path: "/tmp/test.rs".to_string(),
                change_type: ChangeType::Created,
                description: "Created test file".to_string(),
                language: Some("Rust".to_string()),
                lines_changed: Some(10),
                timestamp: Some(Utc::now()),
            }],
            decision_log: vec![],
            root_causes: vec![],
            solution_patterns: vec!["Pattern 1".to_string()],
        };

        let formatted = summary.format_summary();

        assert!(formatted.contains("Test task"));
        assert!(formatted.contains("Test narrative"));
        assert!(formatted.contains("/tmp/test.rs"));
        assert!(formatted.contains("Pattern 1"));
    }

    #[test]
    fn test_change_type_display() {
        assert_eq!(ChangeType::Created.to_string(), "created");
        assert_eq!(ChangeType::Modified.to_string(), "modified");
        assert_eq!(ChangeType::Deleted.to_string(), "deleted");
        assert_eq!(ChangeType::Renamed.to_string(), "renamed");
        assert_eq!(ChangeType::Read.to_string(), "read");
    }

    #[test]
    fn test_language_detection() {
        let summarizer = SessionSummarizer::new(SummarizerConfig::default());

        let test_cases = vec![
            ("main.rs", Some("Rust")),
            ("script.py", Some("Python")),
            ("app.js", Some("JavaScript")),
            ("types.ts", Some("TypeScript")),
            ("main.go", Some("Go")),
            ("Main.java", Some("Java")),
            ("lib.cpp", Some("C++")),
            ("header.h", Some("C/C++ Header")),
            ("config.toml", Some("TOML")),
            ("data.json", Some("JSON")),
            ("config.yaml", Some("YAML")),
            ("README.md", Some("Markdown")),
        ];

        for (path, expected) in test_cases {
            let events = vec![InteractionEvent::FileOperation {
                path: path.to_string(),
                operation: FileOperationType::Created,
                content_hash: None,
            }];

            let changes = summarizer.extract_code_changes(&events);
            assert_eq!(
                changes[0].language.as_deref(),
                expected,
                "Failed for {}",
                path
            );
        }
    }

    #[test]
    fn test_session_outcome_determination() {
        let summarizer = SessionSummarizer::new(SummarizerConfig::default());

        // No errors = success
        let events = vec![];
        assert_eq!(
            summarizer.determine_outcome(&events, &[]),
            SessionOutcome::Success
        );

        // Unresolved errors = failed
        let events = vec![InteractionEvent::Error {
            error_type: "Test".to_string(),
            message: "Test error".to_string(),
            resolution: None,
        }];
        assert_eq!(
            summarizer.determine_outcome(&events, &["error".to_string()]),
            SessionOutcome::Failed
        );

        // Resolved errors = success
        let events = vec![InteractionEvent::Error {
            error_type: "Test".to_string(),
            message: "Test error".to_string(),
            resolution: Some("Fixed".to_string()),
        }];
        assert_eq!(
            summarizer.determine_outcome(&events, &["error".to_string()]),
            SessionOutcome::Success
        );
    }

    #[test]
    fn test_prompt_templates_exist() {
        assert!(!SUMMARY_PROMPT.is_empty());
        assert!(!LEARNING_PROMPT.is_empty());
        assert!(!PATTERN_PROMPT.is_empty());

        // Verify placeholders exist
        assert!(SUMMARY_PROMPT.contains("{{events}}"));
        assert!(LEARNING_PROMPT.contains("{{events}}"));
        assert!(PATTERN_PROMPT.contains("{{events}}"));
    }

    #[test]
    fn test_serde_roundtrip() {
        let config = SummarizerConfig::default();
        let json = serde_json::to_string(&config).unwrap();
        let restored: SummarizerConfig = serde_json::from_str(&json).unwrap();

        assert_eq!(config.model, restored.model);
        assert_eq!(config.max_summary_tokens, restored.max_summary_tokens);
    }
}
