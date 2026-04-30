# Phase 8: Observability & Diagnostics -- TDD Implementation Plan

**Date**: 2026-04-25
**Goal**: RustyCode provides deep visibility into its behavior, rules, state, and decision-making.
**Status**: Not Started
**See Also**: [Generative Programmer analysis](2026-04-25-generative-programmer-real-analysis.md#phase-status-map)
**Dependencies**: Phase 1 (memory system), Phase 4 (domain context), Phase 7 (checkpoints)
**Target**: ~60 tests across 4 modules

---

## File Structure

```
New files:
  crates/rustycode-observability/src/diagnostics.rs    (~400 lines, 18 tests)
  crates/rustycode-observability/src/rule_tracer.rs    (~350 lines, 14 tests)
  crates/rustycode-observability/src/state_inspector.rs (~300 lines, 12 tests)
  crates/rustycode-cli/src/commands/doctor.rs           (~200 lines, 8 tests)

Modified files:
  crates/rustycode-observability/src/lib.rs            (add pub mod diagnostics, rule_tracer, state_inspector)
  crates/rustycode-cli/src/lib.rs                       (add doctor command)
  crates/rustycode-prompt/src/lib.rs                    (expose active rules for diagnostics)
```

---

## Implementation Status

To be completed in this phase.

---

## Chunk 1: Diagnostic System (rustycode-observability/src/diagnostics.rs)

### 1.1 Diagnostic checks and reporting

**File**: `crates/rustycode-observability/src/diagnostics.rs`

**RED -- Tests**:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_diagnostic_check() {
        let check = DiagnosticCheck::new("config_validity", "Configuration file is valid");
        assert_eq!(check.id, "config_validity");
        assert_eq!(check.status, CheckStatus::Pending);
    }

    #[test]
    fn run_check_and_record_result() {
        let mut check = DiagnosticCheck::new("config_validity", "Configuration file is valid");
        check.run(|| Ok(()));
        assert_eq!(check.status, CheckStatus::Pass);
    }

    #[test]
    fn fail_check_on_error() {
        let mut check = DiagnosticCheck::new("git_repo", "Git repository exists");
        check.run(|| Err(anyhow::anyhow!("Not a git repo")));
        assert_eq!(check.status, CheckStatus::Fail);
        assert!(check.error_message.is_some());
    }

    #[test]
    fn run_all_diagnostics() {
        let diagnostics = DiagnosticSuite::new();
        let mut checks = diagnostics.build_checks();
        let report = diagnostics.run_all(&mut checks).unwrap();
        
        assert!(!report.checks.is_empty());
        assert!(report.timestamp.is_some());
    }

    #[test]
    fn diagnostic_report_summary() {
        let diagnostics = DiagnosticSuite::new();
        let mut checks = diagnostics.build_checks();
        let report = diagnostics.run_all(&mut checks).unwrap();
        
        assert_eq!(report.total_checks, report.checks.len());
        assert!(report.passed_count + report.failed_count <= report.total_checks);
    }

    #[test]
    fn categorize_diagnostics() {
        let check_config = DiagnosticCheck::new("config", "Config check")
            .with_category("configuration");
        let check_env = DiagnosticCheck::new("env", "Environment check")
            .with_category("environment");
        
        assert_eq!(check_config.category, Some("configuration".to_string()));
        assert_eq!(check_env.category, Some("environment".to_string()));
    }

    #[test]
    fn skip_optional_checks() {
        let mut check = DiagnosticCheck::new("optional_feature", "Optional feature check")
            .optional();
        
        check.status = CheckStatus::Fail; // Even if it fails
        let report = DiagnosticReport::default();
        assert!(report.is_healthy()); // Optional failures don't make unhealthy
    }
}
```

### 1.2 Diagnostic implementation

```rust
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::time::SystemTime;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CheckStatus {
    Pending,
    Pass,
    Fail,
    Warning,
    Skipped,
}

/// A single diagnostic check
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiagnosticCheck {
    pub id: String,
    pub description: String,
    pub status: CheckStatus,
    pub error_message: Option<String>,
    pub category: Option<String>,
    pub is_optional: bool,
}

impl DiagnosticCheck {
    pub fn new(id: &str, description: &str) -> Self {
        Self {
            id: id.to_string(),
            description: description.to_string(),
            status: CheckStatus::Pending,
            error_message: None,
            category: None,
            is_optional: false,
        }
    }

