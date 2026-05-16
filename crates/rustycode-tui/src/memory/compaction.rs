//! Token autocompaction for RustyCode TUI

use crate::ui::message::{Message, MessageRole};
use rustycode_protocol::estimate_tokens;
use rustycode_protocol::tool_names as tn;
use rustycode_providers::predefined;
use std::time::Instant;

const PRUNE_MINIMUM: usize = 20_000;
const PRUNE_PROTECT: usize = 40_000;
const PRUNE_PROTECTED_TOOLS: &[&str] = &["skill"];

const MAX_CONSECUTIVE_FAILURES: u32 = 3;

/// Stub text replacing pruned tool output (same string opencode uses).
const PRUNED_OUTPUT_STUB: &str = "[Old tool result content cleared]";

#[derive(Clone, Debug, Default)]
pub struct AutoCompactState {
    pub compaction_count: u32,
    pub consecutive_failures: u32,
    pub disabled: bool,
}

impl AutoCompactState {
    pub fn on_success(&mut self) {
        self.compaction_count += 1;
        self.consecutive_failures = 0;
    }

    pub fn on_failure(&mut self) {
        self.consecutive_failures += 1;
        if self.consecutive_failures >= MAX_CONSECUTIVE_FAILURES {
            self.disabled = true;
        }
    }
}

/// Configuration for context monitoring and compaction
#[derive(Clone, Debug)]
pub struct CompactionConfig {
    /// Maximum tokens allowed in context
    pub max_tokens: usize,
    /// Warning threshold (0.0-1.0, e.g., 0.8 = 80%)
    pub warning_threshold: f64,
    /// Number of recent messages to keep intact
    pub keep_recent_count: usize,
    /// Whether auto-compaction is enabled
    pub auto_compact_enabled: bool,
    /// Compaction strategy (aggressive vs conservative)
    pub strategy: CompactionStrategy,
    /// Current model ID for model-aware context window sizing
    pub model_id: Option<String>,
    /// Circuit breaker state for auto-compaction
    pub auto_compact_state: AutoCompactState,
}

impl CompactionConfig {
    pub fn effective_max_tokens(&self) -> usize {
        self.model_id
            .as_deref()
            .map(predefined::context_window_for_model)
            .unwrap_or(self.max_tokens)
    }
}

impl Default for CompactionConfig {
    fn default() -> Self {
        Self {
            max_tokens: 100_000,
            warning_threshold: 0.8,
            keep_recent_count: 50,
            auto_compact_enabled: true,
            strategy: CompactionStrategy::Balanced,
            model_id: None,
            auto_compact_state: AutoCompactState::default(),
        }
    }
}

/// Compaction strategy
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CompactionStrategy {
    /// Keep last 20 messages (aggressive)
    Aggressive,
    /// Keep last 50 messages (balanced)
    Balanced,
    /// Keep last 100 messages (conservative)
    Conservative,
}

impl CompactionStrategy {
    pub fn keep_count(&self) -> usize {
        match self {
            CompactionStrategy::Aggressive => 20,
            CompactionStrategy::Balanced => 50,
            CompactionStrategy::Conservative => 100,
        }
    }
}

// ── Context Monitor ──────────────────────────────────────────────────────────

/// Context token usage monitor
#[derive(Clone, Debug)]
pub struct ContextMonitor {
    /// Current estimated token count
    pub current_tokens: usize,
    /// Maximum tokens allowed
    pub max_tokens: usize,
    /// Warning threshold (0.0-1.0)
    pub warning_threshold: f64,
    /// Last update time
    pub last_update: Instant,
    /// Whether compaction is needed
    pub needs_compaction: bool,
}

impl ContextMonitor {
    pub fn new(max_tokens: usize, warning_threshold: f64) -> Self {
        Self {
            current_tokens: 0,
            max_tokens,
            warning_threshold,
            last_update: Instant::now(),
            needs_compaction: false,
        }
    }

