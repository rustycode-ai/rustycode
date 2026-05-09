//! Tracing for permission, autonomy, and skill rule evaluations.
//!
//! [`RuleTracer`] records every rule decision (allow / deny / require / warn)
//! with metadata such as source location, precedence, and rule type. Traces
//! can be filtered by level, exported as JSON, or rendered as a
//! human-readable table.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

// TraceLevel

/// The outcome of a rule evaluation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TraceLevel {
    Allow,
    Deny,
    Require,
    Warn,
}

impl std::fmt::Display for TraceLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Allow => write!(f, "ALLOW"),
            Self::Deny => write!(f, "DENY"),
            Self::Require => write!(f, "REQUIRE"),
            Self::Warn => write!(f, "WARN"),
        }
    }
}

// TraceEntry

/// A single recorded rule evaluation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceEntry {
    /// When this evaluation occurred.
    pub timestamp: DateTime<Utc>,
    /// Category of rule (e.g. `"permission"`, `"autonomy"`, `"skill"`).
    pub rule_type: String,
    /// Unique identifier for the rule that was evaluated.
    pub rule_id: String,
    /// Human-readable description of the decision context.
    pub decision: String,
    /// The outcome level.
    pub level: TraceLevel,
    /// Source file that defined the rule, if known.
    pub source_file: Option<String>,
    /// Source line number within `source_file`, if known.
    pub source_line: Option<u32>,
    /// Numeric precedence (higher wins on conflict).
    pub precedence: u32,
}

// RuleTracer

/// Collects and queries rule evaluation traces.
#[derive(Debug, Clone, Default)]
pub struct RuleTracer {
    /// All recorded trace entries in chronological order.
    traces: Vec<TraceEntry>,
    /// Maps `rule_id` to its current precedence value.
    precedence_map: std::collections::HashMap<String, u32>,
}

impl RuleTracer {
    /// Create an empty tracer.
    pub fn new() -> Self {
        Self::default()
    }

    // -- recording helpers --------------------------------------------------

    /// Record a permission rule evaluation.
    pub fn trace_permission(
        &mut self,
        rule_id: impl Into<String>,
        decision: impl Into<String>,
        level: TraceLevel,
    ) {
        self.push_entry("permission", rule_id, decision, level, None, None);
    }

    /// Record an autonomy rule evaluation.
    pub fn trace_autonomy(
        &mut self,
        rule_id: impl Into<String>,
        decision: impl Into<String>,
        level: TraceLevel,
    ) {
        self.push_entry("autonomy", rule_id, decision, level, None, None);
    }

    /// Record a skill rule evaluation.
    pub fn trace_skill(
        &mut self,
        rule_id: impl Into<String>,
        decision: impl Into<String>,
        level: TraceLevel,
    ) {
        self.push_entry("skill", rule_id, decision, level, None, None);
    }

    /// Set (or override) the precedence for a rule id.
    pub fn set_precedence(&mut self, rule_id: impl Into<String>, precedence: u32) {
        self.precedence_map.insert(rule_id.into(), precedence);
    }

    // -- queries ------------------------------------------------------------

    /// Return all traces matching the given level.
    pub fn traces_by_level(&self, level: TraceLevel) -> Vec<&TraceEntry> {
        self.traces.iter().filter(|t| t.level == level).collect()
    }

    /// Return the total number of recorded traces.
    pub const fn len(&self) -> usize {
        self.traces.len()
    }

    /// Return `true` if no traces have been recorded.
    pub const fn is_empty(&self) -> bool {
        self.traces.is_empty()
    }

    // -- formatting ---------------------------------------------------------

    /// Produce a human-readable multi-line representation of all traces.
    pub fn format_trace(&self) -> String {
        if self.traces.is_empty() {
            return "No rule traces recorded.".to_string();
        }

        let mut out = String::new();
        for entry in &self.traces {
            let loc = match (&entry.source_file, entry.source_line) {
                (Some(f), Some(l)) => format!(" {f}:{l}"),
                (Some(f), None) => format!(" {f}"),
                _ => String::new(),
            };
            // Using write! avoids the format_push_string clippy lint.
            let _ = std::fmt::Write::write_fmt(
                &mut out,
                format_args!(
                    "[{}] {} {}:{}: {} (precedence={}){}\n",
                    entry.timestamp.format("%Y-%m-%dT%H:%M:%S%.3f"),
                    entry.level,
                    entry.rule_type,
                    entry.rule_id,
                    entry.decision,
                    entry.precedence,
                    loc,
                ),
            );
        }
        out
    }

    /// Serialize all traces to a JSON string.
    pub fn to_json(&self) -> serde_json::Result<String> {
        serde_json::to_string_pretty(&self.traces)
    }

    // -- internals ----------------------------------------------------------