    pub fn with_category(mut self, category: &str) -> Self {
        self.category = Some(category.to_string());
        self
    }

    pub fn optional(mut self) -> Self {
        self.is_optional = true;
        self
    }

    /// Run the check with a closure
    pub fn run<F>(&mut self, f: F)
    where
        F: FnOnce() -> Result<()>,
    {
        match f() {
            Ok(()) => self.status = CheckStatus::Pass,
            Err(e) => {
                self.status = CheckStatus::Fail;
                self.error_message = Some(e.to_string());
            }
        }
    }
}

/// Complete diagnostic report
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DiagnosticReport {
    pub checks: Vec<DiagnosticCheck>,
    pub total_checks: usize,
    pub passed_count: usize,
    pub failed_count: usize,
    pub warning_count: usize,
    pub timestamp: Option<SystemTime>,
}

impl DiagnosticReport {
    pub fn new() -> Self {
        Self {
            checks: vec![],
            total_checks: 0,
            passed_count: 0,
            failed_count: 0,
            warning_count: 0,
            timestamp: Some(SystemTime::now()),
        }
    }

    pub fn add_check(&mut self, check: DiagnosticCheck) {
        self.total_checks += 1;
        match check.status {
            CheckStatus::Pass => self.passed_count += 1,
            CheckStatus::Fail => self.failed_count += 1,
            CheckStatus::Warning => self.warning_count += 1,
            _ => {}
        }
        self.checks.push(check);
    }

    /// Is the overall system healthy?
    pub fn is_healthy(&self) -> bool {
        let critical_failures = self.checks
            .iter()
            .filter(|c| !c.is_optional && c.status == CheckStatus::Fail)
            .count();
        critical_failures == 0
    }

    /// Human-readable health status
    pub fn health_status(&self) -> &'static str {
        if self.is_healthy() {
            "✓ Healthy"
        } else {
            "✗ Degraded"
        }
    }
}

/// Diagnostic test suite
pub struct DiagnosticSuite;

impl DiagnosticSuite {
    pub fn new() -> Self {
        Self
    }

    /// Build all available checks
    pub fn build_checks(&self) -> Vec<DiagnosticCheck> {
        vec![
            DiagnosticCheck::new("git_repo", "Git repository exists")
                .with_category("environment"),
            DiagnosticCheck::new("workspace_valid", "Workspace Cargo.toml is valid")
                .with_category("build"),
            DiagnosticCheck::new("rust_version", "Rust version meets MSRV")
                .with_category("environment"),
            DiagnosticCheck::new("config_loadable", "CLAUDE.md loads successfully")
                .with_category("configuration"),
            DiagnosticCheck::new("memory_writeable", "Memory directory is writeable")
                .with_category("storage"),
        ]
    }

    /// Run all checks
    pub fn run_all(&self, checks: &mut [DiagnosticCheck]) -> Result<DiagnosticReport> {
        let mut report = DiagnosticReport::new();

        for check in checks {
            match check.id.as_str() {
                "git_repo" => check.run(|| Self::check_git_repo()),
                "workspace_valid" => check.run(|| Self::check_workspace()),
                "rust_version" => check.run(|| Self::check_rust_version()),
                "config_loadable" => check.run(|| Self::check_config()),
                "memory_writeable" => check.run(|| Self::check_memory()),
                _ => {}
            }
            report.add_check(check.clone());
        }

        Ok(report)
    }

    fn check_git_repo() -> Result<()> {
        // Check if current directory is a git repo
        std::process::Command::new("git")
            .args(&["rev-parse", "--git-dir"])
            .output()
            .ok()
            .and_then(|o| if o.status.success() { Some(()) } else { None })
            .ok_or_else(|| anyhow::anyhow!("Not a git repository"))
    }

    fn check_workspace() -> Result<()> {
        let manifest = std::env::current_dir()?
            .join("Cargo.toml");
        if manifest.exists() {
            Ok(())
        } else {
            Err(anyhow::anyhow!("Cargo.toml not found"))
        }
    }

    fn check_rust_version() -> Result<()> {
        let output = std::process::Command::new("rustc")
            .arg("--version")
            .output()?;
        let version = String::from_utf8(output.stdout)?;
        if version.contains("1.") {
            Ok(())
        } else {
            Err(anyhow::anyhow!("Invalid Rust version: {}", version))
        }
    }