    /// Count tokens in messages (word-based estimation).
    ///
    /// Uses word-boundary counting which is more accurate for code than the
    /// naive chars/4 heuristic. Accounts for all content that consumes context:
    /// message text, thinking blocks, and tool execution inputs/outputs.
    pub fn count_tokens(&self, messages: &[Message]) -> usize {
        let mut total = 0usize;
        for m in messages {
            total += estimate_tokens(&m.content);
            if let Some(ref thinking) = m.thinking {
                total += estimate_tokens(thinking);
            }
            if let Some(ref tools) = m.tool_executions {
                for t in tools {
                    total += estimate_tokens(&t.name);
                    total += estimate_tokens(&t.result_summary);
                    if let Some(ref output) = t.detailed_output {
                        total += estimate_tokens(output);
                    }
                }
            }
        }
        total.max(1)
    }

    /// Update token count from messages (estimated)
    pub fn update(&mut self, messages: &[Message]) {
        self.current_tokens = self.count_tokens(messages);
        self.last_update = Instant::now();
        self.needs_compaction = self.usage_percentage() >= self.warning_threshold;
    }

    /// Update token count from actual API response (preferred over estimate)
    pub fn update_from_api(&mut self, input_tokens: usize, model_id: &str) {
        self.current_tokens = input_tokens;
        let window = predefined::context_window_for_model(model_id);
        if window > 0 {
            self.max_tokens = window;
        }
        self.last_update = Instant::now();
        self.needs_compaction = self.usage_percentage() >= self.warning_threshold;
    }

    /// Get current usage as percentage
    pub fn usage_percentage(&self) -> f64 {
        if self.max_tokens == 0 {
            return 0.0;
        }
        self.current_tokens as f64 / self.max_tokens as f64
    }

    pub fn should_compact(&self) -> bool {
        self.needs_compaction
    }

    /// Get remaining tokens
    pub fn remaining_tokens(&self) -> usize {
        self.max_tokens.saturating_sub(self.current_tokens)
    }

    /// Get color code for usage (for UI)
    pub fn usage_color(&self) -> UsageColor {
        let pct = self.usage_percentage();
        if pct < 0.5 {
            UsageColor::Green
        } else if pct < 0.8 {
            UsageColor::Yellow
        } else {
            UsageColor::Red
        }
    }
}

/// Usage color for UI display
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum UsageColor {
    Green,
    Yellow,
    Red,
}

// ── Compaction Preview ───────────────────────────────────────────────────────

/// Compaction preview information
#[derive(Clone, Debug)]
pub struct CompactionPreview {
    /// Current token count
    pub current_tokens: usize,
    pub max_tokens: usize,
    /// Number of messages to compact
    pub messages_to_compact: usize,
    /// Number of recent messages to keep
    pub recent_to_keep: usize,
    /// Number of tool outputs that would be pruned
    pub tool_outputs_pruned: usize,
    /// Number of error messages to preserve
    pub error_count: usize,
    /// Estimated tokens saved by pruning tool outputs
    pub prune_savings: usize,
    /// Estimated tokens saved by compacting old messages
    pub compact_savings: usize,
    /// New token count after compaction
    pub new_token_count: usize,
}

impl CompactionPreview {
    fn format_token_count(n: usize) -> String {
        if n >= 1_000_000 {
            format!("{:.1}M", n as f64 / 1_000_000.0)
        } else if n >= 1_000 {
            format!("{:.0}k", n as f64 / 1_000.0)
        } else {
            n.to_string()
        }
    }