    fn push_entry(
        &mut self,
        rule_type: &str,
        rule_id: impl Into<String>,
        decision: impl Into<String>,
        level: TraceLevel,
        source_file: Option<String>,
        source_line: Option<u32>,
    ) {
        let rule_id = rule_id.into();
        let precedence = self.precedence_map.get(&rule_id).copied().unwrap_or(0);

        self.traces.push(TraceEntry {
            timestamp: Utc::now(),
            rule_type: rule_type.to_string(),
            rule_id,
            decision: decision.into(),
            level,
            source_file,
            source_line,
            precedence,
        });
    }
}

// Tests

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trace_level_display() {
        assert_eq!(TraceLevel::Allow.to_string(), "ALLOW");
        assert_eq!(TraceLevel::Deny.to_string(), "DENY");
        assert_eq!(TraceLevel::Require.to_string(), "REQUIRE");
        assert_eq!(TraceLevel::Warn.to_string(), "WARN");
    }

    #[test]
    fn trace_level_serde_roundtrip() {
        let json = serde_json::to_string(&TraceLevel::Deny).unwrap();
        assert_eq!(json, "\"deny\"");
        let back: TraceLevel = serde_json::from_str(&json).unwrap();
        assert_eq!(back, TraceLevel::Deny);
    }

    #[test]
    fn tracer_starts_empty() {
        let tracer = RuleTracer::new();
        assert!(tracer.is_empty());
        assert_eq!(tracer.len(), 0);
    }

    #[test]
    fn trace_permission_records_entry() {
        let mut tracer = RuleTracer::new();
        tracer.trace_permission("Write", "allowed write to src/main.rs", TraceLevel::Allow);
        assert_eq!(tracer.len(), 1);

        let entry = &tracer.traces[0];
        assert_eq!(entry.rule_type, "permission");
        assert_eq!(entry.rule_id, "Write");
        assert_eq!(entry.level, TraceLevel::Allow);
        assert_eq!(entry.decision, "allowed write to src/main.rs");
    }

    #[test]
    fn trace_autonomy_records_entry() {
        let mut tracer = RuleTracer::new();
        tracer.trace_autonomy("auto_commit", "autonomous commit denied", TraceLevel::Deny);
        assert_eq!(tracer.len(), 1);

        let entry = &tracer.traces[0];
        assert_eq!(entry.rule_type, "autonomy");
        assert_eq!(entry.level, TraceLevel::Deny);
    }

    #[test]
    fn trace_skill_records_entry() {
        let mut tracer = RuleTracer::new();
        tracer.trace_skill("rust_patterns", "skill loaded", TraceLevel::Require);
        assert_eq!(tracer.len(), 1);

        let entry = &tracer.traces[0];
        assert_eq!(entry.rule_type, "skill");
        assert_eq!(entry.level, TraceLevel::Require);
    }

    #[test]
    fn set_precedence_applied_to_subsequent_traces() {
        let mut tracer = RuleTracer::new();
        tracer.set_precedence("my_rule", 10);
        tracer.trace_permission("my_rule", "checked", TraceLevel::Allow);

        let entry = &tracer.traces[0];
        assert_eq!(entry.precedence, 10);
    }

    #[test]
    fn traces_by_level_filters_correctly() {
        let mut tracer = RuleTracer::new();
        tracer.trace_permission("a", "ok", TraceLevel::Allow);
        tracer.trace_permission("b", "no", TraceLevel::Deny);
        tracer.trace_permission("c", "maybe", TraceLevel::Warn);

        let denied = tracer.traces_by_level(TraceLevel::Deny);
        assert_eq!(denied.len(), 1);
        assert_eq!(denied[0].rule_id, "b");

        let allowed = tracer.traces_by_level(TraceLevel::Allow);
        assert_eq!(allowed.len(), 1);
    }

    #[test]
    fn format_trace_empty() {
        let tracer = RuleTracer::new();
        assert_eq!(tracer.format_trace(), "No rule traces recorded.");
    }

    #[test]
    fn format_trace_nonempty() {
        let mut tracer = RuleTracer::new();
        tracer.trace_permission("test_rule", "evaluated", TraceLevel::Allow);
        let output = tracer.format_trace();
        assert!(output.contains("ALLOW"));
        assert!(output.contains("permission"));
        assert!(output.contains("test_rule"));
        assert!(output.contains("evaluated"));
        assert!(output.contains("precedence=0"));
    }

    #[test]
    fn to_json_produces_valid_json() {
        let mut tracer = RuleTracer::new();
        tracer.trace_permission("json_test", "serializable", TraceLevel::Warn);
        let json = tracer.to_json().unwrap();
        assert!(json.contains("\"json_test\""));
        assert!(json.contains("\"warn\""));

        // Round-trip: parse back.
        let parsed: Vec<TraceEntry> = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].rule_id, "json_test");
    }
}
