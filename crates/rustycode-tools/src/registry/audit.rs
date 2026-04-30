//! Tool Call Audit Logger
//!
//! Structured audit logging for tool invocations with timing, arguments,
//! and results. Inspired by goose's tool inspection system.
//!
//! Provides:
//! - Per-tool timing statistics
//! - Success/failure rate tracking
//! - Slow tool detection
//! - Session-level audit trail
//!
//! # Example
//!
//! ```ignore
//! use rustycode_tools::tool_audit::{ToolAuditLogger, AuditEntry};
//!
//! let logger = ToolAuditLogger::new(1000);
//! logger.record(AuditEntry::new("bash", "ls -la", true, 150, 256));
//! logger.record(AuditEntry::new("bash", "rm -rf /", false, 50, 0));
//!
//! let stats = logger.tool_stats("bash").unwrap();
//! assert_eq!(stats.call_count, 2);
//! assert_eq!(stats.success_count, 1);
//! ```

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Instant;

/// Maximum number of audit entries to keep in memory
const DEFAULT_MAX_ENTRIES: usize = 1000;

/// A single audit log entry for a tool invocation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEntry {
    /// Tool name that was invoked
    pub tool_name: String,
    /// Hash of arguments (for privacy, we don't store raw args)
    pub args_hash: u64,
    /// Whether the invocation succeeded
    pub success: bool,
    /// Execution duration in milliseconds
    pub duration_ms: u64,
    /// Size of the output in bytes
    pub output_size: usize,
    /// Unix timestamp of the invocation
    pub timestamp: u64,
    /// Error message if failed (truncated)
    pub error: Option<String>,
}

impl AuditEntry {
    /// Create a new audit entry.
    pub fn new(
        tool_name: impl Into<String>,
        args_summary: &str,
        success: bool,
        duration_ms: u64,
        output_size: usize,
    ) -> Self {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        args_summary.hash(&mut hasher);
        let args_hash = hasher.finish();

        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        Self {
            tool_name: tool_name.into(),
            args_hash,
            success,
            duration_ms,
            output_size,
            timestamp,
            error: None,
        }
    }

    /// Add an error message to the entry.
    pub fn with_error(mut self, error: impl Into<String>) -> Self {
        let msg = error.into();
        // Truncate error to prevent memory bloat
        self.error = Some(if msg.len() > 500 {
            let truncated = if msg.is_char_boundary(500) {
                &msg[..500]
            } else {
                let mut b = 500;
                while b > 0 && !msg.is_char_boundary(b) {
                    b -= 1;
                }
                &msg[..b]
            };
            format!("{truncated}...[truncated]")
        } else {
            msg
        });
        self
    }
}

/// Aggregate statistics for a single tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolStats {
    /// Tool name
    pub tool_name: String,
    /// Total number of invocations
    pub call_count: u64,
    /// Number of successful invocations
    pub success_count: u64,
    /// Number of failed invocations
    pub failure_count: u64,
    /// Total execution time in ms
    pub total_duration_ms: u64,
    /// Average execution time in ms
    pub avg_duration_ms: f64,
    /// Maximum execution time in ms
    pub max_duration_ms: u64,
    /// Minimum execution time in ms
    pub min_duration_ms: u64,
    /// Total output size in bytes
    pub total_output_bytes: u64,
    /// Success rate (0.0 - 1.0)
    pub success_rate: f64,
}

impl ToolStats {
    const fn new(tool_name: String) -> Self {
        Self {
            tool_name,
            call_count: 0,
            success_count: 0,
            failure_count: 0,
            total_duration_ms: 0,
            avg_duration_ms: 0.0,
            max_duration_ms: 0,
            min_duration_ms: u64::MAX,
            total_output_bytes: 0,
            success_rate: 0.0,
        }
    }

    fn record(&mut self, entry: &AuditEntry) {
        self.call_count = self.call_count.saturating_add(1);
        if entry.success {
            self.success_count = self.success_count.saturating_add(1);
        } else {
            self.failure_count = self.failure_count.saturating_add(1);
        }
        self.total_duration_ms = self.total_duration_ms.saturating_add(entry.duration_ms);
        self.max_duration_ms = self.max_duration_ms.max(entry.duration_ms);
        self.min_duration_ms = self.min_duration_ms.min(entry.duration_ms);
        self.total_output_bytes = self
            .total_output_bytes
            .saturating_add(entry.output_size as u64);

        if self.call_count > 0 {
            self.avg_duration_ms = self.total_duration_ms as f64 / self.call_count as f64;
            self.success_rate = self.success_count as f64 / self.call_count as f64;
        }
    }
}