    /// Create a compaction preview
    pub fn new(
        current_tokens: usize,
        max_tokens: usize,
        messages: &[Message],
        strategy: CompactionStrategy,
    ) -> Self {
        let keep_count = strategy.keep_count();
        let total_messages = messages.len();
        let messages_to_compact = total_messages.saturating_sub(keep_count);

        let old_messages: Vec<_> = if total_messages > keep_count {
            messages.iter().rev().skip(keep_count).collect()
        } else {
            vec![]
        };

        // Count prunable tool outputs in old messages
        let mut tool_outputs_pruned = 0usize;
        let mut prune_savings = 0usize;
        for m in &old_messages {
            if let Some(ref tools) = m.tool_executions {
                for t in tools {
                    if let Some(ref output) = t.detailed_output {
                        if !output.starts_with(PRUNED_OUTPUT_STUB) {
                            tool_outputs_pruned += 1;
                            prune_savings += estimate_tokens(output);
                        }
                    }
                }
            }
        }

        let error_count = old_messages
            .iter()
            .filter(|m| m.content.to_lowercase().contains("error"))
            .count();

        let compact_savings = if messages_to_compact > 0 {
            let old_tokens: usize = old_messages
                .iter()
                .map(|m| estimate_tokens(&m.content))
                .sum();
            (old_tokens as f64 * 0.7) as usize
        } else {
            0
        };

        let total_savings = prune_savings + compact_savings;
        let new_token_count = current_tokens.saturating_sub(total_savings);

        Self {
            current_tokens,
            max_tokens,
            messages_to_compact,
            recent_to_keep: keep_count,
            tool_outputs_pruned,
            error_count,
            prune_savings,
            compact_savings,
            new_token_count,
        }
    }

    /// Format as display text
    pub fn format(&self) -> String {
        let pct = if self.max_tokens == 0 {
            0.0
        } else {
            (self.current_tokens as f64 / self.max_tokens as f64) * 100.0
        };
        let new_pct = if self.max_tokens == 0 {
            0.0
        } else {
            (self.new_token_count as f64 / self.max_tokens as f64) * 100.0
        };
        let prune_line = if self.tool_outputs_pruned > 0 {
            format!(
                "\n  Prune {} old tool outputs (saves ~{})",
                self.tool_outputs_pruned,
                Self::format_token_count(self.prune_savings)
            )
        } else {
            String::new()
        };
        format!(
            "⚠ Context at {:.0}% ({}/{})\n\nCompaction plan:\n  Keep last {} messages intact\n  Summarize {} older messages{}  Preserve {} errors\n\nEstimated: {} → {} ({:.0}%)\n\n[Enter to compact] [Esc to cancel]",
            pct,
            Self::format_token_count(self.current_tokens),
            Self::format_token_count(self.max_tokens),
            self.recent_to_keep,
            self.messages_to_compact,
            prune_line,
            self.error_count,
            Self::format_token_count(self.current_tokens),
            Self::format_token_count(self.new_token_count),
            new_pct
        )
    }
}

// ── Token Estimation ─────────────────────────────────────────────────────────

// ── Tier 1: Tool Output Pruning ──────────────────────────────────────────────

/// Prune old tool output from messages (opencode pattern).
///
/// Walks backwards through messages. Accumulates token estimates of completed
/// tool outputs until we've passed `PRUNE_PROTECT` tokens of *recent* output,
/// then erases `detailed_output` on everything older. The tool call itself
/// (name, input, summary) is preserved — only the large output is removed.
///
/// Returns `(pruned_count, tokens_saved)`.
pub fn prune_tool_outputs(messages: &mut [Message]) -> (usize, usize) {
    let mut total = 0usize;
    let mut pruned = 0usize;
    let to_prune: Vec<(usize, usize)> = {
        // First pass: walk backwards, find candidates
        let mut candidates = Vec::new();
        let mut turns = 0usize;

        let mut msg_idx = messages.len();
        while msg_idx > 0 {
            msg_idx -= 1;
            let m = &messages[msg_idx];
            if matches!(m.role, MessageRole::User) {
                turns += 1;
            }
            // Protect last 2 user turns
            if turns < 2 {
                continue;
            }
            // Stop at an earlier compaction summary
            if matches!(m.role, MessageRole::Assistant) && is_compaction_summary(m) {
                break;
            }

            if let Some(ref tools) = m.tool_executions {
                for (tool_idx, t) in tools.iter().enumerate().rev() {
                    if PRUNE_PROTECTED_TOOLS.contains(&t.name.as_str()) {
                        continue;
                    }
                    if let Some(ref output) = t.detailed_output {
                        // Already pruned — stop walking
                        if output.starts_with(PRUNED_OUTPUT_STUB) {
                            break;
                        }
                        let estimate = estimate_tokens(output);
                        total += estimate;
                        if total > PRUNE_PROTECT {
                            candidates.push((msg_idx, tool_idx));
                            pruned += estimate;
                        }
                    }
                }
            }
        }
        candidates
    };

    if pruned < PRUNE_MINIMUM {
        return (0, 0);
    }

    let count = to_prune.len();
    // Second pass: apply pruning
    for (msg_idx, tool_idx) in to_prune {
        if let Some(ref mut tools) = messages[msg_idx].tool_executions {
            if tool_idx < tools.len() {
                tools[tool_idx].detailed_output = Some(PRUNED_OUTPUT_STUB.to_string());
            }
        }
    }

    (count, pruned)
}