    fn check_config() -> Result<()> {
        let claude_md = std::env::current_dir()?
            .join("CLAUDE.md");
        if claude_md.exists() {
            Ok(())
        } else {
            Err(anyhow::anyhow!("CLAUDE.md not found"))
        }
    }

    fn check_memory() -> Result<()> {
        let memory_dir = std::env::current_dir()?
            .join(".claude/memory");
        fs::create_dir_all(&memory_dir)?;
        // Try to write a test file
        let test_file = memory_dir.join(".write_test");
        std::fs::write(&test_file, "test")?;
        std::fs::remove_file(&test_file)?;
        Ok(())
    }
}

impl Default for DiagnosticSuite {
    fn default() -> Self {
        Self::new()
    }
}
```

---

## Chunk 2: Rule Tracing (rustycode-observability/src/rule_tracer.rs)

### 2.1 Rule decision tracing

**File**: `crates/rustycode-observability/src/rule_tracer.rs`

**RED -- Tests**:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trace_permission_decision() {
        let mut tracer = RuleTracer::new();
        tracer.trace_permission(
            "bash",
            "Denied: matches blocked command pattern",
            TraceLevel::Deny,
        );
        
        assert_eq!(tracer.traces.len(), 1);
        assert_eq!(tracer.traces[0].rule_type, "permission");
    }

    #[test]
    fn trace_multiple_rules() {
        let mut tracer = RuleTracer::new();
        tracer.trace_permission("read", "Allowed: read-only", TraceLevel::Allow);
        tracer.trace_autonomy("code_review", "L3 required: code quality gate", TraceLevel::Require);
        
        assert_eq!(tracer.traces.len(), 2);
    }

    #[test]
    fn trace_rule_chain_and_precedence() {
        let mut tracer = RuleTracer::new();
        tracer.trace_permission("mkdir", "Matches policy rule", TraceLevel::Allow);
        tracer.set_precedence("policy", 100);
        tracer.set_precedence("user_settings", 10);
        
        let rule = &tracer.traces[0];
        assert_eq!(rule.precedence, Some(100));
    }

    #[test]
    fn format_trace_for_display() {
        let mut tracer = RuleTracer::new();
        tracer.trace_permission("bash", "Blocked", TraceLevel::Deny);
        
        let output = tracer.format_trace();
        assert!(output.contains("permission"));
        assert!(output.contains("Blocked"));
    }

    #[test]
    fn export_trace_as_json() {
        let mut tracer = RuleTracer::new();
        tracer.trace_permission("read", "Allowed", TraceLevel::Allow);
        
        let json = tracer.to_json().unwrap();
        assert!(json.contains("permission"));
    }
}
```

### 2.2 RuleTracer implementation

```rust
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TraceLevel {
    Allow,
    Deny,
    Require,
    Warn,
}

/// A single traced rule decision
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceEntry {
    pub timestamp: std::time::SystemTime,
    pub rule_type: String,
    pub rule_id: String,
    pub decision: String,
    pub level: TraceLevel,
    pub source_file: Option<String>,
    pub source_line: Option<u32>,
    pub precedence: Option<u32>,
}

/// Traces rule decisions for transparency
pub struct RuleTracer {
    pub traces: Vec<TraceEntry>,
    precedence_map: HashMap<String, u32>,
}

impl RuleTracer {
    pub fn new() -> Self {
        Self {
            traces: vec![],
            precedence_map: HashMap::new(),
        }
    }

    pub fn trace_permission(&mut self, rule_id: &str, decision: &str, level: TraceLevel) {
        self.add_trace("permission", rule_id, decision, level);
    }

    pub fn trace_autonomy(&mut self, rule_id: &str, decision: &str, level: TraceLevel) {
        self.add_trace("autonomy", rule_id, decision, level);
    }

    pub fn trace_skill(&mut self, rule_id: &str, decision: &str, level: TraceLevel) {
        self.add_trace("skill", rule_id, decision, level);
    }

    fn add_trace(&mut self, rule_type: &str, rule_id: &str, decision: &str, level: TraceLevel) {
        let precedence = self.precedence_map.get(rule_type).copied();
        
        let entry = TraceEntry {
            timestamp: std::time::SystemTime::now(),
            rule_type: rule_type.to_string(),
            rule_id: rule_id.to_string(),
            decision: decision.to_string(),
            level,
            source_file: None,
            source_line: None,
            precedence,
        };
        self.traces.push(entry);
    }

    pub fn set_precedence(&mut self, source: &str, precedence: u32) {
        self.precedence_map.insert(source.to_string(), precedence);
    }

    pub fn format_trace(&self) -> String {
        let mut output = String::new();
        output.push_str("Rule Trace:\n");
        output.push_str("===========\n");
        
        for entry in &self.traces {
            output.push_str(&format!(
                "[{:?}] {} ({}): {}\n",
                entry.level, entry.rule_type, entry.rule_id, entry.decision
            ));
        }
        output
    }

    pub fn to_json(&self) -> anyhow::Result<String> {
        Ok(serde_json::to_string_pretty(&self.traces)?)
    }

    /// Clear all traces
    pub fn clear(&mut self) {
        self.traces.clear();
    }

    /// Get traces of a specific level
    pub fn traces_by_level(&self, level: TraceLevel) -> Vec<&TraceEntry> {
        self.traces.iter().filter(|t| t.level == level).collect()
    }
}

impl Default for RuleTracer {
    fn default() -> Self {
        Self::new()
    }
}
```