/// Session-level summary of all tool activity.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionSummary {
    /// Total number of tool calls
    pub total_calls: u64,
    /// Number of distinct tools used
    pub distinct_tools: usize,
    /// Total execution time in ms
    pub total_duration_ms: u64,
    /// Overall success rate
    pub overall_success_rate: f64,
    /// Per-tool statistics
    pub tool_stats: Vec<ToolStats>,
    /// Tools ranked by call count (descending)
    pub tools_by_frequency: Vec<(String, u64)>,
    /// Tools ranked by total duration (descending)
    pub tools_by_duration: Vec<(String, u64)>,
}

/// A timing guard that records duration on drop.
pub struct AuditTimer {
    tool_name: String,
    args_summary: String,
    start: Instant,
    logger: Option<std::sync::Arc<Mutex<ToolAuditLoggerInner>>>,
}

impl AuditTimer {
    /// Start a new timer for a tool call.
    pub fn new(
        tool_name: impl Into<String>,
        args_summary: impl Into<String>,
        logger_inner: std::sync::Arc<Mutex<ToolAuditLoggerInner>>,
    ) -> Self {
        Self {
            tool_name: tool_name.into(),
            args_summary: args_summary.into(),
            start: Instant::now(),
            logger: Some(logger_inner),
        }
    }

    /// Complete the timer with a result.
    pub fn complete(mut self, success: bool, output_size: usize, error: Option<String>) {
        self.finish(success, output_size, error);
    }

    fn finish(&mut self, success: bool, output_size: usize, error: Option<String>) {
        if let Some(inner) = self.logger.take() {
            let duration_ms = self.start.elapsed().as_millis() as u64;
            let mut entry = AuditEntry::new(
                &self.tool_name,
                &self.args_summary,
                success,
                duration_ms,
                output_size,
            );
            if let Some(err) = error {
                entry = entry.with_error(err);
            }
            if let Ok(mut guard) = inner.lock() {
                guard.record(entry);
            }
        }
    }
}

impl Drop for AuditTimer {
    fn drop(&mut self) {
        if self.logger.is_some() {
            self.finish(
                false,
                0,
                Some("timer dropped without explicit completion".to_string()),
            );
        }
    }
}

/// Inner state for the audit logger (behind Arc<Mutex>).
pub struct ToolAuditLoggerInner {
    entries: Vec<AuditEntry>,
    stats: HashMap<String, ToolStats>,
    max_entries: usize,
}

impl ToolAuditLoggerInner {
    fn record(&mut self, entry: AuditEntry) {
        let tool_name = entry.tool_name.clone();

        // Update stats
        self.stats
            .entry(tool_name.clone())
            .or_insert_with(|| ToolStats::new(tool_name.clone()))
            .record(&entry);

        // Store entry (with rotation)
        if self.entries.len() >= self.max_entries {
            self.entries.remove(0);
        }
        self.entries.push(entry);
    }
}

/// Structured audit logger for tool invocations.
///
/// Thread-safe, bounded audit log with aggregate statistics.
pub struct ToolAuditLogger {
    inner: std::sync::Arc<Mutex<ToolAuditLoggerInner>>,
}

impl ToolAuditLogger {
    /// Create a new audit logger with the given maximum entry count.
    pub fn new(max_entries: usize) -> Self {
        Self {
            inner: std::sync::Arc::new(Mutex::new(ToolAuditLoggerInner {
                entries: Vec::with_capacity(max_entries.min(100)),
                stats: HashMap::new(),
                max_entries,
            })),
        }
    }

    /// Create with default max entries (1000).
    pub fn with_defaults() -> Self {
        Self::new(DEFAULT_MAX_ENTRIES)
    }

    /// Record an audit entry.
    pub fn record(&self, entry: AuditEntry) {
        if let Ok(mut guard) = self.inner.lock() {
            guard.record(entry);
        }
    }

    /// Start a timed audit for a tool call. Returns an `AuditTimer`.
    pub fn start_timer(
        &self,
        tool_name: impl Into<String>,
        args_summary: impl Into<String>,
    ) -> AuditTimer {
        AuditTimer::new(tool_name, args_summary, self.inner.clone())
    }

    /// Get statistics for a specific tool.
    pub fn tool_stats(&self, tool_name: &str) -> Option<ToolStats> {
        self.inner
            .lock()
            .ok()
            .and_then(|guard| guard.stats.get(tool_name).cloned())
    }

    /// Get all tool statistics.
    pub fn all_stats(&self) -> Vec<ToolStats> {
        self.inner
            .lock()
            .ok()
            .map(|guard| guard.stats.values().cloned().collect())
            .unwrap_or_default()
    }