// ── Tier 2: Structured Summary ───────────────────────────────────────────────

/// Summarize old messages into a structured context (opencode template pattern).
///
/// Extracts Goal, Instructions, Discoveries, Accomplished, and Relevant Files
/// from message content and tool metadata — much richer than 100-char truncation.
fn summarize_messages_structured(messages: &[&Message]) -> Message {
    let mut goals: Vec<String> = Vec::new();
    let mut instructions: Vec<String> = Vec::new();
    let mut discoveries: Vec<String> = Vec::new();
    let mut accomplished: Vec<String> = Vec::new();
    let mut files: Vec<String> = Vec::new();

    for msg in messages {
        if matches!(msg.role, MessageRole::System) || msg.content.trim().is_empty() {
            continue;
        }

        match msg.role {
            MessageRole::User => {
                let preview = truncate_smart(&msg.content, 200);
                if is_directive(&msg.content) {
                    instructions.push(preview);
                } else {
                    goals.push(preview);
                }
            }
            MessageRole::Assistant => {
                // Extract file references from tool executions
                if let Some(ref tools) = msg.tool_executions {
                    for t in tools {
                        // Track files from tool input
                        if let Some(ref input) = t.input_json {
                            if let Some(path) = extract_path_from_input(t.name.as_str(), input) {
                                if !files.contains(&path) {
                                    files.push(path);
                                }
                            }
                        }
                        // Track what was accomplished
                        let summary = t.result_summary.trim();
                        if !summary.is_empty() && summary != format!("{}...", t.name) {
                            accomplished.push(summary.to_string());
                        }
                    }
                }
                // Key text responses as discoveries
                let text = msg.content.trim();
                if !text.is_empty() {
                    discoveries.push(truncate_smart(text, 150));
                }
            }
            MessageRole::System => {}
        }
    }

    let mut sections = Vec::new();
    sections.push(format!(
        "Summary of {} messages (compacted)",
        messages.len()
    ));

    if !goals.is_empty() {
        sections.push(format!("\n## Goal\n{}", goals.join("\n")));
    }
    if !instructions.is_empty() {
        sections.push(format!(
            "\n## Instructions\n- {}",
            instructions.join("\n- ")
        ));
    }
    if !discoveries.is_empty() {
        let deduped = dedup_take(&discoveries, 8);
        sections.push(format!("\n## Discoveries\n- {}", deduped.join("\n- ")));
    }
    if !accomplished.is_empty() {
        let deduped = dedup_take(&accomplished, 10);
        sections.push(format!("\n## Accomplished\n- {}", deduped.join("\n- ")));
    }
    if !files.is_empty() {
        sections.push(format!("\n## Relevant files\n{}", files.join("\n")));
    }

    Message::new(MessageRole::System, sections.join("\n"))
}

// ── Infra-Message Classification ─────────────────────────────────────────────

/// Tools whose call + output should be treated as infrastructure (preserved intact
/// through compaction rather than summarized).
const INFRA_TOOLS: &[&str] = &["skill"];