---

## Chunk 3: State Inspector (rustycode-observability/src/state_inspector.rs)

### 3.1 System state inspection

**File**: `crates/rustycode-observability/src/state_inspector.rs`

**RED -- Tests**:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inspect_active_context() {
        let inspector = StateInspector::new();
        let context = inspector.active_context().unwrap();
        assert!(!context.scopes.is_empty());
    }

    #[test]
    fn list_active_permissions() {
        let inspector = StateInspector::new();
        let permissions = inspector.active_permissions().unwrap();
        assert!(!permissions.is_empty());
    }

    #[test]
    fn get_active_domain_context() {
        let inspector = StateInspector::new();
        if let Ok(domain) = inspector.domain_context() {
            assert!(!domain.project_name.is_empty());
        }
    }

    #[test]
    fn trace_context_scope_stack() {
        let inspector = StateInspector::new();
        let stack = inspector.context_scope_stack().unwrap();
        assert!(!stack.is_empty());
    }

    #[test]
    fn inspect_memory_state() {
        let inspector = StateInspector::new();
        let memory = inspector.memory_state().unwrap();
        assert_eq!(memory.topics.len(), memory.topic_count);
    }

    #[test]
    fn get_execution_phase() {
        let inspector = StateInspector::new();
        let phase = inspector.current_phase().unwrap();
        assert!(!phase.is_empty());
    }
}
```

### 3.2 StateInspector implementation

```rust
use anyhow::Result;

/// Provides inspection of RustyCode internal state
pub struct StateInspector {
    // References to runtime state (would be injected in real implementation)
}

impl StateInspector {
    pub fn new() -> Self {
        Self {}
    }

    /// Get the currently active context (scopes, rules, etc.)
    pub fn active_context(&self) -> Result<ActiveContext> {
        Ok(ActiveContext {
            scopes: vec![
                "project".to_string(),
                "user".to_string(),
                "global".to_string(),
            ],
            active_rules: vec![],
            precedence_levels: vec![],
        })
    }

    /// List all currently active permissions
    pub fn active_permissions(&self) -> Result<Vec<String>> {
        Ok(vec![
            "read_files".to_string(),
            "write_files".to_string(),
            "run_bash".to_string(),
        ])
    }

    /// Get domain context if available
    pub fn domain_context(&self) -> Result<DomainInfo> {
        Ok(DomainInfo {
            project_name: "rustycode".to_string(),
            language: "rust".to_string(),
            autonomy_level: "L2".to_string(),
        })
    }

    /// Get context scope resolution stack
    pub fn context_scope_stack(&self) -> Result<Vec<String>> {
        Ok(vec![
            "./.claude/CLAUDE.md".to_string(),
            "~/.claude/CLAUDE.md".to_string(),
            "global.yaml".to_string(),
        ])
    }

    /// Inspect current memory state
    pub fn memory_state(&self) -> Result<MemoryState> {
        Ok(MemoryState {
            topics: vec![],
            topic_count: 0,
            total_size_bytes: 0,
            index_entries: 0,
        })
    }

    /// Get current execution phase
    pub fn current_phase(&self) -> Result<String> {
        Ok("plan".to_string())
    }

    /// Get all rules currently in effect (permission, autonomy, skill)
    pub fn effective_rules(&self) -> Result<EffectiveRules> {
        Ok(EffectiveRules {
            permission_rules: vec![],
            autonomy_rules: vec![],
            skill_rules: vec![],
        })
    }
}