    /// Get recent audit entries (last N).
    pub fn recent_entries(&self, count: usize) -> Vec<AuditEntry> {
        self.inner
            .lock()
            .ok()
            .map(|guard| {
                let len = guard.entries.len();
                guard
                    .entries
                    .iter()
                    .skip(len.saturating_sub(count))
                    .cloned()
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Get a session summary.
    pub fn session_summary(&self) -> SessionSummary {
        let guard = self.inner.lock().ok();

        match guard {
            Some(guard) => {
                let total_calls: u64 = guard.stats.values().map(|s| s.call_count).sum();
                let total_duration: u64 = guard.stats.values().map(|s| s.total_duration_ms).sum();
                let total_success: u64 = guard.stats.values().map(|s| s.success_count).sum();

                let mut by_freq: Vec<(String, u64)> = guard
                    .stats
                    .values()
                    .map(|s| (s.tool_name.clone(), s.call_count))
                    .collect();
                by_freq.sort_by_key(|a| std::cmp::Reverse(a.1));

                let mut by_dur: Vec<(String, u64)> = guard
                    .stats
                    .values()
                    .map(|s| (s.tool_name.clone(), s.total_duration_ms))
                    .collect();
                by_dur.sort_by_key(|a| std::cmp::Reverse(a.1));

                SessionSummary {
                    total_calls,
                    distinct_tools: guard.stats.len(),
                    total_duration_ms: total_duration,
                    overall_success_rate: if total_calls > 0 {
                        total_success as f64 / total_calls as f64
                    } else {
                        0.0
                    },
                    tool_stats: guard.stats.values().cloned().collect(),
                    tools_by_frequency: by_freq,
                    tools_by_duration: by_dur,
                }
            }
            None => SessionSummary {
                total_calls: 0,
                distinct_tools: 0,
                total_duration_ms: 0,
                overall_success_rate: 0.0,
                tool_stats: Vec::new(),
                tools_by_frequency: Vec::new(),
                tools_by_duration: Vec::new(),
            },
        }
    }

    /// Clear all audit data.
    pub fn clear(&self) {
        if let Ok(mut guard) = self.inner.lock() {
            guard.entries.clear();
            guard.stats.clear();
        }
    }

    /// Get the number of recorded entries.
    pub fn entry_count(&self) -> usize {
        self.inner
            .lock()
            .ok()
            .map(|guard| guard.entries.len())
            .unwrap_or(0)
    }
}

impl Default for ToolAuditLogger {
    fn default() -> Self {
        Self::with_defaults()
    }
}

impl Clone for ToolAuditLogger {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_audit_entry_creation() {
        let entry = AuditEntry::new("bash", "ls -la", true, 150, 256);
        assert_eq!(entry.tool_name, "bash");
        assert!(entry.success);
        assert_eq!(entry.duration_ms, 150);
        assert_eq!(entry.output_size, 256);
        assert!(entry.error.is_none());
    }

    #[test]
    fn test_audit_entry_with_error() {
        let entry =
            AuditEntry::new("bash", "rm -rf /", false, 50, 0).with_error("Permission denied");
        assert!(!entry.success);
        assert_eq!(entry.error.as_deref(), Some("Permission denied"));
    }

    #[test]
    fn test_audit_entry_error_truncation() {
        let long_error = "x".repeat(600);
        let entry = AuditEntry::new("bash", "cmd", false, 10, 0).with_error(long_error);
        assert!(entry.error.unwrap().ends_with("[truncated]"));
    }

    #[test]
    fn test_tool_stats_tracking() {
        let logger = ToolAuditLogger::new(100);

        logger.record(AuditEntry::new("bash", "ls", true, 100, 50));
        logger.record(AuditEntry::new("bash", "pwd", true, 50, 20));
        logger.record(AuditEntry::new("bash", "rm", false, 200, 0));

        let stats = logger.tool_stats("bash").unwrap();
        assert_eq!(stats.call_count, 3);
        assert_eq!(stats.success_count, 2);
        assert_eq!(stats.failure_count, 1);
        assert_eq!(stats.max_duration_ms, 200);
        assert_eq!(stats.min_duration_ms, 50);
        assert!((stats.success_rate - 0.667).abs() < 0.01);
    }

    #[test]
    fn test_tool_stats_multiple_tools() {
        let logger = ToolAuditLogger::new(100);

        logger.record(AuditEntry::new("read_file", "main.rs", true, 10, 500));
        logger.record(AuditEntry::new("write_file", "out.rs", true, 20, 0));
        logger.record(AuditEntry::new("read_file", "lib.rs", true, 8, 300));

        let all = logger.all_stats();
        assert_eq!(all.len(), 2);

        let read_stats = logger.tool_stats("read_file").unwrap();
        assert_eq!(read_stats.call_count, 2);
        assert_eq!(read_stats.total_output_bytes, 800);
    }

    #[test]
    fn test_session_summary() {
        let logger = ToolAuditLogger::new(100);

        logger.record(AuditEntry::new("bash", "cmd1", true, 100, 50));
        logger.record(AuditEntry::new("bash", "cmd2", false, 200, 0));
        logger.record(AuditEntry::new("read_file", "f1", true, 50, 100));

        let summary = logger.session_summary();
        assert_eq!(summary.total_calls, 3);
        assert_eq!(summary.distinct_tools, 2);
        assert_eq!(summary.total_duration_ms, 350);
        assert!((summary.overall_success_rate - 0.667).abs() < 0.01);

        // Frequency ranking
        assert_eq!(summary.tools_by_frequency[0], ("bash".to_string(), 2));
        assert_eq!(summary.tools_by_frequency[1], ("read_file".to_string(), 1));

        // Duration ranking
        assert_eq!(summary.tools_by_duration[0], ("bash".to_string(), 300));
        assert_eq!(summary.tools_by_duration[1], ("read_file".to_string(), 50));
    }

    #[test]
    fn test_recent_entries() {
        let logger = ToolAuditLogger::new(100);

        for i in 0..10 {
            logger.record(AuditEntry::new("tool", &format!("arg{}", i), true, i, 0));
        }

        let recent = logger.recent_entries(3);
        assert_eq!(recent.len(), 3);
        assert!(recent[2].args_hash != recent[0].args_hash);
    }

    #[test]
    fn test_entry_rotation() {
        let logger = ToolAuditLogger::new(5); // Very small

        for i in 0..10 {
            logger.record(AuditEntry::new("tool", &format!("arg{}", i), true, i, 0));
        }

        // Should have kept last 5
        assert_eq!(logger.entry_count(), 5);
    }

    #[test]
    fn test_clear() {
        let logger = ToolAuditLogger::new(100);
        logger.record(AuditEntry::new("bash", "cmd", true, 100, 50));
        assert_eq!(logger.entry_count(), 1);

        logger.clear();
        assert_eq!(logger.entry_count(), 0);
        assert!(logger.all_stats().is_empty());
    }

    #[test]
    fn test_clone_shares_state() {
        let logger = ToolAuditLogger::new(100);
        let clone = logger.clone();

        clone.record(AuditEntry::new("bash", "cmd", true, 100, 50));
        assert_eq!(logger.entry_count(), 1);
    }

    #[test]
    fn test_empty_summary() {
        let logger = ToolAuditLogger::new(100);
        let summary = logger.session_summary();
        assert_eq!(summary.total_calls, 0);
        assert_eq!(summary.distinct_tools, 0);
    }

    #[test]
    fn test_timer_complete() {
        let logger = ToolAuditLogger::new(100);

        let timer = logger.start_timer("bash", "ls -la");
        std::thread::sleep(std::time::Duration::from_millis(10));
        timer.complete(true, 100, None);

        assert_eq!(logger.entry_count(), 1);
        let stats = logger.tool_stats("bash").unwrap();
        assert!(stats.total_duration_ms >= 10);
        assert_eq!(stats.success_count, 1);
        assert_eq!(stats.failure_count, 0);
    }

    #[test]
    fn test_timer_drop_without_complete_records_failure() {
        let logger = ToolAuditLogger::new(100);

        {
            let _timer = logger.start_timer("bash", "ls -la");
            // Timer dropped without calling complete()
        }

        assert_eq!(logger.entry_count(), 1);
        let stats = logger.tool_stats("bash").unwrap();
        assert_eq!(stats.call_count, 1);
        assert_eq!(stats.failure_count, 1);
        assert_eq!(stats.success_count, 0);

        let entries = logger.recent_entries(1);
        assert_eq!(
            entries[0].error.as_deref(),
            Some("timer dropped without explicit completion")
        );
    }

    #[test]
    fn test_timer_complete_then_drop_no_double_record() {
        let logger = ToolAuditLogger::new(100);

        {
            let timer = logger.start_timer("bash", "ls -la");
            timer.complete(true, 50, None);
            // Drop happens after complete — should NOT record a second entry
        }

        assert_eq!(logger.entry_count(), 1);
        let stats = logger.tool_stats("bash").unwrap();
        assert_eq!(stats.success_count, 1);
        assert_eq!(stats.failure_count, 0);
    }
}