/// Classify a message as "infrastructure" — system prompts, skill tool calls,
/// and other non-conversation content that must be preserved through compaction
/// rather than summarized.
///
/// Infrastructure messages include:
/// - System messages (system prompts, compaction summaries from prior runs)
/// - Assistant messages that contain ONLY skill tool calls (no user-facing text)
/// - Messages where skill tool outputs are the primary content
fn is_infra_message(m: &Message) -> bool {
    if matches!(m.role, MessageRole::System) {
        return true;
    }

    if matches!(m.role, MessageRole::Assistant) {
        if let Some(ref tools) = m.tool_executions {
            let has_infra = tools.iter().any(|t| INFRA_TOOLS.contains(&t.name.as_str()));
            if has_infra && m.content.trim().len() < 50 {
                return true;
            }
        }
    }

    false
}

// ── Tier 3: Full Compact ─────────────────────────────────────────────────────

/// Compact messages to reduce token count.
///
/// Pipeline: extract infra → prune tool outputs → structured summary → re-inject infra → drop old messages.
///
/// Infrastructure messages (system prompts, skill tool calls) are extracted before
/// summarization so they survive compaction intact. Only conversation messages
/// (user questions, assistant responses, regular tool calls) are summarized.
pub fn compact_context(messages: Vec<Message>, strategy: CompactionStrategy) -> Vec<Message> {
    let keep_count = strategy.keep_count();

    if messages.len() <= keep_count {
        return messages;
    }

    let mut result = Vec::new();

    // 1. Keep last N messages intact
    let recent: Vec<_> = messages.iter().rev().take(keep_count).cloned().collect();

    // 2. Get older messages and separate infra from conversation
    let old: Vec<&Message> = messages.iter().rev().skip(keep_count).collect();

    if !old.is_empty() {
        let (infra, conversation): (Vec<&Message>, Vec<&Message>) =
            old.iter().partition(|m| is_infra_message(m));

        result.extend(infra.into_iter().rev().cloned());

        // 3. Structured summary of conversation-only messages
        if !conversation.is_empty() {
            let summary = summarize_messages_structured(&conversation);
            result.push(summary);
        }

        // 4. Preserve error messages from the conversation set
        let errors: Vec<Message> = conversation
            .iter()
            .filter(|m| m.content.to_lowercase().contains("error"))
            .map(|m| (*m).clone())
            .collect();
        result.extend(errors);
    }

    // 5. Add recent messages in correct order
    result.extend(recent.into_iter().rev());

    // 6. Prune tool outputs in the kept messages too (protects recent turns)
    prune_tool_outputs(&mut result);

    result
}

// ── Helpers ──────────────────────────────────────────────────────────────────

/// Check if a message is a compaction summary from a prior run.
fn is_compaction_summary(m: &Message) -> bool {
    m.content.contains("Summary of ") && m.content.contains("messages (compacted)")
}

/// Heuristic: does this user message read more like a directive than a question?
fn is_directive(content: &str) -> bool {
    let lower = content.to_ascii_lowercase();
    let trimmed = lower.trim_start();
    trimmed.starts_with("make ")
        || trimmed.starts_with("ensure ")
        || trimmed.starts_with("always ")
        || trimmed.starts_with("never ")
        || trimmed.starts_with("use ")
        || trimmed.starts_with("follow ")
        || trimmed.starts_with("do not ")
        || trimmed.starts_with("don't ")
        || trimmed.starts_with("refactor ")
        || trimmed.starts_with("fix ")
        || trimmed.starts_with("implement ")
        || trimmed.starts_with("add ")
        || trimmed.starts_with("remove ")
        || trimmed.starts_with("update ")
        || trimmed.starts_with("change ")
        || trimmed.starts_with("no ")
}

/// Truncate at a word boundary near `max_len`.
fn truncate_smart(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        return s.to_string();
    }
    let truncated = &s[..s.floor_char_boundary(max_len)];
    if let Some(pos) = truncated.rfind(' ') {
        format!("{}...", &truncated[..pos])
    } else {
        format!("{}...", truncated)
    }
}

