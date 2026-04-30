# Plan Mode Redesign Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace ExecutionPhase-based plan mode with role-based tool access matrices and convoy-level planning gates, integrating with the Coordinator/Builder/Skeptic/Judge team model.

**Architecture:** 
- **Phase 1-2**: Refactor data structures (plan_mode.rs, convoy.rs)
- **Phase 3**: Integrate with Coordinator execution loop
- **Phase 4-5**: Tool gating and TUI updates
- **Phase 6**: Auto mode integration

**Tech Stack:** Rust 2021, Tokio async, Serde JSON, existing team/convoy infrastructure

---

## File Structure

### Creating
- `crates/rustycode-orchestra/src/tool_access_matrix.rs` — Role-based tool access

### Modifying
- [MODIFY] [plan_mode.rs](file:///Users/nat/dev/rustycode/crates/rustycode-orchestra/src/plan_mode.rs) — Complete refactor (remove ExecutionPhase, add role matrix)
- [MODIFY] [convoy.rs](file:///Users/nat/dev/rustycode/crates/rustycode-orchestra/src/convoy.rs) — Add ConvoyPlan struct and status variants
- [MODIFY] [coordinator.rs](file:///Users/nat/dev/rustycode/crates/rustycode-core/src/team/coordinator.rs) — Add plan field and methods
- [MODIFY] [plan_mode_ops.rs](file:///Users/nat/dev/rustycode/crates/rustycode-tui/src/app/plan_mode_ops.rs) — Per-convoy planning display
- [MODIFY] [auto.rs](file:///Users/nat/dev/rustycode/crates/rustycode-orchestra/src/auto.rs) — Remove Arc<Mutex>, integrate role-based access
- [MODIFY] [lib.rs](file:///Users/nat/dev/rustycode/crates/rustycode-tools/src/lib.rs) — Update ToolContext with role and plan mode
- [MODIFY] [bash.rs](file:///Users/nat/dev/rustycode/crates/rustycode-tools/src/bash.rs) — Add role gating
- [MODIFY] [fs.rs](file:///Users/nat/dev/rustycode/crates/rustycode-tools/src/fs.rs) — Add role gating for WriteFileTool
- [MODIFY] [edit.rs](file:///Users/nat/dev/rustycode/crates/rustycode-tools/src/edit.rs) — Add role gating for EditFile

### Testing
- `crates/rustycode-orchestra/tests/plan_mode_roles.rs` — Role-based access tests
- `crates/rustycode-orchestra/tests/convoy_plan_e2e.rs` — End-to-end convoy flow

---

## Phase 1: Refactor plan_mode.rs (Role-Based Tool Access)

### Task 1: Create tool_access_matrix.rs helper module

**Files:**
- Create: `crates/rustycode-orchestra/src/tool_access_matrix.rs`

- [ ] **Step 1: Write tool access matrix builder**

```rust
//! Role-based tool access matrix.
//!
//! Defines which tools each agent role can access during execution.

use std::collections::{HashMap, HashSet};
use rustycode_core::team::AgentRole;

/// Build the role-to-tools access matrix
pub fn build_access_matrix() -> HashMap<AgentRole, HashSet<&'static str>> {
    let mut matrix = HashMap::new();

    // Planner: Analyze, research, write plans
    let mut planner_tools = HashSet::new();
    planner_tools.extend(&["read", "read_file", "grep", "glob", "list_dir", "lsp", "web_search", "web_fetch", "write", "edit_file", "bash", "Agent", "TaskCreate", "TaskList", "TaskUpdate"]);
    matrix.insert(AgentRole::Planner, planner_tools);

    // Worker: Execute approved plans
    let mut worker_tools = HashSet::new();
    worker_tools.extend(&["read", "read_file", "grep", "glob", "list_dir", "lsp", "write", "edit_file", "apply_patch", "bash"]);
    matrix.insert(AgentRole::Worker, worker_tools);

    // Reviewer: Verify and test
    let mut reviewer_tools = HashSet::new();
    reviewer_tools.extend(&["read", "read_file", "grep", "glob", "lsp", "web_fetch", "bash"]);
    matrix.insert(AgentRole::Reviewer, reviewer_tools);

    // Researcher: Explore only
    let mut researcher_tools = HashSet::new();
    researcher_tools.extend(&["read", "read_file", "grep", "glob", "lsp", "web_search", "web_fetch", "Agent"]);
    matrix.insert(AgentRole::Researcher, researcher_tools);

    matrix
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matrix_has_all_roles() {
        let matrix = build_access_matrix();
        assert!(matrix.contains_key(&AgentRole::Planner));
        assert!(matrix.contains_key(&AgentRole::Worker));
        assert!(matrix.contains_key(&AgentRole::Reviewer));
        assert!(matrix.contains_key(&AgentRole::Researcher));
    }

    #[test]
    fn planner_has_write_and_bash() {
        let matrix = build_access_matrix();
        let planner = &matrix[&AgentRole::Planner];
        assert!(planner.contains("write"));
        assert!(planner.contains("bash"));
    }

    #[test]
    fn reviewer_cannot_write() {
        let matrix = build_access_matrix();
        let reviewer = &matrix[&AgentRole::Reviewer];
        assert!(!reviewer.contains("write"));
        assert!(!reviewer.contains("edit_file"));
    }
}
```

- [ ] **Step 2: Add to lib.rs**

```rust
pub mod tool_access_matrix;
```

- [ ] **Step 3: Run tests**

```bash
cd crates/rustycode-orchestra
cargo test tool_access_matrix
```

- [ ] **Step 4: Commit**

```bash
git add crates/rustycode-orchestra/src/tool_access_matrix.rs
git add crates/rustycode-orchestra/src/lib.rs
git commit -m "feat: add role-based tool access matrix"
```

---

### Task 2: Refactor plan_mode.rs (remove ExecutionPhase, add roles)

**Files:**
- Modify: `crates/rustycode-orchestra/src/plan_mode.rs`

- [ ] **Step 1: Update ToolBlockedReason enum**

```rust
use rustycode_core::team::AgentRole;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ToolBlockedReason {
    NotAllowedForRole { tool: String, role: AgentRole },
    RequiresApproval,
    ConvoyPlanNotApproved,
    UnknownRole(AgentRole),
}

impl std::fmt::Display for ToolBlockedReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotAllowedForRole { tool, role } => {
                write!(f, "Tool '{}' not allowed for {:?} role", tool, role)
            }
            Self::RequiresApproval => {
                write!(f, "Plan approval required before tool access")
            }
            Self::ConvoyPlanNotApproved => {
                write!(f, "Convoy plan not yet approved")
            }
            Self::UnknownRole(role) => {
                write!(f, "Unknown agent role: {:?}", role)
            }
        }
    }
}
```

- [ ] **Step 2: Remove ExecutionPhase enum and phase fields**

Delete code containing:
- `ExecutionPhase { Planning, Implementation }`
- `current_phase: ExecutionPhase`
- `approved_plans: HashSet<String>`
- `current_plan: Option<Plan>`

- [ ] **Step 3: Add role-based tool matrix**

```rust
pub struct PlanMode {
    config: PlanModeConfig,
    role_tool_matrix: HashMap<AgentRole, HashSet<&'static str>>,
}

impl PlanMode {
    pub fn new(config: PlanModeConfig) -> Self {
        let role_tool_matrix = crate::tool_access_matrix::build_access_matrix();
        Self {
            config,
            role_tool_matrix,
        }
    }

    /// Check if an agent with a given role can use a tool
    pub fn can_use_tool(
        &self,
        role: AgentRole,
        tool: &str,
    ) -> Result<(), ToolBlockedReason> {
        if !self.config.enabled {
            return Ok(());
        }

        let allowed = self.role_tool_matrix
            .get(&role)
            .ok_or(ToolBlockedReason::UnknownRole(role))?;

        if allowed.contains(tool) {
            Ok(())
        } else {
            Err(ToolBlockedReason::NotAllowedForRole {
                tool: tool.to_string(),
                role,
            })
        }
    }
}
```

- [ ] **Step 4: Add approval assessment method**

```rust
pub fn assess_approval_required(&self, plan: &Plan) -> bool {
    if !self.config.require_approval {
        return false;
    }

    plan.risks.iter().any(|r| r.level >= RiskLevel::High)
        || plan.estimated_cost_usd > 1.00
}
```

- [ ] **Step 5: Remove old methods**

Delete: `set_phase()`, `current_phase()`, `is_edit_dry_run()`, `submit_plan()`, `approve()`, `reject()`, `reset()`, `is_approved()`, `approve_plan()`

- [ ] **Step 6: Update tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use rustycode_core::team::AgentRole;

    #[test]
    fn can_use_tool_planner() {
        let pm = PlanMode::new(PlanModeConfig::default());
        assert!(pm.can_use_tool(AgentRole::Planner, "read").is_ok());
        assert!(pm.can_use_tool(AgentRole::Planner, "write").is_ok());
    }

    #[test]
    fn can_use_tool_reviewer_blocked() {
        let pm = PlanMode::new(PlanModeConfig::default());
        assert!(pm.can_use_tool(AgentRole::Reviewer, "write").is_err());
    }
}
```

- [ ] **Step 7: Run tests**

```bash
cargo test plan_mode
```

- [ ] **Step 8: Commit**

```bash
git add crates/rustycode-orchestra/src/plan_mode.rs
git commit -m "refactor: replace ExecutionPhase with role-based tool access

- Remove ExecutionPhase (global Planning/Implementation states)
- Add role-based tool access matrix (Planner, Worker, Reviewer, Researcher)
- Add can_use_tool(role, tool) -> Result method
- Add assess_approval_required() for plan gating
- Update tests to use role-based access
"
```

---

## Phase 2: Extend Convoy with ConvoyPlan

### Task 3: Add ConvoyPlan struct to convoy.rs

**Files:**
- Modify: `crates/rustycode-orchestra/src/convoy.rs`

- [ ] **Step 1: Add ConvoyPlan struct**

```rust
use rustycode_core::team::AgentId;
use chrono::{DateTime, Utc};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ConvoyPlan {
    pub id: String,
    pub summary: String,
    pub approach: String,
    pub files_to_modify: Vec<FilePlan>,
    pub commands_to_run: Vec<CommandPlan>,
    pub risks: Vec<Risk>,
    pub estimated_cost_usd: f64,
    pub estimated_tokens: TokenEstimate,
    pub success_criteria: Vec<String>,
    pub requires_approval: bool,
    pub approval_reason: Option<String>,
    pub approved_by: Option<String>,
    pub approved_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub created_by_agent: AgentId,
}

impl ConvoyPlan {
    pub fn is_approved(&self) -> bool {
        !self.requires_approval || self.approved_at.is_some()
    }

    pub fn approve(&mut self, approved_by: String) {
        self.approved_by = Some(approved_by);
        self.approved_at = Some(Utc::now());
    }
}
```

- [ ] **Step 2: Update ConvoyStatus enum**

```rust
pub enum ConvoyStatus {
    Pending,
    Planning,
    PlanReady,
    PlanApproved,
    InProgress,
    Completed,
    Failed,
    Paused,
}
```

- [ ] **Step 3: Add plan field to Convoy**

```rust
pub struct Convoy {
    pub id: ConvoyId,
    pub title: String,
    pub status: ConvoyStatus,
    pub plan: Option<ConvoyPlan>,  // NEW
    pub tasks: Vec<ConvoyTask>,
    pub created_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
}
```

- [ ] **Step 4: Add tests**

```rust
#[test]
fn convoy_plan_approval() {
    let mut plan = ConvoyPlan {
        requires_approval: true,
        approved_by: None,
        approved_at: None,
        ..ConvoyPlan::default()
    };
    assert!(!plan.is_approved());
    plan.approve("user-1".to_string());
    assert!(plan.is_approved());
}

#[test]
fn convoy_status_transitions() {
    let mut convoy = Convoy::new("test");
    convoy.status = ConvoyStatus::Planning;
    convoy.status = ConvoyStatus::PlanReady;
    convoy.status = ConvoyStatus::PlanApproved;
    assert_eq!(convoy.status, ConvoyStatus::PlanApproved);
}
```

- [ ] **Step 5: Run tests**

```bash
cargo test convoy
```

- [ ] **Step 6: Commit**

```bash
git add crates/rustycode-orchestra/src/convoy.rs
git commit -m "feat: add ConvoyPlan and extended ConvoyStatus

- Add ConvoyPlan struct with approval tracking
- Extend ConvoyStatus with Planning, PlanReady, PlanApproved
- Add plan: Option<ConvoyPlan> to Convoy struct
- Add approval methods and tests
"
```

---

## Phase 3: Integrate with Coordinator

### Task 4: Extend Coordinator to accept plan

**Files:**
- Modify: `crates/rustycode-core/src/team/coordinator.rs`

- [ ] **Step 1: Add plan field**

```rust
use rustycode_orchestra::convoy::ConvoyPlan;

pub struct Coordinator {
    project_root: PathBuf,
    state: TeamLoopState,
    plan: Option<ConvoyPlan>,
    attempt_log: Vec<AttemptSummary>,
    insights: Vec<String>,
    structural_declaration: Option<StructuralDeclaration>,
}
```

- [ ] **Step 2: Add with_plan constructor**

```rust
impl Coordinator {
    pub fn with_plan(
        project_root: PathBuf,
        team_config: TeamConfig,
        plan: Option<ConvoyPlan>,
    ) -> Self {
        let state = TeamLoopState::new(team_config);
        Self {
            project_root,
            state,
            plan,
            attempt_log: Vec::new(),
            insights: Vec::new(),
            structural_declaration: None,
        }
    }

    pub fn plan(&self) -> Option<&ConvoyPlan> {
        self.plan.as_ref()
    }
}
```

- [ ] **Step 3: Update architect_phase**

```rust
pub fn architect_phase(&mut self) -> Result<ArchitectOutcome, Error> {
    if let Some(ref plan) = self.plan {
        tracing::info!("Architect validating plan assumptions");
    }
    // ... existing logic
    Ok(outcome)
}
```

- [ ] **Step 4: Update builder_phase**

```rust
pub fn builder_phase(&mut self) -> Result<BuilderAction, Error> {
    if let Some(ref plan) = self.plan {
        tracing::info!("Builder executing plan task");
    }
    // ... existing logic
    Ok(action)
}
```

- [ ] **Step 5: Write tests**

```rust
#[test]
fn coordinator_with_plan() {
    let plan = ConvoyPlan::default();
    let coord = Coordinator::with_plan(
        PathBuf::from("."),
        TeamConfig::default(),
        Some(plan.clone()),
    );
    assert!(coord.plan().is_some());
}
```

- [ ] **Step 6: Run tests and commit**

```bash
cargo test coordinator
git add crates/rustycode-core/src/team/coordinator.rs
git commit -m "feat: add plan support to Coordinator"
```

---

## Phase 4: Tool System & Executor Gating

### Task 5: Update Tool System Core Types

**Files:**
- [MODIFY] [lib.rs](file:///Users/nat/dev/rustycode/crates/rustycode-tools/src/lib.rs)

- [ ] **Step 1: Update ToolContext**

Add `agent_role` and `plan_mode` to `ToolContext`.

```rust
// crates/rustycode-tools/src/lib.rs

pub struct ToolContext {
    pub cwd: PathBuf,
    pub sandbox: SandboxConfig,
    pub max_permission: ToolPermission,
    pub cancellation_token: Option<CancellationToken>,
    pub interactive_permissions: bool,
    pub agent_role: rustycode_orchestra::agent_identity::AgentRole, // NEW
    pub plan_mode: Option<std::sync::Arc<std::sync::Mutex<rustycode_orchestra::plan_mode::PlanMode>>>, // NEW
}
```

- [ ] **Step 2: Update ToolContext constructors**

Update `new()`, `with_sandbox()`, etc., to initialize these fields.

---

### Task 6: Add role-based gating to tool executors

**Files:**
- [MODIFY] [bash.rs](file:///Users/nat/dev/rustycode/crates/rustycode-tools/src/bash.rs)
- [MODIFY] [fs.rs](file:///Users/nat/dev/rustycode/crates/rustycode-tools/src/fs.rs) (WriteFileTool)
- [MODIFY] [edit.rs](file:///Users/nat/dev/rustycode/crates/rustycode-tools/src/edit.rs) (EditFile)

- [ ] **Step 1: Update BashTool execute**

```rust
// crates/rustycode-tools/src/bash.rs

impl Tool for BashTool {
    fn execute(&self, params: Value, ctx: &ToolContext) -> Result<ToolOutput> {
         // Check plan mode gating
         if let Some(ref pm_lock) = ctx.plan_mode {
             let pm = pm_lock.lock().unwrap_or_else(|e| e.into_inner());
             pm.can_use_tool(ctx.agent_role, "bash")?;
         }
         
         // ... existing execution logic
    }
}
```

- [ ] **Step 2: Update WriteFileTool execute**

```rust
// crates/rustycode-tools/src/fs.rs

impl Tool for WriteFileTool {
    fn execute(&self, params: Value, ctx: &ToolContext) -> Result<ToolOutput> {
         // Check plan mode gating
         if let Some(ref pm_lock) = ctx.plan_mode {
             let pm = pm_lock.lock().unwrap_or_else(|e| e.into_inner());
             pm.can_use_tool(ctx.agent_role, "write_file")?;
         }

         crate::check_permission(self.permission(), ctx)?;
         // ...
    }
}
```

- [ ] **Step 3: Update EditFile execute**

```rust
// crates/rustycode-tools/src/edit.rs

impl Tool for EditFile {
    fn execute(&self, params: Value, ctx: &ToolContext) -> Result<ToolOutput> {
         // Check plan mode gating
         if let Some(ref pm_lock) = ctx.plan_mode {
             let pm = pm_lock.lock().unwrap_or_else(|e| e.into_inner());
             pm.can_use_tool(ctx.agent_role, "edit_file")?;
         }

         // ... existing execution logic
    }
}
```

- [ ] **Step 4: Write tests for role blocking**

```rust
#[tokio::test]
async fn planner_can_bash() {
    let pm = Arc::new(Mutex::new(PlanMode::new(PlanModeConfig::default())));
    let mut ctx = ToolContext::new(".");
    ctx.agent_role = AgentRole::Planner;
    ctx.plan_mode = Some(pm);
    
    let tool = BashTool::new();
    let result = tool.execute(json!({"command": "echo test"}), &ctx);
    assert!(result.is_ok());
}

#[tokio::test]
async fn reviewer_cannot_bash() {
    let pm = Arc::new(Mutex::new(PlanMode::new(PlanModeConfig::default())));
    let mut ctx = ToolContext::new(".");
    ctx.agent_role = AgentRole::Reviewer;
    ctx.plan_mode = Some(pm);
    
    let tool = BashTool::new();
    let result = tool.execute(json!({"command": "echo test"}), &ctx);
    assert!(result.is_err());
}
```

- [ ] **Step 5: Run tests and commit**

```bash
cargo test bash write edit
git add crates/rustycode-tools/src/bash.rs
git add crates/rustycode-tools/src/write.rs
git add crates/rustycode-tools/src/edit.rs
git commit -m "feat: add role-based gating to tool executors"
```

---

---

## Phase 5: TUI Updates

### Task 7: Update plan_mode_ops.rs for per-convoy tracking

**Files:**
- Modify: `crates/rustycode-tui/src/app/plan_mode_ops.rs`

- [ ] **Step 1: Update PlanModeBanner enum**

```rust
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PlanModeBanner {
    Planning { convoy_id: String, action_hint: String },
    AwaitingApproval { convoy_id: String, plan_summary: String, action_hint: String },
    PlanApproved { convoy_id: String, action_hint: String },
    Executing { convoy_id: String, current_task: String, action_hint: String },
}
```

- [ ] **Step 2: Update display methods**

```rust
impl PlanModeBanner {
    pub(crate) fn description(&self) -> String {
        match self {
            Self::Planning { convoy_id, .. } => format!("[{}] Planning...", convoy_id),
            Self::AwaitingApproval { convoy_id, plan_summary, .. } => {
                format!("[{}] {} — approve to proceed", convoy_id, plan_summary)
            }
            Self::PlanApproved { convoy_id, .. } => format!("[{}] Plan approved, executing...", convoy_id),
            Self::Executing { convoy_id, current_task, .. } => {
                format!("[{}] Executing: {}", convoy_id, current_task)
            }
        }
    }
}
```

- [ ] **Step 3: Add TUI methods**

```rust
impl TUI {
    pub(crate) fn show_plan_mode_planning_for_convoy(&mut self, convoy_id: &str) {
        self.set_plan_mode_banner(Some(PlanModeBanner::Planning {
            convoy_id: convoy_id.to_string(),
            action_hint: "Analyzing...".to_string(),
        }));
    }

    pub(crate) fn show_plan_awaiting_approval(&mut self, convoy_id: &str, summary: &str) {
        self.set_plan_mode_banner(Some(PlanModeBanner::AwaitingApproval {
            convoy_id: convoy_id.to_string(),
            plan_summary: summary.to_string(),
            action_hint: "Type /approve to proceed".to_string(),
        }));
    }
}
```

- [ ] **Step 4: Run tests and commit**

```bash
cargo test plan_mode_ops
git add crates/rustycode-tui/src/app/plan_mode_ops.rs
git commit -m "feat: update TUI plan mode display for per-convoy tracking"
```

---

---

## Phase 6: Auto Mode Integration

### Task 8: Refactor AutoMode

**Files:**
- [MODIFY] [auto.rs](file:///Users/nat/dev/rustycode/crates/rustycode-orchestra/src/auto.rs)

- [ ] **Step 1: Remove Arc<Mutex<PlanMode>>**

```rust
pub struct AutoMode {
    config: AutoConfig,
    plan_mode: PlanMode,
}

impl AutoMode {
    pub fn new(config: AutoConfig) -> Self {
        Self {
            config,
            plan_mode: PlanMode::new(PlanModeConfig::default()),
        }
    }

    pub fn plan_mode(&self) -> &PlanMode {
        &self.plan_mode
    }
}
```

- [ ] **Step 2: Remove legacy methods**

Remove the following mock/backward-compatibility methods that are no longer needed with the new architecture:
- `generate_plan()`
- `approve_plan()`
- `reject_plan()`
- `execute_task()`
- `execute_plan()`

These logic blocks will move into the `Coordinator` and `Convoy` management systems.

- [ ] **Step 3: Run tests and commit**

```bash
cargo test auto
git add crates/rustycode-orchestra/src/auto.rs
git commit -m "refactor: remove Arc<Mutex> from AutoMode"
```

---

---

## Phase 7: Integration & Verification

### Task 9: End-to-end integration test

**Files:**
- Create: `crates/rustycode-orchestra/tests/convoy_plan_e2e.rs`

- [ ] **Step 1: Write full lifecycle test**

```rust
#[tokio::test]
async fn full_convoy_lifecycle() {
    use rustycode_orchestra::convoy::*;
    use rustycode_orchestra::plan_mode::*;
    use rustycode_core::team::AgentRole;

    // Create → Planning → PlanReady → PlanApproved → InProgress → Completed
    let mut convoy = Convoy::new("test");
    assert_eq!(convoy.status, ConvoyStatus::Pending);

    convoy.status = ConvoyStatus::Planning;
    let plan = ConvoyPlan::default();
    convoy.plan = Some(plan);
    convoy.status = ConvoyStatus::PlanReady;
    convoy.status = ConvoyStatus::PlanApproved;
    
    let pm = PlanMode::new(PlanModeConfig::default());
    assert!(pm.can_use_tool(AgentRole::Worker, "write").is_ok());
    
    convoy.status = ConvoyStatus::Completed;
    assert_eq!(convoy.status, ConvoyStatus::Completed);
}
```

- [ ] **Step 2: Write role separation test**

```rust
#[test]
fn role_tool_access() {
    let pm = PlanMode::new(PlanModeConfig::default());

    assert!(pm.can_use_tool(AgentRole::Planner, "write").is_ok());
    assert!(pm.can_use_tool(AgentRole::Worker, "write").is_ok());
    assert!(pm.can_use_tool(AgentRole::Reviewer, "write").is_err());
    assert!(pm.can_use_tool(AgentRole::Researcher, "write").is_err());
}
```

- [ ] **Step 3: Run all tests**

```bash
cargo test --workspace
```

- [ ] **Step 4: Run clippy and fmt**

```bash
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --check
```

- [ ] **Step 5: Final commit**

```bash
git add crates/rustycode-orchestra/tests/convoy_plan_e2e.rs
git commit -m "test: add end-to-end convoy plan integration tests"
```

---

## Completion Checklist

- [ ] All 8 tasks completed
- [ ] All tests passing
- [ ] Clippy clean
- [ ] Code formatted
- [ ] Build succeeds

**Plan complete!** Ready for subagent-driven execution.