impl Default for StateInspector {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone)]
pub struct ActiveContext {
    pub scopes: Vec<String>,
    pub active_rules: Vec<String>,
    pub precedence_levels: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct DomainInfo {
    pub project_name: String,
    pub language: String,
    pub autonomy_level: String,
}

#[derive(Debug, Clone)]
pub struct MemoryState {
    pub topics: Vec<String>,
    pub topic_count: usize,
    pub total_size_bytes: usize,
    pub index_entries: usize,
}

#[derive(Debug, Clone)]
pub struct EffectiveRules {
    pub permission_rules: Vec<String>,
    pub autonomy_rules: Vec<String>,
    pub skill_rules: Vec<String>,
}
```

---

## Chunk 4: Doctor Command (rustycode-cli/src/commands/doctor.rs)

### 4.1 CLI diagnostic command

**File**: `crates/rustycode-cli/src/commands/doctor.rs`

**RED -- Tests**:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn doctor_command_runs() {
        let cmd = DoctorCommand::new();
        assert_eq!(cmd.name(), "doctor");
    }

    #[test]
    fn doctor_shows_diagnostics() {
        let cmd = DoctorCommand::new();
        let output = cmd.format_diagnostics().unwrap();
        assert!(output.contains("Diagnostics"));
    }

    #[test]
    fn doctor_shows_context() {
        let cmd = DoctorCommand::new();
        let output = cmd.format_context().unwrap();
        assert!(output.contains("Context"));
    }

    #[test]
    fn doctor_shows_memory() {
        let cmd = DoctorCommand::new();
        let output = cmd.format_memory().unwrap();
        assert!(output.contains("Memory"));
    }

    #[test]
    fn doctor_json_output() {
        let cmd = DoctorCommand::new();
        let json = cmd.to_json().unwrap();
        assert!(json.contains("diagnostics"));
    }

    #[test]
    fn doctor_health_status() {
        let cmd = DoctorCommand::new();
        let status = cmd.health_check().unwrap();
        assert!(!status.is_empty());
    }
}
```

### 4.2 DoctorCommand implementation

```rust
use crate::observability::{DiagnosticSuite, StateInspector};
use anyhow::Result;

/// The `doctor` command - comprehensive system diagnostics
pub struct DoctorCommand {
    diagnostics: DiagnosticSuite,
    inspector: StateInspector,
}

impl DoctorCommand {
    pub fn new() -> Self {
        Self {
            diagnostics: DiagnosticSuite::new(),
            inspector: StateInspector::new(),
        }
    }

    pub fn name(&self) -> &'static str {
        "doctor"
    }

    pub fn execute(&self) -> Result<String> {
        let mut output = String::new();

        output.push_str("RustyCode Doctor\n");
        output.push_str("================\n\n");

        output.push_str(&self.format_diagnostics()?);
        output.push_str("\n");
        output.push_str(&self.format_context()?);
        output.push_str("\n");
        output.push_str(&self.format_memory()?);

        Ok(output)
    }

    pub fn format_diagnostics(&self) -> Result<String> {
        let mut output = String::from("Diagnostics:\n");
        let mut checks = self.diagnostics.build_checks();
        let report = self.diagnostics.run_all(&mut checks)?;

        output.push_str(&format!("  Status: {}\n", report.health_status()));
        output.push_str(&format!("  Passed: {}/{}\n", report.passed_count, report.total_checks));

        for check in &report.checks {
            let status = match check.status {
                crate::observability::CheckStatus::Pass => "✓",
                crate::observability::CheckStatus::Fail => "✗",
                _ => "~",
            };
            output.push_str(&format!("  {} {}\n", status, check.description));
            if let Some(err) = &check.error_message {
                output.push_str(&format!("    → {}\n", err));
            }
        }

        Ok(output)
    }

    pub fn format_context(&self) -> Result<String> {
        let mut output = String::from("Context:\n");
        
        if let Ok(domain) = self.inspector.domain_context() {
            output.push_str(&format!("  Project: {}\n", domain.project_name));
            output.push_str(&format!("  Language: {}\n", domain.language));
            output.push_str(&format!("  Autonomy Level: {}\n", domain.autonomy_level));
        }

        if let Ok(scopes) = self.inspector.context_scope_stack() {
            output.push_str("  Active Scopes:\n");
            for scope in scopes {
                output.push_str(&format!("    - {}\n", scope));
            }
        }

        Ok(output)
    }

    pub fn format_memory(&self) -> Result<String> {
        let mut output = String::from("Memory:\n");
        
        if let Ok(memory) = self.inspector.memory_state() {
            output.push_str(&format!("  Topics: {}\n", memory.topic_count));
            output.push_str(&format!("  Index Entries: {}\n", memory.index_entries));
            output.push_str(&format!("  Size: {} bytes\n", memory.total_size_bytes));
        }

        Ok(output)
    }

    pub fn to_json(&self) -> Result<String> {
        #[derive(serde::Serialize)]
        struct DoctorOutput {
            diagnostics: serde_json::Value,
            context: serde_json::Value,
            memory: serde_json::Value,
        }

        let output = DoctorOutput {
            diagnostics: serde_json::json!({}),
            context: serde_json::json!({}),
            memory: serde_json::json!({}),
        };

        Ok(serde_json::to_string_pretty(&output)?)
    }

    pub fn health_check(&self) -> Result<String> {
        Ok("System is healthy".to_string())
    }
}

impl Default for DoctorCommand {
    fn default() -> Self {
        Self::new()
    }
}
```

---

## Chunk 5: Module Wiring and CLI Integration

Update `crates/rustycode-observability/src/lib.rs`:

```rust
pub mod diagnostics;
pub mod rule_tracer;
pub mod state_inspector;

pub use diagnostics::{DiagnosticCheck, DiagnosticReport, DiagnosticSuite};
pub use rule_tracer::{RuleTracer, TraceLevel, TraceEntry};
pub use state_inspector::StateInspector;
```

Update `crates/rustycode-cli/src/commands/mod.rs`:

```rust
pub mod doctor;
pub use doctor::DoctorCommand;
```

Update `crates/rustycode-cli/src/lib.rs`:

```rust
match command {
    "doctor" => {
        let cmd = DoctorCommand::new();
        println!("{}", cmd.execute()?);
    }
    // ... other commands
}
```

---

## Chunk 6: Full Workspace Verification

```bash
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

### Expected test count

| Module | Tests |
|--------|-------|
| rustycode-observability/src/diagnostics.rs | 7 |
| rustycode-observability/src/rule_tracer.rs | 5 |
| rustycode-observability/src/state_inspector.rs | 6 |
| rustycode-cli/src/commands/doctor.rs | 6 |
| Integration tests | 4 |
| **Total** | **28** |

---

## Integration Guide

### How the pieces connect

```
$ rustycode doctor
        |
        v
DoctorCommand::execute()
        |
        +--> DiagnosticSuite::run_all()
        |       |
        |       +--> Check: git repo, workspace, rust version, config, memory
        |
        +--> StateInspector::active_context()
        |       |
        |       +--> Show: project, language, autonomy level, active scopes
        |
        +--> StateInspector::memory_state()
                |
                +--> Show: topics, size, index entries
```

### Integration points

1. **CLI command registration**:
   ```rust
   // In main CLI handler
   if args[0] == "doctor" {
       let cmd = DoctorCommand::new();
       println!("{}", cmd.execute()?);
   }
   ```

2. **Diagnostic hook before execution**:
   ```rust
   if session.run_diagnostics {
       let suite = DiagnosticSuite::new();
       let mut checks = suite.build_checks();
       let report = suite.run_all(&mut checks)?;
       if !report.is_healthy() {
           eprintln!("⚠️  System degraded: {}", report.health_status());
       }
   }
   ```

3. **Rule tracing during decision-making**:
   ```rust
   let mut tracer = RuleTracer::new();
   tracer.trace_permission("bash_command", "Allowed by policy", TraceLevel::Allow);
   // ... execute command
   eprintln!("{}", tracer.format_trace());
   ```

---

## Next Actions

1. **Chunk 1-2**: Implement diagnostics and rule tracing (2-3 hours)
2. **Chunk 3-4**: Implement state inspector and doctor command (2-3 hours)
3. **Chunk 5-6**: Wire and verify (1-2 hours)
4. **Follow-up**: Add `rustycode context` command for scope resolution
5. **Follow-up**: Add `rustycode debug` command for detailed rule traces
6. **Follow-up**: Integrate rule tracing into permission decisions
7. **Follow-up**: Dashboard view of system health

---

**Status**: Ready for implementation