/// Extract a file path from tool input JSON if present.
fn extract_path_from_input(tool_name: &str, input: &serde_json::Value) -> Option<String> {
    match tool_name {
        tn::READ | tn::WRITE | tn::EDIT => input
            .get("path")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        tn::BASH => input
            .get("command")
            .and_then(|v| v.as_str())
            .and_then(|cmd| {
                // Extract first path-like thing from command
                for word in cmd.split_whitespace() {
                    if word.contains('/') && !word.starts_with('-') {
                        return Some(word.to_string());
                    }
                }
                None
            }),
        tn::LIST_DIR => input
            .get("path")
            .and_then(|v| v.as_str())
            .map(|s| format!("{}/", s)),
        _ => None,
    }
}

/// Take up to `n` items, deduplicating by exact match.
fn dedup_take(items: &[String], n: usize) -> Vec<String> {
    let mut seen = Vec::new();
    for item in items {
        if seen.len() >= n {
            break;
        }
        if !seen.contains(item) {
            seen.push(item.clone());
        }
    }
    seen
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::message_types::ToolExecution;

    fn create_test_message(role: MessageRole, content: &str) -> Message {
        Message::new(role, content.to_string())
    }

    fn create_message_with_tool(
        role: MessageRole,
        content: &str,
        tool_name: &str,
        tool_output: &str,
    ) -> Message {
        let mut msg = Message::new(role, content.to_string());
        msg.tool_executions = Some(vec![ToolExecution::new_simple(tool_name.to_string())]);
        if let Some(ref mut tools) = msg.tool_executions {
            tools[0].result_summary = format!("{}: done", tool_name);
            tools[0].detailed_output = Some(tool_output.to_string());
        }
        msg
    }

    // ── Context Monitor ────────────────────────────────────

    #[test]
    fn test_context_monitor_new() {
        let monitor = ContextMonitor::new(100_000, 0.8);
        assert_eq!(monitor.max_tokens, 100_000);
        assert_eq!(monitor.warning_threshold, 0.8);
        assert_eq!(monitor.current_tokens, 0);
        assert!(!monitor.needs_compaction);
    }

    #[test]
    fn test_context_monitor_count_tokens() {
        let monitor = ContextMonitor::new(100_000, 0.8);
        let messages = vec![
            create_test_message(MessageRole::User, "Hello world"),
            create_test_message(MessageRole::Assistant, "Hi there!"),
        ];
        let count = monitor.count_tokens(&messages);
        assert!(count > 0);
    }

    #[test]
    fn test_context_monitor_update() {
        let mut monitor = ContextMonitor::new(1000, 0.8);
        let messages = vec![create_test_message(MessageRole::User, "Test message")];
        monitor.update(&messages);
        assert!(monitor.current_tokens > 0);
        assert!(!monitor.needs_compaction);
    }

    #[test]
    fn test_context_monitor_should_compact() {
        let mut monitor = ContextMonitor::new(1000, 0.5);
        // Use space-separated words: estimate_tokens counts words, not chars
        let messages = vec![create_test_message(MessageRole::User, &"x ".repeat(600))];
        monitor.update(&messages);
        assert!(monitor.should_compact());
    }

    #[test]
    fn test_context_monitor_update_from_api() {
        let mut monitor = ContextMonitor::new(100_000, 0.8);
        assert_eq!(monitor.current_tokens, 0);

        // Simulate API reporting 15k input tokens for claude-sonnet-4-5 (200k window)
        monitor.update_from_api(15_000, "claude-sonnet-4-5");
        assert_eq!(monitor.current_tokens, 15_000);
        // max_tokens should update to model's context window (200k)
        assert_eq!(monitor.max_tokens, 200_000);
        assert!(!monitor.needs_compaction);

        // Unknown model: max_tokens falls back to DEFAULT_CONTEXT_WINDOW (100k)
        monitor.update_from_api(90_000, "totally-unknown-model");
        assert_eq!(monitor.current_tokens, 90_000);
        assert_eq!(monitor.max_tokens, 100_000); // DEFAULT_CONTEXT_WINDOW
        assert!(monitor.needs_compaction); // 90k/100k = 90% > 80%
    }

    #[test]
    fn test_usage_color() {
        let mut monitor = ContextMonitor::new(1000, 0.8);
        // estimate_tokens counts words, so use space-separated words
        monitor.update(&[create_test_message(MessageRole::User, &"x ".repeat(200))]);
        assert_eq!(monitor.usage_color(), UsageColor::Green); // 200/1000 = 20%

        monitor.update(&[create_test_message(MessageRole::User, &"x ".repeat(600))]);
        assert_eq!(monitor.usage_color(), UsageColor::Yellow); // 600/1000 = 60%

        monitor.update(&[create_test_message(MessageRole::User, &"x ".repeat(900))]);
        assert_eq!(monitor.usage_color(), UsageColor::Red); // 900/1000 = 90%
    }

    // ── Compaction Strategy ────────────────────────────────

    #[test]
    fn test_compaction_strategy_keep_count() {
        assert_eq!(CompactionStrategy::Aggressive.keep_count(), 20);
        assert_eq!(CompactionStrategy::Balanced.keep_count(), 50);
        assert_eq!(CompactionStrategy::Conservative.keep_count(), 100);
    }

    // ── Compact Context ────────────────────────────────────

    #[test]
    fn test_compact_context_no_compaction_needed() {
        let messages = vec![
            create_test_message(MessageRole::User, "Message 1"),
            create_test_message(MessageRole::Assistant, "Response 1"),
        ];
        let result = compact_context(messages, CompactionStrategy::Balanced);
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn test_compact_context_with_compaction() {
        let messages: Vec<Message> = (0..60)
            .map(|i| {
                if i % 2 == 0 {
                    create_test_message(MessageRole::User, &format!("User message {}", i))
                } else {
                    create_test_message(
                        MessageRole::Assistant,
                        &format!("Assistant response {}", i),
                    )
                }
            })
            .collect();

        let result = compact_context(messages, CompactionStrategy::Balanced);
        assert!(result.len() <= 51);
        assert_eq!(result[0].role, MessageRole::System);
        // Structured summary has sections
        assert!(result[0].content.contains("## Goal"));
    }

    #[test]
    fn test_structured_summary_sections() {
        let messages = [
            create_test_message(MessageRole::User, "How do I create a file?"),
            create_test_message(MessageRole::Assistant, "You can use the write_file tool"),
            create_test_message(MessageRole::User, "What about reading?"),
            create_test_message(MessageRole::Assistant, "Use the read_file tool"),
        ];
        let refs: Vec<&Message> = messages.iter().collect();
        let summary = summarize_messages_structured(&refs);
        assert_eq!(summary.role, MessageRole::System);
        assert!(summary.content.contains("## Goal"));
        assert!(summary.content.contains("create a file"));
    }

    // ── Tool Output Pruning ────────────────────────────────

    #[test]
    fn test_prune_tool_outputs_no_tools() {
        let mut messages = vec![
            create_test_message(MessageRole::User, "Hello"),
            create_test_message(MessageRole::Assistant, "World"),
        ];
        let (count, saved) = prune_tool_outputs(&mut messages);
        assert_eq!(count, 0);
        assert_eq!(saved, 0);
    }

    #[test]
    fn test_prune_tool_outputs_protects_recent() {
        let mut messages = vec![
            create_message_with_tool(
                MessageRole::Assistant,
                "old response",
                "Read",
                &"old content ".repeat(10000),
            ),
            create_test_message(MessageRole::User, "old user msg"),
            create_message_with_tool(
                MessageRole::Assistant,
                "recent response 1",
                "Read",
                &"recent content 1 ".repeat(10000),
            ),
            create_test_message(MessageRole::User, "recent user 1"),
            create_message_with_tool(
                MessageRole::Assistant,
                "recent response 2",
                "Read",
                &"recent content 2 ".repeat(10000),
            ),
            create_test_message(MessageRole::User, "recent user 2"),
        ];

        let (count, saved) = prune_tool_outputs(&mut messages);
        assert!(count >= 1, "expected at least 1 pruned, got {count}");
        assert!(saved > 0);

        let last_assistant = messages
            .iter()
            .rev()
            .find(|m| matches!(m.role, MessageRole::Assistant))
            .unwrap();
        if let Some(ref tools) = last_assistant.tool_executions {
            assert!(tools[0].detailed_output.as_ref().unwrap().len() > PRUNED_OUTPUT_STUB.len());
        }
    }

    #[test]
    fn test_prune_tool_outputs_minimum_threshold() {
        // Small tool output below PRUNE_MINIMUM should not be pruned
        let mut messages = vec![
            create_message_with_tool(
                MessageRole::Assistant,
                "response",
                "Read",
                "small output", // way below PRUNE_MINIMUM
            ),
            create_test_message(MessageRole::User, "user msg 1"),
            create_test_message(MessageRole::User, "user msg 2"),
            create_test_message(MessageRole::User, "user msg 3"),
        ];
        let (count, _) = prune_tool_outputs(&mut messages);
        assert_eq!(count, 0);
    }

    #[test]
    fn test_prune_tool_outputs_respects_already_pruned() {
        let msg = create_message_with_tool(
            MessageRole::Assistant,
            "response",
            "Read",
            PRUNED_OUTPUT_STUB,
        );
        // This is already pruned — should not be counted again
        let mut messages = vec![
            msg,
            create_test_message(MessageRole::User, "u1"),
            create_test_message(MessageRole::User, "u2"),
            create_test_message(MessageRole::User, "u3"),
        ];
        let (count, _) = prune_tool_outputs(&mut messages);
        assert_eq!(count, 0); // already pruned = stops walking
    }

    // ── Helpers ────────────────────────────────────────────

    #[test]
    fn test_truncate_smart() {
        assert_eq!(truncate_smart("hello", 10), "hello");
        assert_eq!(truncate_smart("hello world foo", 12), "hello world...");
    }

    #[test]
    fn test_extract_path_from_input() {
        let input = serde_json::json!({"path": "/src/main.rs"});
        assert_eq!(
            extract_path_from_input("Read", &input),
            Some("/src/main.rs".to_string())
        );

        let input = serde_json::json!({"command": "cat /etc/hosts"});
        assert_eq!(
            extract_path_from_input("Bash", &input),
            Some("/etc/hosts".to_string())
        );
    }

    #[test]
    fn test_dedup_take() {
        let items = vec![
            "a".to_string(),
            "b".to_string(),
            "a".to_string(),
            "c".to_string(),
        ];
        let result = dedup_take(&items, 3);
        assert_eq!(result, vec!["a", "b", "c"]);
    }

    // ── Compaction Preview ─────────────────────────────────

    #[test]
    fn test_compaction_preview() {
        let messages: Vec<Message> = (0..100)
            .map(|i| create_test_message(MessageRole::User, &format!("Message {}", i)))
            .collect();

        let preview =
            CompactionPreview::new(80_000, 100_000, &messages, CompactionStrategy::Balanced);

        assert_eq!(preview.current_tokens, 80_000);
        assert_eq!(preview.max_tokens, 100_000);
        assert_eq!(preview.messages_to_compact, 50);
        assert_eq!(preview.recent_to_keep, 50);
        assert!(preview.new_token_count < preview.current_tokens);
    }

    // ── Config ─────────────────────────────────────────────

    #[test]
    fn test_compaction_config_default() {
        let config = CompactionConfig::default();
        assert_eq!(config.max_tokens, 100_000);
        assert_eq!(config.warning_threshold, 0.8);
        assert_eq!(config.keep_recent_count, 50);
        assert!(config.auto_compact_enabled);
        assert_eq!(config.strategy, CompactionStrategy::Balanced);
    }
}
